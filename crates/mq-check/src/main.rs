use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use colored::Colorize;
use mq_check::{TypeChecker, TypeCheckerOptions, TypeError};
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
    let mut w = BufWriter::new(io::stderr());
    let multi = cli.files.len() > 1;
    let tc_options = TypeCheckerOptions {
        strict_array: cli.strict_array,
    };

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
            type_checker_options: tc_options,
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

/// Runs syntax and type checks on a single source, returns `true` if any errors were found.
fn check_file(w: &mut impl Write, code: &str, source_url: Option<Url>, opts: &CheckOptions<'_>) -> io::Result<bool> {
    let mut hir = Hir::default();

    if opts.no_builtins {
        hir.builtin.disabled = true;
    }

    hir.add_code(source_url, code);

    if let Some(lbl) = opts.label {
        writeln!(w, "{} {}", "──".dimmed(), lbl.bold())?;
    }

    let syntax_errors = check_syntax(w, &hir)?;
    if syntax_errors {
        return Ok(true);
    }

    check_type(w, &hir, opts.show_types, &opts.type_checker_options)
}

/// Checks HIR for syntax errors/warnings and writes them in a unified format.
/// Returns `true` if any errors or warnings were found.
fn check_syntax(w: &mut impl Write, hir: &mq_hir::Hir) -> io::Result<bool> {
    let errors = hir.error_ranges();
    let warnings = hir.warning_ranges();

    if errors.is_empty() && warnings.is_empty() {
        return Ok(false);
    }

    for (message, range) in &errors {
        writeln!(
            w,
            "  {} {} {} {}",
            format!("{}:{}", range.start.line, range.start.column).dimmed(),
            "│".dimmed(),
            "error:".bright_red().bold(),
            message.white(),
        )?;
    }

    for (message, range) in &warnings {
        writeln!(
            w,
            "  {} {} {} {}",
            format!("{}:{}", range.start.line, range.start.column).dimmed(),
            "│".dimmed(),
            "warning:".bright_yellow().bold(),
            message.white(),
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
    hir: &mq_hir::Hir,
    show_types: bool,
    options: &TypeCheckerOptions,
) -> io::Result<bool> {
    let mut checker = TypeChecker::with_options(*options);
    let mut errors = checker.check(hir);

    errors.sort_by_key(|a| a.location());

    for error in &errors {
        write_error(w, error)?;
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
            "{}  {} type error{} found.",
            "✗".bright_red().bold(),
            errors.len().to_string().bright_red().bold(),
            if errors.len() == 1 { "" } else { "s" },
        )?;
        Ok(true)
    }
}

/// Writes a single type error with rich formatting to the given writer.
fn write_error(w: &mut impl Write, error: &TypeError) -> io::Result<()> {
    let location = error.location();
    let loc_str = location
        .map(|(line, col)| format!("{}:{}", line, col).dimmed().to_string())
        .unwrap_or_default();
    let sep = "│".dimmed();

    match error {
        TypeError::Mismatch { expected, found, .. } => writeln!(
            w,
            "  {} {} {} expected {}, found {}",
            loc_str,
            sep,
            "type mismatch:".bright_red().bold(),
            expected.bright_cyan(),
            found.bright_yellow(),
        ),
        TypeError::UnificationError { left, right, .. } => writeln!(
            w,
            "  {} {} {} {} and {}",
            loc_str,
            sep,
            "cannot unify:".bright_red().bold(),
            left.bright_cyan(),
            right.bright_yellow(),
        ),
        TypeError::OccursCheck { var, ty, .. } => writeln!(
            w,
            "  {} {} {} type variable {} occurs in {}",
            loc_str,
            sep,
            "infinite type:".bright_red().bold(),
            var.bright_cyan(),
            ty.bright_yellow(),
        ),
        TypeError::UndefinedSymbol { name, .. } => writeln!(
            w,
            "  {} {} {} {}",
            loc_str,
            sep,
            "undefined symbol:".bright_red().bold(),
            name.bright_magenta(),
        ),
        TypeError::WrongArity { expected, found, .. } => writeln!(
            w,
            "  {} {} {} expected {}, found {}",
            loc_str,
            sep,
            "wrong arity:".bright_red().bold(),
            expected.to_string().bright_cyan(),
            found.to_string().bright_yellow(),
        ),
        TypeError::UndefinedField { field, record_ty, .. } => writeln!(
            w,
            "  {} {} {} field {} not found in {}",
            loc_str,
            sep,
            "undefined field:".bright_red().bold(),
            field.bright_magenta(),
            record_ty.bright_cyan(),
        ),
        TypeError::HeterogeneousArray { types, .. } => writeln!(
            w,
            "  {} {} {} [{}]",
            loc_str,
            sep,
            "heterogeneous array:".bright_red().bold(),
            types.bright_yellow(),
        ),
        TypeError::TypeVarNotFound(name) => writeln!(
            w,
            "  {} {} type variable {} not found",
            loc_str,
            sep,
            name.bright_cyan(),
        ),
        TypeError::Internal(msg) => writeln!(
            w,
            "  {} {} {}",
            loc_str,
            sep,
            format!("internal error: {msg}").bright_red(),
        ),
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
}
