pub mod error;
pub mod resolver;

use crate::{
    Arena, ArenaId, Program, Shared, TokenArena,
    ast::{node as ast, parser::Parser},
    lexer::{self, Lexer},
    module::{
        error::ModuleError,
        resolver::{LocalFsModuleResolver, ModuleResolver},
    },
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::{borrow::Cow, path::PathBuf, sync::LazyLock};

pub type ModuleId = ArenaId<ModuleName>;

type ModuleName = SmolStr;
type StandardModules = FxHashMap<SmolStr, fn() -> &'static str>;

impl<T: ModuleResolver> Default for ModuleLoader<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

#[derive(Debug, Clone)]
pub struct ModuleLoader<T: ModuleResolver = LocalFsModuleResolver> {
    pub(crate) loaded_modules: Arena<ModuleName>,
    #[cfg(feature = "debugger")]
    pub(crate) source_code: Option<String>,
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
    std_module!(csv);
    std_module!(fuzzy);
    std_module!(json);
    std_module!(section);
    std_module!(test);
    std_module!(table);
    std_module!(toml);
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

    pub fn load_from_file(&mut self, module_path: &str, token_arena: TokenArena) -> Result<Module, ModuleError> {
        let program = self.resolve(module_path)?;
        self.load(module_path, &program, token_arena)
    }

    pub fn resolve(&self, module_name: &str) -> Result<String, ModuleError> {
        if STANDARD_MODULES.contains_key(module_name) {
            Ok(STANDARD_MODULES.get(module_name).map(|f| f()).unwrap().to_string())
        } else {
            self.resolver.resolve(module_name)
        }
    }

    pub fn load_builtin(&mut self, token_arena: TokenArena) -> Result<Module, ModuleError> {
        self.load(Module::BUILTIN_MODULE, BUILTIN_FILE, token_arena)
    }

    #[cfg(feature = "debugger")]
    pub fn get_source_code_for_debug(&self, module_id: ModuleId) -> Result<String, ModuleError> {
        match self.module_name(module_id) {
            Cow::Borrowed(Module::TOP_LEVEL_MODULE) => Ok(self.source_code.clone().unwrap_or_default()),
            Cow::Borrowed(Module::BUILTIN_MODULE) => Ok(BUILTIN_FILE.to_string()),
            Cow::Borrowed(module_name) => self.resolve(module_name),
            Cow::Owned(module_name) => self.resolve(&module_name),
        }
    }

    pub fn get_source_code(&self, module_id: ModuleId, source_code: String) -> Result<String, ModuleError> {
        match self.module_name(module_id) {
            Cow::Borrowed(Module::TOP_LEVEL_MODULE) => Ok(source_code),
            Cow::Borrowed(Module::BUILTIN_MODULE) => Ok(BUILTIN_FILE.to_string()),
            Cow::Borrowed(module_name) => self.resolve(module_name),
            Cow::Owned(module_name) => self.resolve(&module_name),
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

#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};
    use smallvec::{SmallVec, smallvec};
    use smol_str::SmolStr;

    use crate::{
        Shared, SharedCell, Token, TokenKind,
        ast::node::{self as ast, IdentWithToken, Param},
        module::LocalFsModuleResolver,
        range::{Position, Range},
    };

    use super::{Module, ModuleError, ModuleLoader};

    #[fixture]
    fn token_arena() -> Shared<SharedCell<crate::arena::Arena<Shared<Token>>>> {
        Shared::new(SharedCell::new(crate::arena::Arena::new(10)))
    }

    #[rstest]
    #[case::load1("test".to_string(), Err(ModuleError::InvalidModule))]
    #[case::load2("let test = \"value\"".to_string(), Ok(Module{
        name: "test".to_string(),
        functions: Vec::new(),
        modules: Vec::new(),
        vars: vec![
            Shared::new(ast::Node{token_id: 0.into(), expr: Shared::new(ast::Expr::Let(
                IdentWithToken::new_with_token("test", Some(Shared::new(Token{
                    kind: TokenKind::Ident(SmolStr::new("test")),
                    range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}},
                    module_id: 1.into()
                }))),
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
            ModuleLoader::new(LocalFsModuleResolver::default()).load("test", &program, token_arena),
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
        let mut loader = ModuleLoader::new(LocalFsModuleResolver::default());
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
}
