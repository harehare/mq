//! CST-to-HIR lowering: converts CST nodes into HIR symbols.

use mq_lang::{Token, TokenKind};
use url::Url;

use crate::{
    Hir,
    scope::{Scope, ScopeId, ScopeKind},
    source::{SourceId, SourceInfo},
    symbol::{ParamInfo, Symbol, SymbolId, SymbolKind},
};

/// Constructs a [`mq_lang::Selector`] from a CST selector node.
///
/// For bracket-based selectors (e.g., `.[n]`, `.[n][m]`), the CST node has
/// `token = Selector(".")` with bracket tokens and optional number literals as
/// children. This function inspects all children to determine bracket count and
/// indices, then returns the appropriate `List` or `Table` selector variant.
///
/// For all other selectors the token value is passed directly to
/// [`mq_lang::Selector::try_from`].
fn selector_from_cst_node(node: &mq_lang::CstNode) -> Option<mq_lang::Selector> {
    let token = node.token.as_ref()?;

    if !matches!(&token.kind, TokenKind::Selector(s) if s == ".") {
        return mq_lang::Selector::try_from(&**token).ok();
    }

    // Bracket-based selector: walk all children to count bracket pairs and
    // collect the optional number literal inside each pair.
    let mut bracket_pairs: u32 = 0;
    let mut indices: Vec<Option<usize>> = Vec::with_capacity(2);
    let mut in_bracket = false;
    let mut bracket_has_number = false;

    for child in &node.children {
        let Some(tok) = child.token.as_ref() else {
            continue;
        };
        match &tok.kind {
            TokenKind::LBracket => {
                in_bracket = true;
                bracket_has_number = false;
                bracket_pairs += 1;
            }
            TokenKind::RBracket => {
                if in_bracket && !bracket_has_number {
                    indices.push(None);
                }
                in_bracket = false;
            }
            TokenKind::NumberLiteral(n) if in_bracket => {
                let idx = if n.is_int() && n.value() >= 0.0 {
                    Some(n.to_int() as usize)
                } else {
                    None
                };
                indices.push(idx);
                bracket_has_number = true;
            }
            _ => {}
        }
    }

    match bracket_pairs {
        1 => Some(mq_lang::Selector::List(indices.first().copied().flatten(), None)),
        2 => Some(mq_lang::Selector::Table(
            indices.first().copied().flatten(),
            indices.get(1).copied().flatten(),
        )),
        _ => None,
    }
}

