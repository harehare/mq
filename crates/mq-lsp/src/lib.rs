//! `mq-lsp` is a Language Server Protocol (LSP) implementation for the [mq](https://mqlang.org/). It provides various language features such as syntax highlighting, code completion, go-to-definition, and more.
//!
//! # Features
//!
//! - **Initialization**: Handles the initialization of the LSP server and sets up the server capabilities.
//! - **Diagnostics**: Publishes diagnostics information to the client.
//! - **Hover**: Provides hover information for symbols, including type information when type checking is enabled.
//! - **Inlay Hints**: Shows inferred type annotations inline in the editor when type checking is enabled.
//! - **Completion**: Offers code completion suggestions.
//! - **Go To Definition**: Allows navigation to the definition of symbols.
//! - **References**: Finds all references to a symbol.
//! - **Document Symbols**: Lists all symbols in a document.
//! - **Semantic Tokens**: Provides semantic tokens for syntax highlighting.
//! - **Formatting**: Formats the document according to the MDQ language formatting rules.
//! - **Code Actions**: Suggests quick fixes, such as adding a missing `include` for an unresolved standard library function call, or a missing `import` for an unresolved `module::func()` qualified access.
//! - **Refactoring**: Extracts a selected expression/pipeline into a variable or function, and inlines a variable or function back into its call sites.
//! - **Rename**: Renames a symbol and all of its references across files.
//!
//! # Usage
//!
//! To use this LSP server, you need to integrate it with an LSP client. The server reads from stdin and writes to stdout, making it compatible with various editors and IDEs that support LSP.
/// Server capability advertisement.
pub mod capabilities;
/// Quick fixes and refactoring code actions.
pub mod code_action;
/// Completion items.
pub mod completions;
/// Document symbols.
pub mod document_symbol;
/// Error types.
pub mod error;
/// Command execution.
pub mod execute_command;
/// Folding ranges.
pub mod folding_range;
/// Go to definition.
pub mod goto_definition;
/// Hover information.
pub mod hover;
/// Inlay hints.
pub mod inlay_hints;
pub mod refactor;
/// Find references.
pub mod references;
/// Rename symbols.
pub mod rename;
/// Semantic token highlighting.
pub mod semantic_tokens;
/// The LSP server.
pub mod server;
/// Signature help.
pub mod signature_help;
mod virtual_document;
/// Workspace symbols.
pub mod workspace_symbol;

pub use server::start;
