//! Command-line interface of the mq linter.

mod format;

use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use clap::Parser;
use colored::Colorize;
use format::OutputFormat;
use mq_hir::Hir;
use mq_lint::{Diagnostic, LintConfig, LintContext, LintMessage, Linter, RuleId, Severity};

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

    /// Rewrite files in place, applying every diagnostic with a machine-applicable fix
    /// (reads stdin if no files are given, writing the fixed code to stdout)
    #[arg(long)]
    fix: bool,

    /// Diagnostic output format: `text` (human-readable), `json` (a single JSON array of
    /// diagnostics), `markdown` (a Markdown table, for PR descriptions or comments), `sarif`
    /// (SARIF 2.1.0 JSON, for GitHub code scanning and other SARIF consumers), or `github`
    /// (GitHub Actions `::error`/`::warning`/`::notice` workflow-command annotations)
    #[arg(long, value_enum, default_value_t)]
    format: OutputFormat,
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

        if cli.fix {
            return match fix_source(&code, &linter, &config) {
                Ok((fixed, _)) => {
                    write!(w, "{fixed}")?;
                    Ok(false)
                }
                Err(errors) => {
                    // stdout carries the fixed source, so the errors go to stderr as text.
                    let report = [("<stdin>".to_string(), code, syntax_diagnostics(errors))];
                    format::write_report(&mut io::stderr(), OutputFormat::Text, &report)?;
                    Ok(true)
                }
            };
        }

        let (diagnostics, failed) = match collect_diagnostics(&code, &linter, &config, min_severity) {
            Ok(diagnostics) => (diagnostics, false),
            Err(errors) => (syntax_diagnostics(errors), true),
        };
        let had_diagnostics = failed || !diagnostics.is_empty();
        format::write_report(&mut w, cli.format, &[("<stdin>".to_string(), code, diagnostics)])?;
        return Ok(had_diagnostics);
    }

    let mut results: Vec<(String, String, Vec<Diagnostic>)> = Vec::with_capacity(cli.files.len());
    let mut had_syntax_errors = false;

    for path in &cli.files {
        let code = std::fs::read_to_string(path)
            .map_err(|e| io::Error::other(format!("reading file {}: {}", path.display(), e)))?;
        let label = path.display().to_string();

        let code = if cli.fix {
            let (fixed, fix_count) = match fix_source(&code, &linter, &config) {
                Ok(fixed) => fixed,
                Err(errors) => {
                    had_syntax_errors = true;
                    results.push((label, code, syntax_diagnostics(errors)));
                    continue;
                }
            };
            if fixed != code {
                std::fs::write(path, &fixed)
                    .map_err(|e| io::Error::other(format!("writing file {}: {}", path.display(), e)))?;
                let issue_word = if fix_count == 1 { "issue" } else { "issues" };
                if cli.format == OutputFormat::Text {
                    writeln!(
                        w,
                        "{} {fix_count} {issue_word} in {label}",
                        "fixed".bright_green().bold()
                    )?;
                } else {
                    eprintln!("fixed {fix_count} {issue_word} in {label}");
                }
            }
            fixed
        } else {
            code
        };

        match collect_diagnostics(&code, &linter, &config, min_severity) {
            Ok(diagnostics) => results.push((label, code, diagnostics)),
            Err(errors) => {
                had_syntax_errors = true;
                results.push((label, code, syntax_diagnostics(errors)));
            }
        }
    }

    let had_diagnostics = results.iter().any(|(_, _code, diagnostics)| !diagnostics.is_empty());
    format::write_report(&mut w, cli.format, &results)?;

    Ok(had_diagnostics || had_syntax_errors)
}

/// Converts parser errors into error-severity diagnostics so every output format reports them.
fn syntax_diagnostics(errors: Vec<mq_lang::Diagnostic>) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .map(|error| {
            let diagnostic = Diagnostic::new(LintMessage::SyntaxError { message: error.message }, Severity::Error);
            match error.range {
                Some(range) => diagnostic.with_range(range),
                None => diagnostic,
            }
        })
        .collect()
}

/// Applies every diagnostic with a fix to `code`, returning the rewritten source and how many
/// fixes were applied, or the syntax errors if `code` does not parse.
fn fix_source(code: &str, linter: &Linter, config: &LintConfig) -> Result<(String, usize), Vec<mq_lang::Diagnostic>> {
    let mut hir = Hir::default();
    let (source_id, _, errors) = hir.add_code(None, code);
    if !errors.is_empty() {
        return Err(errors);
    }
    let ctx = LintContext::new(&hir, source_id, config);

    let edits: Vec<(mq_lang::Range, String)> = linter
        .run(&ctx)
        .into_iter()
        .filter_map(|d| d.fix.as_ref().and_then(|fix| fix.resolve(code)))
        .collect();

    let fix_count = edits.len();
    Ok((mq_lint::fix::apply_edits(code, &edits), fix_count))
}

