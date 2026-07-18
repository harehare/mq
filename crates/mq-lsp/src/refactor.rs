//! Refactoring code actions: extract-to-variable, extract-to-function, and
//! inline-variable/inline-function.
//!
//! Unlike the quickfixes in [`crate::code_action`], these actions are driven by the
//! user's cursor position/selection (`params.range`) rather than by diagnostics, and
//! operate directly on the CST (re-parsed from the live buffer text) since neither the
//! HIR symbol ranges nor `mq_lang::CstNode::node_range` are precise enough on their own
//! (see `deep_end` below).

use mq_hir::{Hir, SourceId, SymbolKind};
use mq_lang::{CstNode, CstNodeKind, Range as MqRange, Shared};
use rustc_hash::FxHashMap;
use tower_lsp_server::ls_types::{
    self, CodeAction, CodeActionKind, CodeActionOrCommand, Position, TextEdit, WorkspaceEdit,
};

fn to_range(text_range: MqRange) -> ls_types::Range {
    ls_types::Range::new(
        Position::new(text_range.start.line - 1, (text_range.start.column - 1) as u32),
        Position::new(text_range.end.line - 1, (text_range.end.column - 1) as u32),
    )
}

fn to_mq_position(position: Position) -> mq_lang::Position {
    mq_lang::Position::new(position.line + 1, (position.character + 1) as usize)
}

/// Converts a 1-indexed `mq_lang::Position` (line/column counted in chars) to a byte
/// offset into `text`, since CST ranges are char-based but Rust string slicing is byte-based.
fn byte_offset(text: &str, pos: mq_lang::Position) -> Option<usize> {
    let mut line = 1u32;
    let mut column = 1usize;

    if line == pos.line && column == pos.column {
        return Some(0);
    }

    for (byte_idx, ch) in text.char_indices() {
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }

        if line == pos.line && column == pos.column {
            return Some(byte_idx + ch.len_utf8());
        }
    }

    (line == pos.line && column == pos.column).then_some(text.len())
}

fn slice(text: &str, range: MqRange) -> Option<&str> {
    let start = byte_offset(text, range.start)?;
    let end = byte_offset(text, range.end)?;
    text.get(start..end)
}

/// The true end of `node`'s span, descending through the last child at each level.
///
/// `CstNode::node_range` only looks at the immediate last child's own `range()` (its own
/// token span), not that child's full subtree, so it under-reports whenever the last
/// child is itself a compound node (e.g. a `let x = foo(1, 2)` where the last child of
/// `Let` is the `Call` node `foo(1, 2)`: `Call.range()` is just `foo`, not `foo(1, 2)`).
fn deep_end(node: &CstNode) -> mq_lang::Position {
    match node.children.last() {
        Some(last) => deep_end(last),
        None => node.range().end,
    }
}

/// The true start of `node`'s span. `node.range().start` is wrong for infix nodes like
/// `BinaryOp`/`Assign`, whose own token is the *operator* sitting between its children
/// (e.g. for `1 + 2`, the node's own token is `+`, not `1`) rather than the leading token.
fn deep_start(node: &CstNode) -> mq_lang::Position {
    let own = node.range().start;
    match node.children.first() {
        Some(first) => own.min(deep_start(first)),
        None => own,
    }
}

fn full_range(node: &CstNode) -> MqRange {
    MqRange {
        start: deep_start(node),
        end: deep_end(node),
    }
}

/// Kinds whose own text never needs wrapping in parens to be substituted safely into an
/// arbitrary expression position (they already bind as tightly as any operator).
fn is_atomic(kind: &CstNodeKind) -> bool {
    matches!(
        kind,
        CstNodeKind::Literal
            | CstNodeKind::Ident
            | CstNodeKind::Self_
            | CstNodeKind::SelfAttr
            | CstNodeKind::Call
            | CstNodeKind::CallDynamic
            | CstNodeKind::Nodes
            | CstNodeKind::InterpolatedString
            | CstNodeKind::Array
            | CstNodeKind::Dict
            | CstNodeKind::Selector
            | CstNodeKind::SelectorCall
            | CstNodeKind::QualifiedAccess
            | CstNodeKind::Quote
            | CstNodeKind::Unquote
            | CstNodeKind::MacroCall
            | CstNodeKind::Group
    )
}

