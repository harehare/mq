//! Command-line type checker for mq programs.

mod format;

use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use colored::Colorize;
use format::OutputFormat;
use mq_check::{TypeChecker, TypeCheckerOptions, TypeError, types::Type};
use mq_hir::Hir;
use url::Url;

/// Type checker for mq programs
#[derive(Parser)]
#[command(name = "mq-typecheck", about = "Type check mq programs")]
struct Cli {
    /// Paths to .mq files to type check (reads from stdin if omitted)
    files: Vec<PathBuf>,

    /// Display inferred types for all symbols
    #[arg(long)]
    show_types: bool,

    /// Disable automatic builtin preloading (use when checking builtin.mq itself)
    #[arg(long)]
    no_builtins: bool,

    /// Enforce homogeneous arrays (reject mixed-type arrays like [1, "hello"])
    #[arg(long)]
    strict_array: bool,

    /// Disable exhaustiveness checking for pattern match expressions
    #[arg(long)]
    no_exhaustive_patterns: bool,

    /// Type of the input document `.`, e.g. `h1 | h2`, `code` or `markdown - code`
    /// (default: unknown)
    #[arg(long, value_name = "TYPE", value_parser = parse_input_type)]
    input_type: Option<Type>,

    /// Diagnostic output format: `text` (human-readable), `json` (a single JSON array of
    /// diagnostics), `markdown` (a Markdown table, for PR descriptions or comments), or
    /// `sarif` (SARIF 2.1.0 JSON, for GitHub code scanning and other SARIF consumers)
    #[arg(long, value_enum, default_value_t)]
    format: OutputFormat,
}

impl Cli {
    fn type_checker_options(&self) -> TypeCheckerOptions {
        TypeCheckerOptions {
            strict_array: self.strict_array,
            no_exhaustive_patterns: self.no_exhaustive_patterns,
            input_type: self.input_type.clone(),
        }
    }
}

/// Parses `--input-type`, showing the valid names when the type is unknown.
fn parse_input_type(src: &str) -> Result<Type, String> {
    mq_check::type_expr::parse_type(src).map_err(|error| error.with_help())
}

/// Options for a single file check
struct CheckOptions<'a> {
    show_types: bool,
    label: Option<&'a str>,
    no_builtins: bool,
    type_checker_options: TypeCheckerOptions,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", "error:".bright_red().bold(), e);
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let cli = Cli::parse();

    if cli.format != OutputFormat::Text {
        return run_machine_format(&cli);
    }

    let mut w = BufWriter::new(io::stderr());
    let multi = cli.files.len() > 1;
    let tc_options = cli.type_checker_options();

    if cli.files.is_empty() {
        // Read from stdin
        let mut code = String::new();
        io::stdin().read_to_string(&mut code)?;
        let source_url = Url::parse("file:///stdin").ok();
        let opts = CheckOptions {
            show_types: cli.show_types,
            label: None,
            no_builtins: cli.no_builtins,
            type_checker_options: tc_options,
        };
        let had_errors = check_file(&mut w, &code, source_url, &opts)?;
        if had_errors {
            return Err(io::Error::other("type check failed"));
        }
        return Ok(());
    }

    let mut total_errors = 0usize;

    for path in &cli.files {
        let code = std::fs::read_to_string(path)
            .map_err(|e| io::Error::other(format!("reading file {}: {}", path.display(), e)))?;
        let source_url = Url::from_file_path(std::fs::canonicalize(path).unwrap_or(path.clone())).ok();
        let label = if multi { Some(path.display().to_string()) } else { None };

        // Debug: dump HIR structure
        if std::env::var("DUMP_HIR").is_ok() {
            let mut hir = Hir::default();
            hir.add_code(source_url.clone(), &code);
            for (id, symbol) in hir.symbols() {
                writeln!(
                    w,
                    "{:?} | {:?} | value={:?} | parent={:?}",
                    id, symbol.kind, symbol.value, symbol.parent
                )?;
            }
            writeln!(w, "---")?;
        }

        let opts = CheckOptions {
            show_types: cli.show_types,
            label: label.as_deref(),
            no_builtins: cli.no_builtins,
            type_checker_options: tc_options.clone(),
        };
        let had_errors = check_file(&mut w, &code, source_url, &opts)?;
        if had_errors {
            total_errors += 1;
        }

        if multi {
            writeln!(w)?;
        }
    }

    if total_errors > 0 {
        Err(io::Error::other("type check failed"))
    } else {
        Ok(())
    }
}

