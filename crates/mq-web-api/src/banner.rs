//! Startup banner printed to stdout when the server boots in an interactive terminal.

use std::io::IsTerminal;

use colored::{ColoredString, Colorize};

use crate::config::Config;

fn is_truecolor_supported() -> bool {
    matches!(std::env::var("COLORTERM").as_deref(), Ok("truecolor") | Ok("24bit"))
}

fn logo(s: &str) -> ColoredString {
    if is_truecolor_supported() {
        s.truecolor(133, 212, 255)
    } else {
        s.bright_cyan()
    }
}

fn muted(s: &str) -> ColoredString {
    if is_truecolor_supported() {
        s.truecolor(148, 163, 184)
    } else {
        s.white()
    }
}

fn label(text: &str) -> String {
    format!("➜  {:<7}", text)
}

/// Prints the mq logo, version, and connection info; skipped when stdout isn't a tty
/// so it never pollutes piped/machine-readable output.
pub fn print_banner(config: &Config) {
    if !std::io::stdout().is_terminal() {
        return;
    }

    let version = env!("CARGO_PKG_VERSION");
    let server_url = config.server_url();
    let cache = if config.query_cache.enabled {
        format!(
            "enabled (ttl {}s, max {})",
            config.query_cache.ttl.as_secs(),
            config.query_cache.max_entries
        )
    } else {
        "disabled".to_string()
    };

    println!();
    println!("  {} {}", logo("mq").bold(), muted(&format!("web-api v{version}")));
    println!("  {}", muted("Query. Filter. Transform Markdown — over HTTP."));
    println!();
    println!("  {}{}", muted(&label("Local:")), server_url.bright_green());
    println!(
        "  {}{}",
        muted(&label("Docs:")),
        format!("{server_url}/openapi.json").bright_green()
    );
    println!("  {}{}", muted(&label("Cache:")), cache.bright_green());
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_aligns_values_regardless_of_key_length() {
        let local = label("Local:");
        let docs = label("Docs:");
        let cache = label("Cache:");

        assert_eq!(local.chars().count(), docs.chars().count());
        assert_eq!(local.chars().count(), cache.chars().count());
        assert!(local.starts_with("➜  Local:"));
        assert!(docs.starts_with("➜  Docs:"));
    }

    #[test]
    fn print_banner_does_not_panic_for_default_config() {
        // stdout isn't a tty under `cargo test`, so this hits the early return.
        print_banner(&Config::default());
    }
}
