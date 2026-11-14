//! Progress and summary output of `mq module add/install/check/update`, written to stderr.
use colored::Colorize;
use std::io::{IsTerminal, Write};
use std::time::Duration;

/// How a fetched module compares with the file already on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Change {
    Added,
    Updated,
    Unchanged,
}

impl Change {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Added => "added",
            Self::Updated => "updated",
            Self::Unchanged => "unchanged",
        }
    }
}

/// `1.2 KB`, `830 B`.
pub(super) fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    }
}

/// The first 12 hex digits of a SHA-256, enough to tell versions apart.
pub(super) fn short_hash(hash: &str) -> &str {
    hash.get(..12).unwrap_or(hash)
}

pub(super) fn format_duration(d: Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}

/// `1 module`, `3 modules`.
pub(super) fn count(n: usize) -> String {
    format!("{n} module{}", if n == 1 { "" } else { "s" })
}

pub(super) fn header(message: &str) {
    eprintln!("{}", message.bold());
}

pub(super) fn ok_mark() -> String {
    "✓".green().bold().to_string()
}

pub(super) fn fail_mark() -> String {
    "✗".red().bold().to_string()
}

pub(super) fn skip_mark() -> String {
    "-".yellow().to_string()
}

pub(super) fn hint(message: &str) {
    eprintln!("{}", message.dimmed());
}

/// A line that shows `[i/n] name ...` while a module is being fetched and is replaced by its
/// result afterwards. Off a terminal only the result line is printed.
pub(super) struct Pending {
    tty: bool,
}

impl Pending {
    pub(super) fn start(index: usize, total: usize, name: &str) -> Self {
        let tty = std::io::stderr().is_terminal();
        if tty {
            eprint!(
                "  {} {} {}",
                "…".dimmed(),
                format!("[{index}/{total}]").dimmed(),
                format!("{name} fetching").dimmed()
            );
            let _ = std::io::stderr().flush();
        }
        Self { tty }
    }

    /// Replaces the pending text with `line`.
    pub(super) fn finish(self, line: &str) {
        if self.tty {
            eprint!("\r\x1b[2K");
        }
        eprintln!("{line}");
    }

    /// Clears the pending text without printing a result, before an error is reported.
    pub(super) fn abort(self) {
        if self.tty {
            eprint!("\r\x1b[2K");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(0, "0 B")]
    #[case(1023, "1023 B")]
    #[case(1536, "1.5 KB")]
    fn test_format_size(#[case] bytes: usize, #[case] expected: &str) {
        assert_eq!(format_size(bytes), expected);
    }

    #[rstest]
    #[case(1, "1 module")]
    #[case(0, "0 modules")]
    #[case(2, "2 modules")]
    fn test_count(#[case] n: usize, #[case] expected: &str) {
        assert_eq!(count(n), expected);
    }

    #[test]
    fn test_short_hash_handles_short_input() {
        assert_eq!(short_hash("0123456789abcdef"), "0123456789ab");
        assert_eq!(short_hash("abc"), "abc");
    }
}
