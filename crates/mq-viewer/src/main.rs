use clap::Parser;
use miette::{IntoDiagnostic, Result};
use mq_markdown::Markdown;
use mq_viewer::render_markdown;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "mqv")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "A CLI markdown viewer with rich text rendering")]
pub struct Args {
    /// Markdown file to view
    #[arg(value_name = "FILE")]
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let content = if io::stdin().is_terminal() {
        if let Some(file) = args.file {
            fs::read_to_string(&file).into_diagnostic()?
        } else {
            return Err(miette::miette!("No input file specified"));
        }
    } else {
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer).into_diagnostic()?;
        buffer
    };
    let markdown: Markdown = content.parse().map_err(|e| miette::miette!("{}", e))?;

    let stdout = io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    render_markdown(&markdown, &mut writer).into_diagnostic()?;
    writer.flush().into_diagnostic()?;

    Ok(())
}
