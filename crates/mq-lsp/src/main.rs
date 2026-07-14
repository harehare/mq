use std::path::PathBuf;

use clap::Parser;

use crate::server::LspConfig;

pub mod capabilities;
pub mod code_action;
pub mod completions;
pub mod document_symbol;
pub mod error;
pub mod execute_command;
pub mod goto_definition;
pub mod hover;
pub mod inlay_hints;
pub mod references;
pub mod rename;
pub mod semantic_tokens;
pub mod server;
pub mod signature_help;
pub mod workspace_symbol;

#[derive(Parser, Debug)]
#[command(name = "mq-lsp")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Language Server Protocol implementation for mq query language")]
struct Cli {
    /// Search modules from the directory
    #[arg(short = 'M', long = "module-path")]
    module_paths: Option<Vec<PathBuf>>,

    #[clap(flatten)]
    type_check: TypeCheckArgs,

    #[clap(flatten)]
    lint: LintArgs,
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

#[derive(Clone, Debug, clap::Args, Default)]
struct LintArgs {
    /// Enable mq-lint diagnostics (style, correctness, complexity, selector, module rules)
    #[arg(short = 'L', long, default_value_t = false)]
    enable_lint: bool,

    /// Disable a specific lint rule by ID (repeatable)
    #[arg(long = "disable-lint-rule", value_name = "RULE_ID")]
    disable_lint_rule: Vec<mq_lint::RuleId>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let type_check_config = mq_check::TypeCheckerOptions {
        strict_array: cli.type_check.strict_array,
        ..Default::default()
    };

    let mut lint_config = mq_lint::LintConfig::default();
    for rule_id in &cli.lint.disable_lint_rule {
        lint_config.disable_rule(*rule_id);
    }

    let config = LspConfig::new(
        cli.module_paths.unwrap_or_default(),
        cli.type_check.enable_type_checking,
        type_check_config,
        cli.lint.enable_lint,
        lint_config,
    );
    server::start(config).await;
}
