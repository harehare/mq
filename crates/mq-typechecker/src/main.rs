use std::io::Read;
use std::path::PathBuf;
use std::process;

use clap::Parser;
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
}

fn main() {
    let cli = Cli::parse();

    let (code, source_url) = match &cli.file {
        Some(path) => {
            let code = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Error reading file {}: {}", path.display(), e);
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
                eprintln!("Error reading stdin: {}", e);
                process::exit(1);
            }
            (code, None)
        }
    };

    let mut hir = Hir::default();
    hir.add_code(source_url, &code);
    hir.resolve();

    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);

    for error in &errors {
        if let Some((line, col)) = error.location() {
            eprintln!("{}:{}  {}", line, col, error);
        } else {
            eprintln!("{}", error);
        }
    }

    if cli.show_types {
        println!("=== Inferred Types ===");
        for (symbol_id, type_scheme) in checker.symbol_types() {
            if let Some(symbol) = hir.symbol(*symbol_id)
                && !hir.is_builtin_symbol(symbol)
                && let Some(name) = &symbol.value
            {
                println!("  {}: {}", name, type_scheme);
            }
        }
    }

    if errors.is_empty() {
        if !cli.show_types {
            println!("No type errors found.");
        }
        process::exit(0);
    } else {
        eprintln!(
            "\n{} type error{} found.",
            errors.len(),
            if errors.len() == 1 { "" } else { "s" }
        );
        process::exit(1);
    }
}
