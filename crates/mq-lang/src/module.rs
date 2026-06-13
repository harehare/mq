pub mod error;
pub mod resolver;

use crate::{
    Arena, ArenaId, Program, Shared, TokenArena,
    ast::{node as ast, parser::Parser},
    lexer::{self, Lexer},
    module::{
        error::ModuleError,
        resolver::{DefaultModuleResolver, ModuleResolver},
    },
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::{borrow::Cow, cell::RefCell, path::PathBuf, sync::LazyLock};

use crate::Token;

struct BuiltinCache {
    tokens: Vec<Shared<Token>>,
    module: Module,
}

thread_local! {
    static BUILTIN_CACHE: RefCell<Option<BuiltinCache>> = const { RefCell::new(None) };
}

pub type ModuleId = ArenaId<ModuleName>;

type ModuleName = SmolStr;
type StandardModules = FxHashMap<SmolStr, fn() -> &'static str>;

impl<T: ModuleResolver> Default for ModuleLoader<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

fn get_module_name(name: &str) -> Cow<'static, str> {
    // For common module names, use static strings to avoid allocation
    match name {
        "ast" => Cow::Borrowed("ast.mq"),
        "cbor" => Cow::Borrowed("cbor.mq"),
        "csv" => Cow::Borrowed("csv.mq"),
        "fuzzy" => Cow::Borrowed("fuzzy.mq"),
        "hcl" => Cow::Borrowed("hcl.mq"),
        "json" => Cow::Borrowed("json.mq"),
        "section" => Cow::Borrowed("section.mq"),
        "semver" => Cow::Borrowed("semver.mq"),
        "test" => Cow::Borrowed("test.mq"),
        "table" => Cow::Borrowed("table.mq"),
        "toml" => Cow::Borrowed("toml.mq"),
        "toon" => Cow::Borrowed("toon.mq"),
        "xml" => Cow::Borrowed("xml.mq"),
        "yaml" => Cow::Borrowed("yaml.mq"),
        _ => Cow::Owned(format!("{}.mq", name)),
    }
}

#[derive(Debug, Clone)]
pub struct ModuleLoader<T: ModuleResolver = DefaultModuleResolver> {
    pub(crate) loaded_modules: Arena<ModuleName>,
    #[cfg(feature = "debugger")]
    pub(crate) source_code: Option<String>,
    source_cache: FxHashMap<SmolStr, String>,
    resolver: T,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    pub functions: Program,
    pub modules: Program,
    pub vars: Program,
    pub macros: Program,
}

impl Module {
    pub const BUILTIN_MODULE: &str = "builtin";
    pub const TOP_LEVEL_MODULE: &str = "top-level";
    pub const TOP_LEVEL_MODULE_ID: ArenaId<ModuleName> = ArenaId::new(0);
}

pub static STANDARD_MODULES: LazyLock<StandardModules> = LazyLock::new(|| {
    let mut map = FxHashMap::default();

    macro_rules! std_module {
        ($name:ident) => {
            fn $name() -> &'static str {
                include_str!(concat!("../modules/", stringify!($name), ".mq"))
            }
            map.insert(SmolStr::new(stringify!($name)), $name as fn() -> &'static str);
        };
    }

    std_module!(ast);
    std_module!(cbor);
    std_module!(csv);
    std_module!(fuzzy);
    std_module!(hcl);
    std_module!(json);
    std_module!(section);
    std_module!(semver);
    std_module!(test);
    std_module!(table);
    std_module!(toml);
    std_module!(toon);
    std_module!(xml);
    std_module!(yaml);

    map
});

pub const BUILTIN_FILE: &str = include_str!("../builtin.mq");

impl<T: ModuleResolver> ModuleLoader<T> {
    pub fn new(resolver: T) -> Self {
        let mut loaded_modules = Arena::new(10);
        loaded_modules.alloc(Module::TOP_LEVEL_MODULE.into());

        Self {
            loaded_modules,
            #[cfg(feature = "debugger")]
            source_code: None,
            source_cache: FxHashMap::default(),
            resolver,
        }
    }

