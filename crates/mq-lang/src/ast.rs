use std::rc::Rc;

use compact_str::CompactString;
use node::Node;

use crate::{Token, arena::ArenaId};

pub mod error;
pub mod node;
pub mod parser;

pub type Program = Vec<Rc<Node>>;
pub type IdentName = CompactString;
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
