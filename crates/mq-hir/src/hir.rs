use std::sync::Arc;

use compact_str::CompactString;
use mq_lang::{Token, TokenKind};
use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use url::Url;

use crate::{
    builtin::Builtin,
    scope::{Scope, ScopeId, ScopeKind},
    source::{Source, SourceId, SourceInfo},
    symbol::{Symbol, SymbolId, SymbolKind},
};

#[derive(Debug)]
pub struct Hir {
    pub builtin: Builtin,
    pub(crate) module_loader: mq_lang::ModuleLoader,
    pub(crate) scopes: SlotMap<ScopeId, Scope>,
    pub(crate) symbols: SlotMap<SymbolId, Symbol>,
    pub(crate) sources: SlotMap<SourceId, Source>,
    pub(crate) source_scopes: FxHashMap<SourceId, ScopeId>,
    pub(crate) references: FxHashMap<SymbolId, SymbolId>,
}

impl Default for Hir {
    fn default() -> Self {
        Self::new()
    }
}

impl Hir {
    pub fn new() -> Self {
        let mut sources = SlotMap::default();
        let mut scopes = SlotMap::default();

        let source = Source::new(None);
        let builtin_source_id = sources.insert(source);
        let builtin_scope_id = scopes.insert(Scope::new(
            SourceInfo::new(Some(builtin_source_id), None),
            ScopeKind::Module(builtin_source_id),
            None,
        ));
        let mut source_scopes = FxHashMap::default();
        source_scopes.insert(builtin_source_id, builtin_scope_id);

        Self {
            builtin: Builtin::new(builtin_source_id, builtin_scope_id),
            symbols: SlotMap::default(),
            sources,
            scopes,
            module_loader: mq_lang::ModuleLoader::new(None),
            source_scopes,
            references: FxHashMap::default(),
        }
    }

    pub fn is_builtin_symbol(&self, symbol: &Symbol) -> bool {
        symbol.source.source_id == Some(self.builtin.source_id)
    }

    /// Returns a list of unused user-defined functions for a given source
    pub fn unused_functions(&self, source_id: SourceId) -> Vec<(SymbolId, &Symbol)> {
        let mut unused = Vec::new();
        let user_functions: Vec<_> = self
            .symbols
            .iter()
            .filter(|(_, symbol)| {
                symbol.is_function()
                    && symbol.source.source_id == Some(source_id)
                    && !self.is_builtin_symbol(symbol)
                    && !symbol.is_internal_function()
            })
            .collect();

        for (func_id, func_symbol) in user_functions {
            let func_name = match &func_symbol.value {
                Some(name) => name,
                None => continue, // Anonymous functions are not considered unused
            };

            // Check if this function is referenced anywhere
            let is_used = self.symbols.iter().any(|(call_id, symbol)| {
                match &symbol.kind {
                    SymbolKind::Call => {
                        // Check if the call symbol directly matches the function name
                        if symbol.value.as_ref() == Some(func_name)
                            && symbol.source.source_id == Some(source_id)
                        {
                            return true;
                        }
                        // Also check if they have argument symbols that match our function name
                        self.symbols.iter().any(|(_, arg_symbol)| {
                            arg_symbol.parent == Some(call_id)
                                && arg_symbol.kind == SymbolKind::Argument
                                && arg_symbol.value.as_ref() == Some(func_name)
                                && arg_symbol.source.source_id == Some(source_id)
                        })
                    }
                    SymbolKind::Ref | SymbolKind::Argument => {
                        symbol.value.as_ref() == Some(func_name)
                            && symbol.source.source_id == Some(source_id)
                    }
                    _ => false,
                }
            });

            if !is_used {
                unused.push((func_id, func_symbol));
            }
        }

        unused
    }

    pub fn builtins(&self) -> impl Iterator<Item = &mq_lang::BuiltinFunctionDoc> {
        self.builtin.functions.values()
    }

    pub fn symbol(&self, symbol_id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(symbol_id)
    }

    pub fn symbols(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        self.symbols.iter()
    }

    pub fn scope(&self, scope_id: ScopeId) -> Option<&Scope> {
        self.scopes.get(scope_id)
    }

