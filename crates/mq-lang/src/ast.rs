use std::rc::Rc;

use node::Node;

use crate::{Token, arena::ArenaId};

pub mod constants;
pub mod error;
pub mod node;
pub mod parser;

pub type Program = Vec<Rc<Node>>;
pub type TokenId = ArenaId<Rc<Token>>;

/// Serializes an AST `Program` to a JSON string.
///
/// # Errors
///
/// Returns a `miette::Error` if serialization fails.
#[cfg(feature = "ast-json")]
pub fn ast_to_json(program: &Program) -> miette::Result<String> {
    serde_json::to_string(program)
        .map_err(|e| miette::miette!("Failed to serialize AST to JSON: {}", e))
}

/// Deserializes a JSON string into an AST `Program`.
///
/// # Errors
///
/// Returns a `miette::Error` if deserialization fails.
#[cfg(feature = "ast-json")]
pub fn ast_from_json(json: &str) -> miette::Result<Program> {
    serde_json::from_str(json)
        .map_err(|e| miette::miette!("Failed to deserialize AST from JSON: {}", e))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "ast-json")]
    use super::*;

    #[cfg(feature = "ast-json")]
    #[test]
    fn test_ast_to_json_and_from_json_roundtrip() {
        use crate::{AstExpr, ast::node::IdentWithToken};

        let ident = Rc::new(Node {
            token_id: TokenId::new(1),
            expr: Rc::new(AstExpr::Ident(IdentWithToken::new("foo"))),
        });
        let program = vec![ident.clone()];

        let json = ast_to_json(&program).expect("Serialization should succeed");
        let deserialized = ast_from_json(&json).expect("Deserialization should succeed");

        assert_eq!(deserialized.len(), 1);
        match &*deserialized[0].expr {
            AstExpr::Ident(name) => assert_eq!(name.name, "foo".into()),
            _ => panic!("Expected Ident node"),
        }
    }
}
