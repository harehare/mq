use std::sync::Arc;

use compact_str::CompactString;
use itertools::Itertools;
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

    pub fn is_builtin_symbol(&self, symbol: Arc<Symbol>) -> bool {
        symbol.source.source_id == Some(self.builtin.source_id)
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

    pub fn add_code(&mut self, url: Url, code: &str) -> (SourceId, ScopeId) {
        let (nodes, _) = mq_lang::parse_recovery(code);

        self.add_nodes(url, &nodes)
    }

    pub fn add_builtin(&mut self) {
        if self.builtin.loaded {
            return;
        }

        let (nodes, _) = mq_lang::parse_recovery(mq_lang::ModuleLoader::BUILTIN_FILE);

        nodes.iter().for_each(|node| {
            self.add_expr(node, self.builtin.source_id, self.builtin.scope_id, None);
        });

        self.builtin.functions.clone().keys().for_each(|name| {
            self.add_symbol(Symbol {
                name: Some(name.clone()),
                kind: SymbolKind::Function(
                    mq_lang::BUILTIN_FUNCTION_DOC[name]
                        .params
                        .iter()
                        .map(|p| CompactString::new(p))
                        .collect_vec(),
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

        self.builtin.selectors.clone().keys().for_each(|name| {
            self.add_symbol(Symbol {
                name: Some(name.clone()),
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

    #[inline(always)]
    fn add_expr(
        &mut self,
        node: &Arc<mq_lang::CstNode>,
        source_id: SourceId,
        scope_id: ScopeId,
        parent: Option<SymbolId>,
    ) {
        let mq_lang::CstNode { kind, .. } = &**node;

        match kind {
            mq_lang::CstNodeKind::Def => {
                self.add_def_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Call => {
                self.add_call_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Let => {
                self.add_let_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::If => {
                self.add_if_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Foreach => {
                self.add_foreach_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::While => {
                self.add_while_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Until => {
                self.add_until_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Ident => {
                self.add_ident_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Include => {
                self.add_include_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Literal => {
                self.add_literal_expr(node, source_id, scope_id, parent);
            }
            mq_lang::CstNodeKind::Selector => {
                self.add_selector_expr(node, source_id, scope_id, parent);
            }
            _ => {}
        }
    }

    #[inline(always)]
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
                name: node.name(),
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

    #[inline(always)]
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
                if let Ok(url) = Url::parse(&module_name) {
                    let code = self.module_loader.read_file(&module_name);
                    let (module_source_id, _) = self.add_code(url, &code.unwrap_or_default());

                    self.add_symbol(Symbol {
                        name: Some(module_name.clone()),
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

    #[inline(always)]
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
                name: node.name(),
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

    #[inline(always)]
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
                name: node.name(),
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

    #[inline(always)]
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
                name: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            let children = node.children_without_token();
            let ident = children.first().unwrap();
            let symbol_id = self.symbols.insert(Symbol {
                name: ident.name(),
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

    #[inline(always)]
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
                name: node.name(),
                kind: SymbolKind::Ref,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });
        }
    }

    #[inline(always)]
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
                name: node.name(),
                kind: SymbolKind::Selector,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });
        }
    }

    #[inline(always)]
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
                name: node.name(),
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

    #[inline(always)]
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
                name: node.name(),
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

    #[inline(always)]
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
                name: node.name(),
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

    #[inline(always)]
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
                name: node.name(),
                kind: SymbolKind::Call,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            node.children_without_token().iter().for_each(|child| {
                if matches!(child.kind, mq_lang::CstNodeKind::Literal) {
                    self.add_literal_expr(child, source_id, scope_id, parent);
                } else {
                    self.add_symbol(Symbol {
                        name: child.name(),
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

    #[inline(always)]
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
                name: node.name(),
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
                name: loop_val.name(),
                kind: SymbolKind::Variable,
                source: SourceInfo::new(Some(source_id), Some(loop_val.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            self.add_symbol(Symbol {
                name: arg.name(),
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

    #[inline(always)]
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
                name: node.name(),
                kind: SymbolKind::Keyword,
                source: SourceInfo::new(Some(source_id), Some(node.range())),
                scope: scope_id,
                doc: node.comments(),
                parent,
            });

            let (params, program) = node.split_cond_and_program();
            let ident = params.first().unwrap();

            let symbol_id = self.add_symbol(Symbol {
                name: ident.name(),
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
                    name: child.name(),
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

    #[inline(always)]
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

    #[inline(always)]
    fn add_symbol(&mut self, symbol: Symbol) -> SymbolId {
        self.symbols.insert(symbol)
    }

    #[inline(always)]
    fn add_source(&mut self, source: Source) -> SourceId {
        self.sources.insert(source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::let_("let x = 1;", "x", SymbolKind::Variable)]
    #[case::def("def foo(): 1", "foo", SymbolKind::Function(Vec::new()))]
    #[case::if_("if (true): 1 else: 2;", "if", SymbolKind::If)]
    #[case::while_("while (true): 1;", "while", SymbolKind::While)]
    #[case::foreach("foreach(x, y): 1;", "foreach", SymbolKind::Foreach)]
    #[case::call("foo()", "foo", SymbolKind::Call)]
    #[case::elif_("if (true): 1 elif (false): 2 else: 3;", "elif", SymbolKind::Elif)]
    #[case::else_("if (true): 1 else: 2;", "else", SymbolKind::Else)]
    fn test_add_code(
        #[case] code: &str,
        #[case] expected_name: &str,
        #[case] expected_kind: SymbolKind,
    ) {
        let mut hir = Hir::new();
        let url = Url::parse("file:///test").unwrap();
        hir.add_code(url, code);

        let symbol = hir
            .symbols
            .iter()
            .find(|(_, symbol)| symbol.name == Some(expected_name.into()))
            .unwrap()
            .1;
        assert_eq!(symbol.kind, expected_kind);
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
        "def foo():1; foo()",
        mq_lang::Position::new(1, 15),
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
        let url = Url::parse("file:///test").unwrap();
        let (source_id, _) = hir.add_code(url.clone(), code);

        let (_, symbol) = hir.find_symbol_in_position(source_id, pos).unwrap();
        assert_eq!(symbol.name, Some(expected_name.into()));
        assert_eq!(symbol.kind, expected_kind);
    }
}