fn wrapped_text(node: &CstNode, text: &str) -> String {
    if is_atomic(&node.kind) {
        text.to_string()
    } else {
        format!("({text})")
    }
}

/// Recursively searches `nodes` and every nested (flat, token-interleaved) children list
/// for the first node matching `pred`, returning the containing sibling list and the
/// matched node's index within it (so callers can inspect adjacent pipe tokens).
fn find_container<'a>(
    nodes: &'a [Shared<CstNode>],
    pred: &impl Fn(&CstNode) -> bool,
) -> Option<(&'a [Shared<CstNode>], usize)> {
    if let Some(idx) = nodes.iter().position(|n| pred(n)) {
        return Some((nodes, idx));
    }
    for n in nodes {
        if let Some(found) = find_container(&n.children, pred) {
            return Some(found);
        }
    }
    None
}

/// Recursively searches for a single non-token node whose full span equals `target`.
fn find_single_node(nodes: &[Shared<CstNode>], target: MqRange) -> Option<&Shared<CstNode>> {
    for n in nodes {
        if !n.is_token() && full_range(n) == target {
            return Some(n);
        }
        if let Some(found) = find_single_node(&n.children, target) {
            return Some(found);
        }
    }
    None
}

/// Recursively searches every sibling list for a maximal contiguous run of non-token
/// nodes, joined only by `|` tokens, whose combined span equals `target`. Unlike
/// [`find_single_node`], the run may not be extracted from inside a comma-separated
/// argument/element list, since a bare name isn't valid in place of `1, 2` there.
fn find_pipe_run(nodes: &[Shared<CstNode>], target: MqRange) -> Option<(&[Shared<CstNode>], usize, usize)> {
    let non_token: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| !n.is_token())
        .map(|(i, _)| i)
        .collect();

    for &start in &non_token {
        if deep_start(&nodes[start]) != target.start {
            continue;
        }
        let mut end = start;
        loop {
            if deep_end(&nodes[end]) == target.end {
                return Some((nodes, start, end));
            }
            let Some(&next) = non_token.iter().find(|&&i| i > end) else {
                break;
            };
            let all_pipes = nodes[(end + 1)..next].iter().all(|n| n.is_pipe());
            if !all_pipes {
                break;
            }
            end = next;
        }
    }

    for n in nodes {
        if let Some(found) = find_pipe_run(&n.children, target) {
            return Some(found);
        }
    }
    None
}

/// The range to delete for a statement at `idx` within `container`, swallowing one
/// adjacent `|` token so the remaining pipeline stays syntactically valid.
fn deletion_range(container: &[Shared<CstNode>], idx: usize) -> MqRange {
    let own = full_range(&container[idx]);
    if let Some(next) = container.get(idx + 1)
        && next.is_pipe()
    {
        MqRange {
            start: own.start,
            end: next.range().end,
        }
    } else if idx > 0 && container[idx - 1].is_pipe() {
        MqRange {
            start: container[idx - 1].range().start,
            end: own.end,
        }
    } else {
        own
    }
}

fn format_snippet(code: &str) -> String {
    mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
        indent_width: 2,
        ..Default::default()
    }))
    .format(code)
    .unwrap_or_else(|_| code.to_string())
}

fn single_edit_action(
    uri: &ls_types::Uri,
    title: String,
    kind: CodeActionKind,
    range: MqRange,
    new_text: String,
) -> CodeActionOrCommand {
    let mut changes = FxHashMap::default();
    changes.insert(
        uri.clone(),
        vec![TextEdit {
            range: to_range(range),
            new_text,
        }],
    );

    CodeActionOrCommand::CodeAction(CodeAction {
        title,
        kind: Some(kind),
        edit: Some(WorkspaceEdit {
            changes: Some(changes.into_iter().collect()),
            ..Default::default()
        }),
        ..Default::default()
    })
}

/// Picks a name not already bound anywhere in the HIR, trying `base`, `base2`, `base3`, ...
fn fresh_name(hir: &Hir, base: &str) -> String {
    if hir.symbols().all(|(_, s)| s.value.as_deref() != Some(base)) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}{n}");
        if hir
            .symbols()
            .all(|(_, s)| s.value.as_deref() != Some(candidate.as_str()))
        {
            return candidate;
        }
        n += 1;
    }
}

