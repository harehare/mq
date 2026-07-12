//! Diagnostic output formats for the `mq-lint` CLI.

mod github;
mod sarif;
mod text;

use std::io::{self, Write};

use mq_lint::Diagnostic;

/// Diagnostic output format.
#[derive(Clone, Copy, Debug, Default, PartialEq, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    /// Credo-style human-readable report (default)
    #[default]
    Text,
    /// SARIF 2.1.0 JSON
    Sarif,
    /// GitHub Actions workflow-command annotations
    Github,
}

/// Dispatches to the writer for the requested output format.
pub(crate) fn write_report(
    w: &mut impl Write,
    format: OutputFormat,
    results: &[(String, Vec<Diagnostic>)],
) -> io::Result<()> {
    match format {
        OutputFormat::Text => {
            for (file_label, diagnostics) in results {
                text::write_text_report(w, file_label, diagnostics)?;
            }
            Ok(())
        }
        OutputFormat::Sarif => sarif::write_sarif_report(w, results),
        OutputFormat::Github => github::write_github_report(w, results),
    }
}