/// Runs syntax and type checks across all inputs and writes a single machine-readable
/// report (JSON or SARIF) to stdout, instead of the colored text report.
fn run_machine_format(cli: &Cli) -> io::Result<()> {
    let tc_options = cli.type_checker_options();

    let mut results: Vec<(String, Vec<format::CheckDiagnostic>)> = Vec::new();

    if cli.files.is_empty() {
        let mut code = String::new();
        io::stdin().read_to_string(&mut code)?;
        let source_url = Url::parse("file:///stdin").ok();
        results.push((
            "<stdin>".to_string(),
            collect_check_diagnostics(&code, source_url, cli.no_builtins, &tc_options),
        ));
    } else {
        for path in &cli.files {
            let code = std::fs::read_to_string(path)
                .map_err(|e| io::Error::other(format!("reading file {}: {}", path.display(), e)))?;
            let source_url = Url::from_file_path(std::fs::canonicalize(path).unwrap_or(path.clone())).ok();
            results.push((
                path.display().to_string(),
                collect_check_diagnostics(&code, source_url, cli.no_builtins, &tc_options),
            ));
        }
    }

    let had_errors = results
        .iter()
        .any(|(_, diagnostics)| diagnostics.iter().any(|d| d.severity == format::Severity::Error));

    let mut w = BufWriter::new(io::stdout());
    format::write_report(&mut w, cli.format, &results)?;
    w.flush()?;

    if had_errors {
        Err(io::Error::other("type check failed"))
    } else {
        Ok(())
    }
}

/// Creates the HIR to check against, knowing the names the test runner injects.
fn new_hir(no_builtins: bool) -> Hir {
    let mut hir = Hir::default();
    hir.builtin.disabled = no_builtins;
    for name in mq_check::TEST_RUNNER_GLOBALS {
        hir.declare_global(name);
    }
    hir
}

/// Whether `code` holds only blank lines and `#` comments.
///
/// The recovery parser reports an error for a program without expressions, which is valid here.
fn is_expression_free(code: &str) -> bool {
    code.lines().all(|line| {
        let line = line.trim_start_matches([' ', '\t', '\r']);
        line.is_empty() || line.starts_with('#')
    })
}

/// Parses `code` into `hir`, returning its source id and parse errors.
///
/// `Hir::add_code` drops parse errors, so the source is parsed here and added as nodes.
fn load_source(
    hir: &mut Hir,
    source_url: Option<Url>,
    code: &str,
) -> (mq_hir::SourceId, Vec<(String, mq_lang::Range)>) {
    let (nodes, errors) = mq_lang::parse_recovery(code);
    let url = source_url.unwrap_or_else(|| Url::parse("file:///").unwrap());
    let (source_id, _) = hir.add_nodes(url, &nodes);
    let errors = if is_expression_free(code) {
        Vec::new()
    } else {
        errors.error_ranges(code)
    };
    (source_id, errors)
}

/// Runs syntax and type checks on a single source, returning every diagnostic found.
/// Type checking is skipped when syntax errors are present, matching the text report's behavior.
fn collect_check_diagnostics(
    code: &str,
    source_url: Option<Url>,
    no_builtins: bool,
    type_checker_options: &TypeCheckerOptions,
) -> Vec<format::CheckDiagnostic> {
    let mut hir = new_hir(no_builtins);
    let (source_id, parse_errors) = load_source(&mut hir, source_url, code);

    let mut diagnostics = format::syntax_diagnostics(&hir, source_id, &parse_errors);
    let has_syntax_errors = diagnostics.iter().any(|d| d.severity == format::Severity::Error);

    if !has_syntax_errors {
        let mut checker = TypeChecker::with_options(type_checker_options.clone());
        let errors = checker.check(&hir);
        diagnostics.extend(format::type_diagnostics(&errors));
    }

    diagnostics
}

