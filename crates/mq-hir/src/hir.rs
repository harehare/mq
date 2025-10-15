use std::{path::PathBuf, vec};

use mq_lang::{Token, TokenKind};
use rustc_hash::FxHashMap;
use slotmap::SlotMap;
use smol_str::SmolStr;
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
        Self::new(vec![])
    }
}

impl Hir {
    /// Creates a new `Hir` instance.
    ///
    /// # Parameters
    /// - `module_paths`: A list of filesystem paths to search for modules when resolving imports.
    ///   These paths are used by the module loader to locate and load external modules.
    ///   Providing additional paths can affect how and where modules are resolved during compilation.
    pub fn new(module_paths: Vec<PathBuf>) -> Self {
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
            module_loader: mq_lang::ModuleLoader::new(Some(module_paths)),
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

    #[inline(always)]
    pub fn builtins(&self) -> impl Iterator<Item = &mq_lang::BuiltinFunctionDoc> {
        self.builtin.functions.values()
    }

    #[inline(always)]
    pub fn symbol(&self, symbol_id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(symbol_id)
    }

    #[inline(always)]
    pub fn symbols(&self) -> impl Iterator<Item = (SymbolId, &Symbol)> {
        self.symbols.iter()
    }

    #[inline(always)]
    pub fn scope(&self, scope_id: ScopeId) -> Option<&Scope> {
        self.scopes.get(scope_id)
    }

    #[inline(always)]
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
                        .map(SmolStr::new)
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
                            .map(SmolStr::new)
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

    pub fn add_nodes(
        &mut self,
        url: Url,
        nodes: &[mq_lang::Shared<mq_lang::CstNode>],
    ) -> (SourceId, ScopeId) {
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

    fn add_binary_op_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
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
        node: &mq_lang::Shared<mq_lang::CstNode>,
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

    fn add_try_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Try,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Try,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            node.children_without_token().iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
            });
        }
    }

    fn add_catch_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Catch,
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
            });

            node.children_without_token().iter().for_each(|child| {
                self.add_expr(child, source_id, scope_id, Some(symbol_id));
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
        node: &mq_lang::Shared<mq_lang::CstNode>,
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
        node: &mq_lang::Shared<mq_lang::CstNode>,
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
            self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Call,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            node.children_without_token().iter().for_each(|child| {
                // Process all arguments recursively to handle complex expressions
                // This ensures that identifiers inside bracket access (e.g., vars in vars["x"])
                // are properly registered as Ref symbols that can be resolved
                self.add_expr(child, source_id, scope_id, parent);
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

            let mut param_names = Vec::with_capacity(params.len().saturating_sub(1));

            // For def expressions, the first param is the function name, so skip it
            params.iter().skip(1).for_each(|child| {
                param_names.push(child.name().unwrap_or("arg".into()));
                self.add_symbol(Symbol {
                    value: child.name(),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
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

            params.iter().for_each(|child| {
                param_names.push(child.name().unwrap_or("arg".into()));
                self.add_symbol(Symbol {
                    value: child.name(),
                    kind: SymbolKind::Parameter,
                    source: SourceInfo::new(Some(source_id), Some(child.range())),
                    scope: scope_id,
                    doc: Vec::new(),
                    parent: Some(symbol_id),
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
        node: &mq_lang::Shared<mq_lang::CstNode>,
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
            });
            for child in node.children_without_token() {
                self.add_expr(&child, source_id, scope_id, Some(symbol_id));
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
        });
    }

    fn add_pattern_expr(
        &mut self,
        node: &mq_lang::Shared<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        if let mq_lang::CstNode {
            kind: mq_lang::CstNodeKind::Pattern,
            ..
        } = &**node
        {
            let symbol_id = self.add_symbol(Symbol {
                value: node.name(),
                kind: SymbolKind::Pattern,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            // Extract pattern variables and add them to the scope
            self.extract_pattern_variables(node, source_id, scope_id, Some(symbol_id));

            // Process nested patterns (for array, dict patterns)
            for child in node.children_without_token() {
                if matches!(child.kind, mq_lang::CstNodeKind::Pattern) {
                    self.add_pattern_expr(&child, source_id, scope_id, Some(symbol_id));
                } else if matches!(child.kind, mq_lang::CstNodeKind::Ident) {
                    // Ident nodes in patterns are part of symbol literals (:foo -> foo)
                    // They should be registered as Symbol, not Ref
                    self.add_symbol(Symbol {
                        value: child.name(),
                        kind: SymbolKind::Symbol,
                        source: SourceInfo::new(Some(source_id), Some(child.range())),
                        scope: scope_id,
                        doc: child.comments(),
                        parent: Some(symbol_id),
                    });
                } else {
                    // Process other expressions in the pattern (e.g., literals, guard conditions)
                    self.add_expr(&child, source_id, scope_id, Some(symbol_id));
                }
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
            // Create MatchArm symbol
            let symbol_id = self.add_symbol(Symbol {
                value: None,
                kind: SymbolKind::MatchArm,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            // Create a dedicated scope for this MatchArm
            // Pattern variables will be visible in this scope
            let arm_scope_id = self.add_scope(Scope::new(
                SourceInfo::new(Some(source_id), Some(node.node_range())),
                ScopeKind::MatchArm(symbol_id),
                Some(scope_id),
            ));

            let children = node.children_without_token();

            // Process pattern (first child after the pipe token)
            // The pattern introduces variables into the arm scope
            if let Some(pattern) = children.first() {
                if matches!(pattern.kind, mq_lang::CstNodeKind::Pattern) {
                    self.add_pattern_expr(pattern, source_id, arm_scope_id, Some(symbol_id));
                }
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
    ) {
        if let Some(token) = &node.token {
            match &token.kind {
                // Identifier pattern: introduces a variable binding
                mq_lang::TokenKind::Ident(name) if name != "_" => {
                    // Skip wildcards
                    self.add_symbol(Symbol {
                        value: Some(name.clone()),
                        kind: SymbolKind::PatternVariable,
                        source: SourceInfo::new(Some(source_id), Some(node.range())),
                        scope: scope_id,
                        doc: node.comments(),
                        parent,
                    });
                }
                _ => {
                    // For other token types (literals, wildcards), no variable is introduced
                }
            }
        }
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
        let mut hir = Hir::default();

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
    #[case::fn_with_params("fn(x, y): add(x, y);", "x", SymbolKind::Parameter)]
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
    #[case::block("do \"hello\" end", "hello", SymbolKind::String)]
    #[case::try_("try: 1 catch: 2", "try", SymbolKind::Try)]
    #[case::catch_("try: 1 catch: 2", "catch", SymbolKind::Catch)]
    #[case::symbol_ident(":foo", "foo", SymbolKind::Symbol)]
    #[case::symbol_string(":\"hello\"", "hello", SymbolKind::Symbol)]
    #[case::pattern_match("match (v): | [1,2,3]: 1 end", "match", SymbolKind::Match)]
    #[case::pattern_match_arm("match (v): | 1: \"one\" end", "1", SymbolKind::Pattern)]
    fn test_add_code(
        #[case] code: &str,
        #[case] expected_name: &str,
        #[case] expected_kind: SymbolKind,
    ) {
        let mut hir = Hir::default();
        hir.builtin.loaded = true;
        hir.add_code(None, code);

        let symbol = hir
            .symbols
            .iter()
            .find(|(_, symbol)| {
                symbol.value == Some(expected_name.into())
                    && match (&symbol.kind, &expected_kind) {
                        (SymbolKind::Function(_), SymbolKind::Function(_)) => true,
                        (SymbolKind::Include(_), SymbolKind::Include(_)) => true,
                        (kind, expected) => kind == expected,
                    }
            })
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
        let mut hir = Hir::default();
        let (source_id, _) = hir.add_code(None, code);

        let (_, symbol) = hir.find_symbol_in_position(source_id, pos).unwrap();
        assert_eq!(symbol.value, Some(expected_name.into()));
        assert_eq!(symbol.kind, expected_kind);
    }

    #[test]
    fn test_builtin() {
        let mut hir = Hir::default();
        hir.add_builtin();
        assert!(hir.builtin.loaded);
    }

    #[test]
    fn test_include_function_resolves() {
        let mut hir = Hir::default();
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
        let mut hir = Hir::default();
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
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "def func1(): 1; def func2(): 2; | func1() | func2()";

        let (source_id, _) = hir.add_code(None, code);
        let unused = hir.unused_functions(source_id);

        assert_eq!(unused.len(), 0);
    }

    #[test]
    fn test_block_symbol() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"do "hello" end"#;
        let _ = mq_lang::parse_recovery(code);
        let _ = hir.add_code(None, code);
        let block_symbol = hir
            .symbols
            .iter()
            .find(|(_, symbol)| matches!(symbol.kind, SymbolKind::Block))
            .map(|(_, symbol)| symbol);

        assert!(block_symbol.is_some(), "Block symbol should exist");

        let string_symbol = hir
            .symbols
            .iter()
            .find(|(_, symbol)| symbol.value == Some("hello".into()))
            .map(|(_, symbol)| symbol);

        assert!(
            string_symbol.is_some(),
            "String literal symbol should exist"
        );

        if let Some(string_sym) = string_symbol {
            assert!(matches!(string_sym.kind, SymbolKind::String));
        }
    }

    #[test]
    fn test_fn_param_resolution() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "fn(x): x";
        hir.add_code(None, code);

        // Find the Ref symbol for the second 'x'
        let ref_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Ref && s.value.as_deref() == Some("x"));

        assert!(ref_symbol.is_some(), "Should have a Ref symbol for x");

        let (ref_id, _) = ref_symbol.unwrap();
        let resolved = hir.resolve_reference_symbol(ref_id);

        assert!(resolved.is_some(), "x Ref should resolve to Parameter");

        let resolved_symbol = &hir.symbols[resolved.unwrap()];
        assert_eq!(resolved_symbol.kind, SymbolKind::Parameter);
        assert_eq!(resolved_symbol.value.as_deref(), Some("x"));

        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }

    #[test]
    fn test_match_expression_basic() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match (1): | 1: "one" | _: "other" end"#;
        hir.add_code(None, code);

        // Check for Match symbol
        let match_symbol = hir
            .symbols()
            .find(|(_, symbol)| matches!(symbol.kind, SymbolKind::Match));
        assert!(match_symbol.is_some(), "Should have a Match symbol");

        // Check for MatchArm symbols
        let match_arms: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::MatchArm))
            .collect();
        assert_eq!(match_arms.len(), 2, "Should have 2 MatchArm symbols");

        // Check for Pattern symbols
        let patterns: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Pattern))
            .collect();
        assert_eq!(patterns.len(), 2, "Should have 2 Pattern symbols");
    }

    #[test]
    fn test_match_pattern_variable_scope() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match (10): | x: x + 1 end"#;
        hir.add_code(None, code);

        // Check for PatternVariable
        let pattern_var = hir.symbols().find(|(_, symbol)| {
            symbol.kind == SymbolKind::PatternVariable && symbol.value.as_deref() == Some("x")
        });
        assert!(pattern_var.is_some(), "Should have a PatternVariable 'x'");

        // Check for Ref to 'x' in the body
        let x_ref = hir.symbols().find(|(_, symbol)| {
            symbol.kind == SymbolKind::Ref && symbol.value.as_deref() == Some("x")
        });
        assert!(x_ref.is_some(), "Should have a Ref to 'x'");

        // Check that MatchArm has its own scope
        let match_arm_scopes: Vec<_> = hir
            .scopes()
            .filter(|(_, scope)| matches!(scope.kind, ScopeKind::MatchArm(_)))
            .collect();
        assert_eq!(match_arm_scopes.len(), 1, "Should have 1 MatchArm scope");
    }

    #[test]
    fn test_match_array_pattern() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match ([1,2,3]): | [a, b, c]: a + b + c end"#;
        hir.add_code(None, code);

        // Check for PatternVariables
        let pattern_vars: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable))
            .collect();
        assert_eq!(
            pattern_vars.len(),
            3,
            "Should have 3 PatternVariables (a, b, c)"
        );

        // Verify the names
        let names: Vec<_> = pattern_vars
            .iter()
            .map(|(_, symbol)| symbol.value.as_ref().unwrap().as_str())
            .collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }

    #[test]
    fn test_match_wildcard_pattern() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match (5): | _: "anything" end"#;
        hir.add_code(None, code);

        // Wildcard should NOT create a PatternVariable
        let pattern_vars: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable))
            .collect();
        assert_eq!(
            pattern_vars.len(),
            0,
            "Wildcard should not create PatternVariables"
        );

        // But should still have a Pattern symbol
        let patterns: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Pattern))
            .collect();
        assert!(!patterns.is_empty(), "Should have Pattern symbols");
    }

    #[test]
    fn test_match_pattern_variable_resolution() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match (10): | x: x + 1 end"#;
        hir.add_code(None, code);

        // Find the PatternVariable 'x'
        let pattern_var = hir
            .symbols()
            .find(|(_, symbol)| {
                symbol.kind == SymbolKind::PatternVariable && symbol.value.as_deref() == Some("x")
            })
            .map(|(id, _)| id);
        assert!(pattern_var.is_some(), "Should have a PatternVariable 'x'");

        // Find the Ref to 'x' in the body
        let x_ref = hir
            .symbols()
            .find(|(_, symbol)| {
                symbol.kind == SymbolKind::Ref && symbol.value.as_deref() == Some("x")
            })
            .map(|(id, _)| id);
        assert!(x_ref.is_some(), "Should have a Ref to 'x'");

        // Verify that the Ref resolves to the PatternVariable
        let resolved = hir.resolve_reference_symbol(x_ref.unwrap());
        assert!(resolved.is_some(), "Ref 'x' should resolve");
        assert_eq!(
            resolved.unwrap(),
            pattern_var.unwrap(),
            "Ref 'x' should resolve to PatternVariable 'x'"
        );

        // Verify no unresolved errors
        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }

    #[test]
    fn test_match_array_pattern_variable_resolution() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match ([1,2,3]): | [a, b, c]: a + b + c end"#;
        hir.add_code(None, code);

        // Find all PatternVariables
        let pattern_vars: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable))
            .map(|(id, symbol)| (id, symbol.value.clone()))
            .collect();
        assert_eq!(pattern_vars.len(), 3, "Should have 3 PatternVariables");

        // Find all Refs in the body
        let refs: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| symbol.kind == SymbolKind::Ref)
            .map(|(id, symbol)| (id, symbol.value.clone()))
            .collect();

        // Each Ref should resolve to a PatternVariable
        for (ref_id, ref_name) in refs {
            let resolved = hir.resolve_reference_symbol(ref_id);
            assert!(resolved.is_some(), "Ref should resolve");

            let resolved_symbol = &hir.symbols[resolved.unwrap()];
            assert_eq!(
                resolved_symbol.kind,
                SymbolKind::PatternVariable,
                "Should resolve to PatternVariable"
            );
            assert_eq!(
                resolved_symbol.value, ref_name,
                "Resolved variable name should match"
            );
        }

        // Verify no unresolved errors
        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }

    #[test]
    fn test_match_array_pattern_with_symbols() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"match ([:foo, :bar]): | [:foo, :bar]: "matched" end"#;
        hir.add_code(None, code);

        // Check for Symbol literals
        let symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Symbol))
            .collect();

        // Should have 4 symbol literals (2 in match value, 2 in pattern)
        assert_eq!(symbols.len(), 4, "Should have 4 Symbol literals");

        // Verify no unresolved errors
        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }
}
