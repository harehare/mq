use std::fs;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use colored::Colorize;
use miette::{IntoDiagnostic, miette};

/// Check mq files for syntax and semantic errors.
///
/// Returns `Ok(())` if no errors are found, or an error if any file contains errors.
pub fn check_files(files: &[PathBuf]) -> miette::Result<()> {
    let stdout = io::stdout();
    let mut handle = BufWriter::new(stdout.lock());
    let mut has_error = false;

    for file in files {
        if !file.exists() {
            return Err(miette!("File not found: {}", file.display()));
        }

        let content = fs::read_to_string(file).into_diagnostic()?;
        let mut hir = mq_hir::Hir::default();
        hir.add_code(None, &content);

        let errors = hir.error_ranges();
        let warnings = hir.warning_ranges();

        if !errors.is_empty() || !warnings.is_empty() {
            has_error = true;
            writeln!(handle, "Checking: {}", file.display()).into_diagnostic()?;

            for (message, range) in errors {
                writeln!(
                    handle,
                    "  {}: {} at line {}, column {}",
                    "Error".red().bold(),
                    message,
                    range.start.line,
                    range.start.column
                )
                .into_diagnostic()?;
            }

            for (message, range) in warnings {
                writeln!(
                    handle,
                    "  {}: {} at line {}, column {}",
                    "Warning".yellow().bold(),
                    message,
                    range.start.line,
                    range.start.column
                )
                .into_diagnostic()?;
            }
            writeln!(handle).into_diagnostic()?;
        }
    }

    handle.flush().into_diagnostic()?;

    if has_error { Err(miette!("")) } else { Ok(()) }
}
