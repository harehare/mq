#![cfg(feature = "ast-json")] // Add this at the beginning of the file

use mq_lang::{Engine, Value, Program, ast::{node::{Node, Expr, Literal, Ident}, Program as AstProgram}, parse_text_input};
use std::rc::Rc;
use smallvec::smallvec; // For AstArgs, AstParams

#[test]
fn test_eval_ast_simple_literal() {
    let mut engine = Engine::default();
    engine.load_builtin_module();

    // AST for: "hello"
    let ast_node = Rc::new(Node {
        token_id: mq_lang::arena::ArenaId::new(0), // Default token_id
        expr: Rc::new(Expr::Literal(Literal::String("hello from ast".to_string()))),
    });
    let program: AstProgram = vec![ast_node];

    let input_values = parse_text_input("").unwrap(); // Dummy input
    let result = engine.eval_ast(program, "test_eval_ast_simple_literal", input_values.into_iter());

    assert!(result.is_ok());
    let output_values = result.unwrap();
    assert_eq!(output_values.len(), 1);
    assert_eq!(output_values[0], Value::String("hello from ast".to_string()));
}

#[test]
fn test_eval_ast_from_json_string() {
    let mut engine = Engine::default();
    engine.load_builtin_module();

    // JSON representation of a simple AST: 42
    // Note: token_id is skipped in serialization and defaulted in deserialization.
    let json_ast_str = r#"
    [
        {
            "expr": {
                "Literal": {
                    "Number": 42.0
                }
            }
        }
    ]
    "#;
    let program: AstProgram = serde_json::from_str(json_ast_str).expect("Failed to deserialize AST from JSON");

    let input_values = parse_text_input("").unwrap();
    let result = engine.eval_ast(program, "test_eval_ast_from_json_string", input_values.into_iter());

    assert!(result.is_ok());
    let output_values = result.unwrap();
    assert_eq!(output_values.len(), 1);
    assert_eq!(output_values[0], Value::Number(42.0.into()));
}

#[test]
fn test_eval_ast_function_call() {
    let mut engine = Engine::default();
    engine.load_builtin_module(); // For 'add' function

    // AST for: add(1, 2)
    let arg1 = Rc::new(Node {
        token_id: mq_lang::arena::ArenaId::new(0),
        expr: Rc::new(Expr::Literal(Literal::Number(1.0.into()))),
    });
    let arg2 = Rc::new(Node {
        token_id: mq_lang::arena::ArenaId::new(0),
        expr: Rc::new(Expr::Literal(Literal::Number(2.0.into()))),
    });
    let call_node = Rc::new(Node {
        token_id: mq_lang::arena::ArenaId::new(0),
        expr: Rc::new(Expr::Call(
            Ident::new("add"),
            smallvec![arg1, arg2],
            false,
        )),
    });
    let program: AstProgram = vec![call_node];

    let input_values = parse_text_input("").unwrap(); // Dummy input, not used by add(1,2)
    let result = engine.eval_ast(program, "test_eval_ast_function_call", input_values.into_iter());

    assert!(result.is_ok(), "eval_ast failed: {:?}", result.err());
    let output_values = result.unwrap();
    assert_eq!(output_values.len(), 1);
    assert_eq!(output_values[0], Value::Number(3.0.into()));
}


// TODO: Add more tests for Engine::eval_ast:
// - AST with function definitions (Def) and their calls.
// - AST with variable assignments (Let) and usage.
// - AST involving control flow (If, While, etc.).
// - AST with `include` statements (this will test ModuleLoader integration).
//   - Need to set up a dummy module file for this.
// - Test with various input values.
// - Test error handling (e.g., undefined function call in AST).
// - Test interaction with __FILE__ variable if applicable (though token_id is default).
// - Test optimization impact if any (though eval_ast has its own optimize call).


// Example for AST with `include` (requires setting up a module file)
// #[test]
// fn test_eval_ast_with_include() {
//     use std::fs;
//     use std::path::PathBuf;
//     use mq_test::defer; // Assuming mq_test crate is available for temp file utils
//
//     let mut engine = Engine::default();
//     engine.load_builtin_module();
//
//     // Create a temporary module file
//     let (temp_dir_guard, temp_module_path) = mq_test::create_file("my_module.mq", "def get_num(): 42;");
//     // defer! { ... cleanup ... }; // Ensure cleanup
//
//     engine.set_paths(vec![temp_dir_guard.path().to_path_buf()]); // Set search path for modules
//
//     // AST for: include "my_module" | get_num()
//     let include_node = Rc::new(Node {
//         token_id: mq_lang::arena::ArenaId::new(0),
//         expr: Rc::new(Expr::Include(Literal::String("my_module".to_string()))),
//     });
//     let call_node = Rc::new(Node {
//         token_id: mq_lang::arena::ArenaId::new(0),
//         expr: Rc::new(Expr::Call(Ident::new("get_num"), smallvec![], false)),
//     });
//     let program: AstProgram = vec![include_node, call_node];
//
//     let input_values = parse_text_input("").unwrap();
//     let result = engine.eval_ast(program, "test_eval_ast_with_include", input_values.into_iter());
//
//     assert!(result.is_ok(), "eval_ast with include failed: {:?}", result.err());
//     let output_values = result.unwrap();
//     assert_eq!(output_values.len(), 1);
//     assert_eq!(output_values[0], Value::Number(42.0.into()));
//
//     // Cleanup (handled by defer or explicitly)
//     fs::remove_file(temp_module_path).unwrap();
//     // temp_dir_guard will clean up the directory
// }

// Test for error case: calling an undefined function from AST
#[test]
fn test_eval_ast_undefined_function() {
    let mut engine = Engine::default();
    // engine.load_builtin_module(); // Don't load builtins to make 'undefined_fn' surely undefined

    let call_node = Rc::new(Node {
        token_id: mq_lang::arena::ArenaId::new(0),
        expr: Rc::new(Expr::Call(
            Ident::new("undefined_fn"),
            smallvec![],
            false,
        )),
    });
    let program: AstProgram = vec![call_node];

    let input_values = parse_text_input("").unwrap();
    let result = engine.eval_ast(program, "test_eval_ast_undefined_function", input_values.into_iter());

    assert!(result.is_err(), "Expected an error for undefined function call");
    // Optionally, check the error type or message
    // e.g., format!("{:?}", result.err().unwrap()).contains("Unknown function")
}
