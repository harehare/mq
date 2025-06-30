#![cfg(feature = "ast-json")] // Add this at the beginning of the file

use std::rc::Rc;
use mq_lang::{AstNode, AstExpr, AstLiteral, Program};
use mq_lang::arena::ArenaId; // For default_token_id comparison if needed

// Helper function to create a default token_id for comparison, if necessary.
// Or, we can implement a custom comparison logic for Nodes that ignores token_id.
fn default_token_id() -> ArenaId<Rc<mq_lang::Token>> {
    ArenaId::new(0)
}

#[test]
fn test_literal_string_serialization_deserialization() {
    let original_node = Rc::new(AstNode {
        token_id: default_token_id(), // Assuming default_token_id is used in deserialization
        expr: Rc::new(AstExpr::Literal(AstLiteral::String("hello".to_string()))),
    });

    let json_string = original_node.to_json().unwrap();

    // Expected JSON might look like:
    // {
    //   "expr": {
    //     "Literal": {
    //       "String": "hello"
    //     }
    //   }
    // }
    // We can do a more robust check, e.g., by deserializing and comparing specific fields,
    // or by using a JSON diff/comparison library if available, or snapshot testing.
    // For now, let's check for key parts.
    assert!(json_string.contains("\"Literal\""));
    assert!(json_string.contains("\"String\""));
    assert!(json_string.contains("\"hello\""));

    let deserialized_node: AstNode = AstNode::from_json(&json_string).unwrap();

    // Compare relevant fields, ignoring token_id if it's not consistently reproducible
    // or using the expected default value for deserialized token_id.
    assert_eq!(deserialized_node.expr, original_node.expr);
    assert_eq!(deserialized_node.token_id, default_token_id()); // Check if token_id is the expected default
}

#[test]
fn test_literal_number_serialization_deserialization() {
    let original_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Literal(AstLiteral::Number(123.45.into()))),
    });

    let json_string = original_node.to_json().unwrap();
    assert!(json_string.contains("\"Literal\""));
    assert!(json_string.contains("\"Number\""));
    assert!(json_string.contains("123.45"));

    let deserialized_node: AstNode = AstNode::from_json(&json_string).unwrap();
    assert_eq!(deserialized_node.expr, original_node.expr);
    assert_eq!(deserialized_node.token_id, default_token_id());
}

#[test]
fn test_program_serialization_deserialization() {
    let node1 = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Literal(AstLiteral::String("first".to_string()))),
    });
    let node2 = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Literal(AstLiteral::Number(10.into()))),
    });
    let original_program: Program = vec![node1, node2];

    let json_string = serde_json::to_string_pretty(&original_program).unwrap();

    // Check for key parts of the program structure
    assert!(json_string.starts_with('['));
    assert!(json_string.contains("\"String\":\"first\""));
    assert!(json_string.contains("\"Number\":10.0")); // Numbers are f64
    assert!(json_string.ends_with("]\n"));


    let deserialized_program: Program = serde_json::from_str(&json_string).unwrap();

    assert_eq!(deserialized_program.len(), original_program.len());
    for (orig, deser) in original_program.iter().zip(deserialized_program.iter()) {
        assert_eq!(deser.expr, orig.expr);
        assert_eq!(deser.token_id, default_token_id());
    }
}

// TODO: Add more tests for other AST node types:
// - Ident
// - Call
// - Def
// - Fn
// - Let
// - InterpolatedString
// - Selector
// - While, Until, Foreach
// - If
// - Include
// - Self_, Nodes
// - Nested structures
// - Empty Program
// - Program with various node types
// - Test error cases for from_json (invalid JSON, malformed AST JSON)
// - Test Ident with and without token (though token is skipped in serialization)

// Example for Ident (token is skipped, so only name matters for now)
#[test]
fn test_ident_serialization_deserialization() {
    let original_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Ident(mq_lang::AstIdent::new("my_var"))),
    });

    let json_string = original_node.to_json().unwrap();
    assert!(json_string.contains("\"Ident\""));
    assert!(json_string.contains("\"name\":\"my_var\""));
    // Ensure the skipped 'token' field is not present or is null if "skip_serializing_if" was used
    // For now, new("my_var") creates token: None, which is skipped by skip_serializing_if = "Option::is_none"

    let deserialized_node: AstNode = AstNode::from_json(&json_string).unwrap();
    assert_eq!(deserialized_node.expr, original_node.expr);
    assert_eq!(deserialized_node.token_id, default_token_id());
    if let AstExpr::Ident(ident) = &*deserialized_node.expr {
        assert_eq!(ident.token, None); // Deserialized token should be None due to `default`
    } else {
        panic!("Deserialized node is not an Ident");
    }
}

// Consider adding a helper for comparing AST nodes that ignores token_id,
// or consistently uses default_token_id for original nodes in tests.
// For now, direct comparison of `expr` and checking `token_id` against default works.

// Test for Call expression (simplified)
use smallvec::smallvec;
#[test]
fn test_call_serialization_deserialization() {
    let arg_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Literal(AstLiteral::Number(1.into()))),
    });
    let original_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Call(
            mq_lang::AstIdent::new("my_func"),
            smallvec![arg_node],
            false,
        )),
    });

    let json_string = original_node.to_json().unwrap();
    // {"expr":{"Call":[{"name":"my_func","token":null},[{"expr":{"Literal":{"Number":1.0}}}],false]}}
    assert!(json_string.contains("\"Call\""));
    assert!(json_string.contains("\"name\":\"my_func\""));
    assert!(json_string.contains("\"Literal\":{\"Number\":1.0}"));


    let deserialized_node: AstNode = AstNode::from_json(&json_string).unwrap();
    assert_eq!(deserialized_node.expr, original_node.expr);
    assert_eq!(deserialized_node.token_id, default_token_id());
}

// Test for If expression (simplified with only one branch, no condition for else)
#[test]
fn test_if_expression_serialization_deserialization() {
    let then_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Literal(AstLiteral::String("then_branch".to_string()))),
    });
    let condition_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::Literal(AstLiteral::Bool(true))),
    });

    let original_node = Rc::new(AstNode {
        token_id: default_token_id(),
        expr: Rc::new(AstExpr::If(smallvec![(Some(condition_node), then_node)])),
    });

    let json_string = original_node.to_json().unwrap();
    assert!(json_string.contains("\"If\""));
    assert!(json_string.contains("\"Bool\":true"));
    assert!(json_string.contains("\"String\":\"then_branch\""));

    let deserialized_node: AstNode = AstNode::from_json(&json_string).unwrap();
    assert_eq!(deserialized_node.expr, original_node.expr);
}

// Test deserialization of invalid JSON
#[test]
fn test_invalid_json_deserialization() {
    let json_string = "{invalid_json}";
    let result: Result<AstNode, _> = AstNode::from_json(json_string);
    assert!(result.is_err());
}

// Test deserialization of valid JSON but malformed AST structure
#[test]
fn test_malformed_ast_json_deserialization() {
    let json_string = r#"{"expr": {"UnknownVariant": "some_data"}}"#;
    let result: Result<AstNode, _> = AstNode::from_json(json_string);
    assert!(result.is_err());
}