    pub fn scopes(&self) -> impl Iterator<Item = (ScopeId, &Scope)> {
        self.scopes.iter()
    }

    pub fn add_new_source(&mut self, url: Option<Url>) -> (SourceId, ScopeId) {
        let source_id = self.add_source(Source::new(url));
        let scope_id = self.add_scope(Scope::new(
            SourceInfo::new(Some(source_id), None),
            ScopeKind::Module(source_id),
            None,
        ));
        self.source_scopes.insert(source_id, scope_id);

        (source_id, scope_id)
    }

    pub fn add_line_of_code(&mut self, source_id: SourceId, scope_id: ScopeId, code: &str) {
        let (nodes, _) = mq_lang::parse_recovery(code);

        self.source_scopes.insert(source_id, scope_id);

        nodes.iter().for_each(|node| {
            self.add_expr(node, source_id, scope_id, None);
        });
    }

    pub fn add_code(&mut self, url: Option<Url>, code: &str) -> (SourceId, ScopeId) {
        let (nodes, _) = mq_lang::parse_recovery(code);

        self.add_nodes(url.unwrap_or(Url::parse("file:///").unwrap()), &nodes)
    }

    pub fn add_builtin(&mut self) {
        if self.builtin.loaded || self.builtin.disabled {
            return;
        }

        let (nodes, _) = mq_lang::parse_recovery(mq_lang::ModuleLoader::BUILTIN_FILE);

        nodes.iter().for_each(|node| {
            self.add_expr(node, self.builtin.source_id, self.builtin.scope_id, None);
        });

        self.builtin.functions.clone().keys().for_each(|name| {
            self.add_symbol(Symbol {
                value: Some(name.clone()),
                kind: SymbolKind::Function(
                    mq_lang::BUILTIN_FUNCTION_DOC[name]
                        .params
                        .iter()
                        .map(|p| CompactString::new(p))
                        .collect::<Vec<_>>(),
                ),
                source: SourceInfo::new(Some(self.builtin.source_id), None),
                scope: self.builtin.scope_id,
                doc: vec![(
                    mq_lang::Range::default(),
                    mq_lang::BUILTIN_FUNCTION_DOC[name].description.to_string(),
                )],
                parent: None,
            });
        });

        self.builtin
            .internal_functions
            .clone()
            .keys()
            .for_each(|name| {
                self.add_symbol(Symbol {
                    value: Some(name.clone()),
                    kind: SymbolKind::Function(
                        mq_lang::INTERNAL_FUNCTION_DOC[name]
                            .params
                            .iter()
                            .map(|p| CompactString::new(p))
                            .collect::<Vec<_>>(),
                    ),
                    source: SourceInfo::new(Some(self.builtin.source_id), None),
                    scope: self.builtin.scope_id,
                    doc: vec![(
                        mq_lang::Range::default(),
                        mq_lang::INTERNAL_FUNCTION_DOC[name].description.to_string(),
                    )],
                    parent: None,
                });
            });

        self.builtin.selectors.clone().keys().for_each(|name| {
            self.add_symbol(Symbol {
                value: Some(name.clone()),
                kind: SymbolKind::Selector,
                source: SourceInfo::new(Some(self.builtin.source_id), None),
                scope: self.builtin.scope_id,
                doc: vec![(
                    mq_lang::Range::default(),
                    mq_lang::BUILTIN_SELECTOR_DOC[name].description.to_string(),
                )],
                parent: None,
            });
        });

        self.builtin.loaded = true;
    }

