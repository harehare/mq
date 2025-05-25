use crate::{
    Token, // Program is now AstProgram
    arena::{Arena, ArenaId},
    ast::{error::ParseError, node::{self as ast, AstArena, NodeId, AstProgram}, parser::Parser}, // Added AstArena, NodeId, AstProgram
    lexer::{self, Lexer, error::LexerError},
};
use compact_str::CompactString;
use std::{cell::RefCell, fs, path::PathBuf, rc::Rc};
use typed_arena::Arena as TypedArena; // For AstArena in tests, if needed directly
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

#[derive(Debug, Clone)]
pub struct ModuleLoader {
    pub(crate) search_paths: Option<Vec<PathBuf>>,
    pub(crate) loaded_modules: Arena<ModuleName>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Module<'ast> { // Add 'ast lifetime
    pub name: String,
    pub modules: AstProgram<'ast>, // Changed to Vec<NodeId>
    pub vars: AstProgram<'ast>,    // Changed to Vec<NodeId>
}

impl<'ast> Module<'ast> { // Add 'ast to impl
    pub const TOP_LEVEL_MODULE_ID: ArenaId<ModuleName> = ArenaId::new(0);
    pub const TOP_LEVEL_MODULE: &str = "<top-level>";
    pub const BUILTIN_MODULE: &str = "<builtin>";
}

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

    pub fn module_name(&self, module_id: ModuleId) -> CompactString {
        self.loaded_modules[module_id].to_owned()
    }

    pub fn load<'ast_param>( // Renamed 'ast to 'ast_param to avoid conflict with struct's 'ast
        &mut self,
        module_name: &str,
        code: &str,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
        ast_arena: &'ast_param AstArena<'ast_param>, // Accept AstArena
    ) -> Result<Option<Module<'ast_param>>, ModuleError> { // Return Module<'ast_param>
        if self.loaded_modules.contains(module_name.into()) {
            return Ok(None);
        }

        let module_id = self.loaded_modules.len().into();
        self.loaded_modules.alloc(module_name.into());

        let tokens = Lexer::new(lexer::Options::default())
            .tokenize(code, module_id)
            .map_err(ModuleError::LexerError)?;
        
        // Parser now needs ast_arena
        let program_node_ids = Parser::new(
            tokens.into_iter().map(Rc::new).collect::<Vec<_>>().iter(),
            token_arena,
            ast_arena, // Pass ast_arena
            module_id,
        )
        .parse()
        .map_err(ModuleError::ParseError)?;

        // Filter NodeIds based on the expression type in NodeData
        let modules_node_ids = program_node_ids
            .iter()
            .filter(|node_id| matches!(ast_arena[**node_id].expr, ast::Expr::Def(_, _, _)))
            .cloned()
            .collect::<Vec<_>>();

        let vars_node_ids = program_node_ids
            .iter()
            .filter(|node_id| matches!(ast_arena[**node_id].expr, ast::Expr::Let(_, _)))
            .cloned()
            .collect::<Vec<_>>();

        // The check for extraneous node types needs to be based on counts,
        // as we've already filtered by NodeId.
        if program_node_ids.len() != modules_node_ids.len() + vars_node_ids.len() {
            return Err(ModuleError::InvalidModule);
        }

        Ok(Some(Module { // Module is now Module<'ast_param>
            name: module_name.to_string(),
            modules: modules_node_ids,
            vars: vars_node_ids,
        }))
    }

    pub fn load_from_file<'ast_param>( // Add 'ast_param
        &mut self,
        module_name: &str,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
        ast_arena: &'ast_param AstArena<'ast_param>, // Accept AstArena
    ) -> Result<Option<Module<'ast_param>>, ModuleError> { // Return Module<'ast_param>
        let file_path = Self::find(module_name, self.search_paths.clone())?;
        let program_code =
            std::fs::read_to_string(&file_path).map_err(|e| ModuleError::IOError(e.to_string()))?;

        self.load(module_name, &program_code, token_arena, ast_arena) // Pass ast_arena
    }

    pub fn read_file(&mut self, module_name: &str) -> Option<String> {
        let file_path = Self::find(module_name, self.search_paths.clone()).ok()?;
        fs::read_to_string(&file_path).ok()
    }

    pub fn load_builtin<'ast_param>( // Add 'ast_param
        &mut self,
        token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
        ast_arena: &'ast_param AstArena<'ast_param>, // Accept AstArena
    ) -> Result<Option<Module<'ast_param>>, ModuleError> { // Return Module<'ast_param>
        self.load(Module::BUILTIN_MODULE, Self::BUILTIN_FILE, token_arena, ast_arena) // Pass ast_arena
    }

    fn find(name: &str, search_paths: Option<Vec<PathBuf>>) -> Result<PathBuf, ModuleError> {
        let home = dirs::home_dir()
            .map(|p| {
                let path = p.clone();
                path.to_str().unwrap_or("").to_string()
            })
            .unwrap_or("".to_string());
        let origin = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));

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
// #[ignore] // Removing ignore
mod tests {
    use std::{cell::RefCell, rc::Rc};
    use compact_str::CompactString;
    use rstest::{fixture, rstest};
    use smallvec::{smallvec, SmallVec};
    use typed_arena::Arena as TypedArena;

    use crate::{
        Token, TokenKind,
        ast::node::{self as ast, AstArena as ActualAstArena, NodeData, NodeId, AstProgram, Expr as AstExpr, Ident as AstIdent, Literal as AstLiteral, Params as AstParams},
        range::{Position, Range},
        arena::ArenaId, // For dummy TokenId
    };
    use crate::eval::module::ModuleId; // For ModuleId::new for dummy tokens

