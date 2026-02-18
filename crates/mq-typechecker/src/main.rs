use std::io::Read;
use std::path::PathBuf;
use std::process;

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

fn main() {
    let cli = Cli::parse();

    let (code, source_url) = match &cli.file {
        Some(path) => {
            let code = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!(
                        "{} {}",
                        "error:".truecolor(239, 68, 68).bold(),
                        format!("reading file {}: {}", path.display(), e).truecolor(226, 232, 240),
                    );
                    process::exit(1);
                }
            };
            let url = Url::from_file_path(std::fs::canonicalize(path).unwrap_or(path.clone()))
                .unwrap_or_else(|_| Url::parse("file:///stdin").unwrap());
            (code, Some(url))
        }
        None => {
            let mut code = String::new();
            if let Err(e) = std::io::stdin().read_to_string(&mut code) {
                eprintln!(
                    "{} {}",
                    "error:".truecolor(239, 68, 68).bold(),
                    format!("reading stdin: {}", e).truecolor(226, 232, 240),
                );
                process::exit(1);
            }
            (code, None)
        }
    };

    let mut hir = Hir::default();
    if cli.no_builtins {
        hir.builtin.disabled = true;
    }
    hir.add_code(source_url, &code);
    hir.resolve();

    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);

    for error in &errors {
        print_error(error);
    }

    if cli.show_types {
        print_inferred_types(&checker, &hir);
    }

    if errors.is_empty() {
        if !cli.show_types {
            eprintln!(
                "{}  {}",
                "✓".truecolor(16, 185, 129).bold(),
                "No type errors found.".truecolor(16, 185, 129),
            );
        }
        process::exit(0);
    } else {
        eprintln!();
        eprintln!(
            "{}  {} type error{} found.",
            "✗".truecolor(239, 68, 68).bold(),
            errors.len().to_string().truecolor(239, 68, 68).bold(),
            if errors.len() == 1 { "" } else { "s" },
        );
        process::exit(1);
    }
}

/// Prints a type error with rich formatting.
fn print_error(error: &mq_typechecker::TypeError) {
    let location_str = if let Some((line, col)) = error.location() {
        format!("{}:{}", line, col)
    } else {
        String::new()
    };

    let error_msg = format!("{}", error);

    if !location_str.is_empty() {
        eprintln!(
            "  {} {} {}",
            location_str.truecolor(148, 163, 184),
            "│".truecolor(74, 85, 104),
            format_error_message(&error_msg),
        );
    } else {
        eprintln!("  {}", format_error_message(&error_msg),);
    }
}

/// Formats an error message with colored type names.
fn format_error_message(msg: &str) -> String {
    // Color "error" label and highlight type names in the message
    if let Some(rest) = msg.strip_prefix("Type mismatch: expected ")
        && let Some((expected, found)) = rest.split_once(", found ")
    {
        return format!(
            "{} expected {}, found {}",
            "type mismatch:".truecolor(239, 68, 68).bold(),
            expected.truecolor(103, 184, 227),
            found.truecolor(251, 146, 60),
        );
    }
    if let Some(rest) = msg.strip_prefix("Cannot unify types: ")
        && let Some((left, right)) = rest.split_once(" and ")
    {
        return format!(
            "{} {} and {}",
            "cannot unify:".truecolor(239, 68, 68).bold(),
            left.truecolor(103, 184, 227),
            right.truecolor(251, 146, 60),
        );
    }
    if let Some(rest) = msg.strip_prefix("Wrong number of arguments: expected ")
        && let Some((expected, found)) = rest.split_once(", found ")
    {
        return format!(
            "{} expected {}, found {}",
            "wrong arity:".truecolor(239, 68, 68).bold(),
            expected.truecolor(103, 184, 227),
            found.truecolor(251, 146, 60),
        );
    }
    if let Some(name) = msg.strip_prefix("Undefined symbol: ") {
        return format!(
            "{} {}",
            "undefined symbol:".truecolor(239, 68, 68).bold(),
            name.truecolor(167, 139, 250),
        );
    }

    // Fallback: just color the whole message
    format!("{}", msg.truecolor(239, 68, 68))
}

/// Prints inferred types with rich formatting.
fn print_inferred_types(checker: &TypeChecker, hir: &Hir) {
    eprintln!();
    eprintln!("  {}", "Inferred Types".truecolor(103, 184, 227).bold(),);
    eprintln!("  {}", "──────────────".truecolor(74, 85, 104),);

    let mut has_types = false;
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && !hir.is_builtin_symbol(symbol)
            && let Some(name) = &symbol.value
        {
            has_types = true;
            eprintln!(
                "  {} {} {}",
                name.to_string().truecolor(167, 139, 250),
                ":".truecolor(148, 163, 184),
                type_scheme.to_string().truecolor(103, 184, 227),
            );
        }
    }

    if !has_types {
        eprintln!("  {}", "(no user-defined symbols)".truecolor(148, 163, 184),);
    }

    eprintln!();
}
