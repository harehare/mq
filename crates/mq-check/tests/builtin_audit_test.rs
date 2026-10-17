//! Audit of the builtin signatures against the runtime.
//!
//! Every native builtin is called with values of each type. A call that computes a value but is
//! rejected by the checker means the signature is narrower than the function (a false positive).
//! A call that gives back `None` or one of its arguments is how a builtin shrugs off a value it
//! has no use for, so the checker may reject it.
//!
//! The builtins written in mq (`builtin.mq`) are audited the same way: their signatures are
//! inferred from the bodies, so a finding there is an inference that is narrower than the body.
//!
//! The builtins without a finding are the ones whose calls the checker may call definite errors;
//! they are pinned in `src/exact_builtins.txt` and regenerated with `UPDATE_EXACT=1`.
//!
//! Slow, so it is ignored by default: `just audit-builtins`. `AUDIT_FUNCTION` limits it to one
//! function and `AUDIT_TRIPLES` sets how many 3-argument combinations are tried (default 60).

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use mq_check::TypeChecker;
use mq_hir::Hir;

/// Builtins that do I/O, never return or depend on the environment.
const SKIPPED: &[&str] = &[
    // Ordering values of different types is defined at runtime but is nearly always a mistake.
    "gt",
    "gte",
    "lt",
    "lte",
    "breakpoint",
    "close",
    "env",
    "error",
    "halt",
    "http",
    "http_all",
    "infinite",
    "input",
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
    "file_exists",
    "file_info",
    "file_size",
    "embed_images",
];

/// Calls that the runtime answers by coercing a value it has no use for, which the checker rejects
/// on purpose: the function and the argument types, as they are listed in a finding.
const TOLERATED: &[(&str, &str)] = &[
    ("len", "(bool)"),
    ("len", "(symbol)"),
    ("len", "(function)"),
    ("flatten", "(function)"),
    ("max", "(none, function)"),
    ("max", "(function, none)"),
    ("contains", "(none, markdown)"),
    ("contains", "(none, function)"),
    ("contains", "([number], function)"),
    ("contains", "([string], function)"),
    ("index", "([number], function)"),
    ("index", "([string], function)"),
];

/// One value of each type: label and source.
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
];

/// Words in a `builtin.mq` body that mean the function does I/O or ends the program.
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

/// The public functions of `builtin.mq` that are safe to call, with their number of parameters.
fn mq_builtins() -> Vec<(String, usize)> {
    let source = include_str!("../../mq-lang/builtin.mq");
    let mut defs: Vec<(String, usize, String)> = Vec::new();
    for line in source.lines() {
        if let Some(rest) = line.strip_prefix("def ") {
            let Some((name, params)) = rest.split_once('(') else {
                continue;
            };
            let params = params.split(')').next().unwrap_or("");
            let arity = params.split(',').filter(|p| !p.trim().is_empty()).count();
            let variadic = params.contains('*');
            if name.starts_with('_') || variadic || arity == 0 {
                continue;
            }
            defs.push((name.to_string(), arity, String::new()));
        } else if let Some(last) = defs.last_mut() {
            last.2.push_str(line);
            last.2.push('\n');
        }
    }
    defs.into_iter()
        .filter(|(name, _, body)| !SKIPPED.contains(&name.as_str()) && !EFFECTS.iter().any(|e| body.contains(e)))
        .map(|(name, arity, _)| (name, arity))
        .collect()
}

fn rejected(code: &str) -> Option<String> {
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut hir = Hir::default();
        hir.add_code(None, code);
        TypeChecker::new().check(&hir)
    }));
    let errors = result.ok()?;
    errors
        .iter()
        .find(|e| {
            !matches!(
                e,
                mq_check::TypeError::NullablePropagation { .. }
                    | mq_check::TypeError::UnreachableCode { .. }
                    | mq_check::TypeError::NonExhaustiveMatch { .. }
            )
        })
        .map(|e| e.to_string())
}

