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
        self.name == BUILTIN
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
            let mut engine = self.build_engine(file, resolver, None);
            engine.set_debugger_handler(Box::new(LineRecorder(coverage.clone())));
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
            let lines_of = |name: &str| {
                hits.iter()
                    .find(|(hit, _)| module_key(hit) == module_key(name))
                    .map(|(_, hits)| hits.lines.clone())
                    .unwrap_or_default()
            };
            let mut modules: Vec<(String, PathBuf, String)> = resolved
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, path, _)| path.is_file())
                .cloned()
                .collect();
            if options.builtin {
                modules.push((
                    BUILTIN.to_string(),
                    PathBuf::from("builtin.mq"),
                    mq_lang::BUILTIN_MODULE_FILE.to_string(),
                ));
            }
            for (name, path, source) in modules {
                let lines = lines_of(&name);
                match targets.iter_mut().find(|target| target.path == path) {
                    Some(target) => {
                        target.lines.insert(suite, lines);
                    }
                    None => targets.push(Target {
                        name,
                        path,
                        source,
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

        let jobs: Vec<(usize, Mutant)> = targets
            .iter()
            .enumerate()
            .flat_map(|(index, target)| {
                mutation::generate(&target.source, |line| {
                    target.lines.values().any(|lines| lines.contains(&line))
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
                .map(|(index, mutant)| {
                    let target = &targets[index];
                    let killed = suites
                        .iter()
                        .enumerate()
                        .filter(|(suite, _)| {
                            target
                                .lines
                                .get(suite)
                                .is_some_and(|lines| lines.contains(&mutant.line))
                        })
                        .any(|(_, suite)| {
                            // A mutant that crashes the engine itself counts as caught.
                            !catch_unwind(AssertUnwindSafe(|| self.survives(suite, target, &mutant))).unwrap_or(false)
                        });
                    (index, MutantResult { mutant, killed })
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
        let source = mutant.apply(&target.source);
        let (resolver, builtin) = if target.is_builtin() {
            (TestModuleResolver::default(), Some(source.as_str()))
        } else {
            (
                TestModuleResolver::default().with_override(target.name.clone(), source.clone()),
                None,
            )
        };
        let mut engine = self.build_engine(&suite.file, resolver, builtin);
        // A mutant can loop forever; give it far longer than the unmutated run took.
        engine.set_max_call_stack_depth(MUTANT_MAX_CALL_DEPTH);
        engine.set_timeout(suite.baseline * 20 + Duration::from_secs(2));
        Self::passed(engine.eval(&suite.query, mq_lang::null_input().into_iter()))
    }
}
