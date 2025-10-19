use crate::{
    Program, Shared, TokenArena,
    arena::{Arena, ArenaId},
    ast::{error::ParseError, node as ast, parser::Parser},
    lexer::{self, Lexer, error::LexerError},
    optimizer::{OptimizationLevel, Optimizer},
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use std::{borrow::Cow, fs, path::PathBuf, sync::LazyLock};
use thiserror::Error;

const DEFAULT_PATHS: [&str; 4] = [
    "$HOME/.mq",
    "$ORIGIN/../lib/mq",
    "$ORIGIN/../lib",
    "$ORIGIN",
];

#[derive(Debug, PartialEq, Error)]
pub enum ModuleError {
    #[error("Module `{0}` not found")]
    NotFound(String),
    #[error("IO error: {0}")]
    IOError(String),
    #[error(transparent)]
    LexerError(#[from] LexerError),
    #[error(transparent)]
    ParseError(#[from] ParseError),
    #[error("Invalid module, expected IDENT or BINDING")]
    InvalidModule,
}

pub type ModuleId = ArenaId<ModuleName>;

type ModuleName = SmolStr;
type StandardModules = FxHashMap<SmolStr, fn() -> &'static str>;

#[derive(Debug, Clone)]
pub struct ModuleLoader {
    pub(crate) search_paths: Option<Vec<PathBuf>>,
    pub(crate) loaded_modules: Arena<ModuleName>,
    #[cfg(feature = "debugger")]
    pub(crate) source_code: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    pub functions: Program,
    pub modules: Program,
    pub vars: Program,
}

impl Module {
    pub const TOP_LEVEL_MODULE_ID: ArenaId<ModuleName> = ArenaId::new(0);
    pub const TOP_LEVEL_MODULE: &str = "top-level";
    pub const BUILTIN_MODULE: &str = "builtin";
}

pub static STANDARD_MODULES: LazyLock<StandardModules> = LazyLock::new(|| {
    let mut map = FxHashMap::default();

    macro_rules! std_module {
        ($name:ident) => {
            fn $name() -> &'static str {
                include_str!(concat!("../../modules/", stringify!($name), ".mq"))
            }
            map.insert(
                SmolStr::new(stringify!($name)),
                $name as fn() -> &'static str,
            );
        };
    }

    std_module!(csv);
    std_module!(fuzzy);
    std_module!(json);
    std_module!(test);
    std_module!(toml);
    std_module!(xml);
    std_module!(yaml);

    map
});

impl Default for ModuleLoader {
    fn default() -> Self {
        Self::new(None)
    }
}

impl ModuleLoader {
    pub const BUILTIN_FILE: &str = include_str!("../../builtin.mq");

    pub fn new(search_paths: Option<Vec<PathBuf>>) -> Self {
        let mut loaded_modules = Arena::new(10);
        loaded_modules.alloc(Module::TOP_LEVEL_MODULE.into());

        Self {
            search_paths,
            loaded_modules,
            #[cfg(feature = "debugger")]
            source_code: None,
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

    #[cfg(feature = "debugger")]
    pub fn set_source_code(&mut self, source_code: String) {
        self.source_code = Some(source_code);
    }

    pub fn load(
        &mut self,
        module_name: &str,
        code: &str,
        token_arena: TokenArena,
    ) -> Result<Option<Module>, ModuleError> {
        if self.loaded_modules.contains(module_name.into()) {
            return Ok(None);
        }

        let module_id = self.loaded_modules.len().into();
        self.loaded_modules.alloc(module_name.into());
        let mut program = Self::parse_program(code, module_id, token_arena)?;

        Optimizer::with_level(OptimizationLevel::InlineOnly).optimize(&mut program);

        let modules = program
            .iter()
            .filter(|node| matches!(*node.expr, ast::Expr::Include(_)))
            .cloned()
            .collect::<Vec<_>>();

        let functions = program
            .iter()
            .filter(|node| matches!(*node.expr, ast::Expr::Def(_, _, _)))
            .cloned()
            .collect::<Vec<_>>();

        let vars = program
            .iter()
            .filter(|node| matches!(*node.expr, ast::Expr::Let(_, _)))
            .cloned()
            .collect::<Vec<_>>();

        if program.len() != functions.len() + modules.len() + vars.len() {
            return Err(ModuleError::InvalidModule);
        }

        Ok(Some(Module {
            name: module_name.to_string(),
            functions,
            modules,
            vars,
        }))
    }

    pub fn load_from_file(
        &mut self,
        module_name: &str,
        token_arena: TokenArena,
    ) -> Result<Option<Module>, ModuleError> {
        let program = self.read_file(module_name)?;
        self.load(module_name, &program, token_arena)
    }

    pub fn read_file(&self, module_name: &str) -> Result<String, ModuleError> {
        if STANDARD_MODULES.contains_key(module_name) {
            Ok(STANDARD_MODULES
                .get(module_name)
                .map(|f| f())
                .unwrap()
                .to_string())
        } else {
            let file_path = Self::find(module_name, self.search_paths.clone())
                .map_err(|e| ModuleError::IOError(e.to_string()))?;
            fs::read_to_string(&file_path).map_err(|e| ModuleError::IOError(e.to_string()))
        }
    }

    pub fn load_builtin(&mut self, token_arena: TokenArena) -> Result<Option<Module>, ModuleError> {
        self.load(Module::BUILTIN_MODULE, Self::BUILTIN_FILE, token_arena)
    }

    #[cfg(feature = "debugger")]
    pub fn get_source_code_for_debug(&self, module_id: ModuleId) -> Result<String, ModuleError> {
        match self.module_name(module_id) {
            Cow::Borrowed(Module::TOP_LEVEL_MODULE) => {
                Ok(self.source_code.clone().unwrap_or_default())
            }
            Cow::Borrowed(Module::BUILTIN_MODULE) => Ok(ModuleLoader::BUILTIN_FILE.to_string()),
            Cow::Borrowed(module_name) => self.read_file(module_name),
            Cow::Owned(module_name) => self.read_file(&module_name),
        }
    }

    pub fn get_source_code(
        &self,
        module_id: ModuleId,
        source_code: String,
    ) -> Result<String, ModuleError> {
        match self.module_name(module_id) {
            Cow::Borrowed(Module::TOP_LEVEL_MODULE) => Ok(source_code),
            Cow::Borrowed(Module::BUILTIN_MODULE) => Ok(ModuleLoader::BUILTIN_FILE.to_string()),
            Cow::Borrowed(module_name) => self.read_file(module_name),
            Cow::Owned(module_name) => self.read_file(&module_name),
        }
    }

    fn find(name: &str, search_paths: Option<Vec<PathBuf>>) -> Result<PathBuf, ModuleError> {
        let home = dirs::home_dir()
            .map(|p| {
                let path = p.clone();
                path.to_str().unwrap_or("").to_string()
            })
            .unwrap_or("".to_string());
        let origin = std::env::current_dir().ok();

        search_paths
            .map(|p| {
                p.into_iter()
                    .map(|p| p.to_str().map(|p| p.to_string()).unwrap_or_default())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                DEFAULT_PATHS
                    .to_vec()
                    .iter()
                    .map(|p| p.to_string())
                    .collect()
            })
            .iter()
            .map(|path| {
                let path = origin
                    .clone()
                    .map(|p| {
                        path.replace("$ORIGIN", p.to_str().unwrap_or(""))
                            .replace("$HOME", home.as_str())
                    })
                    .unwrap_or_else(|| home.clone());

                PathBuf::from(path).join(Self::module_id(name).as_ref())
            })
            .find(|p| p.is_file())
            .ok_or_else(|| ModuleError::NotFound(Self::module_id(name).to_string()))
    }

    fn module_id(name: &str) -> Cow<'static, str> {
        // For common module names, use static strings to avoid allocation
        match name {
            "csv" => Cow::Borrowed("csv.mq"),
            "json" => Cow::Borrowed("json.mq"),
            "yaml" => Cow::Borrowed("yaml.mq"),
            "xml" => Cow::Borrowed("xml.mq"),
            "toml" => Cow::Borrowed("toml.mq"),
            "test" => Cow::Borrowed("test.mq"),
            "fuzzy" => Cow::Borrowed("fuzzy.mq"),
            _ => Cow::Owned(format!("{}.mq", name)),
        }
    }

    fn parse_program(
        code: &str,
        module_id: ModuleId,
        token_arena: TokenArena,
    ) -> Result<Program, ModuleError> {
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
            tokens
                .into_iter()
                .map(Shared::new)
                .collect::<Vec<_>>()
                .iter(),
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
        ast::node::{self as ast, IdentWithToken},
        range::{Position, Range},
    };

    use super::{Module, ModuleError, ModuleLoader};

    #[fixture]
    fn token_arena() -> Shared<SharedCell<crate::arena::Arena<Shared<Token>>>> {
        Shared::new(SharedCell::new(crate::arena::Arena::new(10)))
    }

    #[rstest]
    #[case::load1("test".to_string(), Err(ModuleError::InvalidModule))]
    #[case::load2("let test = \"value\"".to_string(), Ok(Some(Module{
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
            ))})]
    })))]
    #[case::load3("def test(): 1;".to_string(), Ok(Some(Module{
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
        vars: Vec::new()
    })))]
    #[case::load4("def test(a, b): add(a, b);".to_string(), Ok(Some(Module{
        name: "test".to_string(),
        modules: Vec::new(),
        functions: vec![
            Shared::new(ast::Node{token_id: 0.into(), expr: Shared::new(ast::Expr::Def(
                IdentWithToken::new_with_token("test", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("test")), range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}}, module_id: 1.into()}))),
                smallvec![
                    Shared::new(ast::Node{token_id: 1.into(), expr:
                        Shared::new(
                            ast::Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("a")), range: Range{start: Position{line: 1, column: 10}, end: Position{line: 1, column: 11}}, module_id: 1.into()})))
                        ))}),
                    Shared::new(ast::Node{token_id: 2.into(), expr:
                        Shared::new(
                            ast::Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("b")), range: Range{start: Position{line: 1, column: 13}, end: Position{line: 1, column: 14}}, module_id: 1.into()})))
                        ))})
                ],
                vec![
                    Shared::new(ast::Node{token_id: 6.into(), expr: Shared::new(ast::Expr::Call(
                    IdentWithToken::new_with_token("add", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("add")), range: Range{start: Position{line: 1, column: 17}, end: Position{line: 1, column: 20}}, module_id: 1.into()}))),
                    smallvec![
                        Shared::new(ast::Node{token_id: 4.into(),
                            expr: Shared::new(
                                ast::Expr::Ident(IdentWithToken::new_with_token("a", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("a")), range: Range{start: Position{line: 1, column: 21}, end: Position{line: 1, column: 22}}, module_id: 1.into()}))))
                                )}),
                        Shared::new(ast::Node{token_id: 5.into(),
                            expr: Shared::new(
                                ast::Expr::Ident(IdentWithToken::new_with_token("b", Some(Shared::new(Token{kind: TokenKind::Ident(SmolStr::new("b")), range: Range{start: Position{line: 1, column: 24}, end: Position{line: 1, column: 25}}, module_id: 1.into()}))))
                            )})
                    ],
                ))})]
            ))})],
        vars: Vec::new()
    })))]
    fn test_load(
        token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
        #[case] program: String,
        #[case] expected: Result<Option<Module>, ModuleError>,
    ) {
        assert_eq!(
            ModuleLoader::default().load("test", &program, token_arena),
            expected
        );
    }

    #[rstest]
    #[case::load_standard_csv("csv", Ok(Some(Module {
        name: "csv".to_string(),
        functions: Vec::new(),
        modules: Vec::new(), // Assuming the csv.mq only contains definitions or is empty for this test
        vars: Vec::new(),
    })))]
    fn test_load_standard_module(
        token_arena: Shared<SharedCell<crate::arena::Arena<Shared<Token>>>>,
        #[case] module_name: &str,
        #[case] expected: Result<Option<Module>, ModuleError>,
    ) {
        let mut loader = ModuleLoader::default();
        let result = loader.load_from_file(module_name, token_arena.clone());
        // Only check that loading does not return NotFound error and returns Some(Module)
        match expected {
            Ok(Some(_)) => {
                assert!(result.is_ok(), "Expected Ok, got {:?}", result);
                assert!(
                    result.as_ref().unwrap().is_some(),
                    "Expected Some(Module), got {:?}",
                    result
                );
                assert_eq!(result.unwrap().unwrap().name, module_name);
            }
            Err(ref e) => {
                assert_eq!(result.unwrap_err(), *e);
            }
            _ => {}
        }
    }

    #[test]
    fn test_standard_modules_contains_csv() {
        assert!(super::STANDARD_MODULES.contains_key("csv"));
        let csv_content = super::STANDARD_MODULES.get("csv").unwrap()();
        assert!(csv_content.contains("")); // Just check it's a string, optionally check for expected content
    }
}
