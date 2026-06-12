use std::path::PathBuf;

use clap::Parser;

use crate::server::LspConfig;

pub mod capabilities;
pub mod completions;
pub mod document_symbol;
pub mod error;
pub mod execute_command;
pub mod goto_definition;
pub mod hover;
pub mod inlay_hints;
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

    /// Allow HTTP imports only from the specified domain prefix (http-import feature).
    /// Repeat to allow multiple domains. Default: all domains are allowed.
    #[cfg(feature = "http-import")]
    #[arg(long = "allowed-domain")]
    allowed_domains: Option<Vec<String>>,

    #[clap(flatten)]
    type_check: TypeCheckArgs,
}

#[derive(Clone, Debug, clap::Args, Default)]
struct TypeCheckArgs {
    /// Enable type checking for mq queries
    #[arg(short = 'T', long, default_value_t = false)]
    enable_type_checking: bool,

    /// Strict array type checking: if enabled, arrays must have consistent types for all elements
    #[arg(long, default_value_t = false)]
    strict_array: bool,

    /// Enable tuple typing for heterogeneous arrays (e.g., [1, "hello"] → (number, string))
    #[arg(long, default_value_t = false)]
    tuple: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let type_check_config = mq_check::TypeCheckerOptions {
        strict_array: cli.type_check.strict_array,
        ..Default::default()
    };

    #[cfg_attr(not(feature = "http-import"), allow(unused_mut))]
    let mut config = LspConfig::new(
        cli.module_paths.unwrap_or_default(),
        cli.type_check.enable_type_checking,
        type_check_config,
    );
    #[cfg(feature = "http-import")]
    {
        config = config.with_allowed_domains(cli.allowed_domains.unwrap_or_default());
    }
    server::start(config).await;
}
