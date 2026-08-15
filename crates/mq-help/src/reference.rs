use mq_lang::{BUILTIN_FUNCTION_DOC, CstNode, CstNodeKind, Shared, TokenKind};

/// A single verified example extracted from an `Example:` code fence in a doc comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqExample {
    pub code: String,
    pub expected: String,
}

/// Documentation for a single mq-defined function (from `builtin.mq` or a standard module),
/// extracted from its CST and leading `#` doc-comment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqFnDoc {
    pub name: String,
    pub params: Vec<String>,
    pub description: String,
    pub returns: Option<String>,
    pub examples: Vec<MqExample>,
}

/// Module-level documentation, extracted from the header comment at the very top of a
/// standard-module source file (see [`extract_module_doc`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModuleDoc {
    pub description: String,
    pub examples: Vec<MqExample>,
}

/// Extracts documentation for every public `def` in `source`.
///
/// `skip_native` excludes functions whose name is already documented as a native builtin
/// (used for `builtin.mq`, which sometimes re-declares a native function's name).
/// Names starting with `_` are always excluded (internal helpers).
///
/// Doc comments are parsed from the CST produced by `mq_lang::parse_recovery`, so no
/// hand-written text scanning is involved. Recognized Markdown-ish conventions:
///
/// ```text
/// # Checks if input is an array.
/// #
/// # Example:
/// # ```
/// # is_array([1, 2])
/// # #=> true
/// # ```
/// #
/// # Returns: bool
/// def is_array(a): type(a) == "array";
/// ```
///
/// Inside a fenced ` ``` ` block, a line immediately followed by a `#=>` line becomes a
/// runnable example (checked by `doc_examples` tests); a block may contain several such
/// pairs. A `Returns: TYPE` line sets the return type. Everything else is free-text
/// description.
pub fn extract_functions_from_cst(source: &str, skip_native: bool) -> Vec<MqFnDoc> {
    extract_module(source, skip_native).1
}

/// Extracts the header comment at the very top of a standard-module source file (a `#`
/// paragraph followed by a true blank line), if one is present.
pub fn extract_module_doc(source: &str) -> Option<ModuleDoc> {
    extract_module(source, false).0
}

/// Extracts a module's header doc and function docs in a single CST parse. `extract_module_doc`
/// and `extract_functions_from_cst` each call this and discard half the result — fine for one-
/// off lookups, but callers needing both (e.g. `mq help`'s catalog build, over every standard
/// module) should call this directly to parse each module's source only once.
pub fn extract_module(source: &str, skip_native: bool) -> (Option<ModuleDoc>, Vec<MqFnDoc>) {
    let (nodes, _) = mq_lang::parse_recovery(source);
    // A module header shares node 0's leading trivia with that node's own doc comment, so
    // strip the header's lines off the front to avoid leaking it into the first def's
    // description.
    let header = module_header_paragraph(source, &nodes);
    let header_lines = header.as_ref().map(Vec::len).unwrap_or(0);
    let module_doc = header.and_then(|lines| {
        let (description, _returns, examples) = parse_doc_comment(lines);
        (!description.is_empty() || !examples.is_empty()).then_some(ModuleDoc { description, examples })
    });

    let mut functions = Vec::new();
    for (i, node) in nodes.iter().enumerate() {
        if !node.is_def() {
            continue;
        }
        let skip = if i == 0 { header_lines } else { 0 };
        if let Some(info) = def_info(node, skip_native, skip) {
            functions.push(info);
        }
    }

    (module_doc, functions)
}

// If node 0 is a documented def, its first paragraph is only a module header when a second
// paragraph is left over for its own doc — otherwise it's just that def's doc comment.
fn module_header_paragraph(source: &str, nodes: &[Shared<CstNode>]) -> Option<Vec<String>> {
    let mut paragraphs = leading_comment_paragraphs(source);
    if paragraphs.is_empty() {
        return None;
    }

    let first_is_documented_def = nodes
        .first()
        .is_some_and(|n| n.is_def() && def_name(n).is_some_and(|name| !name.starts_with('_')));

    if first_is_documented_def {
        (paragraphs.len() >= 2).then(|| paragraphs.remove(0))
    } else {
        Some(paragraphs.into_iter().flatten().collect())
    }
}