/// Runs syntax and type checks on a single source, returns `true` if any errors were found.
fn check_file(w: &mut impl Write, code: &str, source_url: Option<Url>, opts: &CheckOptions<'_>) -> io::Result<bool> {
    let mut hir = new_hir(opts.no_builtins);
    let (source_id, parse_errors) = load_source(&mut hir, source_url, code);

    if let Some(lbl) = opts.label {
        writeln!(w, "{} {}", "──".dimmed(), lbl.bold())?;
    }

    let syntax_errors = check_syntax(w, &format::syntax_diagnostics(&hir, source_id, &parse_errors))?;
    if syntax_errors {
        return Ok(true);
    }

    check_type(w, code, &hir, opts.show_types, &opts.type_checker_options)
}

/// Writes syntax errors and warnings in a unified format.
/// Returns `true` if any errors were found.
fn check_syntax(w: &mut impl Write, diagnostics: &[format::CheckDiagnostic]) -> io::Result<bool> {
    if diagnostics.is_empty() {
        return Ok(false);
    }

    let (errors, warnings): (Vec<_>, Vec<_>) = diagnostics.iter().partition(|d| d.severity == format::Severity::Error);

    for (label, diagnostic) in errors
        .iter()
        .map(|d| ("error:".bright_red().bold(), d))
        .chain(warnings.iter().map(|d| ("warning:".bright_yellow().bold(), d)))
    {
        let range = diagnostic.range.unwrap_or_default();
        writeln!(
            w,
            "  {} {} {} {}",
            format!("{}:{}", range.start.line, range.start.column).dimmed(),
            "│".dimmed(),
            label,
            diagnostic.message.white(),
        )?;
    }

    writeln!(w)?;
    let error_count = errors.len();
    let warning_count = warnings.len();

    if error_count > 0 {
        writeln!(
            w,
            "{}  {} syntax error{} found.",
            "✗".bright_red().bold(),
            error_count.to_string().bright_red().bold(),
            if error_count == 1 { "" } else { "s" },
        )?;
    }
    if warning_count > 0 {
        writeln!(
            w,
            "{}  {} warning{}.",
            "⚠".bright_yellow().bold(),
            warning_count.to_string().bright_yellow().bold(),
            if warning_count == 1 { "" } else { "s" },
        )?;
    }

    Ok(error_count > 0)
}

/// Runs type inference and writes errors in a unified format.
/// Returns `true` if any type errors were found.
fn check_type(
    w: &mut impl Write,
    code: &str,
    hir: &mq_hir::Hir,
    show_types: bool,
    options: &TypeCheckerOptions,
) -> io::Result<bool> {
    let mut checker = TypeChecker::with_options(options.clone());
    let mut errors = checker.check(hir);

    errors.sort_by_key(|a| a.location());

    let total = errors.len();
    for (i, error) in errors.iter().enumerate() {
        write_error(w, error, code, i + 1, total)?;
    }

    if show_types {
        write_inferred_types(w, &checker, hir)?;
    }

    if errors.is_empty() {
        if !show_types {
            writeln!(
                w,
                "{}  {}",
                "✓".bright_green().bold(),
                "No type errors found.".bright_green(),
            )?;
        }
        Ok(false)
    } else {
        writeln!(
            w,
            "{} {} type error{} found.",
            "✗".bright_red().bold(),
            total.to_string().bright_red().bold(),
            if total == 1 { "" } else { "s" },
        )?;
        Ok(true)
    }
}

/// Writes a source snippet for the error location with a caret underline.
///
/// `prefix_width` is the visual width of the location prefix (e.g. "1:5" = 3)
/// used to align the gutter with the surrounding error message lines.
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
    // Pad gutter to align with the loc_str prefix used in error message lines
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

