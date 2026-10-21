//! Differential tests between the AST parser and the error-recovering CST parser.
#![cfg(feature = "cst")]

use std::path::{Path, PathBuf};

use mq_lang::{Arena, Shared, SharedCell};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;

fn ast_accepts(code: &str) -> bool {
    mq_lang::parse(code, Shared::new(SharedCell::new(Arena::new(1024)))).is_ok()
}

fn cst_accepts(code: &str) -> bool {
    !mq_lang::parse_recovery(code).1.has_errors()
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, ext, out);
        } else if path.extension().is_some_and(|e| e == ext) {
            out.push(path);
        }
    }
}

/// Source files and ```` ```mq ```` code blocks from the docs.
fn corpus() -> Vec<(String, String)> {
    let root = crate_dir().join("../..");
    let mut files = Vec::new();
    collect(&crate_dir(), "mq", &mut files);
    collect(&root.join("scripts"), "mq", &mut files);

    let mut corpus: Vec<_> = files
        .into_iter()
        .filter_map(|p| Some((p.display().to_string(), std::fs::read_to_string(&p).ok()?)))
        .collect();

    let mut docs = Vec::new();
    collect(&root.join("docs/books/src"), "md", &mut docs);
    for path in docs {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut block: Option<String> = None;
        for (i, line) in text.lines().enumerate() {
            match &mut block {
                None if line.trim_start().starts_with("```mq") => block = Some(String::new()),
                Some(code) if line.trim_start().starts_with("```") => {
                    corpus.push((format!("{}:{}", path.display(), i + 1), std::mem::take(code)));
                    block = None;
                }
                Some(code) => {
                    code.push_str(line);
                    code.push('\n');
                }
                None => {}
            }
        }
    }
    corpus
}

/// Inputs the AST parser rejects.
const INVALID: &[&str] = &[
    "1 7",
    "[1] 7",
    "x[0] 7",
    "self 7",
    "let x = 1 let y = 2",
    "(",
    "foo(",
    "1 +",
    "1 | ",
    "[1, 2",
    "{\"a\": 1",
    "def f(x):",
    "let = 1",
    "let x 1",
    "if (true)",
    "if true: 1",
    "foreach (x): x;",
    "while (true)",
    "\"unterminated",
    "fn(x: x",
    "x |",
    "| x",
];

