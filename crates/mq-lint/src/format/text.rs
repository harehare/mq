use std::io::{self, Write};

use colored::Colorize;
use mq_lint::{Diagnostic, Severity};

/// Severities in the order categories are displayed, most severe first.
const SEVERITY_ORDER: [Severity; 4] = [Severity::Error, Severity::Warn, Severity::Perf, Severity::Style];

/// Writes `diagnostics` grouped by severity in a Credo-style report and returns `true` if any
/// were reported.
pub(super) fn write_text_report(
    w: &mut impl Write,
    file_label: &str,
    code: &str,
    diagnostics: &[Diagnostic],
) -> io::Result<bool> {
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
        write_category(w, severity, &group, file_label, code)?;
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

/// Maps a severity to its category title and one-letter marker.
fn severity_category(severity: Severity) -> (colored::ColoredString, colored::ColoredString) {
    match severity {
        Severity::Error => ("Errors".bright_red().bold(), "[E]".bright_red().bold()),
        Severity::Warn => ("Warnings".bright_yellow().bold(), "[W]".bright_yellow().bold()),
        Severity::Perf => ("Performance".blue().bold(), "[P]".blue().bold()),
        Severity::Style => ("Style".cyan().bold(), "[S]".cyan().bold()),
    }
}

/// Colors `s` to match `severity`, shared by the box frame, gutter bar, message text, and the
/// snippet's caret.
fn severity_color(severity: Severity, s: &str) -> colored::ColoredString {
    match severity {
        Severity::Error => s.bright_red(),
        Severity::Warn => s.bright_yellow(),
        Severity::Perf => s.blue(),
        Severity::Style => s.cyan(),
    }
}

/// Writes one severity category as a box-drawn frame (`┌─ Title` … `└─`) around its
/// diagnostics, each as a `[X] message` line, a source snippet with a caret underline (when a
/// range is known), then the `file:line:col .rule_id` location (the rule id rendered as an mq
/// selector, e.g. `.unused_variable`).
fn write_category(
    w: &mut impl Write,
    severity: Severity,
    diagnostics: &[&Diagnostic],
    file_label: &str,
    code: &str,
) -> io::Result<()> {
    let (title, letter) = severity_category(severity);
    let bar = severity_color(severity, "│");

    writeln!(w, "{} {title}", severity_color(severity, "┌─").bold())?;
    writeln!(w, "{bar}")?;

    for (i, diagnostic) in diagnostics.iter().enumerate() {
        writeln!(
            w,
            "{bar} {} {}",
            letter,
            severity_color(diagnostic.severity, &diagnostic.message()).bold()
        )?;

        if let Some(range) = &diagnostic.range {
            writeln!(w, "{bar}")?;
            write_snippet(w, code, range, diagnostic.severity, &bar)?;
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

    writeln!(w, "{}", severity_color(severity, "└─"))?;

    Ok(())
}

/// Writes the offending source line with a caret underline beneath it, colored to match
/// `severity` and indented under the category's gutter `bar`.
fn write_snippet(
    w: &mut impl Write,
    code: &str,
    range: &mq_lang::Range,
    severity: Severity,
    bar: &colored::ColoredString,
) -> io::Result<()> {
    let lines: Vec<&str> = code.lines().collect();
    let line_idx = range.start.line.saturating_sub(1) as usize;
    let Some(source_line) = lines.get(line_idx) else {
        return Ok(());
    };

    let col_start = range.start.column.saturating_sub(1);
    let col_end = if range.end.line == range.start.line {
        range.end.column.saturating_sub(1)
    } else {
        source_line.len()
    };
    let underline_len = col_end.saturating_sub(col_start).max(1);
    let line_num = range.start.line.to_string();

    writeln!(w, "{bar}    {} {} {}", line_num.dimmed(), "│".dimmed(), source_line)?;
    writeln!(
        w,
        "{bar}    {} {} {}{}",
        " ".repeat(line_num.len()),
        "│".dimmed(),
        " ".repeat(col_start),
        severity_color(severity, &"^".repeat(underline_len)).bold(),
    )?;
    writeln!(w, "{bar}")
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
        let had_diagnostics = write_text_report(&mut buf, "test.mq", "", &[]).unwrap();
        assert!(!had_diagnostics);
        assert!(String::from_utf8(buf).unwrap().contains("No lint issues found."));
    }

    #[test]
    fn test_write_text_report_with_diagnostics() {
        let code = r#".checked == true"#;
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        let diagnostics = crate::collect_diagnostics(code, &linter, &config, Severity::Style);

        let mut buf = Vec::new();
        let had_diagnostics = write_text_report(&mut buf, "test.mq", code, &diagnostics).unwrap();
        assert!(had_diagnostics);

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains("┌─ Style"));
        assert!(output.contains("└─"));
        assert!(output.contains("test.mq:1:1"));
        assert!(output.contains("found 1 issue (1 style)."));
    }

    #[test]
    fn test_write_text_report_shows_snippet_with_caret() {
        let code = ".checked == true";
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        let diagnostics = crate::collect_diagnostics(code, &linter, &config, Severity::Style);

        let mut buf = Vec::new();
        write_text_report(&mut buf, "test.mq", code, &diagnostics).unwrap();

        let output = String::from_utf8(buf).unwrap();
        assert!(output.contains(".checked == true"));
        assert!(output.contains('^'));
    }
}
