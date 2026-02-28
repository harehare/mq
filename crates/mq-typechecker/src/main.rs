use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use colored::Colorize;
use mq_hir::Hir;
use mq_typechecker::TypeChecker;
use url::Url;

/// Type checker for mq programs
#[derive(Parser)]
#[command(name = "mq-typecheck", about = "Type check mq programs")]
struct Cli {
    /// Path to a .mq file to type check (reads from stdin if omitted)
    file: Option<PathBuf>,

    /// Display inferred types for all symbols
    #[arg(long)]
    show_types: bool,

    /// Disable automatic builtin preloading (use when checking builtin.mq itself)
    #[arg(long)]
    no_builtins: bool,
}

fn main() -> ExitCode {
    run().unwrap_or_else(|exit_code| exit_code)
}

fn run() -> Result<ExitCode, ExitCode> {
    let cli = Cli::parse();
    let mut w = BufWriter::new(io::stderr());

    let (code, source_url) = match &cli.file {
        Some(path) => {
            let code = std::fs::read_to_string(path).map_err(|e| {
                let _ = writeln!(
                    w,
                    "{} {}",
                    "error:".bright_red().bold(),
                    format!("reading file {}: {}", path.display(), e).white(),
                );
                ExitCode::FAILURE
            })?;
            let url = Url::from_file_path(std::fs::canonicalize(path).unwrap_or(path.clone()))
                .unwrap_or_else(|_| Url::parse("file:///stdin").unwrap());
            (code, Some(url))
        }
        None => {
            let mut code = String::new();
            io::stdin().read_to_string(&mut code).map_err(|e| {
                let _ = writeln!(
                    w,
                    "{} {}",
                    "error:".bright_red().bold(),
                    format!("reading stdin: {}", e).white(),
                );
                ExitCode::FAILURE
            })?;
            (code, None)
        }
    };

    let mut hir = Hir::default();
    if cli.no_builtins {
        hir.builtin.disabled = true;
    }
    hir.add_code(source_url, &code);

    // Debug: dump HIR structure
    if std::env::var("DUMP_HIR").is_ok() {
        for (id, symbol) in hir.symbols() {
            let _ = writeln!(
                w,
                "{:?} | {:?} | value={:?} | parent={:?}",
                id, symbol.kind, symbol.value, symbol.parent
            );
        }
        let _ = writeln!(w, "---");
    }

    let mut checker = TypeChecker::new();
    let mut errors = checker.check(&hir);

    errors.sort_by_key(|a| a.location());

    for error in &errors {
        write_error(&mut w, error);
    }

    if cli.show_types {
        write_inferred_types(&mut w, &checker, &hir);
    }

    if errors.is_empty() {
        if !cli.show_types {
            let _ = writeln!(
                w,
                "{}  {}",
                "✓".bright_green().bold(),
                "No type errors found.".bright_green(),
            );
        }
        Ok(ExitCode::SUCCESS)
    } else {
        let _ = writeln!(w);
        let _ = writeln!(
            w,
            "{}  {} type error{} found.",
            "✗".bright_red().bold(),
            errors.len().to_string().bright_red().bold(),
            if errors.len() == 1 { "" } else { "s" },
        );
        Err(ExitCode::FAILURE)
    }
}

/// Writes a type error with rich formatting to the given writer.
fn write_error(w: &mut impl Write, error: &mq_typechecker::TypeError) {
    let error_msg = format!("{}", error);
    let formatted = format_error_message(&error_msg);

    if let Some((line, col)) = error.location() {
        let _ = writeln!(
            w,
            "  {} {} {}",
            format!("{}:{}", line, col).dimmed(),
            "│".dimmed(),
            formatted,
        );
    } else {
        let _ = writeln!(w, "  {}", formatted);
    }
}

/// Formats an error message with colored type names.
fn format_error_message(msg: &str) -> String {
    if let Some(rest) = msg.strip_prefix("Type mismatch: expected ")
        && let Some((expected, found)) = rest.split_once(", found ")
    {
        return format!(
            "{} expected {}, found {}",
            "type mismatch:".bright_red().bold(),
            expected.bright_cyan(),
            found.bright_yellow(),
        );
    }
    if let Some(rest) = msg.strip_prefix("Cannot unify types: ")
        && let Some((left, right)) = rest.split_once(" and ")
    {
        return format!(
            "{} {} and {}",
            "cannot unify:".bright_red().bold(),
            left.bright_cyan(),
            right.bright_yellow(),
        );
    }
    if let Some(rest) = msg.strip_prefix("Wrong number of arguments: expected ")
        && let Some((expected, found)) = rest.split_once(", found ")
    {
        return format!(
            "{} expected {}, found {}",
            "wrong arity:".bright_red().bold(),
            expected.bright_cyan(),
            found.bright_yellow(),
        );
    }
    if let Some(name) = msg.strip_prefix("Undefined symbol: ") {
        return format!("{} {}", "undefined symbol:".bright_red().bold(), name.bright_magenta(),);
    }

    format!("{}", msg.bright_red())
}

/// Writes inferred types with rich formatting to the given writer.
fn write_inferred_types(w: &mut impl Write, checker: &TypeChecker, hir: &Hir) {
    let _ = writeln!(w);
    let _ = writeln!(w, "  {}", "Inferred Types".bright_cyan().bold());
    let _ = writeln!(w, "  {}", "──────────────".dimmed());

    let mut has_types = false;
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && !hir.is_builtin_symbol(symbol)
            && let Some(name) = &symbol.value
        {
            has_types = true;
            let _ = writeln!(
                w,
                "  {} {} {}",
                name.to_string().bright_magenta(),
                ":".dimmed(),
                type_scheme.to_string().bright_cyan(),
            );
        }
    }

    if !has_types {
        let _ = writeln!(w, "  {}", "(no user-defined symbols)".dimmed());
    }

    let _ = writeln!(w);
}