/// Builds "Extract to variable"/"Extract to function" actions for the user's selection,
/// if it aligns exactly (once surrounding whitespace is trimmed) with either a single
/// expression or a contiguous `|`-joined run of expressions.
pub(crate) fn extract_actions(
    hir: &Hir,
    uri: &ls_types::Uri,
    range: ls_types::Range,
    source_text: &str,
) -> Vec<CodeActionOrCommand> {
    let Some(start) = byte_offset(source_text, to_mq_position(range.start)) else {
        return Vec::new();
    };
    let Some(end) = byte_offset(source_text, to_mq_position(range.end)) else {
        return Vec::new();
    };
    if start >= end {
        return Vec::new();
    }
    let Some(selected) = source_text.get(start..end) else {
        return Vec::new();
    };
    let trimmed = selected.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }
    let leading_ws = selected.len() - selected.trim_start().len();
    let trailing_ws = selected.len() - selected.trim_end().len();
    let Some(target_start) = byte_to_mq_position(source_text, start + leading_ws) else {
        return Vec::new();
    };
    let Some(target_end) = byte_to_mq_position(source_text, end - trailing_ws) else {
        return Vec::new();
    };
    let target = MqRange {
        start: target_start,
        end: target_end,
    };

    let (nodes, reporter) = mq_lang::parse_recovery(source_text);
    if reporter.has_errors() {
        return Vec::new();
    }

    let mut actions = Vec::new();
    let var_name = fresh_name(hir, "extracted");
    let fn_name = fresh_name(hir, "extracted_fn");

    if find_single_node(&nodes, target).is_some() {
        actions.push(single_edit_action(
            uri,
            format!("Extract to variable `{var_name}`"),
            CodeActionKind::new("refactor.extract.variable"),
            target,
            format_snippet(&format!("let {var_name} = {trimmed} | {var_name}")),
        ));
        actions.push(single_edit_action(
            uri,
            format!("Extract to function `{fn_name}`"),
            CodeActionKind::new("refactor.extract.function"),
            target,
            format_snippet(&format!("def {fn_name}(): {trimmed}; | {fn_name}()")),
        ));
    } else if find_pipe_run(&nodes, target).is_some() {
        actions.push(single_edit_action(
            uri,
            format!("Extract to function `{fn_name}`"),
            CodeActionKind::new("refactor.extract.function"),
            target,
            format_snippet(&format!("def {fn_name}(): {trimmed}; | {fn_name}()")),
        ));
    }

    actions
}

