use rustc_hash::{FxHashMap, FxHashSet};
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
    /// Refs resolved via included sources rather than their own scope chain.
    pub(crate) fallback_references: FxHashSet<SymbolId>,
    pub(crate) source_symbols: FxHashMap<SourceId, Vec<SymbolId>>,
    pub(crate) symbol_insertion_counter: u32,
    /// Declarations a reference can resolve to, by name.
    pub(crate) name_index: FxHashMap<SmolStr, Vec<SymbolId>>,
    /// Same ids as `name_index`, grouped by the scope that declares them.
    /// Child scopes of each scope that leave their `let` bindings visible after the construct.
    pub(crate) leaking_scopes: FxHashMap<ScopeId, Vec<ScopeId>>,
    /// For each call `f(x)[k]`, how many of its trailing arguments are bracket-access keys.
    pub(crate) bracket_key_counts: FxHashMap<SymbolId, usize>,
    pub(crate) scope_name_index: FxHashMap<ScopeId, FxHashMap<SmolStr, Vec<SymbolId>>>,
}

impl Default for Hir {
    fn default() -> Self {
        Self::new(mq_lang::ModuleLoader::default())
    }
}

impl Hir {
    /// Creates a new `Hir` instance.
    ///
    /// # Parameters
    /// - `module_loader`: The module loader used to resolve and load external modules during compilation.
    pub fn new(module_loader: mq_lang::ModuleLoader) -> Self {
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
            module_loader,
            source_scopes,
            references: FxHashMap::default(),
            fallback_references: FxHashSet::default(),
            source_symbols: FxHashMap::default(),
            symbol_insertion_counter: 0,
            name_index: FxHashMap::default(),
            leaking_scopes: FxHashMap::default(),
            bracket_key_counts: FxHashMap::default(),
            scope_name_index: FxHashMap::default(),
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

    /// Declares a value the host defines at runtime (like `Engine::define_string_value` or
    /// `register_fn`), so references to `name` resolve. Does nothing when builtins are disabled.
    pub fn declare_global(&mut self, name: &str) {
        self.add_builtin();
        if self.builtin.disabled {
            return;
        }
        self.add_symbol(Symbol {
            value: Some(name.into()),
            kind: SymbolKind::Variable,
            source: SourceInfo::new(Some(self.builtin.source_id), None),
            scope: self.builtin.scope_id,
            doc: Vec::new(),
            parent: None,
            insertion_order: 0,
        });
        self.resolve();
    }

    pub fn add_builtin(&mut self) {
        if self.builtin.loaded || self.builtin.disabled {
            return;
        }

        self.builtin.loaded = true;

        let (nodes, _) = mq_lang::parse_recovery(mq_lang::BUILTIN_MODULE_FILE);

        nodes.iter().for_each(|node| {
            self.add_expr(node, self.builtin.source_id, self.builtin.scope_id, None);
        });

        let source_id = self.builtin.source_id;
        let scope_id = self.builtin.scope_id;

        for doc in self
            .builtin
            .docs
            .iter()
            .filter(|doc| doc.is_available(mq_lang::is_builtin_function))
        {
            for name in doc.names() {
                let kind = match doc.kind {
                    mq_help::DocKind::Function | mq_help::DocKind::Internal => {
                        SymbolKind::Function(doc.params.iter().map(|p| (*p).into()).collect::<Vec<_>>())
                    }
                    mq_help::DocKind::Selector => {
                        let token = mq_lang::Token::new(mq_lang::TokenKind::Selector(name.into()));
                        match mq_lang::Selector::try_from(&token) {
                            Ok(selector) => SymbolKind::Selector(selector),
                            Err(_) => continue,
                        }
                    }
                };
                self.add_symbol(Symbol {
                    value: Some(name.into()),
                    kind,
                    source: SourceInfo::new(Some(source_id), None),
                    scope: scope_id,
                    doc: vec![(mq_lang::Range::default(), doc.description.to_string())],
                    parent: None,
                    insertion_order: 0,
                });
            }
        }
    }

    pub fn add_nodes(&mut self, url: Url, nodes: &[mq_lang::Shared<mq_lang::CstNode>]) -> (SourceId, ScopeId) {
        self.add_builtin();

        let source_id = self
            .source_by_url(&url)
            .inspect(|source_id| self.remove_source_contents(*source_id))
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
        self.source_scopes.get(source_id).copied()
    }

    /// Removes a source's symbols and nested scopes, keeping its module scope for reuse.
    fn remove_source_contents(&mut self, source_id: SourceId) {
        self.symbols
            .retain(|_, symbol| symbol.source.source_id != Some(source_id));
        self.source_symbols.remove(&source_id);

        let module_scope_id = self.scope_by_source(&source_id);
        self.scopes
            .retain(|scope_id, scope| scope.source.source_id != Some(source_id) || Some(scope_id) == module_scope_id);
        if let Some(scope) = module_scope_id.and_then(|id| self.scopes.get_mut(id)) {
            scope.children.clear();
        }
        let scopes = &self.scopes;
        self.leaking_scopes.retain(|parent, children| {
            children.retain(|child| scopes.contains_key(*child));
            scopes.contains_key(*parent) && !children.is_empty()
        });

        let symbols = &self.symbols;
        self.bracket_key_counts.retain(|call, _| symbols.contains_key(*call));
        self.references
            .retain(|ref_id, def_id| symbols.contains_key(*ref_id) && symbols.contains_key(*def_id));
        let references = &self.references;
        self.fallback_references
            .retain(|ref_id| references.contains_key(ref_id));
        self.name_index.retain(|_, ids| {
            ids.retain(|id| symbols.contains_key(*id));
            !ids.is_empty()
        });
        self.scope_name_index.retain(|_, names| {
            names.retain(|_, ids| {
                ids.retain(|id| symbols.contains_key(*id));
                !ids.is_empty()
            });
            !names.is_empty()
        });
    }

    /// Whether a `let` in this scope stays visible to the pipe steps after its construct:
    /// the bodies of `if`/`elif`/`else`/`unless` and loops, but not `match` arms or functions.
    fn scope_leaks_bindings(&self, kind: &ScopeKind) -> bool {
        let (ScopeKind::Block(owner) | ScopeKind::Loop(owner)) = kind else {
            return false;
        };
        self.symbols.get(*owner).is_some_and(|owner| {
            matches!(
                owner.kind,
                SymbolKind::If
                    | SymbolKind::Elif
                    | SymbolKind::Else
                    | SymbolKind::Unless
                    | SymbolKind::While
                    | SymbolKind::Until
                    | SymbolKind::Loop
                    | SymbolKind::Foreach
            )
        })
    }

    fn add_scope(&mut self, scope: Scope) -> ScopeId {
        let parent_scope_id = scope.parent_id;
        let leaks = self.scope_leaks_bindings(&scope.kind);
        let scope_id = self.scopes.insert(scope);
        if let Some(parent_scope_id) = parent_scope_id.filter(|_| leaks) {
            self.leaking_scopes.entry(parent_scope_id).or_default().push(scope_id);
        }

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
        let symbol = &self.symbols[symbol_id];
        if let Some(name) = symbol.value.as_ref().filter(|_| Self::is_resolvable_target(symbol)) {
            self.name_index.entry(name.clone()).or_default().push(symbol_id);
            self.scope_name_index
                .entry(symbol.scope)
                .or_default()
                .entry(name.clone())
                .or_default()
                .push(symbol_id);
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
    #[case::none("first(xs)", 0)]
    #[case::extra_argument("first(xs, 1)", 0)]
    #[case::string_key(r#"first(xs)["k"]"#, 1)]
    #[case::symbol_key("first(xs)[:k]", 1)]
    #[case::index("first(xs)[0]", 1)]
    #[case::chained(r#"f(xs)["a"]["b"]"#, 2)]
    fn test_bracket_key_count_tells_keys_from_arguments(#[case] code: &str, #[case] expected: usize) {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir.add_code(None, code);
        let (call, _) = hir
            .symbols()
            .find(|(_, symbol)| symbol.kind == SymbolKind::Call)
            .unwrap();
        assert_eq!(hir.bracket_key_count(call), expected);
    }

    #[rstest]
    #[case::index_on_a_dict_literal_in_a_dict_value(r#"{"a": {"a": 1}["a"]}"#)]
    #[case::unclosed_dict(r#"{"a": 1, "b": "#)]
    #[case::stray_token_in_a_dict(r#"{"a": 1 ] "b": 2}"#)]
    fn test_syntax_errors_inside_a_dict_do_not_panic(#[case] code: &str) {
        let mut hir = Hir::default();
        hir.add_code(None, code);
    }

    #[test]
    fn test_feature_gated_builtins_are_defined_only_when_enabled() {
        let mut hir = Hir::default();
        hir.add_builtin();
        let defined: std::collections::BTreeSet<&str> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Function(_)))
            .filter_map(|(_, symbol)| symbol.value.as_deref())
            .collect();

        for doc in mq_help::BUILTIN_DOC.iter().filter(|doc| doc.capability.is_some()) {
            assert_eq!(
                defined.contains(doc.name),
                mq_lang::is_builtin_function(doc.name),
                "{} is defined iff its feature is on",
                doc.name
            );
        }
    }

    #[test]
    fn test_selector_aliases_and_attributes_are_defined() {
        let mut hir = Hir::default();
        hir.add_builtin();
        let defined: std::collections::BTreeSet<&str> = hir
            .symbols()
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::Selector(_)))
            .filter_map(|(_, symbol)| symbol.value.as_deref())
            .collect();