#[test]
#[ignore = "calls every builtin with every argument type; run with `just audit-builtins`"]
fn builtins_accept_every_argument_that_the_runtime_accepts() {
    let only = std::env::var("AUDIT_FUNCTION").ok();
    let triples: usize = std::env::var("AUDIT_TRIPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(Duration::from_millis(300));

    let arguments: Vec<_> = VALUES
        .iter()
        .map(|(_, source)| {
            engine
                .eval(source, mq_lang::null_input().into_iter())
                .ok()
                .and_then(|values| values.into_iter().next())
        })
        .collect();

    let mut findings: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // Calls that run and are accepted, per builtin
    let mut accepted: BTreeMap<String, usize> = BTreeMap::new();
    let mut tried = 0usize;
    // Name and the numbers of arguments to try
    let mut targets: Vec<(String, std::ops::RangeInclusive<usize>)> = mq_help::BUILTIN_DOC
        .iter()
        .filter(|doc| doc.kind == mq_help::DocKind::Function)
        .filter(|doc| !SKIPPED.contains(&doc.name))
        .map(|doc| (doc.name.to_string(), 1..=doc.params.len().clamp(1, 3)))
        .collect();
    // A call with fewer arguments than parameters is the piped form, which has no input here.
    targets.extend(
        mq_builtins()
            .into_iter()
            .filter(|(_, params)| *params <= 3)
            .map(|(name, params)| (name, params..=params)),
    );
    targets.retain(|(name, _)| only.as_deref().is_none_or(|only| only == name));

    for (name, arities) in &targets {
        for arity in arities.clone() {
            let mut combos: Vec<Vec<usize>> = vec![Vec::new()];
            for _ in 0..arity {
                combos = combos
                    .into_iter()
                    .flat_map(|prefix| {
                        (0..VALUES.len()).map(move |i| {
                            let mut next = prefix.clone();
                            next.push(i);
                            next
                        })
                    })
                    .collect();
            }
            if arity == 3 {
                // A deterministic spread over the combinations
                let step = (combos.len() / triples.max(1)).max(1);
                combos = combos.into_iter().step_by(step).collect();
            }
            for combo in combos {
                let args: Vec<&str> = combo.iter().map(|&i| VALUES[i].1).collect();
                let code = format!("{}({})", name, args.join(", "));
                tried += 1;
                let Ok(values) = engine.eval(&code, mq_lang::null_input().into_iter()) else {
                    continue;
                };
                // A call that runs and is accepted shows the signature takes real uses.
                if rejected(&code).is_none() {
                    *accepted.entry(name.clone()).or_default() += 1;
                }
                let computed = values.into_iter().next().is_some_and(|result| {
                    !result.is_none()
                        && !combo
                            .iter()
                            .any(|&i| arguments[i].as_ref().is_some_and(|argument| *argument == result))
                });
                if !computed {
                    continue;
                }
                if let Some(error) = rejected(&code) {
                    let labels: Vec<&str> = combo.iter().map(|&i| VALUES[i].0).collect();
                    let signature = format!("({})", labels.join(", "));
                    if TOLERATED.contains(&(name.as_str(), signature.as_str())) {
                        continue;
                    }
                    findings.entry(name.clone()).or_default().push(format!(
                        "({})  {}",
                        labels.join(", "),
                        error.lines().next().unwrap_or("")
                    ));
                }
            }
        }
    }

    let total: usize = findings.values().map(Vec::len).sum();
    let mut report = format!(
        "calls={tried} functions_with_findings={} findings={total}\n",
        findings.len()
    );
    for (name, list) in &findings {
        report.push_str(&format!("{name}: {}\n", list.len()));
        for line in list.iter().take(if only.is_some() { 200 } else { 6 }) {
            report.push_str(&format!("    {line}\n"));
        }
    }
    println!("{report}");

    // The builtins whose signature is not narrower than the function on any call tried, and that
    // accept a call that runs (so the calls tried are not all wrong), have a type that is known
    // for certain: see `exact_builtins.txt`.
    if only.is_none() {
        let exact: std::collections::BTreeSet<&str> = targets
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| !findings.contains_key(*name) && accepted.contains_key(*name))
            .collect();
        let listed: String = exact.iter().map(|name| format!("{name}\n")).collect();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/exact_builtins.txt");
        if std::env::var_os("UPDATE_EXACT").is_some() {
            std::fs::write(path, &listed).unwrap();
        } else {
            let current = std::fs::read_to_string(path).unwrap_or_default();
            assert_eq!(
                current, listed,
                "src/exact_builtins.txt is out of date; regenerate it with UPDATE_EXACT=1"
            );
        }
    }

    assert!(
        total == 0 || std::env::var_os("AUDIT_REPORT_ONLY").is_some(),
        "{report}"
    );
}