/// The reverse of [`byte_offset`]: converts a byte offset back to a 1-indexed
/// `mq_lang::Position`, used to snap a trimmed text selection back onto CST-comparable coordinates.
fn byte_to_mq_position(text: &str, target_byte: usize) -> Option<mq_lang::Position> {
    if target_byte == 0 {
        return Some(mq_lang::Position::new(1, 1));
    }

    let mut line = 1u32;
    let mut column = 1usize;

    for (byte_idx, ch) in text.char_indices() {
        if byte_idx == target_byte {
            return Some(mq_lang::Position::new(line, column));
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (target_byte == text.len()).then_some(mq_lang::Position::new(line, column))
}

/// Builds an "Inline variable"/"Inline function" action for the symbol at `position`, if any.
pub(crate) fn inline_action(
    hir: &Hir,
    source_id: SourceId,
    uri: &ls_types::Uri,
    position: Position,
    source_text: &str,
) -> Option<CodeActionOrCommand> {
    let (symbol_id, symbol) = hir.find_symbol_in_position(source_id, to_mq_position(position))?;
    if hir.is_builtin_symbol(&symbol) {
        return None;
    }

    let def_id = match symbol.kind {
        SymbolKind::Call
        | SymbolKind::Ref
        | SymbolKind::CallDynamic
        | SymbolKind::Argument
        | SymbolKind::QualifiedAccess => hir.resolve_reference_symbol(symbol_id)?,
        _ => symbol_id,
    };
    let def_symbol = hir.symbol(def_id)?;
    if hir.is_builtin_symbol(def_symbol) {
        return None;
    }
    // Only inline definitions living in the currently open source; cross-file inline
    // (e.g. a definition pulled in via `include`) isn't supported.
    if def_symbol.source.source_id != Some(source_id) {
        return None;
    }
    let def_range = def_symbol.source.text_range?;

    let (nodes, reporter) = mq_lang::parse_recovery(source_text);
    if reporter.has_errors() {
        return None;
    }

    let ctx = InlineCtx {
        hir,
        nodes: &nodes,
        uri,
        source_text,
    };

    match &def_symbol.kind {
        SymbolKind::Variable => inline_variable(&ctx, def_id, def_symbol.value.as_deref()?, def_range),
        SymbolKind::Function(params) => inline_function(&ctx, def_id, def_symbol.value.as_deref()?, params, def_range),
        _ => None,
    }
}

struct InlineCtx<'a> {
    hir: &'a Hir,
    nodes: &'a [Shared<CstNode>],
    uri: &'a ls_types::Uri,
    source_text: &'a str,
}

fn inline_variable(
    ctx: &InlineCtx,
    def_id: mq_hir::SymbolId,
    name: &str,
    def_range: MqRange,
) -> Option<CodeActionOrCommand> {
    let (container, idx) = find_container(ctx.nodes, &|n| {
        matches!(n.kind, CstNodeKind::Let | CstNodeKind::Var)
            && n.children.first().map(|c| c.range()) == Some(def_range)
    })?;
    let let_node = &container[idx];
    // children = [lhs ident, `=` token, rhs expr]; destructuring patterns aren't supported.
    let rhs = let_node.children.get(2)?;
    let initializer_range = full_range(rhs);
    let initializer_text = slice(ctx.source_text, initializer_range)?;
    let replacement = wrapped_text(rhs, initializer_text);

    let references = ctx.hir.references(def_id);
    let mut changes: FxHashMap<ls_types::Uri, Vec<TextEdit>> = FxHashMap::default();
    for (_, reference) in &references {
        let text_range = reference.source.text_range?;
        changes.entry(ctx.uri.clone()).or_default().push(TextEdit {
            range: to_range(text_range),
            new_text: replacement.clone(),
        });
    }
    changes.entry(ctx.uri.clone()).or_default().push(TextEdit {
        range: to_range(deletion_range(container, idx)),
        new_text: String::new(),
    });

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Inline variable `{name}`"),
        kind: Some(CodeActionKind::new("refactor.inline.variable")),
        edit: Some(WorkspaceEdit {
            changes: Some(changes.into_iter().collect()),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

fn inline_function(
    ctx: &InlineCtx,
    def_id: mq_hir::SymbolId,
    name: &str,
    params: &[mq_hir::ParamInfo],
    def_range: MqRange,
) -> Option<CodeActionOrCommand> {
    if params.iter().any(|p| p.has_default || p.is_variadic) {
        return None;
    }

    let (container, idx) = find_container(ctx.nodes, &|n| {
        matches!(n.kind, CstNodeKind::Def) && n.children.first().map(|c| c.range()) == Some(def_range)
    })?;
    let def_node = &container[idx];
    let (_, body) = def_node.split_cond_and_program();
    let [body_node] = body.as_slice() else {
        // Multi-statement bodies can't be inlined as a single substituted expression.
        return None;
    };
    let body_range = full_range(body_node);

    // Bail on recursive functions: a textual inline would leave a call to the now-deleted definition.
    let references = ctx.hir.references(def_id);
    if references.iter().any(|(_, r)| {
        r.source
            .text_range
            .is_some_and(|r| body_range.start <= r.start && r.end <= body_range.end)
    }) {
        return None;
    }
    // Every use site must be a static call; a bare `Ref` (passing the function as a
    // value) or a dynamic call can't be textually substituted.
    if references.iter().any(|(_, r)| !matches!(r.kind, SymbolKind::Call)) {
        return None;
    }

    let param_symbols: Vec<(smol_str::SmolStr, mq_hir::SymbolId)> = ctx
        .hir
        .symbols()
        .filter(|(_, s)| s.parent == Some(def_id) && matches!(s.kind, SymbolKind::Parameter))
        .filter_map(|(id, s)| s.value.clone().map(|name| (name, id)))
        .collect();

    let mut changes: FxHashMap<ls_types::Uri, Vec<TextEdit>> = FxHashMap::default();

    for (_, call_symbol) in &references {
        let call_range = call_symbol.source.text_range?;
        let (call_container, call_idx) = find_container(ctx.nodes, &|n| {
            matches!(n.kind, CstNodeKind::Call) && n.range() == call_range
        })?;
        let call_node = &call_container[call_idx];
        let args = call_node.children_without_token();
        if args.len() != params.len() {
            return None;
        }

        let mut substitutions: Vec<(MqRange, String)> = Vec::new();
        for (param, arg) in params.iter().zip(args.iter()) {
            let Some((_, param_symbol_id)) = param_symbols.iter().find(|(n, _)| n == &param.name) else {
                continue;
            };
            let arg_range = full_range(arg);
            let arg_text = slice(ctx.source_text, arg_range)?;
            let arg_replacement = wrapped_text(arg, arg_text);
            for (_, param_ref) in ctx.hir.references(*param_symbol_id) {
                if let Some(r) = param_ref.source.text_range
                    && body_range.start <= r.start
                    && r.end <= body_range.end
                {
                    substitutions.push((r, arg_replacement.clone()));
                }
            }
        }
        substitutions.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));

        let body_start = byte_offset(ctx.source_text, body_range.start)?;
        let body_end = byte_offset(ctx.source_text, body_range.end)?;
        let mut substituted = ctx.source_text.get(body_start..body_end)?.to_string();
        for (r, replacement) in &substitutions {
            let rel_start = byte_offset(ctx.source_text, r.start)?.checked_sub(body_start)?;
            let rel_end = byte_offset(ctx.source_text, r.end)?.checked_sub(body_start)?;
            substituted.replace_range(rel_start..rel_end, replacement);
        }

        let replacement_text = if is_atomic(&body_node.kind) {
            substituted
        } else {
            format!("({substituted})")
        };

        changes.entry(ctx.uri.clone()).or_default().push(TextEdit {
            range: to_range(full_range(call_node)),
            new_text: replacement_text,
        });
    }

    changes.entry(ctx.uri.clone()).or_default().push(TextEdit {
        range: to_range(deletion_range(container, idx)),
        new_text: String::new(),
    });

    Some(CodeActionOrCommand::CodeAction(CodeAction {
        title: format!("Inline function `{name}`"),
        kind: Some(CodeActionKind::new("refactor.inline.function")),
        edit: Some(WorkspaceEdit {
            changes: Some(changes.into_iter().collect()),
            ..Default::default()
        }),
        ..Default::default()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    use url::Url;

    fn setup(code: &str) -> (Hir, SourceId, ls_types::Uri) {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _) = hir.add_code(Some(url.clone()), code);
        let uri = ls_types::Uri::from_str(url.as_str()).unwrap();
        (hir, source_id, uri)
    }

    fn edits_for(action: &CodeActionOrCommand) -> &[TextEdit] {
        match action {
            CodeActionOrCommand::CodeAction(action) => action
                .edit
                .as_ref()
                .unwrap()
                .changes
                .as_ref()
                .unwrap()
                .values()
                .next()
                .unwrap(),
            _ => panic!("expected a CodeAction"),
        }
    }

    #[test]
    fn test_inline_variable_from_definition() {
        let code = "let val1 = 1 + 2 | val1";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code).unwrap();
        let CodeActionOrCommand::CodeAction(ref inner) = action else {
            panic!("expected a CodeAction");
        };
        assert_eq!(inner.title, "Inline variable `val1`");

        let edits = edits_for(&action);
        // One edit replaces the `val1` usage, one deletes the `let` statement.
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|e| e.new_text == "(1 + 2)"));
    }

    #[test]
    fn test_inline_variable_from_usage() {
        let code = "let val1 = 1 + 2 | val1";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 19), code).unwrap();
        let edits = edits_for(&action);
        assert_eq!(edits.len(), 2);
    }

    #[test]
    fn test_inline_variable_atomic_initializer_is_not_wrapped() {
        let code = "let val1 = foo() | val1";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code).unwrap();
        let edits = edits_for(&action);
        assert!(edits.iter().any(|e| e.new_text == "foo()"));
    }

    #[test]
    fn test_no_inline_for_builtin() {
        let code = "\"hello\" | len";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 11), code);
        assert!(action.is_none());
    }

    #[test]
    fn test_no_inline_when_no_symbol_at_position() {
        let code = "let val1 = 1 | val1";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(5, 5), code);
        assert!(action.is_none());
    }

    #[test]
    fn test_inline_nullary_function() {
        let code = "def helper(): 1 + 2; | helper()";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code).unwrap();
        let CodeActionOrCommand::CodeAction(ref inner) = action else {
            panic!("expected a CodeAction");
        };
        assert_eq!(inner.title, "Inline function `helper`");

        let edits = edits_for(&action);
        assert_eq!(edits.len(), 2);
        assert!(edits.iter().any(|e| e.new_text == "(1 + 2)"));
    }

    #[test]
    fn test_inline_function_with_parameter_substitutes_argument() {
        let code = "def double(x): x + x; | double(21)";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code).unwrap();
        let edits = edits_for(&action);
        assert!(edits.iter().any(|e| e.new_text == "(21 + 21)"));
    }

    #[test]
    fn test_inline_function_with_call_argument_wraps_parameter_use() {
        // Substituting `foo() + 1` for `x` must be parenthesized so `x * 2` doesn't
        // become `foo() + 1 * 2`.
        let code = "def double(x): x * 2; | double(foo() + 1)";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code).unwrap();
        let edits = edits_for(&action);
        assert!(edits.iter().any(|e| e.new_text == "((foo() + 1) * 2)"));
    }

    #[test]
    fn test_no_inline_for_recursive_function() {
        let code = "def fact(n): if (n <= 1): 1; else: n * fact(n - 1);; | fact(5)";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code);
        assert!(action.is_none());
    }

    #[test]
    fn test_no_inline_for_multi_statement_function_body() {
        let code = "def helper(): let x = 1 | x + 1; | helper()";
        let (hir, source_id, uri) = setup(code);

        let action = inline_action(&hir, source_id, &uri, Position::new(0, 5), code);
        assert!(action.is_none());
    }

    #[test]
    fn test_extract_single_node_offers_variable_and_function() {
        let code = "1 | foo(1, 2) | bar()";
        let (hir, _source_id, uri) = setup(code);

        // Select exactly `foo(1, 2)`.
        let range = ls_types::Range::new(Position::new(0, 4), Position::new(0, 13));
        let actions = extract_actions(&hir, &uri, range, code);
        assert_eq!(actions.len(), 2);

        let titles: Vec<String> = actions
            .iter()
            .map(|a| match a {
                CodeActionOrCommand::CodeAction(a) => a.title.clone(),
                _ => String::new(),
            })
            .collect();
        assert!(titles.iter().any(|t| t.starts_with("Extract to variable")));
        assert!(titles.iter().any(|t| t.starts_with("Extract to function")));
    }

    #[test]
    fn test_extract_pipe_run_offers_only_function() {
        let code = "1 | foo() | bar() | baz()";
        let (hir, _source_id, uri) = setup(code);

        // Select the two-stage sub-pipeline `foo() | bar()`.
        let range = ls_types::Range::new(Position::new(0, 4), Position::new(0, 18));
        let actions = extract_actions(&hir, &uri, range, code);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            CodeActionOrCommand::CodeAction(a) => assert!(a.title.starts_with("Extract to function")),
            _ => panic!("expected a CodeAction"),
        }
    }

    #[test]
    fn test_extract_no_action_for_misaligned_selection() {
        let code = "1 | foo(1, 2) | bar()";
        let (hir, _source_id, uri) = setup(code);

        // Selects only part of the call (`foo(1`), not a whole node.
        let range = ls_types::Range::new(Position::new(0, 4), Position::new(0, 10));
        let actions = extract_actions(&hir, &uri, range, code);
        assert!(actions.is_empty());
    }

    #[test]
    fn test_extract_variable_edit_shape() {
        let code = "1 | foo(1, 2) | bar()";
        let (hir, _source_id, uri) = setup(code);

        let range = ls_types::Range::new(Position::new(0, 4), Position::new(0, 13));
        let actions = extract_actions(&hir, &uri, range, code);
        let edits = edits_for(&actions[0]);
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0].range, range);
        assert!(edits[0].new_text.contains("let extracted = foo(1, 2)"));
        assert!(edits[0].new_text.trim_end().ends_with("extracted"));
    }
}