    #[inline(always)]
    pub fn module_name(&self, module_id: ModuleId) -> Cow<'static, str> {
        match module_id {
            Module::TOP_LEVEL_MODULE_ID => Cow::Borrowed(Module::TOP_LEVEL_MODULE),
            _ => self
                .loaded_modules
                .get(module_id)
                .map(|s| Cow::Owned(s.to_string()))
                .unwrap_or_else(|| Cow::Borrowed("<unknown>")),
        }
    }

    pub fn get_module_path(&self, module_name: &str) -> Result<String, ModuleError> {
        self.resolver.get_path(module_name)
    }

    #[cfg(feature = "debugger")]
    pub fn set_source_code(&mut self, source_code: String) {
        self.source_code = Some(source_code);
    }

    pub fn search_paths(&self) -> Vec<PathBuf> {
        self.resolver.search_paths()
    }

    pub fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.resolver.set_search_paths(paths);
    }

    pub fn load(&mut self, module_name: &str, code: &str, token_arena: TokenArena) -> Result<Module, ModuleError> {
        if self.loaded_modules.contains(module_name.into()) {
            return Err(ModuleError::AlreadyLoaded(Cow::Owned(module_name.to_string())));
        }

        let module_id = self.loaded_modules.len().into();
        let mut program = Self::parse_program(code, module_id, token_arena)?;

        self.load_from_ast(module_name, &mut program)
    }

    pub fn load_from_ast(&mut self, module_name: &str, program: &mut Program) -> Result<Module, ModuleError> {
        if self.loaded_modules.contains(module_name.into()) {
            return Err(ModuleError::AlreadyLoaded(Cow::Owned(module_name.to_string())));
        }

        let modules = program
            .iter()
            .filter(|node| {
                matches!(
                    *node.expr,
                    ast::Expr::Include(_) | ast::Expr::Module(_, _) | ast::Expr::Import(_)
                )
            })
            .cloned()
            .collect::<Vec<_>>();

        let functions = program
            .iter()
            .filter(|node| matches!(*node.expr, ast::Expr::Def(..)))
            .cloned()
            .collect::<Vec<_>>();

        let vars = program
            .iter()
            .filter(|node| matches!(*node.expr, ast::Expr::Let(..)))
            .cloned()
            .collect::<Vec<_>>();

        let macros = program
            .iter()
            .filter(|node| matches!(*node.expr, ast::Expr::Macro(..)))
            .cloned()
            .collect::<Vec<_>>();

        let expected_len = functions.len() + modules.len() + vars.len() + macros.len();

        if program.len() != expected_len {
            return Err(ModuleError::InvalidModule);
        }

        self.loaded_modules.alloc(module_name.into());

        Ok(Module {
            name: module_name.to_string(),
            functions,
            modules,
            vars,
            macros,
        })
    }

    pub fn canonical_name<'a>(&self, module_path: &'a str) -> &'a str {
        self.resolver.canonical_name(module_path)
    }

    pub fn load_from_file(&mut self, module_path: &str, token_arena: TokenArena) -> Result<Module, ModuleError> {
        // Check before resolving to avoid unnecessary I/O (disk read or network fetch)
        // when the same module is imported more than once.
        let name = self.resolver.canonical_name(module_path).to_owned();
        if self.loaded_modules.contains(name.as_str().into()) {
            return Err(ModuleError::AlreadyLoaded(Cow::Owned(name)));
        }
        let program = self.resolve(module_path)?;
        self.source_cache.insert(SmolStr::new(&name), program.clone());
        self.load(&name, &program, token_arena)
    }

    pub fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        self.resolver.resolve(module_name)
    }

    pub fn load_builtin(&mut self, token_arena: TokenArena) -> Result<Module, ModuleError> {
        if self.loaded_modules.contains(Module::BUILTIN_MODULE.into()) {
            return Err(ModuleError::AlreadyLoaded(Cow::Borrowed(Module::BUILTIN_MODULE)));
        }

        // Cache is only valid when both arenas are in their initial state (builtin
        // module_id == 1, tokens right after the dummy EOF). Fall back to full parse otherwise.
        let pristine = self.loaded_modules.len() == 1 && {
            #[cfg(not(feature = "sync"))]
            {
                token_arena.borrow().len() == 1
            }
            #[cfg(feature = "sync")]
            {
                token_arena.read().unwrap().len() == 1
            }
        };

        if pristine {
            let cached =
                BUILTIN_CACHE.with(|cache| cache.borrow().as_ref().map(|c| (c.tokens.clone(), c.module.clone())));

            if let Some((tokens, module)) = cached {
                {
                    #[cfg(not(feature = "sync"))]
                    token_arena.borrow_mut().extend_from_slice(&tokens);
                    #[cfg(feature = "sync")]
                    token_arena.write().unwrap().extend_from_slice(&tokens);
                }
                self.loaded_modules.alloc(Module::BUILTIN_MODULE.into());
                return Ok(module);
            }
        }

        let module = self.load(Module::BUILTIN_MODULE, BUILTIN_FILE, Shared::clone(&token_arena))?;

        if pristine {
            let tokens = {
                #[cfg(not(feature = "sync"))]
                let arena = token_arena.borrow();
                #[cfg(feature = "sync")]
                let arena = token_arena.read().unwrap();
                arena.as_slice()[1..].iter().map(Shared::clone).collect::<Vec<_>>()
            };

            BUILTIN_CACHE.with(|cache| {
                *cache.borrow_mut() = Some(BuiltinCache {
                    tokens,
                    module: module.clone(),
                });
            });
        }

        Ok(module)
    }

    #[cfg(feature = "debugger")]
    pub fn get_source_code_for_debug(&self, module_id: ModuleId) -> Result<String, ModuleError> {
        let name = self.module_name(module_id);
        match name.as_ref() {
            Module::TOP_LEVEL_MODULE => Ok(self.source_code.clone().unwrap_or_default()),
            Module::BUILTIN_MODULE => Ok(BUILTIN_FILE.to_string()),
            module_name => {
                if let Some(cached) = self.source_cache.get(module_name) {
                    return Ok(cached.clone());
                }
                self.resolve(module_name)
            }
        }
    }

    pub fn get_source_code(&self, module_id: ModuleId, source_code: String) -> Result<String, ModuleError> {
        let name = self.module_name(module_id);
        match name.as_ref() {
            Module::TOP_LEVEL_MODULE => Ok(source_code),
            Module::BUILTIN_MODULE => Ok(BUILTIN_FILE.to_string()),
            module_name => {
                if let Some(cached) = self.source_cache.get(module_name) {
                    return Ok(cached.clone());
                }
                self.resolve(module_name)
            }
        }
    }

    /// Returns the display filename for a module (e.g. `"builtin.mq"`, `"csv.mq"`, `""` for top-level).
    pub fn module_file_name(&self, module_id: ModuleId) -> String {
        let name = self.module_name(module_id);
        match name.as_ref() {
            Module::TOP_LEVEL_MODULE => String::new(),
            other => get_module_name(other).to_string(),
        }
    }

    fn parse_program(code: &str, module_id: ModuleId, token_arena: TokenArena) -> Result<Program, ModuleError> {
        let tokens = Lexer::new(lexer::Options::default()).tokenize(code, module_id)?;
        let mut token_arena = {
            #[cfg(not(feature = "sync"))]
            {
                token_arena.borrow_mut()
            }

            #[cfg(feature = "sync")]
            {
                token_arena.write().unwrap()
            }
        };

        let program = Parser::new(
            tokens.into_iter().map(Shared::new).collect::<Vec<_>>().iter(),
            &mut token_arena,
            module_id,
        )
        .parse()?;

        Ok(program)
    }
}

