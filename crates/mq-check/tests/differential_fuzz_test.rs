//! Differential fuzzing: random small programs are type-checked and run, and the two verdicts are
//! compared.
//!
//! - checker error, runtime success: a false positive of the checker
//! - checker silent, runtime type error: a false negative
//! - any panic of the checker is a failure
//!
//! The generator is deterministic for a given seed. `just fuzz-check` runs many more programs
//! and prints the findings; the default test only runs a few hundred and fails on panics.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use mq_check::{TypeChecker, TypeError};
use mq_hir::Hir;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// True with probability `percent` out of 100.
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a>(&mut self, items: &'a [&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Ty {
    Num,
    Str,
    Bool,
    Arr,
    Dict,
}

const TYPES: [Ty; 5] = [Ty::Num, Ty::Str, Ty::Bool, Ty::Arr, Ty::Dict];

struct Gen {
    rng: Rng,
    /// Percent of sub-expressions that deliberately have the wrong type
    noise: u64,
    vars: Vec<(String, Ty)>,
}

impl Gen {
    fn var_of(&mut self, ty: Ty) -> Option<String> {
        let candidates: Vec<&String> = self.vars.iter().filter(|(_, t)| *t == ty).map(|(n, _)| n).collect();
        if candidates.is_empty() {
            None
        } else {
            Some(candidates[self.rng.below(candidates.len())].clone())
        }
    }

    /// An expression that is meant to have type `ty`.
    fn expr(&mut self, ty: Ty, depth: u32) -> String {
        let ty = if self.rng.chance(self.noise) {
            TYPES[self.rng.below(TYPES.len())]
        } else {
            ty
        };
        if let Some(var) = self.var_of(ty).filter(|_| self.rng.chance(35)) {
            return var;
        }
        let deep = depth > 0;
        match ty {
            Ty::Num => match self.rng.below(if deep { 9 } else { 1 }) {
                0 => self.rng.below(10).to_string(),
                1 => format!(
                    "({} + {})",
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1)
                ),
                2 => format!(
                    "({} - {})",
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1)
                ),
                3 => format!(
                    "({} * {})",
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1)
                ),
                4 => format!("len({})", self.expr(Ty::Arr, depth - 1)),
                5 => format!("len({})", self.expr(Ty::Str, depth - 1)),
                6 => format!("abs({})", self.expr(Ty::Num, depth - 1)),
                7 => format!("get({}, \"a\")", self.expr(Ty::Dict, depth - 1)),
                _ => format!(
                    "max({}, {})",
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1)
                ),
            },
            Ty::Str => match self.rng.below(if deep { 8 } else { 1 }) {
                0 => format!("\"{}\"", self.rng.pick(&["a", "bc", "Hello", " x ", ""])),
                1 => format!(
                    "({} + {})",
                    self.expr(Ty::Str, depth - 1),
                    self.expr(Ty::Str, depth - 1)
                ),
                2 => format!("upcase({})", self.expr(Ty::Str, depth - 1)),
                3 => format!("downcase({})", self.expr(Ty::Str, depth - 1)),
                4 => format!("to_string({})", self.expr(Ty::Num, depth - 1)),
                5 => format!("join({}, \",\")", self.expr(Ty::Arr, depth - 1)),
                6 => format!("trim({})", self.expr(Ty::Str, depth - 1)),
                _ => format!("get({}, \"b\")", self.expr(Ty::Dict, depth - 1)),
            },
            Ty::Bool => match self.rng.below(if deep { 6 } else { 1 }) {
                0 => self.rng.pick(&["true", "false"]).to_string(),
                1 => format!(
                    "({} > {})",
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1)
                ),
                2 => format!(
                    "({} == {})",
                    self.expr(Ty::Str, depth - 1),
                    self.expr(Ty::Str, depth - 1)
                ),
                3 => format!("is_empty({})", self.expr(Ty::Arr, depth - 1)),
                4 => {
                    let any = self.any_type();
                    format!("is_string({})", self.expr(any, depth - 1))
                }
                _ => format!(
                    "({} && {})",
                    self.expr(Ty::Bool, depth - 1),
                    self.expr(Ty::Bool, depth - 1)
                ),
            },
            Ty::Arr => match self.rng.below(if deep { 8 } else { 1 }) {
                0 => format!("[{}, {}]", self.rng.below(10), self.rng.below(10)),
                1 => format!(
                    "[{}, {}, {}]",
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1),
                    self.expr(Ty::Num, depth - 1)
                ),
                2 => format!("map({}, fn(x): x + 1;)", self.expr(Ty::Arr, depth - 1)),
                3 => format!("filter({}, fn(x): x > 1;)", self.expr(Ty::Arr, depth - 1)),
                4 => format!("reverse({})", self.expr(Ty::Arr, depth - 1)),
                5 => format!("sort({})", self.expr(Ty::Arr, depth - 1)),
                6 => format!("range(0, {})", self.rng.below(5)),
                _ => format!(
                    "({} + {})",
                    self.expr(Ty::Arr, depth - 1),
                    self.expr(Ty::Arr, depth - 1)
                ),
            },
            Ty::Dict => format!(
                "{{\"a\": {}, \"b\": {}}}",
                self.expr(Ty::Num, depth.saturating_sub(1)),
                self.expr(Ty::Str, depth.saturating_sub(1))
            ),
        }
    }

    fn any_type(&mut self) -> Ty {
        TYPES[self.rng.below(TYPES.len())]
    }

    /// A whole program: a few `let`s and a final expression.
    fn program(&mut self) -> String {
        self.vars.clear();
        let mut parts = Vec::new();
        for i in 0..self.rng.below(4) {
            let ty = self.any_type();
            let init = self.expr(ty, 2);
            let name = format!("v{i}");
            parts.push(format!("let {name} = {init}"));
            self.vars.push((name, ty));
        }
        let ty = self.any_type();
        let other = self.any_type();
        let last = match self.rng.below(6) {
            0 => format!(
                "if ({}): {} else: {}",
                self.expr(Ty::Bool, 1),
                self.expr(ty, 2),
                self.expr(other, 2)
            ),
            1 => format!("try: {} catch: {}", self.expr(ty, 2), self.expr(ty, 1)),
            _ => self.expr(ty, 3),
        };
        parts.push(last);
        parts.join("\n| ")
    }
}

