use clap::Parser;
use glob::glob;
use miette::IntoDiagnostic;
use miette::miette;
use std::{fs, path::PathBuf};

#[derive(Parser, Debug)]
#[command(name = "mq-fmt")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Format mq files based on specified formatting options.")]
#[command(after_help = "# Examples:\n\n\
    ## To format all mq files in the current directory:\n\
    mq-fmt\n\n\
    ## To check if files are formatted without modifying them:\n\
    mq-fmt --check file.mq\n\n\
    ## To format with custom indent width:\n\
    mq-fmt --indent-width 4 file.mq")]
struct Cli {
    /// Number of spaces for indentation
    #[arg(short, long, default_value_t = 2)]
    indent_width: usize,

    /// Check if files are formatted without modifying them
    #[arg(short, long)]
    check: bool,

    /// Sort imports
    #[arg(long, default_value_t = false)]
    sort_imports: bool,

    /// Sort functions
    #[arg(long, default_value_t = false)]
    sort_functions: bool,

    /// Sort fields
    #[arg(long, default_value_t = false)]
    sort_fields: bool,

    /// Path to the mq file(s) to format
    files: Option<Vec<PathBuf>>,
}

fn main() -> miette::Result<()> {
    let cli = Cli::parse();

    let mut formatter = mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
        indent_width: cli.indent_width,
        sort_imports: cli.sort_imports,
        sort_fields: cli.sort_fields,
        sort_functions: cli.sort_functions,
    }));

    let files = match cli.files {
        Some(f) => f,
        None => glob("./**/*.mq")
            .into_diagnostic()?
            .collect::<Result<Vec<_>, _>>()
            .into_diagnostic()?,
    };

    for file in &files {
        if !file.exists() {
            return Err(miette!("File not found: {}", file.display()));
        }

        let content = fs::read_to_string(file).into_diagnostic()?;
        let formatted = formatter
            .format(&content)
            .map_err(|e| miette!("{}: {e}", file.display()))?;

        if cli.check && formatted != content {
            return Err(miette!("The input is not formatted: {}", file.display()));
        } else if formatted != content {
            fs::write(file, formatted).into_diagnostic()?;
        }
    }

    Ok(())
}
