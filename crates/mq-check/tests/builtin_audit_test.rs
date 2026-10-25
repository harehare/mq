//! Audit of the builtin signatures against the runtime.
//!
//! Every builtin is called with values of each type. A call that computes a value but is rejected
//! by the checker means the signature is narrower than the function: a false positive. A call that
//! gives back `None` or one of its arguments is how a builtin shrugs off a value it has no use for,
//! so the checker may reject it.
//!
//! The builtins written in mq (`builtin.mq`) are audited the same way. Their signatures are
//! inferred from the bodies, so a finding there is an inference narrower than the body.
//!
//! A builtin the audit covers and finds clean is trusted: a call to it can make an error definite.
//! The others are `UNAUDITED_BUILTINS` (`certainty.rs`), which this test keeps up to date.
//!
//! Slow, so it is ignored by default: `just audit-builtins`. `AUDIT_FUNCTION` limits it to one
//! function, `AUDIT_TRIPLES` sets how many 3-argument combinations are tried (default 150), and
//! `AUDIT_REPORT_ONLY` reports the findings without failing.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::RangeInclusive;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use mq_check::certainty::UNAUDITED_BUILTINS;
use mq_check::{TypeChecker, TypeError};
use mq_hir::Hir;
use mq_lang::RuntimeValue;

/// Builtins that do I/O, never return or depend on the environment: not safe to call.
const SKIPPED: &[&str] = &[
    "breakpoint",
    "close",
    "embed_images",
    "env",
    "error",
    "file_exists",
    "file_info",
    "file_size",
    "halt",
    "http",
    "http_all",
    "infinite",
    "input",
    "mkdir",
    "mock_fetch",
    "open_file",
    "open_http",
    "print",
    "read_bytes",
    "read_file",
    "read_file_bytes",
    "read_line",
    "send",
    "status",
    "stderr",
    "system",
    "walk_files",
    "write_file",
];

/// Words in the body of a function of `builtin.mq` that mean it does I/O or ends the program.
const EFFECTS: &[&str] = &[
    "print(",
    "stderr(",
    "input(",
    "read_",
    "open_",
    "http",
    "write_file",
    "halt",
    "system(",
    "walk_files",
    "breakpoint",
    "send(",
    "file_",
    "sleep",
];

/// One value of each type: its label in a finding, and its source.
const VALUES: &[(&str, &str)] = &[
    ("number", "2"),
    ("string", "\"ab\""),
    ("bool", "true"),
    ("none", "None"),
    ("[number]", "[1, 2]"),
    ("[string]", "[\"a\", \"b\"]"),
    ("dict", "{\"a\": 1}"),
    ("symbol", ":sym"),
    ("markdown", "first(to_markdown(\"# h\"))"),
    ("bytes", "to_bytes(\"a\")"),
    ("function", "fn(x): x;"),
    // Values that only some builtins take
    ("function", "fn(a, b): a;"),
    ("string", "\"2024-01-02\""),
    ("string", "\"%Y-%m-%d\""),
    ("string", "\"YQ==\""),
];

/// Valid calls of builtins that the values above do not reach: those that take a particular shape,
/// such as nested arrays or a format name.
const EVIDENCE: &[(&str, &str)] = &[
    ("transpose", "transpose([[1, 2], [3, 4]])"),
    ("combinations", "combinations([[1], [2]])"),
    ("percentile", "percentile([1, 2, 3], 0.5)"),
    ("normalize_newlines", "normalize_newlines(\"a\\r\\nb\", \"lf\")"),
    ("base64urld", "base64urld(\"YQ\")"),
    ("base64urld_bytes", "base64urld_bytes(\"YQ\")"),
    ("from_date", "from_date(\"2024-01-02T00:00:00Z\")"),
    ("strptime", "strptime(\"2024-01-02\", \"%Y-%m-%d\")"),
    ("convert", "convert(\"a\", :html)"),
    ("inspect", "inspect(1)"),
    ("next", "next(to_coroutine([1]))"),
];

/// Single calls that the runtime answers with a value it makes up, and the checker rejects on
/// purpose: the function and the argument types as a finding lists them.
const TOLERATED: &[(&str, &str)] = &[
    ("bsearch", "([number], markdown)"),
    ("bsearch", "([string], markdown)"),
    ("contains", "(none, markdown)"),
    ("implode", "([string])"),
    ("in", "(none, markdown)"),
    ("indices", "(none, markdown)"),
    ("insert", "(dict, string, markdown)"),
    ("len", "(bool)"),
    ("len", "(symbol)"),
    ("lines", "([number])"),
    ("lines", "([string])"),
    ("mul", "([number], [number])"),
    ("set", "([number], number, bool)"),
    ("sort_natural", "(string)"),
    ("unlines", "([number])"),
];

