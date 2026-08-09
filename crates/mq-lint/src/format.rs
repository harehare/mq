//! Diagnostic output formats for the `mq-lint` CLI.

mod github;
mod json;
mod markdown;
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
    /// A single JSON array of diagnostics across every linted file
    Json,
    /// GitHub-flavored Markdown table, suitable for a PR description or comment
    Markdown,
    /// SARIF 2.1.0 JSON
    Sarif,
    /// GitHub Actions workflow-command annotations
    Github,
}

/// Dispatches to the writer for the requested output format.
///
/// Each entry is `(file_label, source_code, diagnostics)`; the source is only used by the
/// `Text` report, to render a snippet with a caret under each diagnostic's range.
pub(crate) fn write_report(
    w: &mut impl Write,
    format: OutputFormat,
    results: &[(String, String, Vec<Diagnostic>)],
) -> io::Result<()> {
    match format {
        OutputFormat::Text => {
            for (file_label, code, diagnostics) in results {
                text::write_text_report(w, file_label, code, diagnostics)?;
            }
            Ok(())
        }
        OutputFormat::Json => json::write_json_report(w, results),
        OutputFormat::Markdown => markdown::write_markdown_report(w, results),
        OutputFormat::Sarif => sarif::write_sarif_report(w, results),
        OutputFormat::Github => github::write_github_report(w, results),
    }
}
