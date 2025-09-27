use crate::server::LspConfig;

pub mod capabilities;
pub mod completions;
pub mod document_symbol;
pub mod execute_command;
pub mod goto_definition;
pub mod hover;
pub mod references;
pub mod semantic_tokens;
pub mod server;

#[tokio::main]
async fn main() {
    server::start(LspConfig::default()).await;
}