        for name in mq_lang::SELECTOR_NAMES {
            assert!(defined.contains(name), "{name} is not defined as a selector");
        }
    }

    #[test]
    fn test_declare_global_resolves_host_defined_names() {
        let mut hir = Hir::default();
        hir.declare_global("TEST_FILE");
        hir.add_code(None, "TEST_FILE | to_string()");
        assert!(hir.errors().is_empty());

        let mut hir = Hir::default();
        hir.add_code(None, "TEST_FILE");
        assert_eq!(hir.errors().len(), 1);
        hir.declare_global("TEST_FILE");
        assert!(hir.errors().is_empty());
    }

    #[test]
    fn test_declare_global_is_ignored_when_builtins_are_disabled() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir.declare_global("TEST_FILE");
        hir.add_code(None, "TEST_FILE");
        assert_eq!(hir.errors().len(), 1);
    }

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
    #[case::until_("until (true): 1;", "until", SymbolKind::Until)]
    #[case::unless_("unless (true): 1;", "unless", SymbolKind::Unless)]
    #[case::foreach("foreach(x, y): 1;", "foreach", SymbolKind::Foreach)]
    #[case::call("foo()", "foo", SymbolKind::Call)]
    #[case::elif_("if (true): 1 elif (false): 2 else: 3;", "elif", SymbolKind::Elif)]
    #[case::else_("if (true): 1 else: 2;", "else", SymbolKind::Else)]
    #[case::literal("42", "42", SymbolKind::Number)]
    #[case::selector(".h", ".h", SymbolKind::Selector(mq_lang::Selector::Heading(None)))]
    #[case::selector(".code.lang", ".code", SymbolKind::Selector(mq_lang::Selector::Code))]
    #[case::standalone_attr_selector(
        ".lang",
        ".lang",
        SymbolKind::Selector(mq_lang::Selector::Attr(mq_lang::AttrKind::Lang))
    )]
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
    #[case::break_in_until("until (true): break;", "break", SymbolKind::Keyword)]
    #[case::continue_in_until("until (true): continue;", "continue", SymbolKind::Keyword)]
    #[case::block("do \"hello\" end", "hello", SymbolKind::String)]
    #[case::try_("try: 1 catch: 2", "try", SymbolKind::Try)]
    #[case::catch_("try: 1 catch: 2", "catch", SymbolKind::Catch)]
    #[case::catch_with_binder("try: 1 catch(e): e", "catch", SymbolKind::Catch)]
    #[case::catch_error_binder_param("try: 1 catch(e): e", "e", SymbolKind::Parameter)]
    #[case::symbol_ident(":foo", "foo", SymbolKind::Symbol)]
    #[case::symbol_string(":\"hello\"", "hello", SymbolKind::Symbol)]
    #[case::pattern_match("match (v): | [1,2,3]: 1 end", "match", SymbolKind::Match)]
    #[case::pattern_match_arm("match (v): | 1: \"one\" end", "1", SymbolKind::Pattern { is_dict: false, is_or: false })]
    #[case::import("import \"foo\"", "foo", SymbolKind::Import(SourceId::default()))]
    #[case::import_as("import \"foo\" as bar", "foo", SymbolKind::Import(SourceId::default()))]
    #[case::import_as_alias_ident("import \"foo\" as bar", "bar", SymbolKind::Ident)]
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
    #[case::until_("until (true): 1;", mq_lang::Position::new(1, 1), "until", SymbolKind::Until)]
    #[case::unless_("unless (true): 1;", mq_lang::Position::new(1, 1), "unless", SymbolKind::Unless)]
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

    #[rstest]
    #[case::bare("fn: self")]
    #[case::call_arg("map([1], fn: self + 1)")]
    #[case::nested("map([[1]], fn: map(fn: self + 1))")]
    fn test_fn_without_params_takes_one_argument(#[case] code: &str) {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir.add_code(None, code);

        assert!(
            hir.symbols()
                .any(|(_, s)| matches!(&s.kind, SymbolKind::Function(params) if params.len() == 1))
        );
        assert!(hir.symbols().all(|(_, s)| s.kind != SymbolKind::Parameter));
    }

    #[test]
    fn test_fn_with_empty_params_takes_no_argument() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir.add_code(None, "fn(): 1");

        assert!(
            hir.symbols()
                .any(|(_, s)| matches!(&s.kind, SymbolKind::Function(params) if params.is_empty()))
        );
    }

    #[test]
    fn test_catch_error_binder_resolution() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "try: 1 catch(e): e";
        hir.add_code(None, code);

        let binder_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Parameter && s.value.as_deref() == Some("e"));
        assert!(binder_symbol.is_some(), "catch(e) should declare e as a Parameter");

        let ref_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Ref && s.value.as_deref() == Some("e"));
        assert!(ref_symbol.is_some(), "Should have a Ref symbol for e in the catch body");

        let (ref_id, _) = ref_symbol.unwrap();
        let resolved = hir.resolve_reference_symbol(ref_id);

        assert_eq!(
            resolved,
            Some(binder_symbol.unwrap().0),
            "e in the catch body should resolve to the catch(e) binder"
        );
        assert!(hir.errors().is_empty(), "Should have no unresolved symbols");
    }

    #[test]
    fn test_catch_error_binder_shadows_outer_variable() {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;

        let code = "let e = \"hello\" | try: 1 catch(e): e";
        hir.add_code(None, code);

        let outer_e = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Variable && s.value.as_deref() == Some("e"))
            .unwrap()
            .0;
        let binder_e = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Parameter && s.value.as_deref() == Some("e"))
            .unwrap()
            .0;

        let ref_symbol = hir
            .symbols()
            .find(|(_, s)| s.kind == SymbolKind::Ref && s.value.as_deref() == Some("e"));
        let (ref_id, _) = ref_symbol.unwrap();
        let resolved = hir.resolve_reference_symbol(ref_id);

        assert_eq!(
            resolved,
            Some(binder_e),
            "e inside the catch body should resolve to the catch(e) binder, not the outer `let e`"
        );
        assert_ne!(resolved, Some(outer_e));
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
            .filter(|(_, symbol)| matches!(symbol.kind, SymbolKind::MatchArm { .. }))
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

    /// (ref name, ref line, target name, target line), independent of slot ids.
    type ResolvedPair = (Option<SmolStr>, Option<u32>, Option<SmolStr>, Option<u32>);

    fn resolved_pairs(hir: &Hir) -> Vec<ResolvedPair> {
        let line = |symbol: &Symbol| symbol.source.text_range.map(|r| r.start.line);
        hir.references
            .iter()
            .map(|(ref_id, def_id)| {
                let (r, d) = (&hir.symbols[*ref_id], &hir.symbols[*def_id]);
                (r.value.clone(), line(r), d.value.clone(), line(d))
            })
            .sorted()
            .collect()
    }

    #[rstest]
    #[case::def("def f(x): let y = x + 1 | y; | f(1)")]
    #[case::nested("def f(x): fn(y): do let z = x + y | z end; end | f(1)")]
    #[case::match_arm("match (1): | [a, b]: a + b | _: upcase(\"a\") end")]
    #[case::builtin_calls("map([1, 2], fn(v): add(v, 1);) | to_string()")]
    fn test_re_adding_source_matches_fresh_hir(#[case] code: &str) {
        let url = Url::parse("file:///test.mq").unwrap();
        let (nodes, _) = mq_lang::parse_recovery(code);

        let mut fresh = Hir::default();
        fresh.add_nodes(url.clone(), &nodes);

        let mut reused = Hir::default();
        reused.add_nodes(url.clone(), &nodes);
        reused.add_nodes(url.clone(), &nodes);
        reused.add_nodes(url, &nodes);

        assert_eq!(
            reused.scopes.len(),
            fresh.scopes.len(),
            "scopes must not leak on re-add"
        );
        assert_eq!(reused.symbols.len(), fresh.symbols.len());
        assert_eq!(resolved_pairs(&reused), resolved_pairs(&fresh));
    }

    #[test]
    fn test_find_scope_in_position_after_re_adding_source() {
        let url = Url::parse("file:///test.mq").unwrap();
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir.add_nodes(url.clone(), &mq_lang::parse_recovery("def a(): 1; | def b(): 2;").0);
        let (source_id, _) = hir.add_nodes(url, &mq_lang::parse_recovery("def f(x):\n  fn(y): x + y;;").0);

        let (_, scope) = hir
            .find_scope_in_position(source_id, mq_lang::Position { line: 2, column: 12 })
            .unwrap();
        // Innermost is the `fn`, nested inside `def f`.
        assert!(matches!(scope.kind, ScopeKind::Function(_)));
        assert!(matches!(
            hir.scopes[scope.parent_id.unwrap()].kind,
            ScopeKind::Function(_)
        ));
    }
}