/// The verdict of the checker on a program.
enum Check {
    Panic(String),
    Clean,
    /// Errors that are definitely wrong programs
    Errors(Vec<String>),
}

fn is_soft(error: &TypeError) -> bool {
    matches!(
        error,
        TypeError::NullablePropagation { .. }
            | TypeError::UnreachableCode { .. }
            | TypeError::NonExhaustiveMatch { .. }
    )
}

fn check(code: &str) -> Check {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut hir = Hir::default();
        hir.add_code(None, code);
        TypeChecker::new().check(&hir)
    }));
    match result {
        Err(payload) => Check::Panic(
            payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default(),
        ),
        Ok(errors) => {
            let errors: Vec<String> = errors.iter().filter(|e| !is_soft(e)).map(|e| e.to_string()).collect();
            if errors.is_empty() {
                Check::Clean
            } else {
                Check::Errors(errors)
            }
        }
    }
}

/// Runtime errors that mean a value had the wrong type for an operation.
fn is_type_error(message: &str) -> bool {
    message.contains("Invalid types") || message.contains("Invalid type") || message.contains("invalid type")
}

#[derive(Default)]
struct Findings {
    programs: usize,
    runtime_ok: usize,
    agree_ok: usize,
    agree_error: usize,
    soft_only: usize,
    panics: Vec<(String, String)>,
    false_positives: Vec<(String, String)>,
    false_negatives: Vec<(String, String)>,
}

fn run(seed: u64, count: usize, noise: u64) -> Findings {
    let mut generator = Gen {
        rng: Rng(seed | 1),
        noise,
        vars: Vec::new(),
    };
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(Duration::from_millis(500));

    let mut findings = Findings::default();
    for _ in 0..count {
        let code = generator.program();
        findings.programs += 1;
        let verdict = check(&code);
        let runtime = engine.eval(&code, mq_lang::null_input().into_iter());

        match (&verdict, &runtime) {
            (Check::Panic(message), _) => findings.panics.push((code, message.clone())),
            (Check::Errors(errors), Ok(_)) => findings.false_positives.push((code, errors[0].clone())),
            (Check::Errors(_), Err(_)) => findings.agree_error += 1,
            (Check::Clean, Err(error)) if is_type_error(&error.to_string()) => findings
                .false_negatives
                .push((code, error.to_string().lines().next().unwrap_or("").to_string())),
            (Check::Clean, Ok(_)) => {
                findings.agree_ok += 1;
                findings.runtime_ok += 1;
            }
            (Check::Clean, Err(_)) => findings.soft_only += 1,
        }
    }
    findings
}

fn report(findings: &Findings) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    writeln!(
        out,
        "programs={} agree_ok={} agree_error={} other_runtime_errors={} false_positives={} false_negatives={} panics={}",
        findings.programs,
        findings.agree_ok,
        findings.agree_error,
        findings.soft_only,
        findings.false_positives.len(),
        findings.false_negatives.len(),
        findings.panics.len()
    )
    .unwrap();
    for (title, items) in [
        ("PANIC", &findings.panics),
        ("FALSE POSITIVE", &findings.false_positives),
        ("FALSE NEGATIVE", &findings.false_negatives),
    ] {
        for (code, detail) in items.iter().take(12) {
            writeln!(out, "--- {title}: {detail}\n{code}").unwrap();
        }
    }
    out
}

#[test]
fn the_checker_does_not_panic_on_random_programs() {
    let findings = run(0x5eed, 400, 15);
    assert!(findings.panics.is_empty(), "{}", report(&findings));
}

#[test]
#[ignore = "runs many programs and prints the findings; use `just fuzz-check`"]
fn random_programs_agree_between_checker_and_runtime() {
    let seed = std::env::var("FUZZ_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0x5eed);
    let count = std::env::var("FUZZ_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5000);
    let noise = std::env::var("FUZZ_NOISE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(15);
    let findings = run(seed, count, noise);
    eprintln!("{}", report(&findings));
    assert!(findings.panics.is_empty());
}
