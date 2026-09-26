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

    // `..` is tokenized as `DoubleDot`, not as `Selector(".."), so handle it explicitly.
    if matches!(token.kind, TokenKind::DoubleDot) {
        return Some(mq_lang::Selector::Recursive);
    }

    if !matches!(&token.kind, TokenKind::Selector(s) if s == ".") {
        return mq_lang::Selector::try_from(&**token).ok();
    }

    // Bracket-based selector: walk all children to count bracket pairs and
    // collect the optional number literal inside each pair.
    let mut bracket_pairs: u32 = 0;
    let mut indices: Vec<Option<usize>> = Vec::with_capacity(2);
    let mut in_bracket = false;
    let mut bracket_has_number = false;

    for child in node.children() {
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
                for child in node.non_token_children() {
                    self.add_expr(child, source_id, scope_id, Some(symbol_id));
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
            mq_lang::CstNodeKind::BinaryOp { .. } => {
                self.add_binary_op_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Block { .. } => {
                self.add_block_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::UnaryOp { .. } => {
                self.add_unary_op_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Call { .. } => {
                self.add_call_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::CallDynamic { .. } => {
                self.add_call_dynamic_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Def { .. } => {
                self.add_def_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Foreach { .. } => {
                self.add_foreach_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Fn { .. } => {
                self.add_fn_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Ident { .. } => {
                self.add_ident_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::If { .. } => {
                self.add_if_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Include { .. } => {
                self.add_include_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Import { .. } => {
                self.add_import_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Module { .. } => {
                self.add_module_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::QualifiedAccess { .. } => {
                self.add_qualified_access_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::InterpolatedString => {
                self.add_interpolated_string(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::As { .. } => {
                self.add_as_binding(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Let { .. } | mq_lang::CstNodeKind::Var { .. } => {
                self.add_var_decl(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Literal | mq_lang::CstNodeKind::Symbol { .. } => {
                self.add_literal_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Selector { .. } | mq_lang::CstNodeKind::SelfAttr => {
                self.add_selector_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::While { .. } => {
                self.add_while_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Until { .. } => {
                self.add_until_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Loop { .. } => {
                self.add_loop_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Unless { .. } => {
                self.add_unless_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Try { .. } => {
                self.add_try_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Catch { .. } => {
                self.add_catch_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Array { .. } => {
                self.add_array_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Dict { .. } => {
                self.add_dict_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Spread { .. } => {
                self.add_spread_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Match { .. } => {
                self.add_match_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::MatchArm { .. } => {
                self.add_match_arm_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Pattern { .. } => {
                self.add_pattern_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Break { .. } => {
                self.add_break_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Yield { .. } => {
                self.add_yield_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Self_ { .. }
            | mq_lang::CstNodeKind::Nodes
            | mq_lang::CstNodeKind::End
            | mq_lang::CstNodeKind::Continue => {
                self.add_keyword(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Assign { .. } => {
                self.add_assign_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Group { .. } => {
                for child in node.non_token_children() {
                    self.add_expr(child, source_id, scope_id, parent);
                }
            }

            _ => {}
        }
    }

    simple_expr!(add_assign_expr, mq_lang::CstNodeKind::Assign { .. }, SymbolKind::Assign);
    simple_expr!(
        add_binary_op_expr,
        mq_lang::CstNodeKind::BinaryOp { .. },
        SymbolKind::BinaryOp
    );
    simple_expr!(
        add_unary_op_expr,
        mq_lang::CstNodeKind::UnaryOp { .. },
        SymbolKind::UnaryOp
    );
    simple_expr!(
        add_qualified_access_expr,
        mq_lang::CstNodeKind::QualifiedAccess { .. },
        SymbolKind::QualifiedAccess
    );
    simple_expr!(add_try_expr, mq_lang::CstNodeKind::Try { .. }, SymbolKind::Try);
    simple_expr!(add_array_expr, mq_lang::CstNodeKind::Array { .. }, SymbolKind::Array);
    simple_expr!(add_spread_expr, mq_lang::CstNodeKind::Spread { .. }, SymbolKind::Spread);

    /// Lowers a `CstNodeKind::Catch { .. }` node into a `SymbolKind::Catch` symbol.
    ///
    /// `catch(e):` carries an optional error binder before the leading `(`,
    /// `Ident`, `)` tokens; when present, it declares `e` as a `Parameter`
    /// in a fresh child scope so the catch body can reference it without
    /// resolving to an unrelated same-named symbol in an enclosing scope.
    fn add_catch_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Catch { .. },
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Catch,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let mq_lang::CstNodeKind::Catch { params, body, .. } = &node.kind else {
                return;
            };
            let binder = params.iter().flatten().find(|child| !child.is_token());
            let has_binder = binder.is_some();

            let body_scope_id = if has_binder {
                self.add_scope(Scope::new(
                    SourceInfo::new(Some(source_id), Some(node.node_range())),
                    ScopeKind::Block(symbol_id),
                    Some(scope_id),
                ))
            } else {
                scope_id
            };

            if let Some(binder) = binder {
                self.add_symbol(Symbol {
                    value: binder.name(),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(binder.range())),
                    scope: body_scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });
            }

            self.add_expr(body, source_id, body_scope_id, Some(symbol_id));
        }
    }

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
            kind: mq_lang::CstNodeKind::Block { .. },
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
            node.children().for_each(|child| {
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
        if let mq_lang::CstNodeKind::Symbol { name, .. } = &node.kind {
            self.add_symbol(Symbol {
                value: name.name(),
                kind: SymbolKind::Symbol,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
        } else if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Literal,
            token: Some(token),
            ..
        } = &**node
        {
            self.add_symbol(Symbol {
                value: node.name(),
                kind: match &token.kind {
                    mq_lang::TokenKind::StringLiteral(_) => SymbolKind::String,
                    mq_lang::TokenKind::BytesLiteral(_) => SymbolKind::Bytes,
                    mq_lang::TokenKind::NumberLiteral(_) => SymbolKind::Number,
                    mq_lang::TokenKind::BoolLiteral(_) => SymbolKind::Boolean,
                    mq_lang::TokenKind::None => SymbolKind::None,
                    _ => unreachable!("Literal nodes should only have string, bytes, number, boolean, or none tokens"),
                },
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });
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
            // Wrap all segments under a single InterpolatedString symbol so that
            // the parent node (e.g. a Call) sees one argument per s-string, not
            // one child per text/expr segment.
            let interp_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::InterpolatedString,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            segments.iter().for_each(|segment| match segment {
                mq_lang::StringSegment::Text(text, range) => {
                    self.add_symbol(Symbol {
                        value: Some(text.into()),
                        kind: SymbolKind::String,
                        source: SourceInfo::new(Some(source_id), Some(*range)),
                        scope: scope_id,
                        doc: node.comments(),
                        parent: Some(interp_id),
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
                        parent: Some(interp_id),
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
        if let mq_lang::CstNodeKind::Include { path } = &node.kind
            && let Some(module_name) = path.name()
        {
            {
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
            }
        }
    }

    fn add_import_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Import { path, alias, .. } = &node.kind
            && let Some(module_name) = path.name()
        {
            {
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

                    // `import "path" as alias` also binds a plain Ident symbol for the
                    // alias, mirroring how `module` registers its name (see add_module_expr).
                    if let Some(alias_node) = alias {
                        self.add_symbol(Symbol {
                            value: alias_node.name(),
                            kind: SymbolKind::Ident,
                            source: SourceInfo::new(Some(source_id), Some(alias_node.range())),
                            scope: scope_id,
                            doc: node.comments(),
                            parent,
                            insertion_order: 0,
                        });
                    }
                }
            }
        }
    }

    fn add_module_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Module {
            name: module_name_node,
            program,
            ..
        } = &node.kind
        {
            // The module name is also registered as an Ident.
            self.add_symbol(Symbol {
                value: module_name_node.name(),
                kind: SymbolKind::Ident,
                source: SourceInfo::new(Some(source_id), Some(module_name_node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let symbol_id = self.add_symbol(Symbol {
                value: module_name_node.name(),
                kind: SymbolKind::Module(source_id),
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            for child in program.iter().filter(|child| !child.is_token()) {
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
            kind: mq_lang::CstNodeKind::While { .. },
            ..
        } = &**node
        {
            self.add_loop_expr_with_kind(node, source_id, scope_id, parent, SymbolKind::While);
        }
    }

    /// Shared lowering for `while`/`until`: both create a loop-scoped symbol and lower
    /// every child (condition, then body) into that scope; only the resulting `SymbolKind`
    /// differs.
    fn add_loop_expr_with_kind(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
        symbol_kind: SymbolKind,
    ) {
        let symbol_id = self.add_symbol(Symbol {
            value: node.name(),
            kind: symbol_kind,
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

        node.non_token_children().for_each(|child| {
            self.add_expr(child, source_id, loop_scope_id, Some(symbol_id));
        });
    }

    fn add_loop_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Loop { .. },
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

            node.non_token_children().for_each(|child| {
                self.add_expr(child, source_id, loop_scope_id, Some(symbol_id));
            });
        }
    }

    fn add_until_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Until { .. },
            ..
        } = &**node
        {
            self.add_loop_expr_with_kind(node, source_id, scope_id, parent, SymbolKind::Until);
        }
    }

    fn add_var_decl(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Let { lhs, rhs, .. } | mq_lang::CstNodeKind::Var { lhs, rhs, .. } = &node.kind {
            let _keyword_id = self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            if matches!(lhs.kind, mq_lang::CstNodeKind::Pattern { .. }) {
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
                self.add_expr(rhs, source_id, scope_id, Some(destructuring_id));
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

                self.add_expr(rhs, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_as_binding(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::As { expr, name: name_node } = &node.kind {
            let _keyword_id = self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            {
                let symbol_id = self.insert_symbol(Symbol {
                    value: name_node.name(),
                    kind: SymbolKind::Variable,
                    source: SourceInfo::new(Some(source_id), Some(name_node.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent,
                    insertion_order: 0,
                });

                self.add_expr(expr, source_id, scope_id, Some(symbol_id));
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
            kind: mq_lang::CstNodeKind::Ident { .. },
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
            for child in node.non_token_children() {
                if matches!(child.kind, mq_lang::CstNodeKind::Selector { .. }) {
                    self.add_selector_expr(child, source_id, scope_id, Some(symbol_id));
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
            kind: mq_lang::CstNodeKind::Selector { .. } | mq_lang::CstNodeKind::SelfAttr,
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

            for child in node.non_token_children() {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
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
        if let mq_lang::CstNodeKind::If {
            args,
            then_branch,
            elifs,
            else_branch,
            ..
        } = &node.kind
        {
            let (symbol_id, if_scope) = self.add_conditional_symbol(node, source_id, scope_id, parent, SymbolKind::If);

            self.add_cond_and_then(args, then_branch, source_id, if_scope, symbol_id);
            for elif in elifs {
                self.add_elif_expr(elif, source_id, scope_id, Some(symbol_id));
            }
            if let Some(else_branch) = else_branch {
                self.add_else_expr(else_branch, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_cond_and_then(
        &mut self,
        args: &[mq_lang::Shared<mq_lang::CstNode>],
        then_branch: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        symbol_id: SymbolId,
    ) {
        for cond in args.iter().filter(|arg| !arg.is_token()) {
            self.add_expr(cond, source_id, scope_id, Some(symbol_id));
        }
        self.add_expr(then_branch, source_id, scope_id, Some(symbol_id));
    }

    /// Shared lowering for `if`/`unless`: both create a block-scoped symbol for the
    /// condition/body pair; `if` additionally walks `elif`/`else` siblings, `unless` doesn't.
    fn add_conditional_symbol(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
        symbol_kind: SymbolKind,
    ) -> (SymbolId, ScopeId) {
        let symbol_id = self.add_symbol(Symbol {
            value: node.name(),
            kind: symbol_kind,
            source: SourceInfo::new(Some(source_id), Some(node.range())),
            scope: scope_id,
            doc: node.comments(),
            parent,
            insertion_order: 0,
        });
        let cond_scope = self.add_scope(Scope::new(
            SourceInfo::new(Some(source_id), Some(node.node_range())),
            ScopeKind::Block(symbol_id),
            Some(scope_id),
        ));

        (symbol_id, cond_scope)
    }

    fn add_unless_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Unless { args, then_branch, .. } = &node.kind {
            let (symbol_id, unless_scope) =
                self.add_conditional_symbol(node, source_id, scope_id, parent, SymbolKind::Unless);
            self.add_cond_and_then(args, then_branch, source_id, unless_scope, symbol_id);
        }
    }

    fn add_elif_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Elif { args, then_branch, .. } = &node.kind {
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

            self.add_cond_and_then(args, then_branch, source_id, elif_scope, symbol_id);
        }
    }

    fn add_else_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Else { then_branch, .. } = &node.kind {
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

            self.add_expr(then_branch, source_id, elif_scope, Some(symbol_id));
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
            kind: mq_lang::CstNodeKind::Call { .. },
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

            node.non_token_children().for_each(|child| {
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
            kind: mq_lang::CstNodeKind::CallDynamic { .. },
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

            // The callable expression (e.g. `arr[0]`), then its arguments.
            for child in node.non_token_children() {
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
        if let mq_lang::CstNodeKind::Foreach { args, program, .. } = &node.kind {
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
            let mut params = args.iter().filter(|arg| !arg.is_token());

            if let Some(loop_val) = params.next() {
                self.add_symbol(Symbol {
                    value: loop_val.name(),
                    kind: SymbolKind::Variable,
                    source: SourceInfo::new(Some(source_id), Some(loop_val.range())),
                    scope: scope_id,
                    doc: node.comments(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });
            }
            if let Some(arg) = params.next() {
                self.add_expr(arg, source_id, scope_id, Some(symbol_id));
            }

            program.iter().filter(|child| !child.is_token()).for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!("add_foreach_expr should only be called on Foreach nodes");
        }
    }

    fn add_def_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Def {
            name: ident,
            params,
            program,
            ..
        } = &node.kind
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

            let params = || params.iter().flatten().filter(|param| !param.is_token());

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

            let mut param_info = Vec::with_capacity(params().count());

            params().for_each(|child| {
                let (is_variadic, default_expr) = match &child.kind {
                    mq_lang::CstNodeKind::Param { asterisk, default, .. } => (asterisk.is_some(), default.as_ref()),
                    _ => (false, None),
                };
                let has_default = default_expr.is_some();
                let param_name = child.name().unwrap_or("arg".into());

                param_info.push(ParamInfo {
                    name: param_name.clone(),
                    has_default,
                    is_variadic,
                });

                let param_symbol_id = self.add_symbol(Symbol {
                    value: Some(param_name),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });

                // The default expression gets its own scope: it runs before the function body
                // starts, so `yield` isn't valid there even though earlier params/outer names
                // still resolve (its scope's parent is the function scope).
                if let Some(default_expr) = default_expr {
                    let default_scope_id = self.add_scope(Scope::new(
                        SourceInfo::new(Some(source_id), Some(default_expr.range())),
                        ScopeKind::DefaultParam(param_symbol_id),
                        Some(scope_id),
                    ));
                    self.add_expr(default_expr, source_id, default_scope_id, Some(symbol_id));
                }
            });

            self.symbols[symbol_id].kind = SymbolKind::Function(param_info);

            program.iter().filter(|child| !child.is_token()).for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!("add_def_expr should only be called on Def nodes");
        }
    }

    fn add_fn_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::Fn { params, program, .. } = &node.kind {
            self.insert_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
                insertion_order: 0,
            });

            let params = || params.iter().filter(|param| !param.is_token());
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

            let mut param_info = Vec::with_capacity(params().count());

            params().for_each(|child| {
                let (is_variadic, default_expr) = match &child.kind {
                    mq_lang::CstNodeKind::Param { asterisk, default, .. } => (asterisk.is_some(), default.as_ref()),
                    _ => (false, None),
                };
                let has_default = default_expr.is_some();
                let param_name = child.name().unwrap_or("arg".into());

                param_info.push(crate::symbol::ParamInfo {
                    name: param_name.clone(),
                    has_default,
                    is_variadic,
                });

                let param_symbol_id = self.add_symbol(Symbol {
                    value: Some(param_name),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
                    insertion_order: 0,
                });

                // The default expression gets its own scope: it runs before the function body
                // starts, so `yield` isn't valid there even though earlier params/outer names
                // still resolve (its scope's parent is the function scope).
                if let Some(default_expr) = default_expr {
                    let default_scope_id = self.add_scope(Scope::new(
                        SourceInfo::new(Some(source_id), Some(default_expr.range())),
                        ScopeKind::DefaultParam(param_symbol_id),
                        Some(scope_id),
                    ));
                    self.add_expr(default_expr, source_id, default_scope_id, Some(symbol_id));
                }
            });

            self.symbols[symbol_id].kind = SymbolKind::Function(param_info);

            program.iter().filter(|child| !child.is_token()).for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!("add_fn_expr should only be called on Fn nodes");
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
            kind: mq_lang::CstNodeKind::Dict { .. },
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

            for entry in node.non_token_children() {
                if matches!(entry.kind, mq_lang::CstNodeKind::Spread { .. }) {
                    self.add_spread_expr(entry, source_id, scope_id, Some(symbol_id));
                } else if let mq_lang::CstNodeKind::DictEntry {
                    key: key_node,
                    value: value_node,
                    ..
                } = &entry.kind
                {
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
                    unreachable!("Dict entry does not have expected structure of key ':' value");
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
        if let mq_lang::CstNodeKind::Match { args, arms, .. } = &node.kind {
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

            if let Some(value_expr) = args.iter().find(|arg| !arg.is_token()) {
                self.add_expr(value_expr, source_id, scope_id, Some(symbol_id));
            }

            for arm in arms {
                self.add_match_arm_expr(arm, source_id, scope_id, Some(symbol_id));
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
        for child in node.non_token_children() {
            self.add_expr(child, source_id, scope_id, Some(symbol_id));
        }
    }

    /// Mirrors `add_break_expr`.
    fn add_yield_expr(
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
        for child in node.non_token_children() {
            self.add_expr(child, source_id, scope_id, Some(symbol_id));
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
            kind: mq_lang::CstNodeKind::OrPattern { .. },
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

            for child in node.non_token_children() {
                if matches!(
                    child.kind,
                    mq_lang::CstNodeKind::Pattern { .. } | mq_lang::CstNodeKind::OrPattern { .. }
                ) {
                    self.add_pattern_expr_inner(child, source_id, scope_id, Some(symbol_id), false, None);
                }
            }
            return;
        }

        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Pattern { .. },
            ..
        } = &**node
        {
            let is_dict_pattern = node.children().any(|child| {
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

            let has_rest_element = node.children().any(|child| {
                child.is_token()
                    && child
                        .token
                        .as_ref()
                        .is_some_and(|t| matches!(t.kind, mq_lang::TokenKind::DoubleDot))
            });

            // Process nested patterns (for array, dict patterns)
            let non_token_children = node.children_without_token();
            let last_pattern_idx = if has_rest_element {
                non_token_children.iter().rposition(|c| {
                    matches!(
                        c.kind,
                        mq_lang::CstNodeKind::Pattern { .. } | mq_lang::CstNodeKind::OrPattern { .. }
                    )
                })
            } else {
                None
            };
            let mut pattern_idx = 0;
            let mut idx = 0;
            while idx < non_token_children.len() {
                let child = &non_token_children[idx];
                if matches!(
                    child.kind,
                    mq_lang::CstNodeKind::Pattern { .. } | mq_lang::CstNodeKind::OrPattern { .. }
                ) {
                    let child_is_rest = last_pattern_idx == Some(pattern_idx);
                    self.add_pattern_expr_inner(child, source_id, scope_id, Some(symbol_id), child_is_rest, None);
                    pattern_idx += 1;
                } else if matches!(child.kind, mq_lang::CstNodeKind::Ident { .. }) {
                    if is_dict_pattern {
                        // In dict patterns `{a, b}` shorthand, an Ident NOT followed by a
                        // Pattern sibling is both the key name and the binding variable.
                        // In `{a: pattern}`, the Ident is the dict key and the following
                        // Pattern carries the binding. Process the pair together here so
                        // the key name can be stored in the inner Pattern's `value` field,
                        // enabling constraint generation to map the binding to its field type.
                        let next = non_token_children.get(idx + 1);
                        let next_is_pattern = next.is_some_and(|c| {
                            matches!(
                                c.kind,
                                mq_lang::CstNodeKind::Pattern { .. } | mq_lang::CstNodeKind::OrPattern { .. }
                            )
                        });
                        if let Some(inner) = next.filter(|_| next_is_pattern) {
                            // Explicit `{key: pattern}`: pass the key name so the inner
                            // Pattern symbol stores it in its `value` field.
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
                        } else {
                            self.add_symbol(Symbol {
                                value: child.name(),
                                kind: SymbolKind::PatternVariable { is_rest: false },
                                source: SourceInfo::new(Some(source_id), Some(child.range())),
                                scope: scope_id,
                                doc: child.comments(),
                                parent: Some(symbol_id),
                                insertion_order: 0,
                            });
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

    fn add_match_arm_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNodeKind::MatchArm {
            pattern,
            guard_args,
            body,
            ..
        } = &node.kind
        {
            let has_guard = guard_args.iter().flatten().any(|arg| !arg.is_token());

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

            // The pattern binds variables visible to the guard and body in the arm scope.
            if matches!(
                pattern.kind,
                mq_lang::CstNodeKind::Pattern { .. } | mq_lang::CstNodeKind::OrPattern { .. }
            ) {
                self.add_pattern_expr(pattern, source_id, arm_scope_id, Some(symbol_id));
            }

            for child in guard_args.iter().flatten().filter(|arg| !arg.is_token()) {
                self.add_expr(child, source_id, arm_scope_id, Some(symbol_id));
            }
            self.add_expr(body, source_id, arm_scope_id, Some(symbol_id));
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
                mq_lang::TokenKind::BytesLiteral(_) => {
                    self.add_symbol(Symbol {
                        value: Some(token.to_string().into()),
                        kind: SymbolKind::Bytes,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                        insertion_order: 0,
                    });
                }
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

#[cfg(test)]
mod tests {
    use std::mem::discriminant;

    use rstest::rstest;

    use crate::{Hir, Symbol, SymbolKind, symbol::ParamInfo};

    fn lower(code: &str) -> Hir {
        let (_, errors) = mq_lang::parse_recovery(code);
        assert!(!errors.has_errors(), "{code}: {errors}");
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir.add_code(None, code);
        hir
    }

    fn find(hir: &Hir, pred: impl Fn(&Symbol) -> bool) -> Vec<(crate::SymbolId, &Symbol)> {
        hir.symbols().filter(|(_, s)| pred(s)).collect()
    }

    fn parent_kind(hir: &Hir, symbol: &Symbol) -> Option<SymbolKind> {
        symbol.parent.map(|id| hir.symbols[id].kind.clone())
    }

    #[rstest]
    #[case::if_elif_else("let a = 1 | if (a): a elif (a): a else: a", "a", 5, SymbolKind::Variable)]
    #[case::unless("let a = 1 | unless (a): a", "a", 2, SymbolKind::Variable)]
    #[case::catch_binder("try: error(\"x\") catch(e): e", "e", 1, SymbolKind::Parameter)]
    #[case::catch_binder_shadows_outer("let e = 1 | try: 1 catch(e): e", "e", 1, SymbolKind::Parameter)]
    #[case::match_guard("match (1): | x if (x > 0): x | _: 0 end", "x", 2, SymbolKind::PatternVariable { is_rest: false })]
    #[case::foreach_loop_var("foreach (v, [1, 2]): v;", "v", 1, SymbolKind::Variable)]
    #[case::foreach_iterable("let xs = [1] | foreach (v, xs): v;", "xs", 1, SymbolKind::Variable)]
    #[case::def_params("def f(a, b = 1): a + b;", "a", 1, SymbolKind::Parameter)]
    #[case::def_default_param("def f(a, b = 1): a + b;", "b", 1, SymbolKind::Parameter)]
    #[case::fn_params("let g = fn(x, y): x + y; | g(1, 2)", "y", 1, SymbolKind::Parameter)]
    #[case::fn_call("let g = fn(x): x; | g(1)", "g", 1, SymbolKind::Variable)]
    #[case::as_binding("1 as n | n", "n", 1, SymbolKind::Variable)]
    #[case::destructuring_let("let [p, q] = [1, 2] | p + q", "q", 1, SymbolKind::PatternVariable { is_rest: false })]
    #[case::call_dynamic("let fs = [fn(x): x;] | fs[0](1)", "fs", 1, SymbolKind::Variable)]
    #[case::module_body("module m: let a = 1 | a end", "a", 1, SymbolKind::Variable)]
    #[case::let_rhs("let a = 1 | let b = a + 1 | b", "a", 1, SymbolKind::Variable)]
    fn test_refs_resolve_to_expected_kind(
        #[case] code: &str,
        #[case] name: &str,
        #[case] expected_refs: usize,
        #[case] expected: SymbolKind,
    ) {
        let hir = lower(code);
        let refs = find(&hir, |s| {
            s.value.as_deref() == Some(name) && matches!(s.kind, SymbolKind::Ref | SymbolKind::Call)
        });

        assert_eq!(refs.len(), expected_refs, "{code}");
        for (ref_id, _) in refs {
            let target = hir
                .resolve_reference_symbol(ref_id)
                .map(|id| hir.symbols[id].kind.clone());
            assert!(
                target
                    .as_ref()
                    .is_some_and(|t| discriminant(t) == discriminant(&expected)),
                "{code}: `{name}` resolved to {target:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn test_elif_and_else_are_children_of_if() {
        let hir = lower("if (1): 1 elif (2): 2 elif (3): 3 else: 4");
        let elifs = find(&hir, |s| matches!(s.kind, SymbolKind::Elif));
        let elses = find(&hir, |s| matches!(s.kind, SymbolKind::Else));

        assert_eq!((elifs.len(), elses.len()), (2, 1));
        for (_, symbol) in elifs.iter().chain(&elses) {
            assert_eq!(parent_kind(&hir, symbol), Some(SymbolKind::If));
        }
        let numbers = find(&hir, |s| matches!(s.kind, SymbolKind::Number));
        assert_eq!(numbers.len(), 7);
    }

    #[rstest]
    #[case::with_guard("match (1): | x if (x > 0): x end", vec![true])]
    #[case::without_guard("match (1): | x: x | _: 0 end", vec![false, false])]
    #[case::mixed("match (1): | 1: :one | x if (x > 1): x | _: 0 end", vec![false, true, false])]
    fn test_match_arm_guard_flag(#[case] code: &str, #[case] expected: Vec<bool>) {
        let hir = lower(code);
        let mut arms: Vec<_> = find(&hir, |s| matches!(s.kind, SymbolKind::MatchArm { .. }))
            .into_iter()
            .map(|(_, s)| (s.insertion_order, s.kind.clone()))
            .collect();
        arms.sort_by_key(|(order, _)| *order);

        let flags: Vec<bool> = arms
            .into_iter()
            .map(|(_, kind)| matches!(kind, SymbolKind::MatchArm { has_guard: true }))
            .collect();
        assert_eq!(flags, expected);
    }

    #[rstest]
    #[case::def("def f(a, b = 1, *c): a;", vec![("a", false, false), ("b", true, false), ("c", false, true)])]
    #[case::fn_("fn(x, y = 2): x;", vec![("x", false, false), ("y", true, false)])]
    #[case::no_params("def f(): 1;", vec![])]
    fn test_function_param_info(#[case] code: &str, #[case] expected: Vec<(&str, bool, bool)>) {
        let hir = lower(code);
        let functions = find(&hir, |s| matches!(s.kind, SymbolKind::Function(_)));
        assert_eq!(functions.len(), 1);

        let SymbolKind::Function(params) = &functions[0].1.kind else {
            unreachable!()
        };
        let expected: Vec<ParamInfo> = expected
            .into_iter()
            .map(|(name, has_default, is_variadic)| ParamInfo {
                name: name.into(),
                has_default,
                is_variadic,
            })
            .collect();
        assert_eq!(params, &expected);
    }

    #[test]
    fn test_default_param_expr_gets_its_own_scope() {
        let hir = lower("let d = 1 | def f(a = d): a;");
        let (ref_id, symbol) = find(&hir, |s| {
            s.value.as_deref() == Some("d") && matches!(s.kind, SymbolKind::Ref)
        })[0];

        assert!(matches!(
            hir.scopes[symbol.scope].kind,
            crate::ScopeKind::DefaultParam(_)
        ));
        let target = hir.resolve_reference_symbol(ref_id).unwrap();
        assert_eq!(hir.symbols[target].kind, SymbolKind::Variable);
    }

    #[test]
    fn test_module_registers_ident_and_owns_its_body() {
        let hir = lower("module m: def f(): 1; end");

        assert_eq!(
            find(&hir, |s| s.value.as_deref() == Some("m")
                && matches!(s.kind, SymbolKind::Ident))
            .len(),
            1
        );
        let (_, f) = find(&hir, |s| matches!(s.kind, SymbolKind::Function(_)))[0];
        assert!(matches!(parent_kind(&hir, f), Some(SymbolKind::Module(_))));
    }

    #[rstest]
    #[case::import_alias("import \"csv\" as c", Some("c"))]
    #[case::import("import \"csv\"", None)]
    fn test_import_registers_module_and_alias(#[case] code: &str, #[case] alias: Option<&str>) {
        let hir = lower(code);

        assert_eq!(
            find(&hir, |s| s.value.as_deref() == Some("csv")
                && matches!(s.kind, SymbolKind::Import(_)))
            .len(),
            1
        );
        let aliases = find(&hir, |s| matches!(s.kind, SymbolKind::Ident));
        assert_eq!(aliases.first().and_then(|(_, s)| s.value.as_deref()), alias);
    }

    #[rstest]
    #[case::string("\"s\"", SymbolKind::String)]
    #[case::number("1", SymbolKind::Number)]
    #[case::boolean("true", SymbolKind::Boolean)]
    #[case::none("None", SymbolKind::None)]
    #[case::bytes("b\"ab\"", SymbolKind::Bytes)]
    #[case::symbol(":sym", SymbolKind::Symbol)]
    fn test_literal_kinds(#[case] code: &str, #[case] expected: SymbolKind) {
        let hir = lower(code);
        let kinds: Vec<SymbolKind> = hir.symbols().map(|(_, s)| s.kind.clone()).collect();
        assert_eq!(kinds, vec![expected]);
    }
}