fn list_rules(w: &mut impl Write) -> io::Result<()> {
    let mut rules: Vec<_> = mq_lint::rules::all_rules();
    rules.sort_by_key(|r| r.id());
    for rule in &rules {
        writeln!(w, "{:<28} {}", rule.id().as_str().bright_cyan(), rule.severity())?;
    }
    Ok(())
}

/// Runs the linter over a single source, returning diagnostics at or above `min_severity`
/// sorted by source position, or the syntax errors if `code` does not parse.
pub(crate) fn collect_diagnostics(
    code: &str,
    linter: &Linter,
    config: &LintConfig,
    min_severity: Severity,
) -> Result<Vec<Diagnostic>, Vec<mq_lang::Diagnostic>> {
    let mut hir = Hir::default();
    let (source_id, _, errors) = hir.add_code(None, code);
    if !errors.is_empty() {
        return Err(errors);
    }
    let ctx = LintContext::new(&hir, source_id, config);

    let mut diagnostics: Vec<_> = linter
        .run(&ctx)
        .into_iter()
        .filter(|d| d.severity >= min_severity)
        .collect();
    diagnostics.sort_by_key(|d| d.range.map(|r| (r.start.line, r.start.column)));
    Ok(diagnostics)
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

    #[test]
    fn test_cli_fix_flag() {
        let cli = Cli::try_parse_from(["mq-lint", "--fix", "test.mq"]).unwrap();
        assert!(cli.fix);

        let cli = Cli::try_parse_from(["mq-lint", "test.mq"]).unwrap();
        assert!(!cli.fix);
    }

    #[rstest]
    #[case(vec!["mq-lint"], OutputFormat::Text)]
    #[case(vec!["mq-lint", "--format", "text"], OutputFormat::Text)]
    #[case(vec!["mq-lint", "--format", "json"], OutputFormat::Json)]
    #[case(vec!["mq-lint", "--format", "markdown"], OutputFormat::Markdown)]
    #[case(vec!["mq-lint", "--format", "sarif"], OutputFormat::Sarif)]
    #[case(vec!["mq-lint", "--format", "github"], OutputFormat::Github)]
    fn test_cli_format(#[case] args: Vec<&str>, #[case] expected: OutputFormat) {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.format, expected);
    }

    #[test]
    fn test_cli_format_invalid() {
        let result = Cli::try_parse_from(["mq-lint", "--format", "bogus"]);
        assert!(result.is_err());
    }

    #[rstest]
    #[case(r#".checked == true"#, ".checked")]
    #[case(r#"try: get("x") catch: none"#, r#"get("x")?"#)]
    #[case(r#"s"${x}""#, "x")]
    fn test_fix_source_applies_known_fixable_rules(#[case] code: &str, #[case] expected: &str) {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        let (fixed, fix_count) = fix_source(code, &linter, &config).unwrap();
        assert_eq!(fixed, expected);
        assert_eq!(fix_count, 1);
    }

    #[test]
    fn test_fix_source_is_a_noop_when_nothing_is_fixable() {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        let (fixed, fix_count) = fix_source(".h1", &linter, &config).unwrap();
        assert_eq!(fixed, ".h1");
        assert_eq!(fix_count, 0);
    }

    #[rstest]
    #[case("[1, 2", true)]
    #[case("1 || | 2", true)]
    #[case("if (true): 1 else", true)]
    #[case("{\"a\": 1,, \"b\": 2}", true)]
    #[case("match (1): | 1: 2", true)]
    #[case("foreach (x [1]): x;", true)]
    #[case("let x = 1 | x", false)]
    #[case("", false)]
    #[case("# only a comment\n", false)]
    fn test_syntax_errors_fail_lint_and_fix(#[case] code: &str, #[case] expected: bool) {
        let config = LintConfig::default();
        let linter = Linter::with_default_rules();
        assert_eq!(
            collect_diagnostics(code, &linter, &config, Severity::Style).is_err(),
            expected
        );
        assert_eq!(fix_source(code, &linter, &config).is_err(), expected);
    }

    #[test]
    fn test_syntax_errors_become_error_diagnostics_in_every_format() {
        let errors = collect_diagnostics(
            "[1, 2",
            &Linter::with_default_rules(),
            &LintConfig::default(),
            Severity::Style,
        )
        .unwrap_err();
        let diagnostics = syntax_diagnostics(errors);
        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .all(|d| d.rule_id() == RuleId::SyntaxError && d.severity == Severity::Error && d.range.is_some())
        );

        for format in [
            OutputFormat::Text,
            OutputFormat::Json,
            OutputFormat::Markdown,
            OutputFormat::Sarif,
            OutputFormat::Github,
        ] {
            let mut out = Vec::new();
            format::write_report(
                &mut out,
                format,
                &[("broken.mq".to_string(), "[1, 2".to_string(), diagnostics.clone())],
            )
            .unwrap();
            assert!(String::from_utf8(out).unwrap().contains("syntax_error"), "{format:?}");
        }
    }
}
