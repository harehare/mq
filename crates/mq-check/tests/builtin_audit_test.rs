//! Audit of the builtin signatures against the runtime.
//!
//! Every native builtin is called with values of each type. A call that computes a value but is
//! rejected by the checker means the signature is narrower than the function (a false positive).
//! A call that gives back `None` or one of its arguments is how a builtin shrugs off a value it
//! has no use for, so the checker may reject it.
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
    let mut tried = 0usize;
    for doc in mq_help::BUILTIN_DOC
        .iter()
        .filter(|doc| doc.kind == mq_help::DocKind::Function)
        .filter(|doc| !SKIPPED.contains(&doc.name))
        .filter(|doc| only.as_deref().is_none_or(|name| name == doc.name))
    {
        let max_arity = doc.params.len().clamp(1, 3);
        for arity in 1..=max_arity {
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
                let code = format!("{}({})", doc.name, args.join(", "));
                tried += 1;
                let Ok(values) = engine.eval(&code, mq_lang::null_input().into_iter()) else {
                    continue;
                };
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
                    findings.entry(doc.name.to_string()).or_default().push(format!(
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
    assert!(
        total == 0 || std::env::var_os("AUDIT_REPORT_ONLY").is_some(),
        "{report}"
    );
}