#[cfg(feature = "http-import")]
impl ModuleLoader<DefaultModuleResolver> {
    /// Replaces the HTTP resolver's domain allowlist.
    pub fn set_http_allowed_domains(&mut self, domains: Vec<String>) {
        self.resolver.set_allowed_domains(domains);
    }

    /// Clears all locally-cached HTTP module files.
    ///
    /// Call this once before processing to force a re-fetch of all cached modules.
    pub fn clear_http_cache(&self) -> Result<(), error::ModuleError> {
        self.resolver.clear_http_cache()
    }
}

#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};
    use smallvec::{SmallVec, smallvec};
    use smol_str::SmolStr;

    use crate::{
        Range, Shared, SharedCell, Token, TokenKind,
        ast::node::{self as ast, IdentWithToken, Param},
        module::resolver::DefaultModuleResolver,
        range::Position,
        token_alloc,
    };

    use super::{Module, ModuleError, ModuleLoader};

    #[fixture]
    fn token_arena() -> Shared<SharedCell<crate::arena::Arena<Shared<Token>>>> {
        Shared::new(SharedCell::new(crate::arena::Arena::new(10)))
    }

    /// Arena that mirrors the engine's initial state: one dummy EOF token at index 0.
    /// Required to exercise the "pristine" cache path in `load_builtin`.
    #[fixture]
    fn pristine_token_arena() -> Shared<SharedCell<crate::arena::Arena<Shared<Token>>>> {
        let arena = Shared::new(SharedCell::new(crate::arena::Arena::new(2048)));
        token_alloc(
            &arena,
            &Shared::new(Token {
                kind: TokenKind::Eof,
                range: Range::default(),
                module_id: Module::TOP_LEVEL_MODULE_ID,
            }),
        );
        arena
    }

    #[rstest]
    #[case::load1("test".to_string(), Err(ModuleError::InvalidModule))]
    #[case::load2("let test = \"value\"".to_string(), Ok(Module{
        name: "test".to_string(),
        functions: Vec::new(),
        modules: Vec::new(),
        vars: vec![
            Shared::new(ast::Node{token_id: 0.into(), expr: Shared::new(ast::Expr::Let(
                ast::Pattern::Ident(IdentWithToken::new_with_token("test", Some(Shared::new(Token{
                    kind: TokenKind::Ident(SmolStr::new("test")),
                    range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}},
                    module_id: 1.into()
                })))),
                Shared::new(ast::Node{token_id: 2.into(), expr: Shared::new(ast::Expr::Literal(ast::Literal::String("value".to_string())))})
            ))})],
        macros: Vec::new(),
    }))]
    #[case::load3("def test(): 1;".to_string(), Ok(Module{
        name: "test".to_string(),
        modules: Vec::new(),
        functions: vec![
            Shared::new(ast::Node{token_id: 0.into(), expr: Shared::new(ast::Expr::Def(
            IdentWithToken::new_with_token("test", Some(Shared::new(Token{
                kind: TokenKind::Ident(SmolStr::new("test")),
                range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}},
                module_id: 1.into()
            }))),
            SmallVec::new(),
            vec![
                Shared::new(ast::Node{token_id: 2.into(), expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(1.into())))})
            ]
            ))})],
        vars: Vec::new(),
        macros: Vec::new(),
    }))]
    #[case::load4("def test(a, b): add(a, b);".to_string(), Ok(Module{
        name: "test".to_string(),
        modules: Vec::new(),
        functions: vec![
            Shared::new(ast::Node{token_id: 0.into(), expr: Shared::new(ast::Expr::Def(
                IdentWithToken::new_with_token("test", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("test")), range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}}, module_id: 1.into()}))),
                smallvec![
                    Param::new(IdentWithToken::new_with_token("a", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("a")), range: Range{start: Position{line: 1, column: 10}, end: Position{line: 1, column: 11}}, module_id: 1.into()})))),
                    Param::new(IdentWithToken::new_with_token("b", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("b")), range: Range{start: Position{line: 1, column: 13}, end: Position{line: 1, column: 14}}, module_id: 1.into()})))),
                ],
                vec![
                    Shared::new(ast::Node{token_id: 4.into(), expr: Shared::new(ast::Expr::Call(
                    IdentWithToken::new_with_token("add", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("add")), range: Range{start: Position{line: 1, column: 17}, end: Position{line: 1, column: 20}}, module_id: 1.into()}))),
                    smallvec![
                        Shared::new(ast::Node{token_id: 2.into(),
                            expr: Shared::new(
                                ast::Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("a")), range: Range{start: Position{line: 1, column: 21}, end: Position{line: 1, column: 22}}, module_id: 1.into()}))))
                                )}),
                        Shared::new(ast::Node{token_id: 3.into(),
                            expr: Shared::new(
                                ast::Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("b")), range: Range{start: Position{line: 1, column: 24}, end: Position{line: 1, column: 25}}, module_id: 1.into()}))))
                            )})
                    ],
                ))})]
            ))})],
        vars: Vec::new(),
        macros: Vec::new(),
    }))]
    fn test_load(
        token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
        #[case] program: String,
        #[case] expected: Result<Module, ModuleError>,
    ) {
        assert_eq!(
            ModuleLoader::new(DefaultModuleResolver::default()).load("test", &program, token_arena),
            expected
        );
    }

    #[rstest]
    #[case::load_standard_csv("csv", Ok(Module {
        name: "csv".to_string(),
        functions: Vec::new(),
        modules: Vec::new(), // Assuming the csv.mq only contains definitions or is empty for this test
        vars: Vec::new(),
        macros: Vec::new(),
    }))]
    fn test_load_standard_module(
        token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
        #[case] module_name: &str,
        #[case] expected: Result<Module, ModuleError>,
    ) {
        let mut loader = ModuleLoader::new(DefaultModuleResolver::default());
        let result = loader.load_from_file(module_name, token_arena.clone());
        // Only check that loading does not return NotFound error and returns Some(Module)
        match expected {
            Ok(_) => {
                assert!(result.is_ok(), "Expected Ok, got {:?}", result);
                assert_eq!(result.unwrap().name, module_name);
            }
            Err(ref e) => {
                assert_eq!(result.unwrap_err(), *e);
            }
        }
    }

    #[test]
    fn test_standard_modules_contains_csv() {
        assert!(super::STANDARD_MODULES.contains_key("csv"));
        let csv_content = super::STANDARD_MODULES.get("csv").unwrap()();
        assert!(csv_content.contains("")); // Just check it's a string, optionally check for expected content
    }

    #[test]
    fn test_load_builtin_idempotent() {
        let token_arena = token_arena();
        let mut loader = ModuleLoader::new(DefaultModuleResolver::default());
        assert!(loader.load_builtin(Shared::clone(&token_arena)).is_ok());
        // Second call on the same loader must return AlreadyLoaded, not corrupt state.
        assert!(matches!(
            loader.load_builtin(Shared::clone(&token_arena)),
            Err(ModuleError::AlreadyLoaded(_))
        ));
    }

    #[test]
    fn test_load_builtin_non_pristine_falls_back_to_parse() {
        // Load another module first so the arenas are no longer in their initial state.
        let token_arena = token_arena();
        let mut loader = ModuleLoader::new(DefaultModuleResolver::default());
        loader
            .load("other", "def dummy(): 1;", Shared::clone(&token_arena))
            .expect("should load other module");

        // load_builtin must still succeed even though the arenas are non-pristine.
        let result = loader.load_builtin(Shared::clone(&token_arena));
        assert!(result.is_ok(), "load_builtin failed on non-pristine state: {result:?}");

        let module = result.unwrap();
        assert_eq!(module.name, Module::BUILTIN_MODULE);
    }

    /// Token arena size must be the same whether the builtin module was loaded from a fresh
    /// parse or replayed from the thread-local cache.
    #[rstest]
    fn test_load_builtin_cache_arena_size_consistent(
        pristine_token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
    ) {
        let arena1 = pristine_token_arena;
        let mut loader1 = ModuleLoader::new(DefaultModuleResolver::default());
        loader1.load_builtin(Shared::clone(&arena1)).unwrap();
        #[cfg(not(feature = "sync"))]
        let size1 = arena1.borrow().len();
        #[cfg(feature = "sync")]
        let size1 = arena1.read().unwrap().len();

        let arena2 = Shared::new(SharedCell::new(crate::arena::Arena::new(2048)));
        token_alloc(
            &arena2,
            &Shared::new(Token {
                kind: TokenKind::Eof,
                range: Range::default(),
                module_id: Module::TOP_LEVEL_MODULE_ID,
            }),
        );
        let mut loader2 = ModuleLoader::new(DefaultModuleResolver::default());
        loader2.load_builtin(Shared::clone(&arena2)).unwrap();
        #[cfg(not(feature = "sync"))]
        let size2 = arena2.borrow().len();
        #[cfg(feature = "sync")]
        let size2 = arena2.read().unwrap().len();

        assert_eq!(size1, size2, "arena size must match between cache and fresh parse");
        assert!(size1 > 1, "builtin tokens must be added to the arena");
    }

    /// The module returned from cache must have the same function/var/macro counts as a fresh parse.
    #[rstest]
    fn test_load_builtin_cache_module_counts_consistent(
        pristine_token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
    ) {
        let mut loader1 = ModuleLoader::new(DefaultModuleResolver::default());
        let module1 = loader1.load_builtin(pristine_token_arena).unwrap();

        let arena2 = Shared::new(SharedCell::new(crate::arena::Arena::new(2048)));
        token_alloc(
            &arena2,
            &Shared::new(Token {
                kind: TokenKind::Eof,
                range: Range::default(),
                module_id: Module::TOP_LEVEL_MODULE_ID,
            }),
        );
        let mut loader2 = ModuleLoader::new(DefaultModuleResolver::default());
        let module2 = loader2.load_builtin(arena2).unwrap();

        assert_eq!(module1.name, module2.name);
        assert_eq!(module1.functions.len(), module2.functions.len());
        assert_eq!(module1.vars.len(), module2.vars.len());
        assert_eq!(module1.macros.len(), module2.macros.len());
        assert_eq!(module1.modules.len(), module2.modules.len());
    }

    /// After load_builtin, the builtin module must be registered at loaded_modules index 1
    /// (TOP_LEVEL_MODULE is always 0).
    #[rstest]
    fn test_load_builtin_module_registered_at_id_one(
        pristine_token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
    ) {
        let mut loader = ModuleLoader::new(DefaultModuleResolver::default());
        loader.load_builtin(pristine_token_arena).unwrap();

        assert_eq!(loader.loaded_modules.len(), 2);
        assert!(loader.loaded_modules.contains(Module::BUILTIN_MODULE.into()));
    }

    /// All tokens injected from cache must carry module_id == 1 (BUILTIN_MODULE_ID),
    /// so that error diagnostics resolve to the builtin source file rather than garbage.
    #[rstest]
    fn test_load_builtin_cache_tokens_have_builtin_module_id(
        pristine_token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
    ) {
        let mut loader1 = ModuleLoader::new(DefaultModuleResolver::default());
        loader1.load_builtin(pristine_token_arena).unwrap();

        // Second pristine load — this is the cache-hit path.
        let arena2 = Shared::new(SharedCell::new(crate::arena::Arena::new(2048)));
        token_alloc(
            &arena2,
            &Shared::new(Token {
                kind: TokenKind::Eof,
                range: Range::default(),
                module_id: Module::TOP_LEVEL_MODULE_ID,
            }),
        );
        let mut loader2 = ModuleLoader::new(DefaultModuleResolver::default());
        loader2.load_builtin(Shared::clone(&arena2)).unwrap();

        let builtin_module_id: crate::ModuleId = 1.into();
        #[cfg(not(feature = "sync"))]
        let arena = arena2.borrow();
        #[cfg(feature = "sync")]
        let arena = arena2.read().unwrap();
        for token in arena.as_slice()[1..].iter() {
            assert_eq!(
                token.module_id, builtin_module_id,
                "cached builtin token must have BUILTIN_MODULE_ID"
            );
        }
    }
}
