use std::path::PathBuf;

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

mod lower;
mod query;

#[derive(Debug)]
pub struct Hir {
    pub builtin: Builtin,
    pub(crate) module_loader: mq_lang::ModuleLoader,
    pub(crate) scopes: SlotMap<ScopeId, Scope>,
    pub(crate) symbols: SlotMap<SymbolId, Symbol>,
    pub(crate) sources: SlotMap<SourceId, Source>,
    pub(crate) source_scopes: FxHashMap<SourceId, ScopeId>,
    pub(crate) references: FxHashMap<SymbolId, SymbolId>,
    pub(crate) source_symbols: FxHashMap<SourceId, Vec<SymbolId>>,
    /// Monotonically-increasing counter assigned to each symbol at insertion time.
    /// Because `SlotMap` reuses freed slots (LIFO), the slot-based key order no longer
    /// reflects insertion order after a source is reloaded (e.g. on repeated LSP saves).
    /// Using an explicit counter guarantees that parent symbols always have a lower order
    /// value than their children, which is required by the type-checker's constraint-
    /// generation passes.
    pub(crate) symbol_insertion_counter: u32,
    /// Reverse index from symbol name → list of SymbolIds that carry that name.
    /// Populated by `insert_symbol` and pruned by `add_nodes` cleanup.
    /// Allows name-based lookups in `resolve.rs` to skip an O(n) full-symbol scan.
    pub(crate) name_index: FxHashMap<SmolStr, Vec<SymbolId>>,
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
            module_loader: mq_lang::ModuleLoader::new(mq_lang::LocalFsModuleResolver::new(Some(module_paths))),
            source_scopes,
            references: FxHashMap::default(),
            source_symbols: FxHashMap::default(),
            symbol_insertion_counter: 0,
            name_index: FxHashMap::default(),
        }
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

        let (nodes, _) = mq_lang::parse_recovery(mq_lang::BUILTIN_MODULE_FILE);

        nodes.iter().for_each(|node| {
            self.add_expr(node, self.builtin.source_id, self.builtin.scope_id, None);
        });

        // Collect keys first to avoid borrow checker issues
        let function_keys: Vec<_> = self.builtin.functions.keys().cloned().collect();
        for name in function_keys {
            self.add_symbol(Symbol {
                value: Some(name.clone()),
                kind: SymbolKind::Function(
                    mq_lang::BUILTIN_FUNCTION_DOC[&name]
                        .params
                        .iter()
                        .map(|p| (*p).into())
                        .collect::<Vec<_>>(),
                ),
                source: SourceInfo::new(Some(self.builtin.source_id), None),
                scope: self.builtin.scope_id,
                doc: vec![(
                    mq_lang::Range::default(),
                    mq_lang::BUILTIN_FUNCTION_DOC[&name].description.to_string(),
                )],
                parent: None,
                insertion_order: 0,
            });
        }

        let internal_function_keys: Vec<_> = self.builtin.internal_functions.keys().cloned().collect();
        for name in internal_function_keys {
            self.add_symbol(Symbol {
                value: Some(name.clone()),
                kind: SymbolKind::Function(
                    mq_lang::INTERNAL_FUNCTION_DOC[&name]
                        .params
                        .iter()
                        .map(|p| (*p).into())
                        .collect::<Vec<_>>(),
                ),
                source: SourceInfo::new(Some(self.builtin.source_id), None),
                scope: self.builtin.scope_id,
                doc: vec![(
                    mq_lang::Range::default(),
                    mq_lang::INTERNAL_FUNCTION_DOC[&name].description.to_string(),
                )],
                parent: None,
                insertion_order: 0,
            });
        }

        let selector_keys: Vec<_> = self.builtin.selectors.keys().cloned().collect();
        for name in selector_keys {
            if let Ok(selector) =
                mq_lang::Selector::try_from(&mq_lang::Token::new(mq_lang::TokenKind::Selector(name.clone())))
            {
                self.add_symbol(Symbol {
                    value: Some(name.clone()),
                    kind: SymbolKind::Selector(selector),
                    source: SourceInfo::new(Some(self.builtin.source_id), None),
                    scope: self.builtin.scope_id,
                    doc: vec![(
                        mq_lang::Range::default(),
                        mq_lang::BUILTIN_SELECTOR_DOC[&name].description.to_string(),
                    )],
                    parent: None,
                    insertion_order: 0,
                });
            }
        }

        self.builtin.loaded = true;
    }

    pub fn add_nodes(&mut self, url: Url, nodes: &[mq_lang::Shared<mq_lang::CstNode>]) -> (SourceId, ScopeId) {
        self.add_builtin();

        let source_id = self
            .source_by_url(&url)
            .inspect(|source_id| {
                // Remove symbols from this source
                self.symbols
                    .retain(|_, symbol| symbol.source.source_id != Some(*source_id));
                // Clear the index for this source
                self.source_symbols.remove(source_id);
            })
            .unwrap_or_else(|| self.add_source(Source::new(Some(url))));

        // Clean up stale entries in the auxiliary maps whose symbols were removed above.
        // Without this, these maps accumulate dead entries across multiple `add_nodes`
        // calls (e.g. repeated LSP saves).
        {
            let symbols = &self.symbols;
            self.references
                .retain(|ref_id, def_id| symbols.contains_key(*ref_id) && symbols.contains_key(*def_id));
            // Prune name_index: remove dead SymbolIds, then drop empty name entries.
            self.name_index.retain(|_, ids| {
                ids.retain(|id| symbols.contains_key(*id));
                !ids.is_empty()
            });
        }

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
        self.sources
            .iter()
            .find_map(|(s, data)| data.url.as_ref().and_then(|u| if *u == *url { Some(s) } else { None }))
    }

    /// Returns the URL associated with a given source_id.
    /// Returns None if the source_id doesn't exist or has no associated URL.
    pub fn url_by_source(&self, source_id: &SourceId) -> Option<&Url> {
        self.sources.get(*source_id).and_then(|source| source.url.as_ref())
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

    fn add_scope(&mut self, scope: Scope) -> ScopeId {
        let parent_scope_id = scope.parent_id;
        let scope_id = self.scopes.insert(scope);

        if let Some(parent_scope_id) = parent_scope_id
            && let Some(parent) = self.scopes.get_mut(parent_scope_id)
        {
            parent.add_child(scope_id);
        }

        scope_id
    }

    /// Inserts a symbol into the SlotMap and stamps its `insertion_order` field.
    ///
    /// This is the low-level primitive used by both `add_symbol` (which also
    /// registers the symbol in `source_symbols`) and by the handful of call
    /// sites that insert symbols without source tracking.  Every symbol must
    /// go through this method so that `insertion_order` is set for all symbols,
    /// enabling stable ordering in the type-checker.
    fn insert_symbol(&mut self, symbol: Symbol) -> SymbolId {
        let symbol_id = self.symbols.insert(symbol);
        self.symbols[symbol_id].insertion_order = self.symbol_insertion_counter;
        self.symbol_insertion_counter += 1;
        if let Some(ref name) = self.symbols[symbol_id].value {
            self.name_index.entry(name.clone()).or_default().push(symbol_id);
        }
        symbol_id
    }

    fn add_symbol(&mut self, symbol: Symbol) -> SymbolId {
        let source_id = symbol.source.source_id;
        let symbol_id = self.insert_symbol(symbol);

        if let Some(source_id) = source_id {
            self.source_symbols.entry(source_id).or_default().push(symbol_id);
        }

        symbol_id
    }

    /// Returns the insertion-order sequence number for a symbol.
    ///
    /// Parent symbols are always assigned a lower sequence number than their
    /// children because `add_expr` inserts parents before recursing into child
    /// nodes.  This ordering is stable across multiple `add_nodes` calls because
    /// the counter is monotonically increasing and never reset.
    #[inline(always)]
    pub fn symbol_insertion_order(&self, symbol_id: SymbolId) -> u32 {
        self.symbols.get(symbol_id).map_or(0, |s| s.insertion_order)
    }

    fn add_source(&mut self, source: Source) -> SourceId {
        self.sources.insert(source)
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
    fn test_symbols(#[case] code: &str, #[case] expected_doc: Vec<String>, #[case] expected_kind: Vec<SymbolKind>) {
        let mut hir = Hir::default();

        hir.builtin.disabled = true;
        hir.add_code(None, code);

        let symbols = hir.symbols().map(|(_, symbol)| symbol.clone()).collect::<Vec<_>>();

        assert_eq!(
            symbols.iter().map(|symbol| symbol.clone().kind).collect::<Vec<_>>(),
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
    #[case::literal("42", "42", SymbolKind::Number)]
    #[case::selector(".h", ".h", SymbolKind::Selector(mq_lang::Selector::Heading(None)))]
    #[case::selector(".code.lang", ".code", SymbolKind::Selector(mq_lang::Selector::Code))]
    // Bracket selectors: .[n] → List, .[n][m] → Table
    #[case::selector_list_any(".[]", ".", SymbolKind::Selector(mq_lang::Selector::List(None, None)))]
    #[case::selector_list_index(".[1]", ".", SymbolKind::Selector(mq_lang::Selector::List(Some(1), None)))]
    #[case::selector_table_any(".[][]", ".", SymbolKind::Selector(mq_lang::Selector::Table(None, None)))]
    #[case::selector_table_row_any(".[1][]", ".", SymbolKind::Selector(mq_lang::Selector::Table(Some(1), None)))]
    #[case::selector_table_row_col(".[1][2]", ".", SymbolKind::Selector(mq_lang::Selector::Table(Some(1), Some(2))))]
    #[case::selector_table_any_col(".[][2]", ".", SymbolKind::Selector(mq_lang::Selector::Table(None, Some(2))))]
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
    #[case::pattern_match_arm("match (v): | 1: \"one\" end", "1", SymbolKind::Pattern { is_dict: false })]
    #[case::import("import \"foo\"", "foo", SymbolKind::Import(SourceId::default()))]
    #[case::module("module a: def b(): 1; end", "a", SymbolKind::Module(SourceId::default()))]
    #[case::module_name_ident("module math: def add(): 1; end", "math", SymbolKind::Ident)]
    fn test_add_code(#[case] code: &str, #[case] expected_name: &str, #[case] expected_kind: SymbolKind) {
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
                        (SymbolKind::Module(_), SymbolKind::Module(_)) => true,
                        (SymbolKind::Import(_), SymbolKind::Import(_)) => true,
                        (kind, expected) => kind == expected,
                    }
            })
            .unwrap()
            .1;

        match (&symbol.kind, &expected_kind) {
            (SymbolKind::Function(_), SymbolKind::Function(_)) => {}
            (SymbolKind::Include(_), SymbolKind::Include(_)) => {}
            (SymbolKind::Module(_), SymbolKind::Module(_)) => {}
            (SymbolKind::Import(_), SymbolKind::Import(_)) => {}
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
    #[case::if_("if (true): 1 else: 2;", mq_lang::Position::new(1, 1), "if", SymbolKind::If)]
    #[case::while_("while (true): 1;", mq_lang::Position::new(1, 1), "while", SymbolKind::While)]
    #[case::foreach_("foreach(x, y): 1", mq_lang::Position::new(1, 1), "foreach", SymbolKind::Foreach)]
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

        assert!(string_symbol.is_some(), "String literal symbol should exist");

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
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Pattern { .. }))
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
            matches!(symbol.kind, SymbolKind::PatternVariable { .. }) && symbol.value.as_deref() == Some("x")
        });
        assert!(pattern_var.is_some(), "Should have a PatternVariable 'x'");

        // Check for Ref to 'x' in the body
        let x_ref = hir
            .symbols()
            .find(|(_, symbol)| symbol.kind == SymbolKind::Ref && symbol.value.as_deref() == Some("x"));
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
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable { .. }))
            .collect();
        assert_eq!(pattern_vars.len(), 3, "Should have 3 PatternVariables (a, b, c)");

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
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable { .. }))
            .collect();
        assert_eq!(pattern_vars.len(), 0, "Wildcard should not create PatternVariables");

        // But should still have a Pattern symbol
        let patterns: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Pattern { .. }))
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
                matches!(symbol.kind, SymbolKind::PatternVariable { .. }) && symbol.value.as_deref() == Some("x")
            })
            .map(|(id, _)| id);
        assert!(pattern_var.is_some(), "Should have a PatternVariable 'x'");

        // Find the Ref to 'x' in the body
        let x_ref = hir
            .symbols()
            .find(|(_, symbol)| symbol.kind == SymbolKind::Ref && symbol.value.as_deref() == Some("x"))
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
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable { .. }))
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
            assert!(
                matches!(resolved_symbol.kind, SymbolKind::PatternVariable { .. }),
                "Should resolve to PatternVariable"
            );
            assert_eq!(resolved_symbol.value, ref_name, "Resolved variable name should match");
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

    #[test]
    fn test_destructuring_let_creates_destructuring_binding() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"let [a, b] = [1, 2]"#;
        hir.add_code(None, code);

        // Should have a DestructuringBinding symbol (sibling to Keyword, same as Variable)
        let binding = hir
            .symbols()
            .find(|(_, symbol)| matches!(symbol.kind, SymbolKind::DestructuringBinding));
        assert!(binding.is_some(), "Should have a DestructuringBinding symbol");

        let (binding_id, _) = binding.unwrap();

        // The outer Pattern node should be a direct child of DestructuringBinding
        let outer_pattern = hir
            .symbols()
            .find(|(_, symbol)| matches!(symbol.kind, SymbolKind::Pattern { .. }) && symbol.parent == Some(binding_id));
        assert!(
            outer_pattern.is_some(),
            "Should have a Pattern child under DestructuringBinding"
        );

        // PatternVariables (a, b) should exist anywhere in the HIR for this let
        let pattern_vars: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::PatternVariable { .. }))
            .collect();
        assert_eq!(pattern_vars.len(), 2, "Should have 2 PatternVariables");

        let names: Vec<_> = pattern_vars.iter().map(|(_, s)| s.value.as_deref().unwrap()).collect();
        assert!(names.contains(&"a"));
        assert!(names.contains(&"b"));

        // Should have no unresolved errors
        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }

    #[test]
    fn test_macro_definition_and_call() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"macro inc(x): x + 1 | inc(2)"#;
        hir.add_code(None, code);

        // Macro symbol
        let macro_symbols: Vec<_> = hir.symbols().filter(|(_, symbol)| symbol.is_macro()).collect();

        let macro_symbol = macro_symbols[0].1;
        assert_eq!(macro_symbol.value.as_deref(), Some("inc"));

        // Macro parameter
        if let SymbolKind::Macro(params) = &macro_symbol.kind {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name.as_str(), "x");
            assert!(!params[0].has_default);
        } else {
            panic!("Expected macro symbol kind");
        }

        // Macro call symbol
        let call_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| symbol.kind == SymbolKind::Call && symbol.value.as_deref() == Some("inc"))
            .collect();
        assert_eq!(call_symbols.len(), 1, "Should have 1 macro call symbol");

        // Parameter symbol
        let param_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, symbol)| symbol.kind == SymbolKind::Parameter && symbol.value.as_deref() == Some("x"))
            .collect();
        assert_eq!(param_symbols.len(), 1, "Should have 1 parameter symbol for macro");

        // No unresolved errors
        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }

    #[test]
    fn test_function_single_default_param() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        hir.add_code(None, "def add(x = 5): x + 1");

        let func_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_)))
            .collect();

        assert_eq!(func_symbols.len(), 1);

        if let SymbolKind::Function(params) = &func_symbols[0].1.kind {
            assert_eq!(params.len(), 1);
            assert_eq!(params[0].name.as_str(), "x");
            assert!(params[0].has_default, "Parameter 'x' should have default value");
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_function_mixed_parameters() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        hir.add_code(None, "def foo(a, b = 2, c = 3): a + b + c");

        let func_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_)))
            .collect();

        if let SymbolKind::Function(params) = &func_symbols[0].1.kind {
            assert_eq!(params.len(), 3);
            assert_eq!(params[0].name.as_str(), "a");
            assert!(!params[0].has_default, "Parameter 'a' should NOT have default");
            assert_eq!(params[1].name.as_str(), "b");
            assert!(params[1].has_default, "Parameter 'b' should have default");
            assert_eq!(params[2].name.as_str(), "c");
            assert!(params[2].has_default, "Parameter 'c' should have default");
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_all_parameters_with_defaults() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        hir.add_code(None, "def calc(a = 1, b = 2, c = 3): a + b + c");

        let func_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_)))
            .collect();

        if let SymbolKind::Function(params) = &func_symbols[0].1.kind {
            assert_eq!(params.len(), 3);
            for param in params {
                assert!(param.has_default, "All parameters should have defaults");
            }
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_function_default_with_array_literal() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        hir.add_code(None, "def test(x = [1, 2, 3]): x;");

        let func_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_)))
            .collect();

        if let SymbolKind::Function(params) = &func_symbols[0].1.kind {
            assert_eq!(params.len(), 1);
            assert!(params[0].has_default);
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_function_default_with_string_literal() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        hir.add_code(None, "def calc(x = \"test\"): x;");

        let func_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_)))
            .collect();

        if let SymbolKind::Function(params) = &func_symbols[0].1.kind {
            assert_eq!(params.len(), 1);
            assert!(params[0].has_default);
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_function_default_with_boolean_literal() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        hir.add_code(None, "def greet(enabled = true): enabled;");

        let func_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| matches!(s.kind, SymbolKind::Function(_)))
            .collect();

        if let SymbolKind::Function(params) = &func_symbols[0].1.kind {
            assert_eq!(params.len(), 1);
            assert!(params[0].has_default);
        }

        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_url_by_source() {
        let mut hir = Hir::default();
        let url = Url::parse("file:///test.mq").unwrap();
        let (source_id, _) = hir.add_code(Some(url.clone()), "let x = 1");

        assert_eq!(hir.url_by_source(&source_id), Some(&url));
    }

    #[test]
    fn test_url_by_source_builtin_returns_none() {
        let hir = Hir::default();
        // Builtin source has no URL
        assert!(hir.url_by_source(&hir.builtin.source_id).is_none());
    }

    #[test]
    fn test_var_index_assign() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "var arr = [1, 2, 3] | arr[0] = 10 | arr";
        hir.add_code(None, code);

        let var_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Variable && s.value.as_deref() == Some("arr"));
        assert!(var_symbol.is_some(), "Should have a Variable symbol for arr");

        let ref_symbols: Vec<_> = hir
            .symbols()
            .filter(|(_, s)| s.kind == SymbolKind::Ref && s.value.as_deref() == Some("arr"))
            .collect();
        assert!(!ref_symbols.is_empty(), "Should have Ref symbols for arr usage");

        assert!(hir.errors().is_empty(), "Should have no errors");
    }

    #[test]
    fn test_var_index_compound_assign() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "var arr = [1, 2, 3] | arr[0] += 1 | arr";
        hir.add_code(None, code);

        let var_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Variable && s.value.as_deref() == Some("arr"));
        assert!(var_symbol.is_some(), "Should have a Variable symbol for arr");

        assert!(hir.errors().is_empty(), "Should have no errors");
    }

    #[test]
    fn test_dict_index_assign() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = r#"var d = {"a": 1} | d["a"] = 2 | d"#;
        hir.add_code(None, code);

        let var_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Variable && s.value.as_deref() == Some("d"));
        assert!(var_symbol.is_some(), "Should have a Variable symbol for d");

        assert!(hir.errors().is_empty(), "Should have no errors");
    }

    #[test]
    fn test_assign_creates_symbol() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "var x = 10 | x = 20";
        hir.add_code(None, code);

        let assign_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Assign && s.value.as_deref() == Some("="));
        assert!(assign_symbol.is_some(), "Should have an Assign symbol for =");

        assert!(hir.errors().is_empty(), "Should have no errors");
    }

    #[test]
    fn test_compound_assign_creates_symbol() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "var x = 10 | x += 1";
        hir.add_code(None, code);

        let assign_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Assign && s.value.as_deref() == Some("+="));
        assert!(assign_symbol.is_some(), "Should have an Assign symbol for +=");

        assert!(hir.errors().is_empty(), "Should have no errors");
    }
}
