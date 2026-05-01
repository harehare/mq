//! Module introspection for the `mq modules` subcommand.
//!
//! Enumerates built-in standard modules, extracts public function signatures
//! from the AST, and associates doc comments via the CST.

use rustc_hash::FxHashMap;

/// Information about a single parameter of a module function.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamInfo {
    /// Parameter name.
    pub name: String,
    /// Rendered default value, if any (e.g. `"false"`, `"0"`, `":symbol"`).
    pub default: Option<String>,
    /// Whether the parameter is variadic (`*name`).
    pub is_variadic: bool,
}

/// Information about a public function exported by a standard module.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionInfo {
    /// Function name.
    pub name: String,
    /// Ordered list of parameters.
    pub params: Vec<ParamInfo>,
    /// Doc comment lines joined by `\n`, extracted from the module source.
    pub doc: Option<String>,
}

fn render_default(node: &mq_lang::AstNode) -> String {
    node.to_code()
}

fn extract_doc_comments(source: &str) -> FxHashMap<String, String> {
    let (nodes, _) = mq_lang::parse_recovery(source);

    nodes
        .iter()
        .filter(|node| node.is_def())
        .filter_map(|node| {
            let name = node
                .children
                .first()
                .and_then(|child| child.token.as_ref())
                .and_then(|token| match &token.kind {
                    mq_lang::TokenKind::Ident(s) => Some(s.to_string()),
                    _ => None,
                })?;

            // Find the position just after the last blank-line boundary.
            // A blank line = two consecutive NewLine trivia (ignoring whitespace between them).
            let trivia = &node.leading_trivia;
            let start = {
                let mut last_blank_end = 0;
                let mut i = 0;
                while i < trivia.len() {
                    if matches!(&trivia[i], mq_lang::CstTrivia::NewLine) {
                        let mut j = i + 1;
                        while j < trivia.len()
                            && matches!(
                                &trivia[j],
                                mq_lang::CstTrivia::Whitespace(_) | mq_lang::CstTrivia::Tab(_)
                            )
                        {
                            j += 1;
                        }
                        if j < trivia.len() && matches!(&trivia[j], mq_lang::CstTrivia::NewLine) {
                            last_blank_end = j + 1;
                        }
                    }
                    i += 1;
                }
                last_blank_end
            };

            let comments: Vec<String> = trivia[start..]
                .iter()
                .filter_map(|t| t.comment())
                .map(|s| s.trim_start().to_string())
                .collect();

            if comments.is_empty() {
                None
            } else {
                Some((name, comments.join("\n")))
            }
        })
        .collect()
}

/// Returns information about all public functions of each standard module, sorted by module name.
///
/// Each entry is `(module_name, vec_of_function_info)`.
/// Private functions (names starting with `_`) are excluded.
pub fn standard_module_functions() -> Vec<(String, Vec<FunctionInfo>)> {
    let mut result: Vec<(String, Vec<FunctionInfo>)> = mq_lang::STANDARD_MODULES
        .iter()
        .filter_map(|(mod_name, content_fn)| {
            let content = content_fn();
            let doc_map = extract_doc_comments(content);
            let module = mq_lang::load_standard_module(mod_name.as_str())?;

            let functions: Vec<FunctionInfo> = module
                .functions
                .iter()
                .filter_map(|node| {
                    if let mq_lang::AstExpr::Def(ident, params, _) = &*node.expr {
                        let fname = ident.name.to_string();
                        if fname.starts_with('_') {
                            return None;
                        }
                        let param_infos = params
                            .iter()
                            .map(|p| ParamInfo {
                                name: p.ident.name.to_string(),
                                default: p.default.as_ref().map(|d| render_default(d)),
                                is_variadic: p.is_variadic,
                            })
                            .collect();
                        let doc = doc_map.get(&fname).cloned();
                        Some(FunctionInfo {
                            name: fname,
                            params: param_infos,
                            doc,
                        })
                    } else {
                        None
                    }
                })
                .collect();

            Some((mod_name.to_string(), functions))
        })
        .collect();

    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
