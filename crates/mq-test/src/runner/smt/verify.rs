//! Runs an SMT solver over the obligations and renders the verdicts (`mq-test --verify`).

use std::io::Write;
use std::process::{Command, Stdio};

use super::{Entry, Obligation};

/// What the solver concluded about one `@ensures`.
#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    /// No input satisfying the `@requires` violates it (`unsat`).
    Holds,
    /// An input violating it exists (`sat`), with the solver's model.
    Counterexample(String),
    /// The solver gave up (`unknown` or a timeout).
    Unknown,
    /// The solver could not be run or printed something unexpected.
    Error(String),
}

/// Runs `solver` (a command line such as `z3 -in -t:10000`) on `obligation`.
pub(crate) fn solve(solver: &str, obligation: &Obligation) -> Verdict {
    let mut parts = solver.split_whitespace();
    let Some(program) = parts.next() else {
        return Verdict::Error("empty solver command".to_string());
    };
    let child = Command::new(program)
        .args(parts)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(e) => return Verdict::Error(format!("cannot run `{program}`: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        // A solver that exits early closes the pipe; its output explains why.
        let _ = stdin.write_all(obligation.script(true).as_bytes());
    }
    match child.wait_with_output() {
        Ok(output) => parse_output(&String::from_utf8_lossy(&output.stdout)),
        Err(e) => Verdict::Error(e.to_string()),
    }
}

pub(crate) fn parse_output(stdout: &str) -> Verdict {
    let mut lines = stdout.lines();
    match lines.next().map(str::trim) {
        Some("unsat") => Verdict::Holds,
        Some("sat") => Verdict::Counterexample(lines.collect::<Vec<_>>().join("\n")),
        Some("unknown" | "timeout") => Verdict::Unknown,
        _ => Verdict::Error(stdout.trim().to_string()),
    }
}

/// A Markdown report of `results` (one verdict per obligation, in order) and the skipped
/// functions, plus whether every check passed or at least stayed inconclusive.
pub(crate) fn report_markdown(solver: &str, entries: &[Entry], results: &[Vec<Verdict>]) -> (String, bool) {
    let mut out =
        format!("## SMT verification (`{solver}`)\n\n| | Function | Contract | Result |\n| :-: | :-- | :-- | :-- |\n");
    let mut details = String::new();
    let mut skipped = String::new();
    let mut ok = true;
    for (entry, verdicts) in entries.iter().zip(results) {
        match entry {
            Entry::Skipped { function, reason } => {
                skipped.push_str(&format!("- `{function}`: {reason}\n"));
            }
            Entry::Checks(obligations) => {
                for (o, verdict) in obligations.iter().zip(verdicts) {
                    let (icon, text) = match verdict {
                        Verdict::Holds => ("✅", "holds for all inputs".to_string()),
                        Verdict::Counterexample(_) => ("❌", "counterexample found".to_string()),
                        Verdict::Unknown => ("⚠️", "unknown (solver gave up)".to_string()),
                        Verdict::Error(_) => ("❌", "solver error".to_string()),
                    };
                    out.push_str(&format!(
                        "| {icon} | `{}` | `@ensures({})` (line {}) | {text} |\n",
                        o.function,
                        o.expr.replace('|', "\\|"),
                        o.line
                    ));
                    match verdict {
                        Verdict::Counterexample(model) | Verdict::Error(model) => {
                            ok = false;
                            details.push_str(&format!(
                                "\n### {} `@ensures({})`\n\n```\n{}\n```\n",
                                o.function, o.expr, model
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    out.push_str(&details);
    if !skipped.is_empty() {
        out.push_str("\nNot translated to SMT-LIB:\n\n");
        out.push_str(&skipped);
    }
    (out, ok)
}
