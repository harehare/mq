//! Mutation testing: mutates the modules the tests use and checks that the tests notice.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mq_lang::RuntimeValue;
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{OutputFormat, TestRunner};
use crate::coverage::{CoverageData, CoverageHandler};
use crate::mutation::{self, Mutant};
use crate::resolver::{ResolvedModules, TestModuleResolver};

/// A test file with the query that runs its selected tests.
struct Suite {
    file: PathBuf,
    query: String,
    /// Wall-clock time of the unmutated run.
    baseline: Duration,
}

/// A module under test, with the lines each suite executed.
struct Target {
    /// Name used to `include`/`import` the module.
    name: String,
    path: PathBuf,
    source: String,
    /// Suite index -> executed lines.
    lines: FxHashMap<usize, FxHashSet<usize>>,
}

struct Outcome {
    path: PathBuf,
    mutant: Mutant,
    killed: bool,
}

fn module_key(name: &str) -> &str {
    name.strip_suffix(".mq").unwrap_or(name)
}

impl TestRunner {
    pub(super) fn run_mutation(&self, test_files: &[PathBuf]) -> miette::Result<bool> {
        let mut suites = Vec::new();
        let mut targets: Vec<Target> = Vec::new();

        for file in test_files {
            let content =
                fs::read_to_string(file).map_err(|e| miette::miette!("failed to read {}: {e}", file.display()))?;
            let tests: Vec<_> = Self::discover_tests(&content)
                .into_iter()
                .filter(|test| self.matches(test))
                .collect();
            if tests.is_empty() {
                continue;
            }
            let query = Self::build_test_query_with_fn(&content, &tests, "run_tests_data");

            let resolved: ResolvedModules = Arc::default();
            let coverage = CoverageData::default();
            let resolver = TestModuleResolver::default().with_recorder(Arc::clone(&resolved));
            let mut engine = self.build_engine(file, resolver);
            engine.set_debugger_handler(Box::new(CoverageHandler(coverage.clone())));
            let debugger = engine.debugger();
            debugger.write().unwrap().activate();
            debugger
                .write()
                .unwrap()
                .set_command(mq_lang::DebuggerCommand::StepInto);

            let started = Instant::now();
            let passed = Self::passed(engine.eval(&query, mq_lang::null_input().into_iter()));
            let baseline = started.elapsed();
            if !passed {
                return Err(miette::miette!(
                    "tests in {} fail without mutation; fix them before running mutation testing",
                    file.display()
                ));
            }

            let suite = suites.len();
            let hits = coverage.snapshot();
            for (name, path, source) in resolved.lock().unwrap().iter() {
                if !path.is_file() {
                    continue;
                }
                let lines = hits
                    .iter()
                    .find(|(hit, _)| module_key(hit) == module_key(name))
                    .map(|(_, hits)| hits.lines.clone())
                    .unwrap_or_default();
                match targets.iter_mut().find(|target| target.path == *path) {
                    Some(target) => {
                        target.lines.insert(suite, lines);
                    }
                    None => targets.push(Target {
                        name: name.clone(),
                        path: path.clone(),
                        source: source.clone(),
                        lines: FxHashMap::from_iter([(suite, lines)]),
                    }),
                }
            }
            suites.push(Suite {
                file: file.clone(),
                query,
                baseline,
            });
        }

        let jobs: Vec<(&Target, Mutant)> = targets
            .iter()
            .flat_map(|target| {
                mutation::generate(&target.source, |line| {
                    target.lines.values().any(|lines| lines.contains(&line))
                })
                .into_iter()
                .map(move |mutant| (target, mutant))
            })
            .collect();

        let mut outcomes: Vec<Outcome> = jobs
            .into_par_iter()
            .map(|(target, mutant)| {
                let killed = suites
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| {
                        target
                            .lines
                            .get(index)
                            .is_some_and(|lines| lines.contains(&mutant.line))
                    })
                    .any(|(_, suite)| !self.survives(suite, target, &mutant));
                Outcome {
                    path: target.path.clone(),
                    mutant,
                    killed,
                }
            })
            .collect();
        outcomes
            .sort_by(|a, b| (&a.path, a.mutant.line, a.mutant.column).cmp(&(&b.path, b.mutant.line, b.mutant.column)));

        let killed = outcomes.iter().filter(|o| o.killed).count();
        let score = if outcomes.is_empty() {
            100.0
        } else {
            killed as f64 * 100.0 / outcomes.len() as f64
        };
        match self.format {
            OutputFormat::Text => print!("{}", Self::format_mutation_text(&outcomes, killed, score)),
            OutputFormat::Json => println!("{}", Self::format_mutation_json(&outcomes, killed, score)),
        }

        Ok(self.min_score.is_none_or(|min| score >= f64::from(min)))
    }

    /// `true` when the run finished and no test failed.
    fn passed(result: Result<mq_lang::RuntimeValues, Box<mq_lang::Error>>) -> bool {
        let Ok(values) = result else { return false };
        values
            .values()
            .first()
            .cloned()
            .map(RuntimeValue::to_json_value)
            .and_then(|json| {
                json.as_array().map(|tests| {
                    !tests
                        .iter()
                        .any(|t| t.get("status").and_then(|s| s.as_str()) == Some("failed"))
                })
            })
            .unwrap_or(false)
    }

    /// Runs `suite` against `mutant`; `true` when the tests still pass.
    fn survives(&self, suite: &Suite, target: &Target, mutant: &Mutant) -> bool {
        let resolver = TestModuleResolver::default().with_override(target.name.clone(), mutant.source.clone());
        let mut engine = self.build_engine(&suite.file, resolver);
        // A mutant can loop forever; give it far longer than the unmutated run took.
        engine.set_timeout(suite.baseline * 20 + Duration::from_secs(2));
        Self::passed(engine.eval(&suite.query, mq_lang::null_input().into_iter()))
    }

    fn format_mutation_text(outcomes: &[Outcome], killed: usize, score: f64) -> String {
        let mut out = String::from("# Mutation testing\n\n");
        let survived: Vec<&Outcome> = outcomes.iter().filter(|o| !o.killed).collect();
        if !survived.is_empty() {
            out.push_str("Survived mutants:\n");
            for o in survived {
                out.push_str(&format!(
                    "  {}:{}:{}  {}\n",
                    o.path.display(),
                    o.mutant.line,
                    o.mutant.column,
                    o.mutant.describe()
                ));
            }
            out.push('\n');
        }
        out.push_str(&format!(
            "Mutation score: {score:.1}% ({killed} killed, {} survived, {} total)\n",
            outcomes.len() - killed,
            outcomes.len()
        ));
        out
    }

    fn format_mutation_json(outcomes: &[Outcome], killed: usize, score: f64) -> String {
        let mutants: Vec<serde_json::Value> = outcomes
            .iter()
            .map(|o| {
                serde_json::json!({
                    "file": o.path.display().to_string(),
                    "line": o.mutant.line,
                    "column": o.mutant.column,
                    "original": o.mutant.original,
                    "replacement": o.mutant.replacement,
                    "killed": o.killed,
                })
            })
            .collect();
        serde_json::json!({
            "score": score,
            "killed": killed,
            "survived": outcomes.len() - killed,
            "mutants": mutants,
        })
        .to_string()
    }
}