/// Splits the leading `#`/blank lines of `source` into paragraphs, split on a truly blank
/// line (an empty `#` line is a soft break, kept as an empty entry within the paragraph).
/// Stops at the first line that's neither blank nor `#`-prefixed.
fn leading_comment_paragraphs(source: &str) -> Vec<Vec<String>> {
    let mut paragraphs = Vec::new();
    let mut current = Vec::new();

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !current.is_empty() {
                paragraphs.push(std::mem::take(&mut current));
            }
            continue;
        }
        match trimmed.strip_prefix('#') {
            Some(rest) => current.push(rest.trim().to_string()),
            None => break,
        }
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }

    paragraphs
}

fn def_info(node: &Shared<CstNode>, skip_native: bool, skip_comment_lines: usize) -> Option<MqFnDoc> {
    let name = def_name(node)?;

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

    let comments = node.comments().into_iter().map(|(_, s)| s).skip(skip_comment_lines);
    let (description, returns, examples) = parse_doc_comment(comments);

    Some(MqFnDoc {
        name,
        params,
        description,
        returns,
        examples,
    })
}

/// Function name: first child with `CstNodeKind::Ident`.
fn def_name(node: &Shared<CstNode>) -> Option<String> {
    let name_node = node.children.iter().find(|c| matches!(c.kind, CstNodeKind::Ident))?;
    ident_text(name_node)
}