    use super::{Module, ModuleError, ModuleLoader};

    #[fixture]
    fn token_arena_fixture() -> Rc<RefCell<crate::arena::Arena<Rc<Token>>>> {
        let arena = Rc::new(RefCell::new(crate::arena::Arena::new(1024)));
        // Pre-allocate a dummy token at index 0 for simple TokenId creation if needed by helpers
        arena.borrow_mut().alloc(Rc::new(Token {
            kind: TokenKind::Eof, // Dummy kind
            range: Range::default(),
            module_id: ModuleId::new(0),
        }));
        arena
    }

    // Helper to get NodeData (unsafe, for test verification only)
    unsafe fn get_node_data_test<'a, 'ast_lifetime>(node_id: NodeId, arena: &'a ActualAstArena<'ast_lifetime>) -> &'a NodeData<'ast_lifetime> 
    where 'a: 'ast_lifetime
    {
        &*(node_id.0 as *const NodeData<'ast_lifetime>)
    }
    
    fn dummy_token_id_for_tests() -> ArenaId<Rc<Token>> {
        ArenaId::new(0) // Assumes a token is pre-allocated at index 0 by the fixture
    }


    #[rstest]
    #[case("test", "let test = \"value\"", 
        |module: &Module, ast_arena: &ActualAstArena| {
            assert_eq!(module.name, "test");
            assert_eq!(module.modules.len(), 0);
            assert_eq!(module.vars.len(), 1);
            let var_node_data = unsafe { get_node_data_test(module.vars[0], ast_arena) };
            if let AstExpr::Let(ident, val_id) = &var_node_data.expr {
                assert_eq!(ident.name.as_str(), "test");
                let val_data = unsafe { get_node_data_test(*val_id, ast_arena) };
                assert!(matches!(val_data.expr, AstExpr::Literal(AstLiteral::String(s)) if s == "value"));
            } else {
                panic!("Expected Let expr, got {:?}", var_node_data.expr);
            }
        }
    )]
    #[case("test_def", "def my_func(): 1;", 
        |module: &Module, ast_arena: &ActualAstArena| {
            assert_eq!(module.name, "test_def");
            assert_eq!(module.vars.len(), 0);
            assert_eq!(module.modules.len(), 1);
            let func_node_data = unsafe { get_node_data_test(module.modules[0], ast_arena) };
            if let AstExpr::Def(ident, params, body_ids) = &func_node_data.expr {
                assert_eq!(ident.name.as_str(), "my_func");
                assert!(params.is_empty());
                assert_eq!(body_ids.len(), 1);
                let body_node_data = unsafe { get_node_data_test(body_ids[0], ast_arena) };
                assert!(matches!(body_node_data.expr, AstExpr::Literal(AstLiteral::Number(n)) if n.value() == 1.0)));
            } else {
                panic!("Expected Def expr, got {:?}", func_node_data.expr);
            }
        }
    )]
    fn test_load_ok<'ast_test>( // Lifetime for the arena created in this test
        token_arena_fixture: Rc<RefCell<crate::arena::Arena<Rc<Token>>>>,
        #[case] module_name: &str,
        #[case] program_code: &str,
        #[case] assertions: impl Fn(&Module<'ast_test>, &ActualAstArena<'ast_test>),
        // ast_arena is created locally in the test
    ) {
        let ast_arena = TypedArena::new(); // Each test case gets its own arena
        let mut loader = ModuleLoader::new(None);
        let result = loader.load(module_name, program_code, token_arena_fixture, &ast_arena);

        assert!(result.is_ok(), "Expected Ok, got Err: {:?}", result.err());
        let loaded_module_opt = result.unwrap();
        assert!(loaded_module_opt.is_some(), "Expected Some(Module), got None");
        if let Some(loaded_module) = loaded_module_opt {
            assertions(&loaded_module, &ast_arena);
        }
    }

    #[rstest]
    #[case("invalid_syntax", "let a = ;", ModuleError::ParseError(ParseError::UnexpectedToken(Token{kind: TokenKind::SemiColon, range: Range::default(), module_id: ModuleId::new(1)})))] // ModuleId will be dynamic
    #[case("invalid_module_content", "ident_only", ModuleError::InvalidModule)] // Assuming this is caught by len check after filtering
    fn test_load_err(
        token_arena_fixture: Rc<RefCell<crate::arena::Arena<Rc<Token>>>>,
        #[case] module_name: &str,
        #[case] program_code: &str,
        #[case] expected_err_variant: ModuleError,
        // ast_arena is created locally in the test
    ) {
        let ast_arena = TypedArena::new(); 
        let mut loader = ModuleLoader::new(None);
        let result = loader.load(module_name, program_code, token_arena_fixture, &ast_arena);

        assert!(result.is_err(), "Expected Err, got Ok: {:?}", result.ok());
        if let Err(actual_err) = result {
            assert_eq!(std::mem::discriminant(&actual_err), std::mem::discriminant(&expected_err), "Error variants differ. Actual: {:?}, Expected: {:?}", actual_err, expected_err);
            // Further checks for specific error details can be added here if ParseError implements PartialEq or by field matching
            if let (ModuleError::ParseError(ParseError::UnexpectedToken(ref actual_tok)), ModuleError::ParseError(ParseError::UnexpectedToken(ref expected_tok))) = (actual_err, expected_err) {
                 assert_eq!(actual_tok.kind, expected_tok.kind);
                 // Note: Range and ModuleId might differ due to dynamic allocation in parser.
                 // Comparing just the kind is safer for this test.
            }
        }
    }
}
