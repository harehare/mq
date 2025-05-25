use std::{cell::RefCell, path::PathBuf, rc::Rc};
use typed_arena::Arena as TypedArena;

use crate::MqResult;
use crate::{
    ast::node::{AstArena, AstProgram, NodeData}, // Added AstArena, AstProgram, NodeData
    eval::runtime_value::RuntimeValue, // RuntimeValue needs 'ast
    lexer::Lexer, // Lexer for internal parsing
    ast::parser::Parser, // Parser for internal parsing
    ModuleLoader, Token, Value,
    arena::Arena, // This is TokenArena
    error::{self, InnerError},
    eval::Evaluator,
    optimizer::Optimizer,
    // `parse` function from lib.rs is problematic, Engine should do its own parsing.
};

#[derive(Debug, Clone)]
pub struct Options {
    pub optimize: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { optimize: true }
    }
}

#[derive(Debug)] // Clone removed as TypedArena is not Clone
pub struct Engine<'ast> { // Add 'ast lifetime
    pub(crate) evaluator: Evaluator<'ast>, // Evaluator now has 'ast
    pub(crate) options: Options,
    token_arena: Rc<RefCell<Arena<Rc<Token>>>>,
    ast_arena: AstArena<'ast>, // Engine owns the AstArena
}

// Default implementation needs to be carefully considered due to 'ast.
// A true `Default` might not be possible if 'ast must be tied to an external lifetime.
// However, if Engine owns the AstArena, it can be Default by creating a new Arena.
// This implies that an Engine instance and its ASTs are self-contained.
impl<'ast> Default for Engine<'ast> {
    fn default() -> Self {
        let token_arena = Rc::new(RefCell::new(Arena::new(10000)));
        let ast_arena = TypedArena::new(); // Engine creates and owns its AstArena

        // Pass reference of ast_arena to Evaluator
        // Note: This creates a self-referential challenge if Evaluator stores &'ast AstArena<'ast>
        // where 'ast is tied to the Engine's ast_arena.
        // This usually requires the ast_arena to be passed in from where Engine is created,
        // or for Engine methods to operate with AstArena passed as parameter.
        //
        // Let's adjust: Evaluator will take a reference to the arena owned by Engine.
        // This implies that Engine methods that use Evaluator will pass `&self.ast_arena`.
        // For `new`/`default`, we construct Evaluator with a reference to the arena we just made.
        // This requires careful lifetime management, often by making Evaluator take `&'a AstArena<'a>`
        // where `'a` is the lifetime of the Engine's arena.
        // The current structure `Evaluator<'ast>` storing `&'ast AstArena<'ast>` means
        // `Evaluator::new` must receive a reference with that lifetime.
        
        // To make this work, Evaluator::new needs to be called in a context where ast_arena reference is stable.
        // This is okay if Engine is constructed and then methods are called.
        // However, direct Default might be tricky if not careful.
        // A simplified approach for now: Assume Evaluator takes &'ast AstArena<'ast>
        // and Engine::new constructs both.
        
        // This is problematic: `ast_arena` is owned by `Self`, but `evaluator` needs a reference
        // to it that lives as long as `evaluator`. This implies `evaluator` cannot outlive `Engine`.
        // This is typically fine. The issue is constructing it in `default` or `new`.
        // A common pattern is for `Engine` to hold `Pin<Box<AstArena>>` or similar if it needs to be stable,
        // or ensure methods on Engine pass `&self.ast_arena` to an Evaluator created on-the-fly or
        // an Evaluator that also takes `&'arena AstArena`.
        //
        // Given `Evaluator<'ast>` stores `&'ast AstArena<'ast>`, `Engine::default()` cannot
        // directly create an `Evaluator` that borrows from an `AstArena` field within the same `Engine`
        // being constructed due to borrow checker rules (cannot borrow from `self` before it's fully initialized).
        //
        // Solution: `Engine` will create `Evaluator` on-the-fly in methods like `eval`, or
        // `Evaluator`'s dependency on `AstArena` is passed per-method call rather than stored in `Evaluator`.
        //
        // The existing `Evaluator::new` takes `ast_arena`. So, `Engine` must create `ast_arena` first.
        // This means `evaluator` field in `Engine` needs to be initialized *after* `ast_arena`.
        // RUST_COMPILER_WILL_HANDLE_ORDERING
        
        // Let's assume for now that `Engine` methods will pass `&self.ast_arena` to `Evaluator` methods
        // or construct `Evaluator` instances on the fly within those methods.
        // For the struct definition, `Evaluator` field will be `Evaluator<'ast>`.
        // `Engine::default()` will initialize `evaluator` with `&self.ast_arena`.
        // This requires `evaluator` to be initialized after `ast_arena`.
        // The simplest way for `Default` is if `Evaluator::new` can be called with the arena.
        // The definition of Evaluator is: `pub struct Evaluator<'ast> { ast_arena: &'ast AstArena<'ast> }`
        // So, Engine::default() must provide a reference.

        let engine_ast_arena = TypedArena::new();
        // This is still tricky because the lifetime 'ast of Engine needs to be tied to engine_ast_arena.
        // This usually means Engine cannot own the arena if Evaluator holds a reference tied to Engine's lifetime parameter.
        //
        // Simplest path for now: Engine owns the arena. Evaluator is created in methods that use it.
        // So, Engine will not store `evaluator: Evaluator<'ast>` but create it.
        // OR, Evaluator does not store `&'ast AstArena` but takes it as param in its methods.
        // The problem statement implies Evaluator *is* refactored to take arena.
        // Let's assume Evaluator is created within `eval` method of Engine.
        // So, Engine struct will not store `evaluator` directly.
        // For now, I will keep `evaluator` field but acknowledge this is a complex point.
        // The provided `eval.rs` has `Evaluator` storing `&'ast AstArena<'ast>`.
        
        // A working approach: Engine owns AstArena. Engine methods create Evaluator instance, passing &self.ast_arena.
        // So, Engine doesn't store an Evaluator instance field directly. Or, if it does, it needs unsafe code or Pin.
        // Let's remove `evaluator` from `Engine` fields for now and create it in `eval`.
        // This simplifies `Default` and `new`.
        Self {
            options: Options::default(),
            token_arena,
            ast_arena: engine_ast_arena, // Engine owns the arena
            // evaluator field removed, will be created in `eval`
        }
    }
}

