use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::Parser;
use colored::Colorize;
use mq_hir::Hir;
use mq_lint::{Diagnostic, LintConfig, LintContext, Linter, RuleId, Severity};

/// Static analysis linter for mq programs
#[derive(Parser)]
#[command(name = "mq-lint", about = "Lint mq programs")]
struct Cli {
    /// Paths to .mq files to lint (reads from stdin if omitted)
    files: Vec<PathBuf>,

    /// Disable a rule by ID (repeatable)
    #[arg(long = "disable", value_name = "RULE_ID")]
    disable: Vec<RuleId>,

    /// Only report diagnostics at or above this severity (style, perf, warn, error)
    #[arg(long, default_value = "style")]
    min_severity: SeverityArg,

    /// Print all available rule IDs and their default severity, then exit
    #[arg(long)]
    list_rules: bool,
}

#[derive(Clone, Copy)]
struct SeverityArg(Severity);

impl FromStr for SeverityArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "style" => Ok(SeverityArg(Severity::Style)),
            "perf" => Ok(SeverityArg(Severity::Perf)),
            "warn" => Ok(SeverityArg(Severity::Warn)),
            "error" => Ok(SeverityArg(Severity::Error)),
            other => Err(format!(
                "invalid severity `{other}` (expected style, perf, warn, or error)"
            )),
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(had_diagnostics) => {
            if had_diagnostics {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("{} {}", "error:".bright_red().bold(), e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<bool> {
    let cli = Cli::parse();
    let mut w = BufWriter::new(io::stdout());

    if cli.list_rules {
        list_rules(&mut w)?;
        return Ok(false);
    }

    let mut config = LintConfig::default();
    for rule_id in &cli.disable {
        config.disable_rule(*rule_id);
    }
    let min_severity = cli.min_severity.0;
    let linter = Linter::with_default_rules();

    if cli.files.is_empty() {
        let mut code = String::new();
        io::stdin().read_to_string(&mut code)?;
        let had_diagnostics = lint_source(&mut w, &code, None, &linter, &config, min_severity)?;
        return Ok(had_diagnostics);
    }

    let multi = cli.files.len() > 1;
    let mut had_diagnostics = false;

    for path in &cli.files {
        let code = std::fs::read_to_string(path)
            .map_err(|e| io::Error::other(format!("reading file {}: {}", path.display(), e)))?;
        let label = if multi { Some(path.display().to_string()) } else { None };
        if lint_source(&mut w, &code, label.as_deref(), &linter, &config, min_severity)? {
            had_diagnostics = true;
        }
        if multi {
            writeln!(w)?;
        }
    }

    Ok(had_diagnostics)
}

fn list_rules(w: &mut impl Write) -> io::Result<()> {
    let mut rules: Vec<_> = mq_lint::rules::all_rules();
    rules.sort_by_key(|r| r.id());
    for rule in &rules {
        writeln!(w, "{:<28} {}", rule.id().as_str().bright_cyan(), rule.severity())?;
    }
    Ok(())
}

/// Lints a single source, writes diagnostics, and returns `true` if any were reported.
fn lint_source(
    w: &mut impl Write,
    code: &str,
    label: Option<&str>,
    linter: &Linter,
    config: &LintConfig,
    min_severity: Severity,
) -> io::Result<bool> {
    let mut hir = Hir::default();
    let (source_id, _) = hir.add_code(None, code);
    let ctx = LintContext::new(&hir, source_id, config);

    let mut diagnostics: Vec<_> = linter
        .run(&ctx)
        .into_iter()
        .filter(|d| d.severity >= min_severity)
        .collect();
    diagnostics.sort_by_key(|d| d.range.map(|r| (r.start.line, r.start.column)));

    if let Some(lbl) = label {
        writeln!(w, "{} {}", "──".dimmed(), lbl.bold())?;
    }

    for diagnostic in &diagnostics {
        write_diagnostic(w, diagnostic, code)?;
    }

    if diagnostics.is_empty() {
        writeln!(
            w,
            "{}  {}",
            "✓".bright_green().bold(),
            "No lint issues found.".bright_green()
        )?;
    } else {
        writeln!(
            w,
            "{}  {} issue{} found.",
            "✗".bright_red().bold(),
            diagnostics.len().to_string().bright_red().bold(),
            if diagnostics.len() == 1 { "" } else { "s" },
        )?;
    }

    Ok(!diagnostics.is_empty())
}

fn severity_label(severity: Severity) -> colored::ColoredString {
    match severity {
        Severity::Style => "style".cyan().bold(),
        Severity::Perf => "perf".blue().bold(),
        Severity::Warn => "warn".bright_yellow().bold(),
        Severity::Error => "error".bright_red().bold(),
    }
}

fn write_diagnostic(w: &mut impl Write, diagnostic: &Diagnostic, code: &str) -> io::Result<()> {
    let loc_plain = diagnostic
        .range
        .map(|range| format!("{}:{}", range.start.line, range.start.column))
        .unwrap_or_default();
    let prefix_width = loc_plain.len();
    let sep = "│".dimmed();

    writeln!(
        w,
        "  {} {} {} {} {}",
        loc_plain.dimmed(),
        sep,
        severity_label(diagnostic.severity),
        diagnostic.rule_id().as_str().bright_magenta(),
        diagnostic.message(),
    )?;

    if let Some(range) = &diagnostic.range {
        write_snippet(w, code, range, prefix_width)?;
    }

    if let Some(help) = diagnostic.help() {
        writeln!(
            w,
            "  {} {} {}",
            " ".repeat(prefix_width),
            sep,
            format!("help: {help}").bright_blue(),
        )?;
    }

    Ok(())
}

/// Writes a source snippet for the diagnostic location with a caret underline.
fn write_snippet(w: &mut impl Write, code: &str, range: &mq_lang::Range, prefix_width: usize) -> io::Result<()> {
    let sep = "│".dimmed();
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
    let gutter = " ".repeat(prefix_width.max(line_num.len()));

    writeln!(w, "  {} {} {}", gutter, sep, source_line.dimmed())?;
    writeln!(
        w,
        "  {} {} {}{}",
        format!("{:>width$}", line_num, width = prefix_width).dimmed(),
        sep,
        " ".repeat(col_start),
        "^".repeat(underline_len).bright_red().bold(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use rstest::rstest;

    #[rstest]
    #[case(vec!["mq-lint", "test.mq"], vec!["test.mq"])]
    #[case(vec!["mq-lint"], vec![])]
    fn test_cli_parsing(#[case] args: Vec<&str>, #[case] expected_files: Vec<&str>) {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(
            cli.files,
            expected_files.into_iter().map(PathBuf::from).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_cli_disable_rule() {
        let cli = Cli::try_parse_from([
            "mq-lint",
            "--disable",
            "naming_convention",
            "--disable",
            "shadow_variable",
        ])
        .unwrap();
        assert_eq!(cli.disable, vec![RuleId::NamingConvention, RuleId::ShadowVariable]);
    }

    #[test]
    fn test_cli_min_severity() {
        let cli = Cli::try_parse_from(["mq-lint", "--min-severity", "warn"]).unwrap();
        assert_eq!(cli.min_severity.0, Severity::Warn);
    }

    #[test]
    fn test_cli_min_severity_invalid() {
        let result = Cli::try_parse_from(["mq-lint", "--min-severity", "bogus"]);
        assert!(result.is_err());
    }
}
