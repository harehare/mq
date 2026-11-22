use node::Node;

use crate::{Shared, Token, arena::ArenaId};
use thin_vec::ThinVec;

/// Rendering of AST nodes back to source code.
pub mod code;
/// Names of builtins and identifiers the parser treats specially.
#[allow(missing_docs)]
pub mod constants;
/// AST node types.
pub mod node;
/// The AST parser.
pub mod parser;

/// A sequence of top-level or block-level nodes.
pub type Program = ThinVec<Shared<Node>>;
/// The id of a token in a [`TokenArena`](crate::TokenArena).
pub type TokenId = ArenaId<Token>;

/// Serializes an AST `Program` to a JSON string.
///
/// # Errors
///
/// Returns a `miette::Error` if serialization fails.
#[cfg(feature = "ast-json")]
pub fn ast_to_json(program: &[Shared<Node>]) -> miette::Result<String> {
    serde_json::to_string(program).map_err(|e| miette::miette!("Failed to serialize AST to JSON: {}", e))
}

/// Deserializes a JSON string into an AST `Program`.
///
/// # Errors
///
/// Returns a `miette::Error` if deserialization fails.
#[cfg(feature = "ast-json")]
pub fn ast_from_json(json: &str) -> miette::Result<Program> {
    serde_json::from_str(json).map_err(|e| miette::miette!("Failed to deserialize AST from JSON: {}", e))
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "ast-json")]
    use super::*;

    #[cfg(feature = "ast-json")]
    #[test]
    fn test_ast_to_json_and_from_json_roundtrip() {
        use crate::ast::node::{Expr as AstExpr, IdentWithToken};

        let ident = Shared::new(Node {
            token_id: TokenId::new(1),
            expr: AstExpr::Ident(IdentWithToken::new("foo")),
        });
        let program: Program = std::iter::once(ident.clone()).collect();

        let json = ast_to_json(&program).expect("Serialization should succeed");
        let deserialized = ast_from_json(&json).expect("Deserialization should succeed");

        assert_eq!(deserialized.len(), 1);
        match &deserialized[0].expr {
            AstExpr::Ident(name) => assert_eq!(name.name, "foo".into()),
            _ => panic!("Expected Ident node"),
        }
    }
}
