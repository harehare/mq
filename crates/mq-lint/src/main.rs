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
        let had_diagnostics = lint_source(&mut w, &code, "<stdin>", &linter, &config, min_severity)?;
        return Ok(had_diagnostics);
    }

    let mut had_diagnostics = false;

    for path in &cli.files {
        let code = std::fs::read_to_string(path)
            .map_err(|e| io::Error::other(format!("reading file {}: {}", path.display(), e)))?;
        let label = path.display().to_string();
        if lint_source(&mut w, &code, &label, &linter, &config, min_severity)? {
            had_diagnostics = true;
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

/// Severities in the order categories are displayed, most severe first.
const SEVERITY_ORDER: [Severity; 4] = [Severity::Error, Severity::Warn, Severity::Perf, Severity::Style];

/// Lints a single source, writes diagnostics grouped by severity in a Credo-style report,
/// and returns `true` if any were reported.
fn lint_source(
    w: &mut impl Write,
    code: &str,
    file_label: &str,
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
        write_summary(w, &diagnostics)?;
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