const ORDERING: &[&str] = &["gt", "gte", "lt", "lte"];
const ARITHMETIC: &[&str] = &["add", "sub", "mul", "div", "mod", "shift_left", "shift_right", "not"];
const NODE_SETTERS: &[&str] = &[
    "attr",
    "set_attr",
    "set_check",
    "set_children",
    "set_code_block_lang",
    "set_list_ordered",
    "set_ref",
];
/// Functions of text, which read anything else as an empty text
const TEXT: &[&str] = &["grapheme_len", "graphemes", "utf8bytelen"];
/// Functions of a string only
const STRING_ONLY: &[&str] = &["extract_images", "intern", "safe_filename"];

/// Whether the checker rejects the call on purpose, because the runtime reads a value it has no
/// use for as a default one. `labels` are the types of the arguments.
fn is_tolerated(name: &str, labels: &[&str]) -> bool {
    let has = |wanted: &[&str]| labels.iter().any(|label| wanted.contains(label));
    let lacks = |allowed: &[&str]| labels.iter().any(|label| !allowed.contains(label));
    let signature = format!("({})", labels.join(", "));

    // Values of different types are ordered by type, which is nearly always a mistake.
    if ORDERING.contains(&name) && labels.len() == 2 && labels[0] != labels[1] {
        return true;
    }
    // A function is a callback, never data.
    if has(&["function"]) {
        return true;
    }
    // Arithmetic reads a bool, none, dict, symbol, bytes or node as a number, or passes it through.
    if ARITHMETIC.contains(&name) && has(&["bool", "none", "dict", "symbol", "bytes", "markdown"]) {
        return true;
    }
    if NODE_SETTERS.contains(&name) || name == "token_compress" {
        return true;
    }
    if TEXT.contains(&name) {
        return lacks(&["string", "markdown", "none"]);
    }
    if STRING_ONLY.contains(&name) {
        return lacks(&["string"]);
    }
    // Shifting by an array is not meant, and neither is shifting with a single argument.
    if ["shift_left", "shift_right"].contains(&name) {
        return labels.iter().any(|label| label.starts_with('[')) || labels.len() == 1;
    }
    TOLERATED.contains(&(name, signature.as_str()))
}

/// A function of `builtin.mq`.
struct MqDef {
    name: String,
    params: usize,
    variadic: bool,
    /// The body without its comments
    body: String,
}

/// The public functions of `builtin.mq`.
fn mq_defs() -> Vec<MqDef> {
    let mut defs: Vec<MqDef> = Vec::new();
    for line in include_str!("../../mq-lang/builtin.mq").lines() {
        if let Some(rest) = line.strip_prefix("def ") {
            let Some((name, params)) = rest.split_once('(') else {
                continue;
            };
            if name.starts_with('_') {
                continue;
            }
            let params = params.split(')').next().unwrap_or("");
            defs.push(MqDef {
                name: name.to_string(),
                params: params.split(',').filter(|p| !p.trim().is_empty()).count(),
                variadic: params.contains('*'),
                body: String::new(),
            });
        } else if let Some(last) = defs.last_mut()
            && !line.trim_start().starts_with('#')
        {
            last.body.push_str(line);
            last.body.push('\n');
        }
    }
    defs
}

/// A builtin to call, and the numbers of arguments to call it with.
struct Target {
    name: String,
    arities: RangeInclusive<usize>,
}

/// The builtins that are safe to call. A function of `builtin.mq` is called with exactly its
/// parameters, since fewer is the piped form, which has no input here.
fn targets() -> Vec<Target> {
    let natives = mq_help::BUILTIN_DOC
        .iter()
        .filter(|doc| doc.kind == mq_help::DocKind::Function && !SKIPPED.contains(&doc.name))
        .map(|doc| Target {
            name: doc.name.to_string(),
            arities: if doc.params.is_empty() {
                0..=0
            } else {
                1..=doc.params.len().clamp(1, 3)
            },
        });
    let written_in_mq = mq_defs()
        .into_iter()
        .filter(|def| {
            !def.variadic
                && def.params <= 3
                && !SKIPPED.contains(&def.name.as_str())
                && !EFFECTS.iter().any(|effect| def.body.contains(effect))
        })
        .map(|def| Target {
            name: def.name,
            arities: def.params..=def.params,
        });
    natives.chain(written_in_mq).collect()
}

/// Every builtin, whether the audit can call it or not.
fn all_builtin_names() -> BTreeSet<String> {
    mq_help::BUILTIN_DOC
        .iter()
        .filter(|doc| doc.kind == mq_help::DocKind::Function)
        .map(|doc| doc.name.to_string())
        .chain(mq_defs().into_iter().map(|def| def.name))
        .collect()
}

/// The first error of the checker for `code` that is a type error, if any.
fn rejection(code: &str) -> Option<String> {
    let errors = catch_unwind(AssertUnwindSafe(|| {
        let mut hir = Hir::default();
        hir.add_code(None, code);
        TypeChecker::new().check(&hir)
    }))
    .ok()?;
    errors
        .iter()
        .find(|error| !error.is_advisory())
        .map(TypeError::to_string)
}

