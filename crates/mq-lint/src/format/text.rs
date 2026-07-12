use std::io::{self, Write};

use colored::Colorize;
use mq_lint::{Diagnostic, Severity};

/// Severities in the order categories are displayed, most severe first.
const SEVERITY_ORDER: [Severity; 4] = [Severity::Error, Severity::Warn, Severity::Perf, Severity::Style];

/// Writes `diagnostics` grouped by severity in a Credo-style report and returns `true` if any
/// were reported.
pub(super) fn write_text_report(w: &mut impl Write, file_label: &str, diagnostics: &[Diagnostic]) -> io::Result<bool> {
    let mut printed_category = false;
    for severity in SEVERITY_ORDER {
        let group: Vec<&Diagnostic> = diagnostics.iter().filter(|d| d.severity == severity).collect();
        if group.is_empty() {
            continue;
        }
        if printed_category {
            writeln!(w)?;
        }
        printed_category = true;
        write_category(w, severity, &group, file_label)?;
    }

    if diagnostics.is_empty() {
        writeln!(
            w,
            "{}  {}",
            "✓".bright_green().bold(),
            "No lint issues found.".bright_green()
        )?;
    } else {
        writeln!(w)?;
        write_summary(w, diagnostics)?;
    }

    Ok(!diagnostics.is_empty())
}

/// Maps a severity to its Credo-style category title and one-letter marker.
fn severity_category(severity: Severity) -> (colored::ColoredString, colored::ColoredString) {
    match severity {
        Severity::Error => ("## Errors".bright_red().bold(), "[E]".bright_red().bold()),
        Severity::Warn => ("## Warnings".bright_yellow().bold(), "[W]".bright_yellow().bold()),
        Severity::Perf => ("## Performance".blue().bold(), "[P]".blue().bold()),
        Severity::Style => ("## Style".cyan().bold(), "[S]".cyan().bold()),
    }
}

/// Uses mq's own pipe operator `|` as the gutter bar.
fn severity_bar(severity: Severity) -> colored::ColoredString {
    match severity {
        Severity::Error => "|".bright_red(),
        Severity::Warn => "|".bright_yellow(),
        Severity::Perf => "|".blue(),
        Severity::Style => "|".cyan(),
    }
}

/// Writes one severity category: a heading followed by its diagnostics, each as a
/// `[X] message` line with the `file:line:col .rule_id` location on the line below
/// (the rule id rendered as an mq selector, e.g. `.unused_variable`).
fn write_category(
    w: &mut impl Write,
    severity: Severity,
    diagnostics: &[&Diagnostic],
    file_label: &str,
) -> io::Result<()> {
    let (title, letter) = severity_category(severity);
    let bar = severity_bar(severity);

    writeln!(w, "{}\n", title)?;

    for (i, diagnostic) in diagnostics.iter().enumerate() {
        match diagnostic.severity {
            Severity::Error => writeln!(w, "{bar} {} {}", letter, diagnostic.message().bright_red().bold())?,
            Severity::Warn => writeln!(w, "{bar} {} {}", letter, diagnostic.message().bright_yellow().bold())?,
            Severity::Perf => writeln!(w, "{bar} {} {}", letter, diagnostic.message().blue().bold())?,
            Severity::Style => writeln!(w, "{bar} {} {}", letter, diagnostic.message().cyan().bold())?,
        }

        let loc = match &diagnostic.range {
            Some(range) => format!("{}:{}:{}", file_label, range.start.line, range.start.column),
            None => file_label.to_string(),
        };
        writeln!(
            w,
            "{bar}     {} {}",
            loc.dimmed(),
            format!(".{}", diagnostic.rule_id().as_str()).dimmed(),
        )?;

        if let Some(help) = diagnostic.help() {
            writeln!(w, "{bar}       {}", format!("help: {help}").bright_blue())?;
        }

        if i + 1 < diagnostics.len() {
            writeln!(w, "{bar}")?;
        }
    }

    Ok(())
}

/// Writes the trailing summary line, e.g. `found 3 issues (2 warnings, 1 style).`
fn write_summary(w: &mut impl Write, diagnostics: &[Diagnostic]) -> io::Result<()> {
    let breakdown: Vec<String> = SEVERITY_ORDER
        .into_iter()
        .filter_map(|severity| {
            let count = diagnostics.iter().filter(|d| d.severity == severity).count();
            if count == 0 {
                return None;
            }
            let (singular, plural) = match severity {
                Severity::Error => ("error".bright_red(), "errors".bright_red()),
                Severity::Warn => ("warning".bright_yellow(), "warnings".bright_yellow()),
                Severity::Perf => ("performance".blue(), "performance".blue()),
                Severity::Style => ("style".cyan(), "style".cyan()),
            };
            Some(format!("{count} {}", if count == 1 { singular } else { plural }))
        })
        .collect();

    writeln!(
        w,
        "{} {} issue{} ({}).",
        "found".bold(),
        diagnostics.len().to_string().bold(),
        if diagnostics.len() == 1 { "" } else { "s" },
        breakdown.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use mq_lint::{LintConfig, Linter};

    #[test]
    fn test_write_text_report_no_issues() {
        let mut buf = Vec::new();
        let had_diagnostics = write_text_report(&mut buf, "test.mq", &[]).unwrap();
        assert!(!had_diagnostics);
        assert!(String::from_utf8(buf).unwrap().contains("No lint issues found."));
    }

    #[test]
    fn test_write_text_report_with_diagnostics() {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        let diagnostics = crate::collect_diagnostics(r#".checked == true"#, &linter, &config, Severity::Style);

        let mut buf = Vec::new();
        let had_diagnostics = write_text_report(&mut buf, "test.mq", &diagnostics).unwrap();
        assert!(had_diagnostics);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("## Style"));
        assert!(output.contains("test.mq:1:1"));
        assert!(output.contains("found 1 issue (1 style)."));
    }
}
