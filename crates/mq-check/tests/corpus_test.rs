//! Runs the type checker over every `.mq` file in the repository and pins the diagnostic counts.
//!
//! The snapshot `tests/corpus.snap` must match exactly: a regression fails the test, and an
//! improvement requires regenerating it with `UPDATE_CORPUS=1`.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use mq_check::{TypeChecker, types::Type};
use mq_hir::{Hir, HirError};
use url::Url;

const SOURCE_DIRS: &[&str] = &[
    "scripts",
    "crates/mq-lang",
    "crates/mq-lang/modules",
    "crates/mq-lang/benches",
];

/// Their resolution depends on the `html` feature of mq-lang, which differs between feature sets.
const FEATURE_DEPENDENT: &[&str] = &["html.mq", "html_test.mq"];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn corpus_files(root: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = SOURCE_DIRS
        .iter()
        .filter_map(|dir| std::fs::read_dir(root.join(dir)).ok())
        .flatten()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "mq"))
        .filter(|path| {
            !FEATURE_DEPENDENT
                .iter()
                .any(|name| path.file_name().is_some_and(|n| n == *name))
        })
        .collect();
    files.sort();
    files
}

fn contains_dynamic(ty: &Type) -> bool {
    match ty {
        Type::Dynamic => true,
        Type::Array(inner) => contains_dynamic(inner),
        Type::Tuple(items) | Type::Union(items) => items.iter().any(contains_dynamic),
        Type::Dict(key, value) => contains_dynamic(key) || contains_dynamic(value),
        Type::Function(params, ret) => params.iter().any(contains_dynamic) || contains_dynamic(ret),
        Type::Record(fields, rest) => fields.values().any(contains_dynamic) || contains_dynamic(rest),
        _ => false,
    }
}

#[derive(Default)]
struct FileReport {
    syntax: usize,
    type_errors: Vec<String>,
    unresolved: Vec<String>,
    dynamic_symbols: usize,
}

fn check_file(path: &Path) -> FileReport {
    let code = std::fs::read_to_string(path).unwrap();
    let mut hir = Hir::default();
    // builtin.mq defines the builtins, so it is checked on its own.
    hir.builtin.disabled = path.file_name().is_some_and(|n| n == "builtin.mq");
    for name in mq_check::TEST_RUNNER_GLOBALS {
        hir.declare_global(name);
    }
    let (source_id, _) = hir.add_code(Url::from_file_path(path).ok(), &code);

    let mut report = FileReport::default();
    for error in hir.errors() {
        match &error {
            HirError::UnresolvedSymbol { symbol, .. } if symbol.source.source_id == Some(source_id) => {
                let name = symbol.value.as_deref().unwrap_or_default();
                report.unresolved.push(name.to_string());
            }
            HirError::ModuleNotFound { symbol, .. } | HirError::YieldOutsideFunction { symbol }
                if symbol.source.source_id == Some(source_id) =>
            {
                report.syntax += 1;
            }
            _ => {}
        }
    }

    let mut checker = TypeChecker::new();
    report.type_errors = checker
        .check(&hir)
        .iter()
        .map(|e| e.code().map(|c| c.to_string()).unwrap_or_default())
        .collect();
    report.type_errors.sort();

    report.dynamic_symbols = checker
        .symbol_types()
        .into_iter()
        .filter(|(id, _)| hir.symbol(**id).is_some_and(|s| !hir.is_builtin_symbol(s)))
        .filter(|(_, scheme)| contains_dynamic(&scheme.ty))
        .count();
    report
}

fn render(root: &Path) -> String {
    let mut out = String::new();
    let (mut type_total, mut unresolved_total, mut dynamic_total) = (0, 0, 0);
    for path in corpus_files(root) {
        let report = check_file(&path);
        let rel = path.strip_prefix(root).unwrap().display();
        let mut unresolved = report.unresolved.clone();
        unresolved.sort();
        unresolved.dedup();
        writeln!(
            out,
            "{rel}: syntax={} type={} unresolved={} dynamic={}",
            report.syntax,
            report.type_errors.len(),
            report.unresolved.len(),
            report.dynamic_symbols,
        )
        .unwrap();
        for code in &report.type_errors {
            writeln!(out, "  {code}").unwrap();
        }
        if !unresolved.is_empty() {
            writeln!(out, "  unresolved: {}", unresolved.join(", ")).unwrap();
        }
        type_total += report.type_errors.len();
        unresolved_total += report.unresolved.len();
        dynamic_total += report.dynamic_symbols;
    }
    writeln!(
        out,
        "TOTAL: type={type_total} unresolved={unresolved_total} dynamic={dynamic_total}"
    )
    .unwrap();
    out
}

#[test]
fn corpus_diagnostics_match_snapshot() {
    let root = workspace_root();
    let actual = render(&root);
    let snap = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus.snap");

    if std::env::var_os("UPDATE_CORPUS").is_some() {
        std::fs::write(&snap, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&snap).unwrap_or_default();
    if actual != expected {
        let changed: Vec<String> = expected
            .lines()
            .filter(|line| !actual.lines().any(|a| a == *line))
            .map(|line| format!("- {line}"))
            .chain(
                actual
                    .lines()
                    .filter(|line| !expected.lines().any(|e| e == *line))
                    .map(|line| format!("+ {line}")),
            )
            .collect();
        panic!(
            "corpus diagnostics changed; if intended, regenerate with UPDATE_CORPUS=1\n{}",
            changed.join("\n")
        );
    }
}
