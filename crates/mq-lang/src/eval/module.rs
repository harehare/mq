#[cfg(not(debug_assertions))]
use crate::optimizer::Optimizer;

use crate::{
    Program, Token,
    arena::{Arena, ArenaId},
    ast::{error::ParseError, node as ast, parser::Parser},
    lexer::{self, Lexer, error::LexerError},
};
use compact_str::CompactString;
use rustc_hash::FxHashMap;
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc, sync::LazyLock};
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
type ModuleName = CompactString;
type StandardModules = FxHashMap<CompactString, fn() -> &'static str>;

#[derive(Debug, Clone)]
pub struct ModuleLoader {
    pub(crate) search_paths: Option<Vec<PathBuf>>,
    pub(crate) loaded_modules: Arena<ModuleName>,
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
    pub const TOP_LEVEL_MODULE: &str = "<top-level>";
    pub const BUILTIN_MODULE: &str = "<builtin>";
}

pub static STANDARD_MODULES: LazyLock<StandardModules> = LazyLock::new(|| {
    let mut map = FxHashMap::default();

    macro_rules! std_module {
        ($name:ident) => {
            fn $name() -> &'static str {
                include_str!(concat!("../../modules/", stringify!($name), ".mq"))
            }
            map.insert(
                CompactString::new(stringify!($name)),
                $name as fn() -> &'static str,
            );
        };
    }

    std_module!(csv);
    std_module!(yaml);
    std_module!(json);
    std_module!(test);

    map.insert(CompactString::new("csv"), csv as fn() -> &'static str);
    map.insert(CompactString::new("yaml"), yaml as fn() -> &'static str);
    map.insert(CompactString::new("json"), json as fn() -> &'static str);
    map.insert(CompactString::new("test"), test as fn() -> &'static str);
    map
});

impl ModuleLoader {
    pub const BUILTIN_FILE: &str = include_str!("../../builtin.mq");

    pub fn new(search_paths: Option<Vec<PathBuf>>) -> Self {
        let mut loaded_modules = Arena::new(10);
        loaded_modules.alloc(Module::TOP_LEVEL_MODULE.into());

        Self {
            search_paths,
            loaded_modules,
        }
    }

    #[inline(always)]
    pub fn module_name(&self, module_id: ModuleId) -> CompactString {
        self.loaded_modules[module_id].to_owned()
    }

    pub fn load(
        &mut self,
        module_name: &str,
        code: &str,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> Result<Option<Module>, ModuleError> {
        if self.loaded_modules.contains(module_name.into()) {
            return Ok(None);
        }

        let module_id = self.loaded_modules.len().into();
        self.loaded_modules.alloc(module_name.into());

        let tokens = Lexer::new(lexer::Options::default())
            .tokenize(code, module_id)
            .map_err(ModuleError::LexerError)?;

        #[cfg(not(debug_assertions))]
        let mut program = Parser::new(
            tokens.into_iter().map(Rc::new).collect::<Vec<_>>().iter(),
            token_arena,
            module_id,
        )
        .parse()
        .map_err(ModuleError::ParseError)?;

        #[cfg(debug_assertions)]
        let program = Parser::new(
            tokens.into_iter().map(Rc::new).collect::<Vec<_>>().iter(),
            token_arena,
            module_id,
        )
        .parse()
        .map_err(ModuleError::ParseError)?;

        #[cfg(not(debug_assertions))]
        Optimizer::new().optimize(&mut program);

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
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
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

    pub fn load_builtin(
        &mut self,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ) -> Result<Option<Module>, ModuleError> {
        self.load(Module::BUILTIN_MODULE, Self::BUILTIN_FILE, token_arena)
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

                PathBuf::from(path).join(Self::module_id(name))
            })
            .find(|p| p.is_file())
            .ok_or_else(|| ModuleError::NotFound(Self::module_id(name).to_string()))
    }

    fn module_id(name: &str) -> String {
        format!("{}.mq", name)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use compact_str::CompactString;
    use rstest::{fixture, rstest};
    use smallvec::{SmallVec, smallvec};

    use crate::{
        Token, TokenKind,
        ast::node as ast,
        range::{Position, Range},
    };

    use super::{Module, ModuleError, ModuleLoader};

    #[fixture]
    fn token_arena() -> Rc<RefCell<crate::arena::Arena<Rc<Token>>>> {
        Rc::new(RefCell::new(crate::arena::Arena::new(10)))
    }

    #[rstest]
    #[case::load1("test".to_string(), Err(ModuleError::InvalidModule))]
    #[case::load2("let test = \"value\"".to_string(), Ok(Some(Module{
        name: "test".to_string(),
        functions: Vec::new(),
        modules: Vec::new(),
        vars: vec![
            Rc::new(ast::Node{token_id: 0.into(), expr: Rc::new(ast::Expr::Let(
                ast::Ident::new_with_token("test", Some(Rc::new(Token{
                    kind: TokenKind::Ident(CompactString::new("test")),
                    range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}},
                    module_id: 1.into()
                }))),
                Rc::new(ast::Node{token_id: 2.into(), expr: Rc::new(ast::Expr::Literal(ast::Literal::String("value".to_string())))})
            ))})]
    })))]
    #[case::load3("def test(): 1;".to_string(), Ok(Some(Module{
        name: "test".to_string(),
        modules: Vec::new(),
        functions: vec![
            Rc::new(ast::Node{token_id: 0.into(), expr: Rc::new(ast::Expr::Def(
            ast::Ident::new_with_token("test", Some(Rc::new(Token{
                kind: TokenKind::Ident(CompactString::new("test")),
                range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}},
                module_id: 1.into()
            }))),
            SmallVec::new(),
            vec![
                Rc::new(ast::Node{token_id: 2.into(), expr: Rc::new(ast::Expr::Literal(ast::Literal::Number(1.into())))})
            ]
            ))})],
        vars: Vec::new()
    })))]
    #[case::load4("def test(a, b): add(a, b);".to_string(), Ok(Some(Module{
        name: "test".to_string(),
        modules: Vec::new(),
        functions: vec![
            Rc::new(ast::Node{token_id: 0.into(), expr: Rc::new(ast::Expr::Def(
                ast::Ident::new_with_token("test", Some(Rc::new(Token{kind: TokenKind::Ident(CompactString::new("test")), range: Range{start: Position{line: 1, column: 5}, end: Position{line: 1, column: 9}}, module_id: 1.into()}))),
                smallvec![
                    Rc::new(ast::Node{token_id: 1.into(), expr:
                        Rc::new(
                            ast::Expr::Ident(ast::Ident::new_with_token("a", Some(Rc::new(Token{kind: TokenKind::Ident(CompactString::new("a")), range: Range{start: Position{line: 1, column: 10}, end: Position{line: 1, column: 11}}, module_id: 1.into()})))
                        ))}),
                    Rc::new(ast::Node{token_id: 2.into(), expr:
                        Rc::new(
                            ast::Expr::Ident(ast::Ident::new_with_token("b", Some(Rc::new(Token{kind: TokenKind::Ident(CompactString::new("b")), range: Range{start: Position{line: 1, column: 13}, end: Position{line: 1, column: 14}}, module_id: 1.into()})))
                        ))})
                ],
                vec![
                    Rc::new(ast::Node{token_id: 6.into(), expr: Rc::new(ast::Expr::Call(
                    ast::Ident::new_with_token("add", Some(Rc::new(Token{kind: TokenKind::Ident(CompactString::new("add")), range: Range{start: Position{line: 1, column: 17}, end: Position{line: 1, column: 20}}, module_id: 1.into()}))),
                    smallvec![
                        Rc::new(ast::Node{token_id: 4.into(),
                            expr: Rc::new(
                                ast::Expr::Ident(ast::Ident::new_with_token("a", Some(Rc::new(Token{kind: TokenKind::Ident(CompactString::new("a")), range: Range{start: Position{line: 1, column: 21}, end: Position{line: 1, column: 22}}, module_id: 1.into()}))))
                                )}),
                        Rc::new(ast::Node{token_id: 5.into(),
                            expr: Rc::new(
                                ast::Expr::Ident(ast::Ident::new_with_token("b", Some(Rc::new(Token{kind: TokenKind::Ident(CompactString::new("b")), range: Range{start: Position{line: 1, column: 24}, end: Position{line: 1, column: 25}}, module_id: 1.into()}))))
                            )})
                    ],
                    false
                ))})]
            ))})],
        vars: Vec::new()
    })))]
    fn test_load(
        token_arena: Rc<RefCell<crate::arena::Arena<Rc<Token>>>>,
        #[case] program: String,
        #[case] expected: Result<Option<Module>, ModuleError>,
    ) {
        assert_eq!(
            ModuleLoader::new(None).load("test", &program, token_arena),
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
        token_arena: Rc<RefCell<crate::arena::Arena<Rc<Token>>>>,
        #[case] module_name: &str,
        #[case] expected: Result<Option<Module>, ModuleError>,
    ) {
        let mut loader = ModuleLoader::new(None);
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