#[test]
fn corpus_acceptance_agrees() {
    let mismatches: Vec<_> = corpus()
        .into_iter()
        .filter_map(|(name, code)| {
            let (ast, cst) = (ast_accepts(&code), cst_accepts(&code));
            (ast != cst).then(|| format!("{name}: ast={ast} cst={cst}"))
        })
        .collect();

    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn invalid_inputs_are_rejected_by_both() {
    let mismatches: Vec<_> = INVALID
        .iter()
        .filter(|code| ast_accepts(code) || cst_accepts(code))
        .map(|code| format!("{code:?}: ast={} cst={}", ast_accepts(code), cst_accepts(code)))
        .collect();

    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

proptest! {
    // The seed is fixed so CI is deterministic. Change it to explore other mutations.
    #![proptest_config(ProptestConfig {
        cases: 256,
        rng_seed: RngSeed::Fixed(0x6d71),
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    /// Cutting or damaging a valid program must never make the CST reject something the AST accepts.
    #[test]
    fn mutated_corpus_cst_accepts_whenever_ast_does(idx in 0usize..10_000, cut in 0.0f64..1.0, drop_at in 0.0f64..1.0, truncate in any::<bool>()) {
        let corpus = small_corpus();
        let code = &corpus[idx % corpus.len()];
        let chars: Vec<char> = code.chars().collect();
        let at = ((chars.len() as f64) * cut) as usize;
        let mutated: String = if truncate {
            chars[..at].iter().collect()
        } else {
            let hole = ((chars.len() as f64) * drop_at) as usize;
            chars.iter().enumerate().filter(|(i, _)| *i != hole).map(|(_, c)| c).collect()
        };

        if ast_accepts(&mutated) {
            prop_assert!(cst_accepts(&mutated), "CST rejects AST-valid input:\n{mutated}");
        }
    }
}

/// Per-definition snippets: whole files are too slow to re-parse thousands of times.
fn small_corpus() -> &'static [String] {
    static CORPUS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(|| {
        let mut out: Vec<String> = corpus()
            .into_iter()
            .flat_map(|(_, code)| code.split("\n\n").map(str::to_string).collect::<Vec<_>>())
            .filter(|c| !c.trim().is_empty() && c.len() < 400 && ast_accepts(c))
            .collect();
        out.sort();
        out.dedup();
        out
    })
}

/// Projection of the declarations in a program, in source order: `(kind, name)` pairs.
#[cfg(feature = "ast-json")]
mod declarations {
    use super::*;
    use mq_lang::{CstNode, CstNodeKind, TokenKind};
    use serde_json::Value;

    pub type Decls = Vec<(&'static str, String)>;

    fn name_of(v: &Value) -> String {
        v["name"].as_str().unwrap_or_default().to_string()
    }

    fn ast_pattern(v: &Value, out: &mut Decls, kind: &'static str) {
        match v {
            Value::Object(m) => {
                for (k, val) in m {
                    if k == "Ident" {
                        out.push((kind, name_of(val)));
                    } else {
                        ast_pattern(val, out, kind);
                    }
                }
            }
            Value::Array(a) => a.iter().for_each(|x| ast_pattern(x, out, kind)),
            _ => {}
        }
    }

    fn ast_params(params: &Value, out: &mut Decls) {
        for p in params.as_array().into_iter().flatten() {
            let name = name_of(&p["ident"]);
            // The AST parser adds this parameter to lambdas that declare none.
            if name != mq_lang::IMPLICIT_FN_ARG {
                out.push(("param", name));
            }
        }
    }

    fn ast_walk(v: &Value, out: &mut Decls) {
        match v {
            Value::Object(m) => {
                for (k, val) in m {
                    match k.as_str() {
                        "Def" => {
                            out.push(("def", name_of(&val[0])));
                            ast_params(&val[1], out);
                        }
                        "Fn" => ast_params(&val[0], out),
                        "Module" => out.push(("module", name_of(&val[0]))),
                        "Foreach" => out.push(("foreach", name_of(&val[0]))),
                        "Let" => ast_pattern(&val[0], out, "let"),
                        "Var" => ast_pattern(&val[0], out, "var"),
                        _ => {}
                    }
                    ast_walk(val, out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| ast_walk(x, out)),
            _ => {}
        }
    }

    pub fn ast(code: &str) -> Option<Decls> {
        let program = mq_lang::parse(code, Shared::new(SharedCell::new(Arena::new(1024)))).ok()?;
        let json: Value = serde_json::from_str(&mq_lang::ast_to_json(&program).ok()?).ok()?;
        let mut out = Vec::new();
        ast_walk(&json, &mut out);
        Some(out)
    }

    fn cst_pattern(node: &CstNode, out: &mut Decls, kind: &'static str) {
        if matches!(node.kind, CstNodeKind::Pattern { .. } | CstNodeKind::Ident { .. })
            && let Some(token) = &node.token
            && let TokenKind::Ident(name) = &token.kind
        {
            out.push((kind, name.to_string()));
        }
        node.children().for_each(|c| cst_pattern(c, out, kind));
    }

    fn cst_params(params: &[Shared<CstNode>], out: &mut Decls) {
        for p in params {
            if matches!(p.kind, CstNodeKind::Param { .. }) {
                out.push(("param", p.name().unwrap_or_default().to_string()));
            }
        }
    }

    fn cst_walk(node: &CstNode, out: &mut Decls) {
        match &node.kind {
            CstNodeKind::Def { name, params, .. } => {
                out.push(("def", name.name().unwrap_or_default().to_string()));
                cst_params(params.as_deref().unwrap_or_default(), out);
            }
            CstNodeKind::Fn { params, .. } => cst_params(params, out),
            CstNodeKind::Module { name, .. } => out.push(("module", name.name().unwrap_or_default().to_string())),
            CstNodeKind::Foreach { args, .. } => {
                if let Some(binder) = args.iter().find(|a| matches!(a.kind, CstNodeKind::Ident { .. })) {
                    out.push(("foreach", binder.name().unwrap_or_default().to_string()));
                }
            }
            CstNodeKind::Let { lhs, .. } => cst_pattern(lhs, out, "let"),
            CstNodeKind::Var { lhs, .. } => cst_pattern(lhs, out, "var"),
            _ => {}
        }
        node.children().for_each(|c| cst_walk(c, out));
    }

    pub fn cst(code: &str) -> Decls {
        let (nodes, _) = mq_lang::parse_recovery(code);
        let mut out = Vec::new();
        nodes.iter().for_each(|n| cst_walk(n, &mut out));
        out
    }
}

#[cfg(feature = "ast-json")]
#[test]
fn corpus_declarations_agree() {
    let mismatches: Vec<_> = corpus()
        .into_iter()
        .filter_map(|(name, code)| {
            let ast = declarations::ast(&code)?;
            let cst = declarations::cst(&code);
            (ast != cst).then(|| {
                let at = ast
                    .iter()
                    .zip(&cst)
                    .position(|(a, c)| a != c)
                    .unwrap_or(ast.len().min(cst.len()));
                format!(
                    "{name}: first difference at #{at}: ast={:?} cst={:?}",
                    ast.get(at),
                    cst.get(at)
                )
            })
        })
        .collect();

    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}