    pub fn add_nodes(&mut self, url: Url, nodes: &[Arc<mq_lang::CstNode>]) -> (SourceId, ScopeId) {
        self.add_builtin();

        let source_id = self
            .source_by_url(&url)
            .inspect(|source_id| {
                self.symbols
                    .retain(|_, symbol| symbol.source.source_id != Some(*source_id));
            })
            .unwrap_or_else(|| self.add_source(Source::new(Some(url))));

        let scope_id = self.scope_by_source(&source_id).unwrap_or_else(|| {
            self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), None),
                ScopeKind::Module(source_id),
                None,
            ))
        });

        self.source_scopes.insert(source_id, scope_id);

        nodes.iter().for_each(|node| {
            self.add_expr(node, source_id, scope_id, None);
        });
        self.resolve();

        (source_id, scope_id)
    }

    pub fn source_by_url(&self, url: &Url) -> Option<SourceId> {
        self.sources.iter().find_map(|(s, data)| {
            data.url
                .as_ref()
                .and_then(|u| if *u == *url { Some(s) } else { None })
        })
    }

    fn scope_by_source(&self, source_id: &SourceId) -> Option<ScopeId> {
        self.scopes.iter().find_map(|(s, data)| {
            if data.source.source_id == Some(*source_id) {
                Some(s)
            } else {
                None
            }
        })
    }

    fn add_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        let mq_lang::CstNode { kind, .. } = &**node;

        match kind {
            mq_lang::CstNodeKind::BinaryOp(_) => {
                self.add_binary_op_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::UnaryOp(_) => {
                self.add_unary_op_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Call => {
                self.add_call_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Def => {
                self.add_def_expr(node, source_id, scope_id, parent);
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
            mq_lang::CstNodeKind::InterpolatedString => {
                self.add_interpolated_string(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Let => {
                self.add_let_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Literal => {
                self.add_literal_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Selector => {
                self.add_selector_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Until => {
                self.add_until_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::While => {
                self.add_while_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Array => {
                self.add_array_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Dict => {
                self.add_dict_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Self_
            | mq_lang::CstNodeKind::Nodes
            | mq_lang::CstNodeKind::End
            | mq_lang::CstNodeKind::Break
            | mq_lang::CstNodeKind::Continue => {
                self.add_keyword(node, source_id, scope_id, parent);
            }

            _ => {}
        }
    }

    fn add_binary_op_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::BinaryOp(_),
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::BinaryOp,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_unary_op_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::UnaryOp(_),
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::UnaryOp,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_literal_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Literal,
            ..
        } = &**node
        {
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
            });
        }
    }

    fn add_interpolated_string(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::InterpolatedString,
            token: Some(token),
            ..
        } = &**node
        {
            if let Token {
                kind: TokenKind::InterpolatedString(segments),
                ..
            } = &**token
            {
                segments.iter().for_each(|segment| match segment {
                    mq_lang::StringSegment::Text(text, range) => {
                        self.add_symbol(Symbol {
                            value: Some(text.into()),
                            kind: SymbolKind::String,
                            source: SourceInfo::new(Some(source_id), Some(range.clone())),
                            scope: scope_id,
                            doc: node.comments(),
                            parent,
                        });
                    }
                    mq_lang::StringSegment::Ident(ident, range) => {
                        self.symbols.insert(Symbol {
                            value: Some(ident.clone()),
                            kind: SymbolKind::Variable,
                            source: SourceInfo::new(Some(source_id), Some(range.clone())),
                            scope: scope_id,
                            doc: node.comments(),
                            parent,
                        });
                    }
                });
            }
        }
    }

    fn add_include_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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
                if let Ok(url) = Url::parse(&format!("file:///{}", module_name)) {
                    let code = self.module_loader.read_file(&module_name);
                    let (module_source_id, _) = self.add_code(Some(url), &code.unwrap_or_default());

                    self.add_symbol(Symbol {
                        value: Some(module_name.clone()),
                        kind: SymbolKind::Include(module_source_id),
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                    });
                }
            });
        }
    }

    fn add_while_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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

    fn add_until_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Until,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Until,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
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

    fn add_let_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Let,
            ..
        } = &**node
        {
            self.symbols.insert(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            let children = node.children_without_token();
            let ident = children.first().unwrap();
            let symbol_id = self.symbols.insert(Symbol {
                value: ident.name(),
                kind: SymbolKind::Variable,
                source: SourceInfo::new(Some(source_id), Some(ident.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            children.iter().skip(1).for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        }
    }

    fn add_ident_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Ident,
            ..
        } = &**node
        {
            self.symbols.insert(Symbol {
                value: node.name(),
                kind: SymbolKind::Ref,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });
        }
    }

    fn add_selector_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Selector,
            ..
        } = &**node
        {
            self.symbols.insert(Symbol {
                value: node.name(),
                kind: SymbolKind::Selector,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });
        }
    }

    fn add_if_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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
                    self.add_elif_expr(child, source_id, scope_id, parent);
                    self.add_else_expr(child, source_id, scope_id, parent);
                }
            }
        }
    }

    fn add_elif_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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
        node: &Arc<mq_lang::CstNode>,
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
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Call,
            ..
        } = &**node
        {
            self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Call,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            node.children_without_token().iter().for_each(|child| {
                if matches!(child.kind, mq_lang::CstNodeKind::Literal)
                    || matches!(child.kind, mq_lang::CstNodeKind::InterpolatedString)
                {
                    self.add_literal_expr(child, source_id, scope_id, parent);
                } else {
                    self.add_symbol(Symbol {
                        value: child.name(),
                        kind: SymbolKind::Argument,
                        source: SourceInfo::new(Some(source_id), Some(child.range())),
                        scope: scope_id,
                        doc: Vec::new(),
                        parent,
                    });
                }
            });
        }
    }

    fn add_foreach_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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
                parent,
            });

            self.add_symbol(Symbol {
                value: arg.name(),
                kind: SymbolKind::Ref,
                source: SourceInfo::new(Some(source_id), Some(arg.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_def_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Def,
            ..
        } = &**node
        {
            self.symbols.insert(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
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
            });

            let scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Function(symbol_id),
                Some(scope_id),
            ));

            let mut param_names = Vec::with_capacity(params.len());

            params.iter().skip(1).for_each(|child| {
                param_names.push(child.name().unwrap_or("arg".into()));
                self.add_symbol(Symbol {
                    value: child.name(),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent,
                });
            });

            self.symbols[symbol_id].kind = SymbolKind::Function(param_names);

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_fn_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Fn,
            ..
        } = &**node
        {
            self.symbols.insert(Symbol {
                value: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            let (params, program) = node.split_cond_and_program();
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::Function(Vec::new()),
                source: SourceInfo::new(Some(source_id), None),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            let scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::Function(symbol_id),
                Some(scope_id),
            ));

            let mut param_names = Vec::with_capacity(params.len());

            params.iter().skip(1).for_each(|child| {
                param_names.push(child.name().unwrap_or("arg".into()));
                self.add_symbol(Symbol {
                    value: child.name(),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent,
                });
            });

            self.symbols[symbol_id].kind = SymbolKind::Function(param_names);

            program.iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        } else {
            unreachable!()
        }
    }

    fn add_scope(&mut self, scope: Scope) -> ScopeId {
        let scope_id = self.scopes.insert(scope.clone());

        if let Some(parent_scope_id) = scope.parent_id {
            let _ = self
                .scopes
                .get_mut(parent_scope_id)
                .map(|scope| scope.add_child(scope_id));
        }

        scope_id
    }

    fn add_symbol(&mut self, symbol: Symbol) -> SymbolId {
        self.symbols.insert(symbol)
    }

    fn add_source(&mut self, source: Source) -> SourceId {
        self.sources.insert(source)
    }

    fn add_array_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Array,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Array,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });
            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_dict_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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
            });
            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
            }
        }
    }

    fn add_keyword(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
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
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use itertools::Itertools;
    use rstest::rstest;

    #[rstest]
    #[case::def("# test
def foo(): 1", vec![" test".to_owned(), " test".to_owned(), "".to_owned()], vec![SymbolKind::Keyword, SymbolKind::Function(Vec::new()), SymbolKind::Number])]
    fn test_symbols(
        #[case] code: &str,
        #[case] expected_doc: Vec<String>,
        #[case] expected_kind: Vec<SymbolKind>,
    ) {
        let mut hir = Hir::new();

        hir.builtin.disabled = true;
        hir.add_code(None, code);

        let symbols = hir
            .symbols()
            .map(|(_, symbol)| symbol.clone())
            .collect::<Vec<_>>();

        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.clone().kind)
                .collect::<Vec<_>>(),
            expected_kind
        );

        assert_eq!(
            symbols
                .iter()
                .map(|symbol| symbol.doc.iter().map(|(_, doc)| doc.clone()).join("\n"))
                .collect::<Vec<_>>(),
            expected_doc
        );
    }

    #[rstest]
    #[case::let_("let x = 1;", "x", SymbolKind::Variable)]
    #[case::def("def foo(): 1", "foo", SymbolKind::Function(Vec::new()))]
    #[case::if_("if (true): 1 else: 2;", "if", SymbolKind::If)]
    #[case::while_("while (true): 1;", "while", SymbolKind::While)]
    #[case::foreach("foreach(x, y): 1;", "foreach", SymbolKind::Foreach)]
    #[case::call("foo()", "foo", SymbolKind::Call)]
    #[case::elif_("if (true): 1 elif (false): 2 else: 3;", "elif", SymbolKind::Elif)]
    #[case::else_("if (true): 1 else: 2;", "else", SymbolKind::Else)]
    #[case::until("until (true): 1;", "until", SymbolKind::Until)]
    #[case::literal("42", "42", SymbolKind::Number)]
    #[case::selector(".h", ".h", SymbolKind::Selector)]
    #[case::selector(".code.lang", ".code.lang", SymbolKind::Selector)]
    #[case::interpolated_string("s\"hello ${world}\"", "world", SymbolKind::Variable)]
    #[case::include("include \"foo\"", "foo", SymbolKind::Include(SourceId::default()))]
    #[case::fn_expr("fn(): 42", "fn", SymbolKind::Keyword)]
    #[case::fn_with_params("fn(x, y): add(x, y);", "x", SymbolKind::Argument)]
    #[case::fn_with_body("fn(): let x = 1 | x;", "x", SymbolKind::Variable)]
    #[case::fn_anonymous("let f = fn(): 42;", "fn", SymbolKind::Keyword)]
    #[case::eq("1 == 2", "==", SymbolKind::BinaryOp)]
    #[case::neq("1 != 2", "!=", SymbolKind::BinaryOp)]
    #[case::plus("1 + 2", "+", SymbolKind::BinaryOp)]
    #[case::minus("1 - 2", "-", SymbolKind::BinaryOp)]
    #[case::mul("1 * 2", "*", SymbolKind::BinaryOp)]
    #[case::div("1 / 2", "/", SymbolKind::BinaryOp)]
    #[case::mod_("1 % 2", "%", SymbolKind::BinaryOp)]
    #[case::lt("1 < 2", "<", SymbolKind::BinaryOp)]
    #[case::lte("1 <= 2", "<=", SymbolKind::BinaryOp)]
    #[case::gt("1 > 2", ">", SymbolKind::BinaryOp)]
    #[case::gte("1 >= 2", ">=", SymbolKind::BinaryOp)]
    #[case::and("true && true", "&&", SymbolKind::BinaryOp)]
    #[case::or("true || false", "||", SymbolKind::BinaryOp)]
    #[case::range_op("1..2", "..", SymbolKind::BinaryOp)]
    #[case::array_with_numbers("[1, 2, 3]", "1", SymbolKind::Number)]
    #[case::array_with_strings("[\"a\", \"b\"]", "a", SymbolKind::String)]
    #[case::array_nested("[[1], [2]]", "1", SymbolKind::Number)]
    #[case::dict_simple("{\"a\": 1, \"b\": 2}", "a", SymbolKind::String)]
    #[case::dict_nested("{\"a\": {\"b\": 2}}", "b", SymbolKind::String)]
    #[case::not_unary("!true", "!", SymbolKind::UnaryOp)]
    #[case::not_variable("!x", "!", SymbolKind::UnaryOp)]
    #[case::not_variable("nodes", "nodes", SymbolKind::Keyword)]
    #[case::not_variable("self", "self", SymbolKind::Keyword)]
    #[case::break_("while (true): break;", "break", SymbolKind::Keyword)]
    #[case::continue_("while (true): continue;", "continue", SymbolKind::Keyword)]
    fn test_add_code(
        #[case] code: &str,
        #[case] expected_name: &str,
        #[case] expected_kind: SymbolKind,
    ) {
        let mut hir = Hir::new();
        hir.builtin.loaded = true;
        hir.add_code(None, code);

        let symbol = hir
            .symbols
            .iter()
            .find(|(_, symbol)| symbol.value == Some(expected_name.into()))
            .unwrap()
            .1;

        match (&symbol.kind, &expected_kind) {
            (SymbolKind::Function(_), SymbolKind::Function(_)) => {}
            (SymbolKind::Include(_), SymbolKind::Include(_)) => {}
            _ => assert_eq!(symbol.kind, expected_kind),
        }
    }

    #[rstest]
    #[case::let_("let x = 1;", mq_lang::Position::new(1, 5), "x", SymbolKind::Variable)]
    #[case::def(
        "def foo(): 1",
        mq_lang::Position::new(1, 6),
        "foo",
        SymbolKind::Function(Vec::new())
    )]
    #[case::if_(
        "if (true): 1 else: 2;",
        mq_lang::Position::new(1, 1),
        "if",
        SymbolKind::If
    )]
    #[case::while_(
        "while (true): 1;",
        mq_lang::Position::new(1, 1),
        "while",
        SymbolKind::While
    )]
    #[case::foreach_(
        "foreach(x, y): 1",
        mq_lang::Position::new(1, 1),
        "foreach",
        SymbolKind::Foreach
    )]
    #[case::call(
        "def foo():1; | foo()",
        mq_lang::Position::new(1, 16),
        "foo",
        SymbolKind::Function(Vec::new())
    )]
    fn test_find_symbol_in_position(
        #[case] code: &str,
        #[case] pos: mq_lang::Position,
        #[case] expected_name: &str,
        #[case] expected_kind: SymbolKind,
    ) {
        let mut hir = Hir::new();
        let (source_id, _) = hir.add_code(None, code);

        let (_, symbol) = hir.find_symbol_in_position(source_id, pos).unwrap();
        assert_eq!(symbol.value, Some(expected_name.into()));
        assert_eq!(symbol.kind, expected_kind);
    }

    #[test]
    fn test_builtin() {
        let mut hir = Hir::new();
        hir.add_builtin();
        assert!(hir.builtin.loaded);
    }

    #[test]
    fn test_include_function_resolves() {
        let mut hir = Hir::new();
        hir.builtin.loaded = false; // Ensure builtins are loaded by add_code
        let code = r#"include "csv"
| def test_csv():
  csv_parse("a,b,c\na,b,c", true)
end"#;
        let (_, _) = hir.add_code(None, code);

        // Find the symbol for "csv_parse"
        let symbol = hir
            .symbols()
            .find(|(_, symbol)| symbol.value.as_deref() == Some("csv_parse"))
            .map(|(_, symbol)| symbol)
            .expect("csv_parse symbol should be present");

        // It should be a function
        match &symbol.kind {
            SymbolKind::Function(params) => {
                assert!(!params.is_empty(), "csv_parse should have parameters");
            }
            _ => panic!("csv_parse should be a function"),
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_unused_functions() {
        let mut hir = Hir::new();
        hir.builtin.disabled = true; // Disable builtins for cleaner test

        let code = "def used_function(): 1; def unused_function(): 2; def another_unused(): 3; | used_function()";

        let (source_id, _) = hir.add_code(None, code);
        let unused = hir.unused_functions(source_id);

        // Should find 2 unused functions
        assert_eq!(unused.len(), 2);

        let unused_names: Vec<_> = unused
            .iter()
            .map(|(_, symbol)| symbol.value.as_ref().unwrap().as_str())
            .collect();

        assert!(unused_names.contains(&"unused_function"));
        assert!(unused_names.contains(&"another_unused"));
        assert!(!unused_names.contains(&"used_function"));
    }

    #[test]
    fn test_unused_functions_empty_when_all_used() {
        let mut hir = Hir::new();
        hir.builtin.disabled = true;

        let code = "def func1(): 1; def func2(): 2; | func1() | func2()";

        let (source_id, _) = hir.add_code(None, code);
        let unused = hir.unused_functions(source_id);

        assert_eq!(unused.len(), 0);
    }
}
