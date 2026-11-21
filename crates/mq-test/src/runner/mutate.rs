//! Mutation testing: mutates the modules the tests use and checks that the tests notice.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mq_lang::{DebugContext, DebuggerAction, DebuggerHandler, RuntimeValue};
use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use super::TestRunner;
use crate::coverage::CoverageData;
use crate::mutation::{self, Mutant};
use crate::mutation_report::{self, ModuleResult, MutantResult, MutationFormat};
use crate::resolver::{ResolvedModules, TestModuleResolver};

/// Name the bundled `builtin.mq` is registered under.
const BUILTIN: &str = "builtin";

/// Stack size of the threads that run mutants.
const MUTANT_STACK_SIZE: usize = 64 * 1024 * 1024;

/// Call depth limit for mutants, so runaway recursion fails fast.
const MUTANT_MAX_CALL_DEPTH: u32 = 10_000;

/// A test file with the query that runs its selected tests.
struct Suite {
    file: PathBuf,
    query: String,
    /// Wall-clock time of the unmutated run.
    baseline: Duration,
    /// Names of the cases the unmutated run executed, in order.
    cases: Vec<String>,
}

/// How a suite reacted to a mutant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Survived,
    Killed,
    /// The run broke before the tests could judge the mutant (e.g. the test harness failed),
    /// so it says nothing about the tests.
    Inconclusive,
}

/// How a module is imported by one suite, and which of its lines the suite executed.
#[derive(Default)]
struct SuiteUse {
    /// Every import name that resolved to the module.
    names: Vec<String>,
    lines: FxHashSet<usize>,
}

/// Identity of a module under test. The embedded `builtin.mq` is distinct from any file on
/// disk, even one named `builtin.mq`.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetKey {
    Bundled,
    File(PathBuf),
}

/// A module under test, with the lines each suite executed.
struct Target {
    path: PathBuf,
    /// Identity, so differently spelled imports share one target.
    key: TargetKey,
    source: String,
    /// Suite index -> how the suite uses the module.
    suites: FxHashMap<usize, SuiteUse>,
}

/// Settings of a mutation run.
#[derive(Debug, Clone, Default)]
pub struct MutationOptions {
    /// Also mutate the bundled `builtin.mq`, which is slow.
    pub builtin: bool,
    /// Report format.
    pub format: MutationFormat,
    /// Write the report here instead of stdout.
    pub output: Option<PathBuf>,
    /// Mutants run at once. Defaults to half the cores, at most four.
    pub jobs: Option<usize>,
    /// Fail when the score (percent of mutants killed) is below this.
    pub min_score: Option<u32>,
}

impl Target {
    fn is_builtin(&self) -> bool {
        self.key == TargetKey::Bundled
    }
}

/// Records the executed lines of every module, `builtin` included.
#[derive(Debug)]
struct LineRecorder(CoverageData);

impl DebuggerHandler for LineRecorder {
    fn on_step(&self, context: &DebugContext) -> DebuggerAction {
        if let Some(name) = &context.source.name {
            self.0
                .record(name, &context.source.code, context.range.start.line as usize);
        }
        DebuggerAction::StepInto
    }
}

/// Half the cores, at most four: each running mutant holds its own parsed modules.
fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, |n| (n.get() / 2).clamp(1, 4))
}

fn module_key(name: &str) -> &str {
    name.strip_suffix(".mq").unwrap_or(name)
}