/// The argument lists of `arity` values, and for 3 a spread over them.
fn argument_lists(arity: usize, triples: usize) -> Vec<Vec<usize>> {
    let mut lists: Vec<Vec<usize>> = vec![Vec::new()];
    for _ in 0..arity {
        lists = lists
            .into_iter()
            .flat_map(|prefix| {
                (0..VALUES.len()).map(move |value| {
                    let mut next = prefix.clone();
                    next.push(value);
                    next
                })
            })
            .collect();
    }
    if arity == 3 {
        let step = (lists.len() / triples.max(1)).max(1);
        lists = lists.into_iter().step_by(step).collect();
    }
    lists
}

#[derive(Default)]
struct Audit {
    calls: usize,
    /// The findings of each builtin, one line each
    findings: BTreeMap<String, Vec<String>>,
    /// How many calls of each builtin ran and were accepted
    accepted: BTreeMap<String, usize>,
}

impl Audit {
    fn call(
        &mut self,
        engine: &mut mq_lang::DefaultEngine,
        values: &[Option<RuntimeValue>],
        name: &str,
        list: &[usize],
    ) {
        let sources: Vec<&str> = list.iter().map(|&value| VALUES[value].1).collect();
        let code = format!("{name}({})", sources.join(", "));
        self.calls += 1;
        let Ok(result) = engine.eval(&code, mq_lang::null_input().into_iter()) else {
            return;
        };
        let rejection = rejection(&code);
        if rejection.is_none() {
            *self.accepted.entry(name.to_string()).or_default() += 1;
        }
        let computed = result.into_iter().next().is_some_and(|result| {
            !result.is_none()
                && !list
                    .iter()
                    .any(|&value| values[value].as_ref().is_some_and(|argument| *argument == result))
        });
        let (true, Some(rejection)) = (computed, rejection) else {
            return;
        };
        let labels: Vec<&str> = list.iter().map(|&value| VALUES[value].0).collect();
        if !is_tolerated(name, &labels) {
            self.findings.entry(name.to_string()).or_default().push(format!(
                "({})  {}",
                labels.join(", "),
                rejection.lines().next().unwrap_or("")
            ));
        }
    }

    /// Counts a valid call of a builtin that the values do not reach.
    fn evidence(&mut self, engine: &mut mq_lang::DefaultEngine) {
        for (name, code) in EVIDENCE {
            if engine.eval(code, mq_lang::null_input().into_iter()).is_ok() && rejection(code).is_none() {
                *self.accepted.entry((*name).to_string()).or_default() += 1;
            }
        }
    }

    fn finding_count(&self) -> usize {
        self.findings.values().map(Vec::len).sum()
    }

    fn report(&self, lines_per_builtin: usize) -> String {
        let mut report = format!(
            "calls={} functions_with_findings={} findings={}\n",
            self.calls,
            self.findings.len(),
            self.finding_count()
        );
        for (name, list) in &self.findings {
            report.push_str(&format!("{name}: {}\n", list.len()));
            for line in list.iter().take(lines_per_builtin) {
                report.push_str(&format!("    {line}\n"));
            }
        }
        report
    }

    /// The builtins the audit does not vouch for: those it could not call, that have a finding, or
    /// that never ran with an accepted call.
    fn unaudited(&self, called: &[Target]) -> Vec<String> {
        let audited: BTreeSet<&str> = called
            .iter()
            .map(|target| target.name.as_str())
            .filter(|name| !self.findings.contains_key(*name) && self.accepted.contains_key(*name))
            .collect();
        all_builtin_names()
            .into_iter()
            .filter(|name| !audited.contains(name.as_str()))
            .collect()
    }
}

#[test]
#[ignore = "calls every builtin with every argument type; run with `just audit-builtins`"]
fn builtins_accept_every_argument_that_the_runtime_accepts() {
    let only = std::env::var("AUDIT_FUNCTION").ok();
    let triples = std::env::var("AUDIT_TRIPLES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(150);

    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(Duration::from_millis(300));
    let values: Vec<Option<RuntimeValue>> = VALUES
        .iter()
        .map(|(_, source)| {
            engine
                .eval(source, mq_lang::null_input().into_iter())
                .ok()
                .and_then(|result| result.into_iter().next())
        })
        .collect();

    let mut called = targets();
    called.retain(|target| only.as_deref().is_none_or(|only| only == target.name));

    let mut audit = Audit::default();
    audit.evidence(&mut engine);
    for target in &called {
        for arity in target.arities.clone() {
            for list in argument_lists(arity, triples) {
                audit.call(&mut engine, &values, &target.name, &list);
            }
        }
    }

    let all = only.is_some() || std::env::var_os("AUDIT_ALL").is_some();
    let report = audit.report(if all { 400 } else { 6 });
    println!("{report}");

    if only.is_none() {
        let unaudited = audit.unaudited(&called);
        println!("unaudited ({}): {}", unaudited.len(), unaudited.join(" "));
        assert_eq!(
            unaudited, UNAUDITED_BUILTINS,
            "UNAUDITED_BUILTINS (certainty.rs) must list the builtins the audit does not cover"
        );
    }
    assert!(
        audit.finding_count() == 0 || std::env::var_os("AUDIT_REPORT_ONLY").is_some(),
        "{report}"
    );
}