/// Generates a simple `add_*_expr` method: guards on a CST node kind, creates
/// one HIR symbol with `node.name()` as the value, then recurses into children.
macro_rules! simple_expr {
    ($name:ident, $cst_kind:pat, $sym_kind:expr) => {
        fn $name(
            &mut self,
            node: &mq_lang::Shared<mq_lang::CstNode>,
            source_id: SourceId,
            scope_id: ScopeId,
            parent: Option<SymbolId>,
        ) {
            if matches!((**node).kind, $cst_kind) {
                let symbol_id = self.add_symbol(Symbol {
                    value: node.name(),
                    kind: $sym_kind,
                    source: SourceInfo::new(Some(source_id), Some(node.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent,
                    insertion_order: 0,
                });
                for child in node.children_without_token() {
                    self.add_expr(&child, source_id, scope_id, Some(symbol_id));
                }
            }
        }
    };
}

impl Hir {
    pub(super) fn add_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        let mq_lang::CstNode { kind, .. } = &**node;

        match kind {
            mq_lang::CstNodeKind::BinaryOp(_) => {
                self.add_binary_op_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Block => {
                self.add_block_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::UnaryOp(_) => {
                self.add_unary_op_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Call => {
                self.add_call_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::CallDynamic => {
                self.add_call_dynamic_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Def => {
                self.add_def_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Macro => {
                self.add_macro_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::MacroCall => {
                self.add_macro_call_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Foreach => {
                self.add_foreach_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Fn => {
                self.add_fn_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Ident => {
                self.add_ident_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::If => {
                self.add_if_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Include => {
                self.add_include_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Import => {
                self.add_import_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Module => {
                self.add_module_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::QualifiedAccess => {
                self.add_qualified_access_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::InterpolatedString => {
                self.add_interpolated_string(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Let | mq_lang::CstNodeKind::Var => {
                self.add_var_decl(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Literal => {
                self.add_literal_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Selector => {
                self.add_selector_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::While => {
                self.add_while_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Loop => {
                self.add_loop_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Try => {
                self.add_try_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Catch => {
                self.add_catch_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Array => {
                self.add_array_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Dict => {
                self.add_dict_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Match => {
                self.add_match_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::MatchArm => {
                self.add_match_arm_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Pattern => {
                self.add_pattern_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Quote => {
                self.add_quote_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Unquote => {
                self.add_unquote_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Break => {
                self.add_break_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Self_
            | mq_lang::CstNodeKind::Nodes
            | mq_lang::CstNodeKind::End
            | mq_lang::CstNodeKind::Continue => {
                self.add_keyword(node, source_id, scope_id, parent);
            }

            mq_lang::CstNodeKind::Assign => {
                self.add_assign_expr(node, source_id, scope_id, parent);
            }

            _ => {}
        }
    }

    simple_expr!(add_assign_expr, mq_lang::CstNodeKind::Assign, SymbolKind::Assign);
    simple_expr!(
        add_binary_op_expr,
        mq_lang::CstNodeKind::BinaryOp(_),
        SymbolKind::BinaryOp
    );
    simple_expr!(add_unary_op_expr, mq_lang::CstNodeKind::UnaryOp(_), SymbolKind::UnaryOp);
    simple_expr!(
        add_qualified_access_expr,
        mq_lang::CstNodeKind::QualifiedAccess,
        SymbolKind::QualifiedAccess
    );
    simple_expr!(add_try_expr, mq_lang::CstNodeKind::Try, SymbolKind::Try);
    simple_expr!(add_catch_expr, mq_lang::CstNodeKind::Catch, SymbolKind::Catch);
    simple_expr!(add_array_expr, mq_lang::CstNodeKind::Array, SymbolKind::Array);

    /// Lowers a `CstNodeKind::Assign` node into a `SymbolKind::Assign` symbol.
    ///
    /// Assignment nodes (e.g., `x = 10`, `x += 1`) have two children: the LHS
    fn add_block_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Block,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::Block,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Create a new scope for the block
            let block_scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Block(symbol_id),
                Some(scope_id),
            ));

            // Process all child nodes within the block scope
            node.children.iter().for_each(|child| {
                self.add_expr(child, source_id, block_scope_id, Some(symbol_id));
            });
        }
    }

    fn add_literal_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Literal,
            ..
        } = &**node
        {
            // Check if this is a symbol literal (has children: colon + identifier/string)
            if !node.children.is_empty() {
                // Symbol literal: extract the symbol name from the second child
                if let Some(symbol_child) = node.children.get(1) {
                    self.add_symbol(Symbol {
                        value: symbol_child.name(),
                        kind: SymbolKind::Symbol,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
            } else {
                // Regular literal with token
                self.add_symbol(Symbol {
                    value: node.name(),
                    kind: match &node.token.clone().unwrap().kind {
                        mq_lang::TokenKind::StringLiteral(_) => SymbolKind::String,
                        mq_lang::TokenKind::NumberLiteral(_) => SymbolKind::Number,
                        mq_lang::TokenKind::BoolLiteral(_) => SymbolKind::Boolean,
                        mq_lang::TokenKind::None => SymbolKind::None,
                        _ => unreachable!(),
                    },
                    source: SourceInfo::new(Some(source_id), Some(node.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent,
                    insertion_order: 0,
                });
            }
        }
    }

    fn add_interpolated_string(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::InterpolatedString,
            token: Some(token),
            ..
        } = &**node
            && let Token {
                kind: TokenKind::InterpolatedString(segments),
                ..
            } = &**token
        {
            segments.iter().for_each(|segment| match segment {
                mq_lang::StringSegment::Text(text, range) => {
                    self.add_symbol(Symbol {
                        value: Some(text.into()),
                        kind: SymbolKind::String,
                        source: SourceInfo::new(Some(source_id), Some(*range)),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
                mq_lang::StringSegment::Expr(expr, range) => {
                    self.insert_symbol(Symbol {
                        value: Some(expr.clone()),
                        kind: SymbolKind::Variable,
                        source: SourceInfo::new(Some(source_id), Some(*range)),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
            });
        }
    }

    fn add_include_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Include,
            ..
        } = &**node
        {
            let _ = node.children_without_token().first().map(|child| {
                let module_name = child.name().unwrap();
                let module_path = self.module_loader.get_module_path(&module_name);

                if let Ok(url) = Url::parse(&format!("file:///{}", module_path.unwrap_or(module_name.to_string()))) {
                    let code = self.module_loader.resolve(&module_name);
                    let (module_source_id, _) = self.add_code(Some(url), &code.unwrap_or_default());

                    self.add_symbol(Symbol {
                        value: Some(module_name.clone()),
                        kind: SymbolKind::Include(module_source_id),
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
            });
        }
    }

    fn add_import_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Import,
            ..
        } = &**node
        {
            let _ = node.children_without_token().first().map(|child| {
                let module_name = child.name().unwrap();
                let module_path = self.module_loader.get_module_path(&module_name);

                if let Ok(url) = Url::parse(&format!("file:///{}", module_path.unwrap_or(module_name.to_string()))) {
                    let code = self.module_loader.resolve(&module_name);
                    let (module_source_id, _) = self.add_code(Some(url), &code.unwrap_or_default());

                    self.add_symbol(Symbol {
                        value: Some(module_name.clone()),
                        kind: SymbolKind::Import(module_source_id),
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
            });
        }
    }

    fn add_module_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Module,
            ..
        } = &**node
        {
            let children = node.children_without_token();

            // Get the module name from the first child
            let module_name = children.first().and_then(|n| n.name());

            // First child is the module name - register as Ident
            if let Some(module_name_node) = children.first() {
                self.add_symbol(Symbol {
                    value: module_name_node.name(),
                    kind: SymbolKind::Ident,
                    source: SourceInfo::new(Some(source_id), Some(module_name_node.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent,
                    insertion_order: 0,
                });
            }

            let symbol_id = self.add_symbol(Symbol {
                value: module_name,
                kind: SymbolKind::Module(source_id),
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Process remaining child nodes (module body)
            for child in children.iter().skip(1) {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_while_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::While,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::While,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
            let loop_scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Loop(symbol_id),
                Some(scope_id),
            ));

            node.children_without_token().iter().for_each(|child| {
                self.add_expr(child, source_id, loop_scope_id, Some(symbol_id));
            });
        }
    }

    fn add_loop_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Loop,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Loop,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
            let loop_scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Loop(symbol_id),
                Some(scope_id),
            ));

            node.children_without_token().iter().for_each(|child| {
                self.add_expr(child, source_id, loop_scope_id, Some(symbol_id));
            });
        }
    }

    fn add_var_decl(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if matches!(node.kind, mq_lang::CstNodeKind::Let | mq_lang::CstNodeKind::Var) {
            let _keyword_id = self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let children = node.children_without_token();
            let lhs = children.first().unwrap();

            if matches!(lhs.kind, mq_lang::CstNodeKind::Pattern) {
                // Destructuring pattern: create a DestructuringBinding symbol (sibling to the
                // Keyword, same as Variable for simple let) that owns PatternVariable children
                // and the initializer, so piped-input propagation and type constraints can
                // treat it identically to Variable.
                let destructuring_id = self.insert_symbol(Symbol {
                    value: None,
                    kind: SymbolKind::DestructuringBinding,
                    source: SourceInfo::new(Some(source_id), Some(lhs.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent,
                    insertion_order: 0,
                });
                self.add_pattern_expr(lhs, source_id, scope_id, Some(destructuring_id));
                children.iter().skip(1).for_each(|child| {
                    self.add_expr(child, source_id, scope_id, Some(destructuring_id));
                });
            } else {
                // Simple identifier: create a single Variable symbol
                let symbol_id = self.insert_symbol(Symbol {
                    value: lhs.name(),
                    kind: SymbolKind::Variable,
                    source: SourceInfo::new(Some(source_id), Some(lhs.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent,
                    insertion_order: 0,
                });

                children.iter().skip(1).for_each(|child| {
                    self.add_expr(child, source_id, scope_id, Some(symbol_id));
                });
            }
        }
    }

    fn add_ident_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Ident,
            ..
        } = &**node
        {
            let symbol_id = self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Ref,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Process Selector children, e.g. `md.depth` is an Ident(md) with a Selector(.depth) child.
            for child in node.children_without_token() {
                if matches!(child.kind, mq_lang::CstNodeKind::Selector) {
                    self.add_selector_expr(&child, source_id, scope_id, Some(symbol_id));
                }
            }
        }
    }

    fn add_selector_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Selector,
            ..
        } = &**node
            && let Some(selector) = selector_from_cst_node(node)
        {
            let symbol_id = self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Selector(selector),
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_if_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::If,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::If,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
            let if_scope = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Block(symbol_id),
                Some(scope_id),
            ));

            if let [cond, then_expr, rest @ ..] = node.children_without_token().as_slice() {
                self.add_expr(cond, source_id, if_scope, Some(symbol_id));
                self.add_expr(then_expr, source_id, if_scope, Some(symbol_id));

                for child in rest {
                    self.add_elif_expr(child, source_id, scope_id, Some(symbol_id));
                    self.add_else_expr(child, source_id, scope_id, Some(symbol_id));
                }
            }
        }
    }

    fn add_elif_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Elif,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Elif,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
            let elif_scope = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Block(symbol_id),
                Some(scope_id),
            ));

            if let [cond, then_expr] = node.children_without_token().as_slice() {
                self.add_expr(cond, source_id, elif_scope, Some(symbol_id));
                self.add_expr(then_expr, source_id, elif_scope, Some(symbol_id));
            }
        }
    }

    fn add_else_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Else,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Else,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
            let elif_scope = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Block(symbol_id),
                Some(scope_id),
            ));

            if let [then_expr] = node.children_without_token().as_slice() {
                self.add_expr(then_expr, source_id, elif_scope, Some(symbol_id));
            }
        }
    }

    fn add_call_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Call,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Call,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            node.children_without_token().iter().for_each(|child| {
                // Process all arguments recursively to handle complex expressions
                // This ensures that identifiers inside bracket access (e.g., vars in vars["x"])
                // are properly registered as Ref symbols that can be resolved
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        }
    }

    fn add_call_dynamic_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::CallDynamic,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: None, // Dynamic calls don't have a static name
                kind: SymbolKind::CallDynamic,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Process all children (callable expression and arguments)
            let children = node.children_without_token();

            // First child is the callable expression (e.g., arr[0])
            if let Some(callable) = children.first() {
                self.add_expr(callable, source_id, scope_id, Some(symbol_id));
            }

            // Remaining children are arguments - process them recursively
            for child in children.iter().skip(1) {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_foreach_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Foreach,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Foreach,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Loop(symbol_id),
                Some(scope_id),
            ));
            let (params, program) = node.split_cond_and_program();
            let loop_val = params.first().unwrap();
            let arg = params.get(1).unwrap();

            self.add_symbol(Symbol {
                value: loop_val.name(),
                kind: SymbolKind::Variable,
                source: SourceInfo::new(Some(source_id), Some(loop_val.range())),
                scope: scope_id,
                doc: node.comments(),
                parent: Some(symbol_id),
                insertion_order: 0,
            });

            self.add_expr(arg, source_id, scope_id, Some(symbol_id));

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_def_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Def,
            ..
        } = &**node
        {
            self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let (params, program) = node.split_cond_and_program();
            let ident = params.first().unwrap();

            let symbol_id = self.add_symbol(Symbol {
                value: ident.name(),
                kind: SymbolKind::Function(Vec::new()),
                source: SourceInfo::new(Some(source_id), Some(ident.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Function(symbol_id),
                Some(scope_id),
            ));

            let mut param_info = Vec::with_capacity(params.len().saturating_sub(1));

            // For def expressions, the first param is the function name, so skip it
            params.iter().skip(1).for_each(|child| {
                // Check if parameter has default value
                // In CST, param with default has children: ident, '=', default_expr
                let has_default = child.children.len() > 1;
                // Check if parameter is variadic
                // In CST, variadic param has exactly 1 child with Asterisk token
                let is_variadic = child.children.len() == 1
                    && child.children[0]
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::Asterisk));
                let param_name = child.name().unwrap_or("arg".into());

                param_info.push(ParamInfo {
                    name: param_name.clone(),
                    has_default,
                    is_variadic,
                });

                self.add_symbol(Symbol {
                    value: Some(param_name),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });

                // If has default, also analyze the default expression
                if has_default && child.children.len() >= 3 {
                    let default_expr = &child.children[2];
                    self.add_expr(default_expr, source_id, scope_id, Some(symbol_id));
                }
            });

            self.symbols[symbol_id].kind = SymbolKind::Function(param_info);

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_macro_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Macro,
            ..
        } = &**node
        {
            self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let (params, program) = node.split_cond_and_program();
            let ident = params.first().unwrap();

            let symbol_id = self.add_symbol(Symbol {
                value: ident.name(),
                kind: SymbolKind::Macro(Vec::new()),
                source: SourceInfo::new(Some(source_id), Some(ident.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Function(symbol_id),
                Some(scope_id),
            ));

            let mut param_info = Vec::with_capacity(params.len().saturating_sub(1));

            // For macro expressions, the first param is the macro name, so skip it
            params.iter().skip(1).for_each(|child| {
                // Macros should not have defaults, but we still need to store param info
                let has_default = child.children.len() > 1;
                let is_variadic = child.children.len() == 1
                    && child.children[0]
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::Asterisk));
                let param_name = child.name().unwrap_or("arg".into());

                param_info.push(ParamInfo {
                    name: param_name.clone(),
                    has_default,
                    is_variadic,
                });

                self.add_symbol(Symbol {
                    value: Some(param_name),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });
            });

            self.symbols[symbol_id].kind = SymbolKind::Macro(param_info);

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_macro_call_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::MacroCall,
            ..
        } = &**node
        {
            // Add the macro call as a regular call symbol
            self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Call,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Process all children (macro arguments and program body)
            node.children_without_token().iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, parent);
            });
        } else {
            unreachable!()
        }
    }

    fn add_fn_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Fn,
            ..
        } = &**node
        {
            self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let (params, program) = node.split_cond_and_program();
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::Function(Vec::new()),
                source: SourceInfo::new(Some(source_id), None),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Function(symbol_id),
                Some(scope_id),
            ));

            let mut param_info = Vec::with_capacity(params.len());

            params.iter().for_each(|child| {
                // Check if parameter has default value
                // In CST, param with default has children: ident, '=', default_expr
                let has_default = child.children.len() > 1;
                // Check if parameter is variadic
                let is_variadic = child.children.len() == 1
                    && child.children[0]
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::Asterisk));
                let param_name = child.name().unwrap_or("arg".into());

                param_info.push(crate::symbol::ParamInfo {
                    name: param_name.clone(),
                    has_default,
                    is_variadic,
                });

                self.add_symbol(Symbol {
                    value: Some(param_name),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });

                // If has default, also analyze the default expression
                if has_default && child.children.len() >= 3 {
                    let default_expr = &child.children[2];
                    self.add_expr(default_expr, source_id, scope_id, Some(symbol_id));
                }
            });

            self.symbols[symbol_id].kind = SymbolKind::Function(param_info);

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_dict_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Dict,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Dict,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            for entry in node.children_without_token() {
                if let (Some(key_node), Some(value_node)) = (entry.children.first(), entry.children.get(2)) {
                    let key_symbol_id = self.add_symbol(Symbol {
                        value: key_node.name(),
                        kind: match &key_node.token {
                            Some(token) => match &token.kind {
                                mq_lang::TokenKind::StringLiteral(_) => SymbolKind::String,
                                mq_lang::TokenKind::Ident(_) => SymbolKind::Symbol,
                                _ => SymbolKind::Symbol,
                            },
                            None => SymbolKind::Symbol,
                        },
                        source: SourceInfo::new(Some(source_id), Some(key_node.range())),
                        scope: scope_id,
                        doc: key_node.comments(),
                        parent: Some(symbol_id),
                        insertion_order: 0,
                    });

                    self.add_expr(value_node, source_id, scope_id, Some(key_symbol_id));
                } else {
                    unreachable!()
                }
            }
        }
    }

    fn add_match_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Match,
            ..
        } = &**node
        {
            // Create Match symbol
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Match,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let children = node.children_without_token();

            // Process the value expression (first child: match (value))
            if let Some(value_expr) = children.first() {
                // Skip MatchArm nodes when looking for the value expression
                if !matches!(value_expr.kind, mq_lang::CstNodeKind::MatchArm) {
                    self.add_expr(value_expr, source_id, scope_id, Some(symbol_id));
                }
            }

            // Process each MatchArm
            for child in children.iter() {
                if matches!(child.kind, mq_lang::CstNodeKind::MatchArm) {
                    self.add_match_arm_expr(child, source_id, scope_id, Some(symbol_id));
                }
            }
        }
    }

    fn add_keyword(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        self.add_symbol(Symbol {
            value: node.name(),
            kind: SymbolKind::Keyword,
            source: SourceInfo::new(Some(source_id), Some(node.range())),
            scope: scope_id,
            doc: node.comments(),
            parent,
            insertion_order: 0,
        });
    }

    /// Adds a `break` expression to the HIR.
    ///
    /// Unlike bare keywords, `break` may carry a value (`break: expr`).
    /// The value expression is added as a child of the break symbol so that
    /// the type checker can infer the break's type and propagate it to the
    /// enclosing loop as part of a union type.
    fn add_break_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        let symbol_id = self.add_symbol(Symbol {
            value: node.name(),
            kind: SymbolKind::Keyword,
            source: SourceInfo::new(Some(source_id), Some(node.range())),
            scope: scope_id,
            doc: node.comments(),
            parent,
            insertion_order: 0,
        });
        // Process break value expression (if present) as a child of this symbol.
        for child in node.children_without_token() {
            self.add_expr(&child, source_id, scope_id, Some(symbol_id));
        }
    }

    fn add_pattern_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        self.add_pattern_expr_inner(node, source_id, scope_id, parent, false, None);
    }

    fn add_pattern_expr_inner(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
        is_rest: bool,
        dict_key: Option<smol_str::SmolStr>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::OrPattern,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::Pattern { is_dict: false },
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            for child in node.children_without_token() {
                if matches!(
                    child.kind,
                    mq_lang::CstNodeKind::Pattern | mq_lang::CstNodeKind::OrPattern
                ) {
                    self.add_pattern_expr_inner(&child, source_id, scope_id, Some(symbol_id), false, None);
                }
            }
            return;
        }

        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Pattern,
            ..
        } = &**node
        {
            let is_dict_pattern = node.children.iter().any(|child| {
                child.is_token()
                    && child
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::LBrace))
            });

            let symbol_id = self.add_symbol(Symbol {
                value: dict_key.or_else(|| node.name()),
                kind: SymbolKind::Pattern {
                    is_dict: is_dict_pattern,
                },
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Extract pattern variables and add them to the scope.
            // Pass `is_rest` so the rest binding (`..rest`) gets the correct kind.
            self.extract_pattern_variables(node, source_id, scope_id, Some(symbol_id), is_rest);

            let has_rest_element = node.children.iter().any(|child| {
                child.is_token()
                    && child
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::DoubleDot))
            });

            // Process nested patterns (for array, dict patterns)
            let non_token_children = node.children_without_token();
            let last_pattern_idx = if has_rest_element {
                non_token_children
                    .iter()
                    .rposition(|c| matches!(c.kind, mq_lang::CstNodeKind::Pattern | mq_lang::CstNodeKind::OrPattern))
            } else {
                None
            };
            let mut pattern_idx = 0;
            let mut idx = 0;
            while idx < non_token_children.len() {
                let child = &non_token_children[idx];
                if matches!(
                    child.kind,
                    mq_lang::CstNodeKind::Pattern | mq_lang::CstNodeKind::OrPattern
                ) {
                    let child_is_rest = last_pattern_idx == Some(pattern_idx);
                    self.add_pattern_expr_inner(child, source_id, scope_id, Some(symbol_id), child_is_rest, None);
                    pattern_idx += 1;
                } else if matches!(child.kind, mq_lang::CstNodeKind::Ident) {
                    if is_dict_pattern {
                        // In dict patterns `{a, b}` shorthand, an Ident NOT followed by a
                        // Pattern sibling is both the key name and the binding variable.
                        // In `{a: pattern}`, the Ident is the dict key and the following
                        // Pattern carries the binding. Process the pair together here so
                        // the key name can be stored in the inner Pattern's `value` field,
                        // enabling constraint generation to map the binding to its field type.
                        let next = non_token_children.get(idx + 1);
                        let next_is_pattern = next.is_some_and(|c| {
                            matches!(c.kind, mq_lang::CstNodeKind::Pattern | mq_lang::CstNodeKind::OrPattern)
                        });
                        if !next_is_pattern {
                            self.add_symbol(Symbol {
                                value: child.name(),
                                kind: SymbolKind::PatternVariable { is_rest: false },
                                source: SourceInfo::new(Some(source_id), Some(child.range())),
                                scope: scope_id,
                                doc: child.comments(),
                                parent: Some(symbol_id),
                                insertion_order: 0,
                            });
                        } else {
                            // Explicit `{key: pattern}`: pass the key name so the inner
                            // Pattern symbol stores it in its `value` field.
                            let inner = next.unwrap();
                            self.add_pattern_expr_inner(
                                inner,
                                source_id,
                                scope_id,
                                Some(symbol_id),
                                false,
                                child.name(),
                            );
                            idx += 1; // skip the inner Pattern on the next iteration
                            pattern_idx += 1;
                        }
                    } else {
                        // Ident nodes in non-dict patterns are symbol literal names (:foo -> foo)
                        self.add_symbol(Symbol {
                            value: child.name(),
                            kind: SymbolKind::Symbol,
                            source: SourceInfo::new(Some(source_id), Some(child.range())),
                            scope: scope_id,
                            doc: child.comments(),
                            parent: Some(symbol_id),
                            insertion_order: 0,
                        });
                    }
                } else {
                    // Process other expressions in the pattern (e.g., literals, guard conditions)
                    self.add_expr(child, source_id, scope_id, Some(symbol_id));
                }
                idx += 1;
            }
        }
    }

    fn add_quote_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Quote,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Process children (quoted expressions)
            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_unquote_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Unquote,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Process children (unquoted expressions)
            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_match_arm_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::MatchArm,
            ..
        } = &**node
        {
            let children = node.children_without_token();

            // A guard is present when the arm has more than 2 non-token children:
            // [Pattern, guard_expr, body] vs [Pattern, body]
            let has_guard = children.len() > 2;

            // Create MatchArm symbol
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::MatchArm { has_guard },
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            // Create a dedicated scope for this MatchArm
            // Pattern variables will be visible in this scope
            let arm_scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::MatchArm(symbol_id),
                Some(scope_id),
            ));

            // Process pattern (first child after the pipe token)
            // The pattern introduces variables into the arm scope
            if let Some(pattern) = children.first()
                && matches!(
                    pattern.kind,
                    mq_lang::CstNodeKind::Pattern | mq_lang::CstNodeKind::OrPattern
                )
            {
                self.add_pattern_expr(pattern, source_id, arm_scope_id, Some(symbol_id));
            }

            // Process remaining children (guard and body)
            // These execute in the arm scope where pattern variables are visible
            for child in children.iter().skip(1) {
                self.add_expr(child, source_id, arm_scope_id, Some(symbol_id));
            }
        }
    }

    fn extract_pattern_variables(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
        is_rest: bool,
    ) {
        if let Some(token) = &node.token {
            match &token.kind {
                // Identifier pattern: introduces a variable binding
                mq_lang::TokenKind::Ident(name) if name != "_" => {
                    // Skip wildcards
                    self.add_symbol(Symbol {
                        value: Some(name.clone()),
                        kind: SymbolKind::PatternVariable { is_rest },
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
                // Literal patterns: create a literal child symbol for type checking
                mq_lang::TokenKind::StringLiteral(s) => {
                    self.add_symbol(Symbol {
                        value: Some(s.as_str().into()),
                        kind: SymbolKind::String,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
                mq_lang::TokenKind::NumberLiteral(n) => {
                    self.add_symbol(Symbol {
                        value: Some(n.to_string().into()),
                        kind: SymbolKind::Number,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
                mq_lang::TokenKind::BoolLiteral(b) => {
                    self.add_symbol(Symbol {
                        value: Some(b.to_string().into()),
                        kind: SymbolKind::Boolean,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
                mq_lang::TokenKind::None => {
                    self.add_symbol(Symbol {
                        value: Some("none".into()),
                        kind: SymbolKind::None,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
                _ => {
                    // For other token types (wildcards), no variable or literal is introduced
                }
            }
        }
    }
}