/// Returns a short one-line title for an error (used in the numbered header).
fn error_title(error: &TypeError) -> String {
    match error {
        TypeError::Mismatch { expected, found, .. } => {
            format!("type mismatch: expected {expected}, found {found}")
        }
        TypeError::UnificationError { left, right, .. } => {
            format!("cannot unify: {left} and {right}")
        }
        TypeError::OccursCheck { var, ty, .. } => format!("infinite type: {var} in {ty}"),
        TypeError::UndefinedSymbol { name, .. } => format!("undefined symbol: {name}"),
        TypeError::WrongArity { expected, found, .. } => {
            format!("wrong arity: expected {expected}, found {found}")
        }
        TypeError::UndefinedField { field, record_ty, .. } => {
            format!("undefined field `{field}` in {record_ty}")
        }
        TypeError::UndefinedAttribute { attr, node_ty, .. } => {
            format!("undefined attribute `{attr}` on {node_ty}")
        }
        TypeError::HeterogeneousArray { types, .. } => format!("heterogeneous array: [{types}]"),
        TypeError::TypeVarNotFound(name) => format!("type variable not found: {name}"),
        TypeError::Internal(msg) => format!("internal error: {msg}"),
        TypeError::NullablePropagation { op, nullable_arg, .. } => {
            format!("nullable propagation: `{op}` with nullable arg `{nullable_arg}`")
        }
        TypeError::UnreachableCode { reason, .. } => format!("unreachable code: {reason}"),
        TypeError::NonExhaustiveMatch { missing, .. } => {
            format!("non-exhaustive patterns: missing case for {missing}")
        }
    }
}

/// Writes a single type error with rich formatting to the given writer.
///
/// `index` is the 1-based error number; `total` is the total error count.
/// - Single error  → compact inline format  (`loc │ message` + snippet)
/// - Multiple errors → Rust-style block format (`error[E000N] title` / ` --> loc` / snippet)
fn write_error(w: &mut impl Write, error: &TypeError, code: &str, index: usize, total: usize) -> io::Result<()> {
    let location = error.location();
    let loc_plain = location
        .map(|range| format!("{}:{}", range.start.line, range.start.column))
        .unwrap_or_default();

    if total > 1 {
        // ── Rust-style block ──────────────────────────────────────────────────
        writeln!(
            w,
            "{} {}",
            format!("error[E{index:04}]").bright_red().bold(),
            error_title(error).white().bold(),
        )?;
        if !loc_plain.is_empty() {
            writeln!(w, "  {} {}", "-->".dimmed(), loc_plain.dimmed())?;
        }
        if let Some(range) = &location {
            write_snippet(w, code, range, loc_plain.len())?;
        }
        if let Some(ctx) = error_help(error) {
            writeln!(
                w,
                "  {} {} {}",
                " ".repeat(loc_plain.len()),
                "│".dimmed(),
                format!("help: {ctx}").bright_blue(),
            )?;
        }
        writeln!(w)?; // blank line between blocks
    } else {
        // ── Compact inline format ─────────────────────────────────────────────
        let prefix_width = loc_plain.len();
        let loc_str = if loc_plain.is_empty() {
            String::new()
        } else {
            loc_plain.dimmed().to_string()
        };
        let sep = "│".dimmed();

        writeln!(w, "  {} {} {}", loc_str, sep, error_title(error).white(),)?;
        if let Some(range) = &location {
            write_snippet(w, code, range, prefix_width)?;
        }
        if let Some(ctx) = error_help(error) {
            writeln!(
                w,
                "  {} {} {}",
                " ".repeat(prefix_width),
                sep,
                format!("help: {ctx}").bright_blue(),
            )?;
        }
    }
    Ok(())
}

/// Returns the optional help/context string for an error.
fn error_help(error: &TypeError) -> Option<&str> {
    match error {
        TypeError::Mismatch { context, .. }
        | TypeError::UnificationError { context, .. }
        | TypeError::WrongArity { context, .. } => context.as_deref(),
        _ => None,
    }
}