impl TestRunner {
    pub(super) fn run_mutation(&self, test_files: &[PathBuf], options: &MutationOptions) -> miette::Result<bool> {
        let mut suites = Vec::new();
        let mut targets: Vec<Target> = Vec::new();

        for file in test_files {
            let content =
                fs::read_to_string(file).map_err(|e| miette::miette!("failed to read {}: {e}", file.display()))?;
            let discovered = Self::discover_tests(&content);
            if let Some(name) = Self::find_duplicate_name(&discovered) {
                return Err(miette::miette!(
                    "duplicate test function `{name}` in {}",
                    file.display()
                ));
            }
            let tests: Vec<_> = discovered.into_iter().filter(|test| self.matches(test)).collect();
            if tests.is_empty() {
                continue;
            }
            let query = Self::build_test_query_with_fn(&content, &tests, "run_tests_data");

            let resolved: ResolvedModules = Arc::default();
            let coverage = CoverageData::default();
            let resolver = TestModuleResolver::default().with_recorder(Arc::clone(&resolved));
            let mut engine = self.build_engine(file, resolver, None);
            engine.set_debugger_handler(Box::new(LineRecorder(coverage.clone())));
            let debugger = engine.debugger();
            debugger.write().unwrap().activate();
            debugger
                .write()
                .unwrap()
                .set_command(mq_lang::DebuggerCommand::StepInto);

            let started = Instant::now();
            let result = engine.eval(&query, mq_lang::null_input().into_iter());
            let baseline = started.elapsed();
            let cases = match Self::case_results(&result) {
                Some(cases) if cases.iter().all(|(_, passed)| *passed) => {
                    cases.into_iter().map(|(name, _)| name).collect()
                }
                _ => {
                    return Err(miette::miette!(
                        "tests in {} fail without mutation; fix them before running mutation testing",
                        file.display()
                    ));
                }
            };

            let suite = suites.len();
            let hits = coverage.snapshot();
            let lines_of = |name: &str| {
                hits.iter()
                    .find(|(hit, _)| module_key(hit) == module_key(name))
                    .map(|(_, hits)| hits.lines.clone())
                    .unwrap_or_default()
            };
            let mut modules: Vec<(String, PathBuf, String, TargetKey)> = resolved
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, path, _)| path.is_file())
                .map(|(name, path, source)| {
                    let key = TargetKey::File(fs::canonicalize(path).unwrap_or_else(|_| path.clone()));
                    (name.clone(), path.clone(), source.clone(), key)
                })
                .collect();
            if options.builtin {
                modules.push((
                    BUILTIN.to_string(),
                    PathBuf::from("builtin.mq"),
                    mq_lang::BUILTIN_MODULE_FILE.to_string(),
                    TargetKey::Bundled,
                ));
            }
            for (name, path, source, key) in modules {
                let lines = lines_of(&name);
                let index = match targets.iter().position(|target| target.key == key) {
                    Some(index) => index,
                    None => {
                        targets.push(Target {
                            path,
                            key,
                            source,
                            suites: FxHashMap::default(),
                        });
                        targets.len() - 1
                    }
                };
                let usage = targets[index].suites.entry(suite).or_default();
                if !usage.names.contains(&name) {
                    usage.names.push(name);
                }
                usage.lines.extend(lines);
            }
            suites.push(Suite {
                file: file.clone(),
                query,
                baseline,
                cases,
            });
        }

        if suites.is_empty() && (self.filter.is_some() || !self.tags.is_empty()) {
            return Err(miette::miette!("no tests matched the given --filter/--tag"));
        }

        let jobs: Vec<(usize, Mutant)> = targets
            .iter()
            .enumerate()
            .flat_map(|(index, target)| {
                mutation::generate(&target.source, |line| {
                    target.suites.values().any(|usage| usage.lines.contains(&line))
                })
                .into_iter()
                .map(move |mutant| (index, mutant))
            })
            .collect();

        // Mutants can recurse without bound, so they run on threads with a large stack.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(options.jobs.unwrap_or_else(default_jobs))
            .stack_size(MUTANT_STACK_SIZE)
            .build()
            .map_err(|e| miette::miette!("failed to start worker threads: {e}"))?;
        let results: Vec<(usize, MutantResult)> = pool.install(|| {
            jobs.into_par_iter()
                .filter_map(|(index, mutant)| {
                    let target = &targets[index];
                    let mut outcome = Outcome::Inconclusive;
                    for (suite, usage) in suites.iter().enumerate().filter_map(|(suite, _)| {
                        target
                            .suites
                            .get(&suite)
                            .filter(|usage| usage.lines.contains(&mutant.line))
                            .map(|usage| (suite, usage))
                    }) {
                        // A mutant that crashes the engine itself counts as caught.
                        let run = catch_unwind(AssertUnwindSafe(|| {
                            self.run_mutant(&suites[suite], target, &usage.names, &mutant)
                        }))
                        .unwrap_or(Outcome::Killed);
                        match run {
                            Outcome::Killed => {
                                outcome = Outcome::Killed;
                                break;
                            }
                            Outcome::Survived => outcome = Outcome::Survived,
                            Outcome::Inconclusive => {}
                        }
                    }
                    let killed = match outcome {
                        Outcome::Killed => true,
                        Outcome::Survived => false,
                        Outcome::Inconclusive => return None,
                    };
                    Some((index, MutantResult { mutant, killed }))
                })
                .collect()
        });

        let mut modules: Vec<ModuleResult> = targets
            .into_iter()
            .map(|target| ModuleResult {
                path: target.path,
                source: target.source,
                mutants: Vec::new(),
            })
            .collect();
        for (index, result) in results {
            modules[index].mutants.push(result);
        }
        modules.retain(|module| !module.mutants.is_empty());
        for module in &mut modules {
            module.mutants.sort_by_key(|r| (r.mutant.line, r.mutant.column));
        }
        modules.sort_by(|a, b| a.path.cmp(&b.path));

        let report = mutation_report::format_report(&modules, options.format);
        match &options.output {
            Some(path) => fs::write(path, report)
                .map_err(|e| miette::miette!("failed to write mutation report to {}: {e}", path.display()))?,
            None => print!("{report}"),
        }

        Ok(options
            .min_score
            .is_none_or(|min| mutation_report::score(&modules) >= f64::from(min)))
    }

    /// The `(name, passed)` of every case in the run, or `None` when the run errored or
    /// returned something other than the test results.
    fn case_results(result: &Result<mq_lang::RuntimeValues, Box<mq_lang::Error>>) -> Option<Vec<(String, bool)>> {
        let values = result.as_ref().ok()?;
        let json = values.values().first().cloned().map(RuntimeValue::to_json_value)?;
        json.as_array()?
            .iter()
            .map(|test| {
                let name = test.get("name")?.as_str()?.to_string();
                let status = test.get("status")?.as_str()?;
                Some((name, status != "failed"))
            })
            .collect()
    }

    /// Whether `cases` contains every baseline case name, counting duplicates.
    fn runs_all(cases: &[(String, bool)], baseline: &[String]) -> bool {
        let mut counts: FxHashMap<&str, isize> = FxHashMap::default();
        for name in baseline {
            *counts.entry(name).or_default() += 1;
        }
        for (name, _) in cases {
            if let Some(count) = counts.get_mut(name.as_str()) {
                *count -= 1;
            }
        }
        counts.values().all(|count| *count <= 0)
    }

    /// Runs `suite` against `mutant`, importing the module as any of `names`.
    fn run_mutant(&self, suite: &Suite, target: &Target, names: &[String], mutant: &Mutant) -> Outcome {
        let source = mutant.apply(&target.source);
        let (resolver, builtin) = if target.is_builtin() {
            (TestModuleResolver::default(), Some(source.as_str()))
        } else {
            (TestModuleResolver::default().with_overrides(names, &source), None)
        };
        let mut engine = self.build_engine(&suite.file, resolver, builtin);
        // A mutant can loop forever; each one costs the whole timeout, so keep it tight.
        engine.set_max_call_stack_depth(MUTANT_MAX_CALL_DEPTH);
        engine.set_timeout(suite.baseline * 5 + Duration::from_millis(100));
        let result = engine.eval(&suite.query, mq_lang::null_input().into_iter());

        let timed_out = result.as_ref().is_err_and(|e| e.is_timeout());
        match Self::case_results(&result) {
            // Extra cases are fine: a mutant can add cases that pass, and that is no kill.
            Some(cases) if Self::runs_all(&cases, &suite.cases) => {
                if cases.iter().all(|(_, passed)| *passed) {
                    Outcome::Survived
                } else {
                    Outcome::Killed
                }
            }
            // The run broke or ran different cases. In `builtin` that is as likely the
            // test harness failing as the tests noticing, so it is not credited.
            _ if target.is_builtin() && !timed_out => Outcome::Inconclusive,
            _ => Outcome::Killed,
        }
    }
}