/// One line of a doc comment outside any fenced code block, classified by its role in the
/// Markdown-ish convention documented on [`extract_functions_from_cst`]. Adding a new
/// recognized line kind (e.g. a future `Capability:` line) means adding one variant here and
/// one match arm in [`parse_doc_comment`] — the fence-toggling logic never has to change.
enum DocLine<'a> {
    /// A blank line, or the literal `Example:` label — pure formatting, carries no data.
    Skip,
    /// `Returns: TYPE`
    Returns(&'a str),
    /// Anything else, folded into the free-text description.
    Description(&'a str),
}

impl<'a> DocLine<'a> {
    fn classify(line: &'a str) -> Self {
        if line.is_empty() || line == "Example:" {
            Self::Skip
        } else if let Some(rest) = line.strip_prefix("Returns:") {
            Self::Returns(rest.trim())
        } else {
            Self::Description(line)
        }
    }
}

/// Parses the Markdown-ish doc-comment convention documented on
/// [`extract_functions_from_cst`] out of a def's leading comment lines.
fn parse_doc_comment(lines: impl IntoIterator<Item = String>) -> (String, Option<String>, Vec<MqExample>) {
    let mut description_parts = Vec::new();
    let mut returns = None;
    let mut examples = Vec::new();
    // `Some(lines)` while inside a ` ``` ` fence, accumulating its (de-indented) lines.
    let mut fence: Option<Vec<String>> = None;

    for raw in lines {
        let line = raw.trim();

        if line == "```" {
            match fence.take() {
                Some(fence_lines) => examples.extend(parse_fence_example(&fence_lines)),
                None => fence = Some(Vec::new()),
            }
            continue;
        }

        if let Some(fence_lines) = &mut fence {
            fence_lines.push(line.to_string());
            continue;
        }

        match DocLine::classify(line) {
            DocLine::Skip => {}
            DocLine::Returns(ty) => returns = Some(ty.to_string()),
            DocLine::Description(text) => description_parts.push(text.to_string()),
        }
    }

    (description_parts.join(" "), returns, examples)
}

/// Parses the (already de-indented) lines inside one ` ``` ` fence into a single example.
///
/// Everything before the first `#=>` line is the code; the arrow's own text plus every line
/// after it (up to the fence close) is the expected value, joined with `\n` — so output that's
/// naturally multi-line (a CSV/table/frontmatter render, ...) doesn't need to be contorted onto
/// one line. A fence with no `#=>` line, or no code before it, yields no example. A function
/// with multiple examples uses multiple fences (see `test_extract_functions_multiple_examples`),
/// so this never has to disambiguate more than one `#=>` per fence.
fn parse_fence_example(fence_lines: &[String]) -> Option<MqExample> {
    let arrow_index = fence_lines.iter().position(|l| l.starts_with("#=>"))?;
    let code = fence_lines[..arrow_index].join("\n");
    if code.is_empty() {
        return None;
    }

    let mut expected_lines = vec![fence_lines[arrow_index].trim_start_matches("#=>").trim().to_string()];
    expected_lines.extend(fence_lines[arrow_index + 1..].iter().cloned());

    Some(MqExample {
        code,
        expected: expected_lines.join("\n"),
    })
}

fn ident_text(node: &Shared<CstNode>) -> Option<String> {
    node.token.as_ref().and_then(|t| match &t.kind {
        TokenKind::Ident(s) => Some(s.to_string()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_functions_from_cst_skips_private() {
        let src = "def _internal(x): x;\ndef public(x): x;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "public");
    }

    #[test]
    fn test_extract_functions_captures_doc() {
        let src = "# Does something useful\ndef useful(x): x;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns.len(), 1);
        assert!(fns[0].description.contains("Does something useful"));
    }

    #[test]
    fn test_extract_functions_captures_params() {
        let src = "def csv_parse(input, has_header): expr;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns[0].params, vec!["input", "has_header"]);
    }

    #[test]
    fn test_extract_functions_default_param() {
        // Note: mq keywords (nodes, self, fn, etc.) cannot be used as parameter names.
        // Use non-keyword names to test default value parsing.
        let src = "def section(items, pattern, depth = false): items;";
        let fns = extract_functions_from_cst(src, false);
        assert!(!fns.is_empty(), "should parse function with default param");
        assert_eq!(fns[0].params, vec!["items", "pattern", "depth"]);
    }

    #[test]
    fn test_extract_functions_captures_example_and_returns() {
        let src = "# Checks if input is an array.\n#\n# Example:\n# ```\n# is_array([1, 2])\n# #=> true\n# ```\n#\n# Returns: bool\ndef is_array(a): a;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].description, "Checks if input is an array.");
        assert_eq!(fns[0].returns.as_deref(), Some("bool"));
        assert_eq!(fns[0].examples.len(), 1);
        assert_eq!(fns[0].examples[0].code, "is_array([1, 2])");
        assert_eq!(fns[0].examples[0].expected, "true");
    }

    #[test]
    fn test_extract_functions_multiple_examples() {
        let src = "# Adds one.\n#\n# Example:\n# ```\n# add1(1)\n# #=> 2\n# ```\n# Example:\n# ```\n# add1(-1)\n# #=> 0\n# ```\ndef add1(x): add(x, 1);";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns[0].examples.len(), 2);
        assert_eq!(fns[0].examples[0].expected, "2");
        assert_eq!(fns[0].examples[1].expected, "0");
    }

    #[test]
    fn test_extract_functions_multiline_expected() {
        let src = "# Joins with newlines.\n#\n# Example:\n# ```\n# join_lines([\"a\", \"b\"])\n# #=> a\n# b\n# ```\ndef join_lines(arr): arr;";
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns[0].examples.len(), 1);
        assert_eq!(fns[0].examples[0].expected, "a\nb");
    }

    #[test]
    fn test_module_doc_absent_without_leading_comment() {
        assert_eq!(extract_module_doc("def foo(x): x;"), None);
    }

    #[test]
    fn test_module_doc_absent_when_only_first_fn_doc() {
        // A single paragraph directly above the first (public) def is that def's own doc,
        // not a module header — even though it happens to precede a blank-separated Example.
        let src = "# Checks if input is an array.\n#\n# Example:\n# ```\n# is_array([1, 2])\n# #=> true\n# ```\ndef is_array(a): a;";
        assert_eq!(extract_module_doc(src), None);
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns[0].description, "Checks if input is an array.");
    }

    #[test]
    fn test_module_doc_split_from_first_public_def() {
        // A header paragraph, blank line, then the first def's own doc: the header becomes
        // the module doc and is stripped out of the def's description (regression test for
        // the header-leaking-into-the-first-function bug).
        let src = "# CSV/TSV Implementation in mq.\n\n# Checks a field.\ndef csv_needs_quote(field): field;";
        let doc = extract_module_doc(src).unwrap();
        assert_eq!(doc.description, "CSV/TSV Implementation in mq.");
        let fns = extract_functions_from_cst(src, false);
        assert_eq!(fns[0].description, "Checks a field.");
    }

    #[test]
    fn test_module_doc_present_when_first_def_is_private() {
        // The header isn't followed by a second paragraph, but the first def is private
        // (filtered out entirely), so the single paragraph is free to be the module doc.
        let src = "# Semantic Versioning Implementation in mq\n\ndef _compare(a, b): a;";
        let doc = extract_module_doc(src).unwrap();
        assert_eq!(doc.description, "Semantic Versioning Implementation in mq");
    }

    #[test]
    fn test_module_doc_present_when_first_node_is_not_a_def() {
        // e.g. table.mq: an `import` precedes the first def, so the header can't be confused
        // with any function's own doc regardless of paragraph count.
        let src = "# This module is under development.\n\nimport \"csv\" |\n\n# Extracts tables.\ndef tables(x): x;";
        let doc = extract_module_doc(src).unwrap();
        assert_eq!(doc.description, "This module is under development.");
    }
}
