use std::path::PathBuf;

use clap::Parser;

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

#[derive(Parser, Debug)]
#[command(name = "mq-lsp")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Language Server Protocol implementation for mq query language")]
struct Cli {
    /// Search modules from the directory
    #[arg(short = 'M', long = "module-path")]
    module_paths: Option<Vec<PathBuf>>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let config = LspConfig::new(cli.module_paths.unwrap_or_default());
    server::start(config).await;
}