/// Writes inferred types with rich formatting to the given writer.
fn write_inferred_types(w: &mut impl Write, checker: &TypeChecker, hir: &Hir) -> io::Result<()> {
    writeln!(w)?;
    writeln!(w, "  {}", "Inferred Types".bright_cyan().bold())?;
    writeln!(w, "  {}", "──────────────".dimmed())?;

    let mut has_types = false;
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && !hir.is_builtin_symbol(symbol)
            && let Some(name) = &symbol.value
        {
            has_types = true;
            writeln!(
                w,
                "  {} {} {}",
                name.to_string().bright_magenta(),
                ":".dimmed(),
                type_scheme.to_string().bright_cyan(),
            )?;
        }
    }

    if !has_types {
        writeln!(w, "  {}", "(no user-defined symbols)".dimmed())?;
    }

    writeln!(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use rstest::rstest;

    #[test]
    fn test_cli_input_type() {
        let cli = Cli::try_parse_from(["mq-check", "--input-type", "h1 | h2"]).unwrap();
        assert_eq!(
            cli.type_checker_options().input_type.unwrap().display_renumbered(),
            "h1 | h2"
        );
        assert!(Cli::try_parse_from(["mq-check"]).unwrap().input_type.is_none());
    }

    #[test]
    fn test_cli_input_type_rejects_unknown_names_and_lists_the_valid_ones() {
        let Err(error) = Cli::try_parse_from(["mq-check", "--input-type", "headin"]) else {
            panic!("an unknown type must be rejected");
        };
        let error = error.to_string();
        assert!(
            error.contains("unknown type `headin`") && error.contains("h1"),
            "{error}"
        );
    }

    #[test]
    fn test_input_type_is_applied_to_the_checked_source() {
        let options = TypeCheckerOptions {
            input_type: Some(mq_check::type_expr::parse_type("code").unwrap()),
            ..Default::default()
        };
        let diagnostics = collect_check_diagnostics(".depth", None, false, &options);
        assert!(diagnostics.iter().any(|d| d.code == "typechecker::undefined_attribute"));
        let diagnostics = collect_check_diagnostics(".depth", None, false, &TypeCheckerOptions::default());
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn test_circular_module_closed_in_a_nested_module_fails_the_checked_source() {
        let dir = std::env::temp_dir().join(format!("mq-check-circular-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.mq"), "include \"b\"\ndef fa(): 1;").unwrap();
        std::fs::write(dir.join("b.mq"), "include \"a\"\ndef fb(): 2;").unwrap();
        let mut loader = mq_lang::ModuleLoader::default();
        loader.set_search_paths(vec![dir]);
        let mut hir = Hir::new(loader);
        let (source_id, parse_errors) = load_source(&mut hir, None, "include \"a\" | fa()");

        let diagnostics = format::syntax_diagnostics(&hir, source_id, &parse_errors);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "hir::circular_module");
        assert_eq!(diagnostics[0].severity, format::Severity::Error);
        assert!(diagnostics[0].message.contains("b.mq"), "{}", diagnostics[0].message);
        assert!(diagnostics[0].range.is_none());
    }

    #[rstest]
    #[case(vec!["mq-check", "test.mq"], vec!["test.mq"], false, false)]
    #[case(vec!["mq-check", "test.mq", "--strict-array"], vec!["test.mq"], true, false)]
    #[case(vec!["mq-check", "--show-types"], vec![], false, true)]
    #[case(vec!["mq-check", "--no-builtins"], vec![], false, false)]
    fn test_cli_parsing(
        #[case] args: Vec<&str>,
        #[case] expected_files: Vec<&str>,
        #[case] expected_strict: bool,
        #[case] expected_show_types: bool,
    ) {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(
            cli.files,
            expected_files.into_iter().map(PathBuf::from).collect::<Vec<_>>()
        );
        assert_eq!(cli.strict_array, expected_strict);
        assert_eq!(cli.show_types, expected_show_types);
    }

    #[test]
    fn test_cli_no_builtins() {
        let cli = Cli::try_parse_from(["mq-check", "--no-builtins"]).unwrap();
        assert!(cli.no_builtins);
    }

    #[rstest]
    #[case(vec!["mq-check"], OutputFormat::Text)]
    #[case(vec!["mq-check", "--format", "text"], OutputFormat::Text)]
    #[case(vec!["mq-check", "--format", "json"], OutputFormat::Json)]
    #[case(vec!["mq-check", "--format", "markdown"], OutputFormat::Markdown)]
    #[case(vec!["mq-check", "--format", "sarif"], OutputFormat::Sarif)]
    fn test_cli_format(#[case] args: Vec<&str>, #[case] expected: OutputFormat) {
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.format, expected);
    }

    #[test]
    fn test_cli_format_invalid() {
        let result = Cli::try_parse_from(["mq-check", "--format", "bogus"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_collect_check_diagnostics_reports_undefined_symbol() {
        let diagnostics = collect_check_diagnostics("undefined_fn()", None, false, &TypeCheckerOptions::default());
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code == "typechecker::undefined_symbol" || d.code == "hir::unresolved_symbol")
        );
        assert!(diagnostics.iter().any(|d| d.severity == format::Severity::Error));
    }

    #[test]
    fn test_collect_check_diagnostics_ignores_errors_inside_included_modules() {
        // `TEST_FILE` is injected by the test runner, so the `test` module itself leaves it
        // unresolved. That is a problem of the module, not of the checked source.
        let diagnostics = collect_check_diagnostics(
            "include \"test\" | assert_eq(1, 1)",
            None,
            false,
            &TypeCheckerOptions::default(),
        );
        assert!(
            !diagnostics.iter().any(|d| d.message.contains("TEST_FILE")),
            "{diagnostics:?}"
        );
    }

    #[rstest]
    #[case("include \"test\" | assert_eq([], [])")]
    #[case("include \"test\" | assert_eq([1], [1])")]
    #[case("include \"test\" | assert_eq({a: 1}, {a: 1})")]
    #[case("include \"test\" | def f(): assert_eq([1], [2]); | f()")]
    fn test_collect_check_diagnostics_calls_included_functions_with_any_arguments(#[case] code: &str) {
        // Calling an included function must not be read as a bracket access (`slice`) on a
        // variable.
        let diagnostics = collect_check_diagnostics(code, None, false, &TypeCheckerOptions::default());
        assert!(diagnostics.is_empty(), "{code}: {diagnostics:?}");
    }

    #[rstest]
    #[case::unclosed_paren("if (")]
    #[case::missing_value("let x = | x")]
    #[case::bad_parameter_list("def f(: 1;")]
    fn test_syntax_errors_are_reported_and_skip_type_checking(#[case] code: &str) {
        let diagnostics = collect_check_diagnostics(code, None, false, &TypeCheckerOptions::default());
        assert!(!diagnostics.is_empty(), "{code}");
        assert!(
            diagnostics
                .iter()
                .all(|d| d.code == "mq::syntax" && d.severity == format::Severity::Error),
            "{code}: {diagnostics:?}"
        );

        let mut out = Vec::new();
        let opts = CheckOptions {
            show_types: false,
            label: None,
            no_builtins: false,
            type_checker_options: TypeCheckerOptions::default(),
        };
        assert!(check_file(&mut out, code, None, &opts).unwrap());
        assert!(String::from_utf8(out).unwrap().contains("syntax error"));
    }

    #[rstest]
    #[case::empty("")]
    #[case::blank_lines("\n  \n\t\n")]
    #[case::comment("# only a comment")]
    #[case::comments_and_blanks("# a\n\n  # b\n")]
    fn test_expression_free_source_has_no_errors(#[case] code: &str) {
        let diagnostics = collect_check_diagnostics(code, None, false, &TypeCheckerOptions::default());
        assert!(diagnostics.is_empty(), "{code:?}: {diagnostics:?}");

        let mut out = Vec::new();
        let opts = CheckOptions {
            show_types: false,
            label: None,
            no_builtins: false,
            type_checker_options: TypeCheckerOptions::default(),
        };
        assert!(!check_file(&mut out, code, None, &opts).unwrap());
    }

    #[rstest]
    #[case::unclosed_string("\"abc")]
    #[case::comment_then_incomplete("# c\nif (")]
    fn test_syntax_errors_after_comments_are_still_reported(#[case] code: &str) {
        let diagnostics = collect_check_diagnostics(code, None, false, &TypeCheckerOptions::default());
        assert!(
            !diagnostics.is_empty() && diagnostics.iter().all(|d| d.code == "mq::syntax"),
            "{code:?}: {diagnostics:?}"
        );
    }

    #[test]
    fn test_collect_check_diagnostics_no_errors_on_valid_code() {
        let diagnostics = collect_check_diagnostics(".h1", None, true, &TypeCheckerOptions::default());
        assert!(!diagnostics.iter().any(|d| d.severity == format::Severity::Error));
    }
}
