use std::sync::{Arc, RwLock};

use mq_lang::{CstNode, Shared, TokenKind};
use tower_lsp_server::ls_types::{ParameterInformation, ParameterLabel, Position, SignatureHelp, SignatureInformation};
use url::Url;

/// Signature help needs exact call/paren/comma boundaries, which `mq_hir::Symbol` ranges
/// don't preserve (a `Call` symbol's range is only its name token; punctuation isn't
/// lowered into HIR at all). So this re-parses the document into a CST — cheap for the
/// small scripts mq targets — and walks it directly instead of going through the HIR.
pub(crate) fn response(
    hir: Arc<RwLock<mq_hir::Hir>>,
    url: Url,
    position: Position,
    source_text: Option<&str>,
) -> Option<SignatureHelp> {
    let source_text = source_text?;
    let hir_guard = hir.read().unwrap();
    let source_id = hir_guard.source_by_url(&url)?;
    let cursor = mq_lang::Position::new(position.line + 1, (position.character + 1) as usize);

    let (nodes, _) = mq_lang::parse_recovery(source_text);
    let call = find_enclosing_call(&nodes, cursor)?;

    let (_, target) = hir_guard.find_symbol_in_position(source_id, call.range().start)?;
    let params = match &target.kind {
        mq_hir::SymbolKind::Function(params) | mq_hir::SymbolKind::Macro(params) => params.clone(),
        _ => return None,
    };

    let name = target.value.as_deref().unwrap_or_default();
    let param_labels = params.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    let label = format!("{}({})", name, param_labels.join(", "));
    let active_parameter = active_parameter_index(&call, cursor, params.len());

    Some(SignatureHelp {
        signatures: vec![SignatureInformation {
            label,
            documentation: None,
            parameters: Some(
                param_labels
                    .into_iter()
                    .map(|label| ParameterInformation {
                        label: ParameterLabel::Simple(label),
                        documentation: None,
                    })
                    .collect(),
            ),
            active_parameter,
        }],
        active_signature: Some(0),
        active_parameter,
    })
}

/// Finds the innermost `Call`/`CallDynamic` node whose full span (name through closing
/// paren, via `node_range()`) contains `position`. Recursing into children after recording
/// a match means a nested call (e.g. `outer(inner(1))`) naturally overwrites the outer one
/// once the cursor is confirmed to be inside it.
fn find_enclosing_call(nodes: &[Shared<CstNode>], position: mq_lang::Position) -> Option<Shared<CstNode>> {
    let mut best = None;
    for node in nodes {
        visit(node, position, &mut best);
    }
    best
}

fn visit(node: &Shared<CstNode>, position: mq_lang::Position, best: &mut Option<Shared<CstNode>>) {
    if !node.node_range().contains(&position) {
        return;
    }
    if matches!(
        node.kind,
        mq_lang::CstNodeKind::Call | mq_lang::CstNodeKind::CallDynamic
    ) {
        *best = Some(node.clone());
    }
    for child in &node.children {
        visit(child, position, best);
    }
}

/// Counts the call's own top-level `,` children that fall before `position`. Only direct
/// children are considered, so commas belonging to a nested call or array argument aren't
/// mistaken for this call's own argument separators.
fn active_parameter_index(call: &Shared<CstNode>, position: mq_lang::Position, param_count: usize) -> Option<u32> {
    if param_count == 0 {
        return None;
    }

    let commas_before_cursor = call
        .children
        .iter()
        .filter(|child| {
            child.is_token()
                && child
                    .token
                    .as_deref()
                    .is_some_and(|token| matches!(token.kind, TokenKind::Comma))
        })
        .filter(|child| child.range().start < position)
        .count();

    Some(commas_before_cursor.min(param_count - 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_hir::Hir;

    #[test]
    fn test_no_call_at_position() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "let x = 1";
        hir.add_code(Some(url.clone()), code);

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 5), Some(code));
        assert!(result.is_none());
    }

    #[test]
    fn test_no_source_text() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def foo(a): a; | foo(1)";
        hir.add_code(Some(url.clone()), code);

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 21), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_invalid_url() {
        let hir = Hir::default();
        let url = Url::parse("file:///nonexistent.mq").unwrap();

        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 0), Some(""));
        assert!(result.is_none());
    }

    #[test]
    fn test_signature_help_first_parameter() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def foo(a, b): a + b; | foo(1, 2)";
        hir.add_code(Some(url.clone()), code);

        // Cursor right before the `1` argument.
        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 28), Some(code));
        assert!(result.is_some());
        let help = result.unwrap();

        assert_eq!(help.signatures.len(), 1);
        assert_eq!(help.signatures[0].label, "foo(a, b)");
        assert_eq!(help.active_parameter, Some(0));
    }

    #[test]
    fn test_signature_help_second_parameter() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def foo(a, b): a + b; | foo(1, 2)";
        hir.add_code(Some(url.clone()), code);

        // Cursor right before the `2` argument.
        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 31), Some(code));
        assert!(result.is_some());
        let help = result.unwrap();

        assert_eq!(help.active_parameter, Some(1));
    }

    #[test]
    fn test_signature_help_no_parameters() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def noop(): 1; | noop()";
        hir.add_code(Some(url.clone()), code);

        // Cursor inside the empty argument list.
        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 22), Some(code));
        assert!(result.is_some());
        let help = result.unwrap();

        assert_eq!(help.signatures[0].label, "noop()");
        assert_eq!(help.active_parameter, None);
    }

    #[test]
    fn test_signature_help_nested_call_uses_innermost() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def inner(x): x; | def outer(y): y; | outer(inner(1))";
        hir.add_code(Some(url.clone()), code);

        // Cursor on the `1` inside `inner(1)`.
        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 51), Some(code));
        assert!(result.is_some());
        let help = result.unwrap();

        assert_eq!(help.signatures[0].label, "inner(x)");
    }

    #[test]
    fn test_signature_help_outer_call_when_between_calls() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def inner(x): x; | def outer(y): y; | outer(inner(1))";
        hir.add_code(Some(url.clone()), code);

        // Cursor right on `outer`'s opening paren, before `inner(1)` starts.
        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 43), Some(code));
        assert!(result.is_some());
        let help = result.unwrap();

        assert_eq!(help.signatures[0].label, "outer(y)");
    }

    #[test]
    fn test_signature_help_variadic_clamps_active_parameter() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let code = "def foo(*rest): rest; | foo(1, 2, 3)";
        hir.add_code(Some(url.clone()), code);

        // Cursor on the third argument.
        let result = response(Arc::new(RwLock::new(hir)), url, Position::new(0, 34), Some(code));
        assert!(result.is_some());
        let help = result.unwrap();

        assert_eq!(help.signatures[0].label, "foo(*rest)");
        assert_eq!(help.active_parameter, Some(0));
    }
}
