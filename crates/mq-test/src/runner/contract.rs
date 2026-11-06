//! `# @requires(expr)` / `# @ensures(expr)` function contracts.
//!
//! A contracted top-level `def f(a, b)` is renamed in place to `__orig_f` (or a longer unused name), and a wrapper `f`
//! that checks the contracts around a call to `__orig_f` is appended after the file content.
//! Recursive and forward calls resolve `f` at call time, so they go through the wrapper too.
//! Appending (like `@property` does) keeps every original line number intact; only the
//! columns on the renamed `def` line shift.

use mq_lang::{CstNodeKind, CstTrivia};

use super::{TestAnnotation, TestRunner};

/// Prefix of the renamed original function. A suffix of `_` is appended until the name is
/// unused anywhere in the file.
const ORIG_PREFIX: &str = "__orig_";

/// Start of the message of a `@requires` violation raised by a direct call.
const REQUIRES_PREFIX: &str = "@requires violated (caller's responsibility)";

/// One `@requires` / `@ensures` annotation attached to a `def`.
#[derive(Debug, PartialEq)]
pub(super) struct Contract {
    pub(super) expr: String,
    /// One-based line of the annotation, for the violation message.
    pub(super) line: u32,
}

/// A top-level `def` with its contracts.
#[derive(Debug, PartialEq)]
struct Contracted {
    name: String,
    /// Collision-free name the original `def` is renamed to.
    orig_name: String,
    params: Vec<String>,
    requires: Vec<Contract>,
    ensures: Vec<Contract>,
    /// One-based line and column of the name token.
    name_line: u32,
    name_column: usize,
}

impl TestRunner {
    /// Rewrites `content` so every top-level `def` with `@requires`/`@ensures` annotations
    /// checks them on each call. Returns `content` unchanged when there are none, and a
    /// user-facing message when a contracted `def` cannot be wrapped.
    ///
    /// With `label_nested`, a `@requires` violation escaping
    /// from a contracted function's body is re-labelled "in nested call", so `@small_scope`
    /// can tell it from a violation by the function the test called directly. A violation
    /// that passes through an uncontracted helper first is still seen as direct.
    pub(super) fn apply_contracts(content: &str, label_nested: bool) -> Result<String, String> {
        let (nodes, _) = mq_lang::parse_recovery(content);
        let mut contracted = Vec::new();

        for node in &nodes {
            let CstNodeKind::Def { name, .. } = &node.kind else {
                continue;
            };
            let (requires, ensures) = Self::collect_contracts(&node.leading_trivia);
            if requires.is_empty() && ensures.is_empty() {
                continue;
            }

            let name_text = name.name().map(|n| n.to_string()).unwrap_or_default();
            let (sig, _) = node.split_cond_and_program();
            let mut params = Vec::new();
            for param in sig.iter().skip(1) {
                match (&param.kind, param.name()) {
                    (
                        CstNodeKind::Param {
                            asterisk: None,
                            default: None,
                            ..
                        },
                        Some(p),
                    ) => params.push(p.to_string()),
                    _ => {
                        return Err(format!(
                            "contracts on '{name_text}': only plain parameters are supported (no defaults or variadics)"
                        ));
                    }
                }
            }
            if !ensures.is_empty() && params.iter().any(|p| p == "result") {
                return Err(format!(
                    "@ensures on '{name_text}': a parameter named 'result' clashes with the bound return value"
                ));
            }

            let range = name.range();
            contracted.push(Contracted {
                name: name_text,
                orig_name: String::new(),
                params,
                requires,
                ensures,
                name_line: range.start.line,
                name_column: range.start.column,
            });
        }

        if contracted.is_empty() {
            return Ok(content.to_string());
        }

        let mut taken = std::collections::HashSet::new();
        for c in &mut contracted {
            let mut candidate = format!("{ORIG_PREFIX}{}", c.name);
            while content.contains(&candidate) || taken.contains(&candidate) {
                candidate.push('_');
            }
            taken.insert(candidate.clone());
            c.orig_name = candidate;
        }

        let mut out = Self::rename_originals(content, &contracted)?;
        for c in &contracted {
            out.push('\n');
            out.push_str(&Self::build_contract_wrapper(c, label_nested));
        }
        Ok(out)
    }

    /// Collects the `@requires` and `@ensures` annotations among `trivia`, in source order.
    pub(super) fn collect_contracts(trivia: &[CstTrivia]) -> (Vec<Contract>, Vec<Contract>) {
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        for (line, comment) in Self::merge_annotation_lines(trivia) {
            match Self::parse_annotation(&comment) {
                Some(TestAnnotation::Requires(expr)) => requires.push(Contract { expr, line }),
                Some(TestAnnotation::Ensures(expr)) => ensures.push(Contract { expr, line }),
                _ => {}
            }
        }
        (requires, ensures)
    }

    /// Renames each contracted `def` to its `orig_name`, verifying the name token really
    /// sits at the reported position.
    fn rename_originals(content: &str, contracted: &[Contracted]) -> Result<String, String> {
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        let mut offsets = Vec::new();
        for c in contracted {
            let offset = line_starts
                .get(c.name_line as usize - 1)
                .and_then(|&start| {
                    let line = &content[start..];
                    line.char_indices().nth(c.name_column - 1).map(|(i, _)| start + i)
                })
                .filter(|&at| content[at..].starts_with(&c.name))
                .ok_or_else(|| format!("could not locate 'def {}' to attach its contracts", c.name))?;
            offsets.push((offset, c));
        }

        let mut out = content.to_string();
        offsets.sort_unstable_by_key(|(offset, _)| *offset);
        for (offset, c) in offsets.into_iter().rev() {
            out.replace_range(offset..offset + c.name.len(), &c.orig_name);
        }
        Ok(out)
    }

    /// Builds the checking wrapper `def f(...)` that calls the renamed original.
    fn build_contract_wrapper(c: &Contracted, label_nested: bool) -> String {
        let name = &c.name;
        let params = c.params.join(", ");
        let arg_names = c.params.join(", ");
        let args_value = format!("to_string([{params}])");

        let mut steps = Vec::new();
        for r in &c.requires {
            let msg = format!(
                "\"@requires violated (caller's responsibility): {name}: {} (line {}); arguments ({arg_names}) = \" + {args_value}",
                escape_mq_string(&r.expr),
                r.line
            );
            steps.push(format!("(if (not({})): error({msg}) else: None)", r.expr));
        }
        let orig = &c.orig_name;
        if label_nested {
            steps.push(format!(
                "let result = (try: {orig}({params}) catch(e): (error(if (starts_with(e[\"message\"], \"{REQUIRES_PREFIX}\")): \"@requires violated in nested call: \" + e[\"message\"] else: e[\"message\"])))"
            ));
        } else {
            steps.push(format!("let result = {orig}({params})"));
        }
        for e in &c.ensures {
            let msg = format!(
                "\"@ensures violated (function's responsibility): {name}: {} (line {}); arguments ({arg_names}) = \" + {args_value} + \"; result = \" + to_string(result)",
                escape_mq_string(&e.expr),
                e.line
            );
            steps.push(format!("(if (not({})): error({msg}) else: None)", e.expr));
        }
        steps.push("result".to_string());

        format!("def {name}({params}) do\n    {}\nend\n", steps.join("\n    | "))
    }
}

/// Escapes `s` for use inside an mq double-quoted string literal.
fn escape_mq_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
