use mq_markdown::{ConversionOptions, convert_html_to_markdown};
use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: html_convert <file.html>");
        std::process::exit(1);
    }

    let path = &args[1];
    let html = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Failed to read {}: {}", path, e);
        std::process::exit(1);
    });

    let options = ConversionOptions {
        extract_scripts_as_code_blocks: false,
        generate_front_matter: false,
        use_title_as_h1: false,
    };

    match convert_html_to_markdown(&html, options) {
        Ok(md) => print!("{}", md),
        Err(e) => {
            eprintln!("Conversion error: {}", e);
            std::process::exit(1);
        }
    }
}
