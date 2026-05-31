use mq_lang::{
    BUILTIN_FUNCTION_DOC, BUILTIN_MODULE_FILE, BUILTIN_SELECTOR_DOC, CstNode, CstNodeKind, STANDARD_MODULES, Shared,
    TokenKind,
};

/// Generates a Markdown reference document covering:
/// - Native built-in functions (from `BUILTIN_FUNCTION_DOC`)
/// - Selectors (from `BUILTIN_SELECTOR_DOC`)
/// - The `builtin` standard library (`builtin.mq` user-facing functions)
/// - Each importable standard module
///
/// Function signatures and doc comments are extracted from the CST produced by
/// `mq_lang::parse_recovery`, so no hand-written text scanning is involved.
///
/// The returned string can be fed into `mq_lang::parse_markdown_input` so that
/// the caller can query it with any mq expression.
pub fn generate() -> String {
    let mut md = String::with_capacity(64 * 1024);

    append_native_functions(&mut md);
    append_selectors(&mut md);
    append_module_section(&mut md, "builtin", BUILTIN_MODULE_FILE, true);

    let mut modules: Vec<_> = STANDARD_MODULES.iter().collect();
    modules.sort_by_key(|(k, _)| *k);
    for (name, get_source) in modules {
        let source = get_source();
        append_module_section(&mut md, name, source, false);
    }

    md
}

fn append_native_functions(md: &mut String) {
    md.push_str("# Built-in Functions\n\n");
    md.push_str("| name | params | description |\n");
    md.push_str("|------|--------|-------------|\n");

    let mut entries: Vec<_> = BUILTIN_FUNCTION_DOC.iter().collect();
    entries.sort_by_key(|(k, _)| *k);

    for (name, doc) in entries {
        if name.starts_with('_') {
            continue;
        }
        let params = doc.params.join(", ");
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            cell(name),
            cell(&params),
            cell(doc.description),
        ));
    }
    md.push('\n');
}

fn append_selectors(md: &mut String) {
    md.push_str("# Selectors\n\n");
    md.push_str("| selector | description |\n");
    md.push_str("|----------|-------------|\n");

    let mut entries: Vec<_> = BUILTIN_SELECTOR_DOC.iter().collect();
    entries.sort_by_key(|(k, _)| *k);

    for (name, doc) in entries {
        md.push_str(&format!("| {} | {} |\n", cell(name), cell(doc.description)));
    }
    md.push('\n');
}

fn append_module_section(md: &mut String, name: &str, source: &str, is_builtin: bool) {
    if is_builtin {
        md.push_str("# Module: builtin\n\n");
        md.push_str("The standard library, always available without import.\n\n");
    } else {
        md.push_str(&format!("\n# Module: {name}\n\n"));
        md.push_str(&format!("import \"{name}\" |\n\n"));
    }

    let fns = extract_functions_from_cst(source, is_builtin);
    if fns.is_empty() {
        return;
    }
    md.push_str("| name | params | description |\n");
    md.push_str("|------|--------|-------------|\n");
    for (fn_name, params, desc) in &fns {
        md.push_str(&format!(
            "| {} | {} | {} |\n",
            cell(fn_name),
            cell(&params.join(", ")),
            cell(desc),
        ));
    }
}

/// Parses `.mq` source with the CST and returns `(name, params, description)`
/// for each public function.
///
/// When `skip_native` is true (used for `builtin.mq`), functions that appear
/// in `BUILTIN_FUNCTION_DOC` are skipped so they aren't duplicated in the
/// native-functions section.
fn extract_functions_from_cst(source: &str, skip_native: bool) -> Vec<(String, Vec<String>, String)> {
    let (nodes, _) = mq_lang::parse_recovery(source);
    let mut result = Vec::new();

    for node in &nodes {
        if !node.is_def() {
            continue;
        }
        if let Some(info) = def_info(node, skip_native) {
            result.push(info);
        }
    }

    result
}