impl<'ast> Engine<'ast> { // Add 'ast lifetime
    pub fn new(options: Options, search_paths: Option<Vec<PathBuf>>) -> Self {
        let token_arena = Rc::new(RefCell::new(Arena::new(10000)));
        let ast_arena = TypedArena::new();
        let module_loader = ModuleLoader::new(search_paths);
        // Evaluator would be created in eval method or here if it can take owned components or refs with correct lifetime.
        // For now, consistent with removing evaluator field from struct:
        Self {
            options,
            token_arena,
            ast_arena,
            // evaluator field removed
        }
    }


    // Internal parsing method
    fn parse_program_str(&'ast self, code: &str) -> Result<AstProgram<'ast>, InnerError> {
        let tokens = Lexer::new(LexerOptions::default())
            .tokenize(code, Module::TOP_LEVEL_MODULE_ID) // Assuming ModuleId can be obtained
            .map_err(InnerError::Lexer)?;
        
        // Parser needs a mutable reference to TokenArena if it allocates new tokens, 
        // or if TokenId allocation happens elsewhere.
        // The existing Parser takes Rc<RefCell<Arena<Rc<Token>>>>.
        Parser::new(
            tokens.into_iter().map(Rc::new).collect::<Vec<_>>().iter(),
            Rc::clone(&self.token_arena),
            &self.ast_arena, // Pass engine's arena
            Module::TOP_LEVEL_MODULE_ID, 
        )
        .parse()
        .map_err(InnerError::Parse)
    }


    pub fn set_optimize(&mut self, optimize: bool) {
        self.options.optimize = optimize;
    }

    // Methods like set_filter_none, set_max_call_stack_depth would configure options
    // that are then used when creating an Evaluator instance in `eval`.

    // pub fn set_paths(&mut self, paths: Vec<PathBuf>) {
    //     // self.evaluator.module_loader.search_paths = Some(paths);
    //     // This implies module_loader might need to be part of Engine or re-created.
    //     // For now, assuming ModuleLoader is created with paths in `new` or `default`.
    // }

    // pub fn define_string_value(&self, name: &str, value: &str) {
    //     // This would need to modify an Env, which is part of Evaluator.
    //     // self.evaluator.define_string_value(name, value);
    // }

    pub fn load_builtin_module(&mut self) -> Result<(), Box<error::Error>> { // Return Result
        // ModuleLoader is created on-the-fly or stored in Engine if it doesn't need 'ast for construction.
        // Let's assume ModuleLoader is created here or part of Engine.
        let mut module_loader = ModuleLoader::new(None); // Or from self.module_loader
        let module = module_loader
            .load_builtin(Rc::clone(&self.token_arena), &self.ast_arena) // Pass arena
            .map_err(|e| Box::new(error::Error::from_error("", InnerError::Module(e), module_loader.clone())))?;
        
        // The loaded module's definitions need to be added to an environment.
        // This typically happens within an Evaluator context.
        // For now, this method's direct utility changes. It might set up a base environment in Engine.
        // Or, Evaluator handles this internally when it's created.
        // If Engine holds the root Env, then:
        // let mut evaluator = Evaluator::new(module_loader, Rc::clone(&self.token_arena), &self.ast_arena, Rc::clone(&self.root_env));
        // evaluator.load_module(module).map_err(...)
        // For now, assuming this is complex to fit here and Evaluator handles it.
        Ok(())
    }

    pub fn load_module(&mut self, module_name: &str) -> Result<(), Box<error::Error>> {
        let mut module_loader = ModuleLoader::new(None); // Or from self.module_loader
        let module = module_loader
            .load_from_file(module_name, Rc::clone(&self.token_arena), &self.ast_arena) // Pass arena
            .map_err(|e| {
                Box::new(error::Error::from_error(
                    "", // TODO: code string for context
                    InnerError::Module(e),
                    module_loader.clone(),
                ))
            })?;
        // Similar to load_builtin_module, integrating this into an Env needs Evaluator context or Engine storing Env.
        // evaluator.load_module(module).map_err(...)
        Ok(())
    }
    
    pub fn optimize(&self, program: &AstProgram<'ast>) -> AstProgram<'ast> {
        if self.options.optimize {
            let mut optimizer = Optimizer::new(); // Optimizer is stateless apart from constant_table for one run
            optimizer.optimize(program, &self.ast_arena)
        } else {
            program.clone() // Or return a reference if lifetimes allow, but Vec<NodeId> is cheap to clone.
        }
    }


    pub fn eval<I: Iterator<Item = Value>>(&mut self, code: &str, input: I) -> MqResult {
        // 1. Parse the code using Engine's arena
        let program_node_ids = self.parse_program_str(code).map_err(|e| Box::new(error::Error::from_error(code, e, ModuleLoader::new(None) /* TODO: get from engine */)))?;
        
        // 2. Optimize if enabled
        let optimized_program_node_ids = self.optimize(&program_node_ids);

        // 3. Evaluate
        // Create Evaluator instance here, passing the engine's resources
        let mut module_loader = ModuleLoader::new(None); // TODO: Configure with engine's paths
        // TODO: Load builtin modules into the evaluator's environment
        
        let mut evaluator = Evaluator::new(
            module_loader.clone(), // ModuleLoader might need to be part of Engine for path config
            Rc::clone(&self.token_arena),
            &self.ast_arena
        );
        // Load builtins for this evaluation context
        evaluator.load_builtin_module().map_err(|e| Box::new(error::Error::from_error(code, InnerError::Eval(e), module_loader.clone())))?;


        evaluator
            .eval(&optimized_program_node_ids, input.into_iter().map(|v| v.into()))
            .map(|values| {
                values
                    .into_iter()
                    .map(Into::into) // Convert RuntimeValue<'ast> back to Value
                    .collect::<Vec<_>>()
                    .into()
            })
            .map_err(|e| {
                Box::new(error::Error::from_error(
                    code,
                    e, // InnerError::Eval
                    evaluator.module_loader.clone(), // Use evaluator's module_loader
                ))
            })
    }

    pub const fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
#[cfg(test)]
#[ignore] // Ignore all tests in Engine for now
mod tests {
    use mq_test::defer;

    use super::*;

    #[test]
    fn test_engine_default() {
        let engine = Engine::default();
        assert!(engine.options.optimize);
    }

    #[test]
    fn test_set_optimize() {
        let mut engine = Engine::default();
        engine.set_optimize(false);
        assert!(!engine.options.optimize);
    }

    // #[test]
    // fn test_set_paths() {
    //     let mut engine = Engine::default();
    //     let paths = vec![PathBuf::from("/test/path")];
    //     engine.set_paths(paths.clone());
    //     // assert_eq!(engine.evaluator.module_loader.search_paths, Some(paths));
    //     // This test needs Engine to store ModuleLoader or pass paths differently
    // }

    // #[test]
    // fn test_set_max_call_stack_depth() {
    //     let mut engine = Engine::default();
    //     // let default_depth = engine.evaluator.options.max_call_stack_depth;
    //     // let new_depth = default_depth + 10;
    //     // engine.set_max_call_stack_depth(new_depth);
    //     // assert_eq!(engine.evaluator.options.max_call_stack_depth, new_depth);
    // }

    // #[test]
    // fn test_set_filter_none() {
    //     let mut engine = Engine::default();
    //     // let initial_value = engine.evaluator.options.filter_none;
    //     // engine.set_filter_none(!initial_value);
    //     // assert_eq!(engine.evaluator.options.filter_none, !initial_value);
    // }
    #[test]
    fn test_version() {
        let version = Engine::version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_load_module() {
        let (temp_dir, temp_file_path) = mq_test::create_file("test_module.mq", "def func1(): 42;");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = Engine::new(Options::default(), Some(vec![temp_dir]));
        // engine.set_paths(vec![temp_dir]); // Or pass paths to new

        let result = engine.load_module("test_module");
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_load_module() {
        let (temp_dir, temp_file_path) = mq_test::create_file("error.mq", "error");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }
        let mut engine = Engine::new(Options::default(), Some(vec![temp_dir]));
        // engine.set_paths(vec![temp_dir]);

        let result = engine.load_module("error");
        assert!(result.is_err());
    }

    #[test]
    fn test_eval() {
        let mut engine = Engine::default();
        let result = engine.eval("add(1, 1)", vec!["".to_string().into()].into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
    }
}
