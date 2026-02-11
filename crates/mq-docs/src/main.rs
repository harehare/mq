use std::fs;
use std::path::PathBuf;

use clap::Parser;
use miette::{IntoDiagnostic, miette};
use mq_docs::{DocFormat, generate_docs};

/// Show functions documentation for the query
#[derive(Parser, Debug)]
#[command(name = "mq-docs")]
struct Cli {
    /// Input files to generate documentation from
    files: Option<Vec<PathBuf>>,
    /// Specify additional module names to load for documentation
    #[arg(short = 'M', long)]
    module_names: Option<Vec<String>>,
    /// Specify the documentation output format
    #[arg(short = 'F', long, value_enum, default_value_t)]
    format: DocFormat,
    /// Include built-in functions alongside specified modules/files
    #[arg(short = 'B', long)]
    include_builtin: bool,
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    let files = if let Some(paths) = &cli.files {
        let mut file_contents = Vec::new();
        for path in paths {
            if !path.exists() {
                return Err(miette!("File not found: {}", path.display()));
            }
            let content = fs::read_to_string(path).into_diagnostic()?;
            file_contents.push((path.display().to_string(), content));
        }
        Some(file_contents)
    } else {
        None
    };

    let output = generate_docs(&cli.module_names, &files, &cli.format, cli.include_builtin)?;
    println!("{output}");
    Ok(())
}