/// Extracts `(name, params, description)` from a CST `Def` node.
///
/// - `name`: the `Ident` token text of the first child
/// - `params`: `Ident` tokens that appear between `(` and `)` in children
/// - `description`: leading-trivia `Comment` tokens joined with a space
///
/// Returns `None` for private functions (names starting with `_`).
fn def_info(node: &Shared<CstNode>, skip_native: bool) -> Option<(String, Vec<String>, String)> {
    // Function name: first child with NodeKind::Ident
    let name_node = node.children.iter().find(|c| matches!(c.kind, CstNodeKind::Ident))?;
    let name = ident_text(name_node)?;

    if name.starts_with('_') {
        return None;
    }
    if skip_native && BUILTIN_FUNCTION_DOC.contains_key(name.as_str()) {
        return None;
    }

    // Params: Ident children that sit between ( and ) — i.e. before the first
    // Colon/Do token encountered after the function-name child.
    let params: Vec<String> = node
        .children
        .iter()
        .skip(1) // skip function name
        .take_while(|c| {
            c.token
                .as_ref()
                .is_none_or(|t| !matches!(t.kind, TokenKind::Colon | TokenKind::Do))
        })
        .filter(|c| matches!(c.kind, CstNodeKind::Ident))
        .filter_map(ident_text)
        .collect();

    // Description: leading-trivia Comment tokens on the Def node itself
    let desc = node
        .comments()
        .into_iter()
        .map(|(_, s)| s)
        .collect::<Vec<_>>()
        .join(" ");

    Some((name, params, desc))
}

fn ident_text(node: &Shared<CstNode>) -> Option<String> {
    node.token.as_ref().and_then(|t| match &t.kind {
        TokenKind::Ident(s) => Some(s.to_string()),
        _ => None,
    })
}

/// Escapes `|` and newlines so text is safe inside a Markdown table cell.
fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_contains_sections() {
        let md = generate();
        assert!(md.contains("# Built-in Functions"));
        assert!(md.contains("# Selectors"));
        assert!(md.contains("# Module: builtin"));
        assert!(md.contains("# Module: csv"));
        assert!(md.contains("# Module: yaml"));
    }

    #[test]
    fn test_generate_has_table_headers() {
        let md = generate();
        assert!(md.contains("| name | params | description |"));
        assert!(md.contains("| selector | description |"));
    }

    #[test]
    fn test_builtin_functions_excludes_internal() {
        let md = generate();
        assert!(!md.contains("| _sort_by_impl |"));
        assert!(!md.contains("| _csv_parse |"));
    }

    #[test]
    fn test_module_builtin_has_mq_defined_functions() {
        let md = generate();
        // builtin.mq defines is_array, map, filter etc.
        assert!(md.contains("is_array"));
        assert!(md.contains("map"));
    }

    #[test]
    fn test_generate_is_parseable() {
        let md = generate();
        let nodes = mq_lang::parse_markdown_input(&md);
        assert!(nodes.is_ok());
        assert!(!nodes.unwrap().is_empty());
    }

    #[test]
    fn test_cell_escapes_pipe() {
        assert_eq!(cell("a|b"), "a\\|b");
    }

    #[test]
    fn test_extract_functions_from_cst_skips_private() {
        let src = "def _internal(x): x;\ndef public(x): x;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].0, "public");
    }

    #[test]
    fn test_extract_functions_captures_doc() {
        let src = "# Does something useful\ndef useful(x): x;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].2.contains("Does something useful"));
    }

    #[test]
    fn test_extract_functions_captures_params() {
        let src = "def csv_parse(input, has_header): expr;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns[0].1, vec!["input", "has_header"]);
    }

    #[test]
    fn test_extract_functions_default_param() {
        // Note: mq keywords (nodes, self, fn, etc.) cannot be used as parameter names.
        // Use non-keyword names to test default value parsing.
        let src = "def section(items, pattern, depth = false): items;";
        let fns = extract_functions_from_cst(src, false);
        assert!(!fns.is_empty(), "should parse function with default param");
        assert_eq!(fns[0].1, vec!["items", "pattern", "depth"]);
    }
}
