//! Runs an SMT solver over the obligations and renders the verdicts (`mq-test --verify`).

use std::io::Write;
use std::process::{Command, Stdio};

use super::term::Sort;
use super::{Entry, Obligation};

/// Whether running the function on a counterexample really violates the contract.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Replay {
    /// mq itself reported the same `@ensures` violation.
    Reproduced,
    /// mq did not, so the translation may differ from mq's semantics. Carries the reason.
    NotReproduced(String),
}

/// An input the solver found that violates an `@ensures`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Counterexample {
    /// Parameter name and its value as an mq literal.
    pub(crate) inputs: Vec<(String, String)>,
    /// The function's result as an mq literal.
    pub(crate) result: Option<String>,
    /// The solver's raw answer, used when it cannot be turned into mq literals.
    pub(crate) raw: String,
    pub(crate) replay: Option<Replay>,
}

/// What the solver concluded about one `@ensures`.
#[derive(Debug, PartialEq)]
pub(crate) enum Verdict {
    /// No input satisfying the `@requires` violates it (`unsat`).
    Holds,
    /// An input violating it exists (`sat`).
    Counterexample(Counterexample),
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
        Ok(output) => parse_output(
            &String::from_utf8_lossy(&output.stdout),
            &obligation.params,
            &obligation.sorts,
        ),
        Err(e) => Verdict::Error(e.to_string()),
    }
}

/// Classifies the solver's answer. `params` names the values `get-value` reports, followed by
/// the function's result; `sorts` gives the sort of each of those values.
pub(crate) fn parse_output(stdout: &str, params: &[String], sorts: &[Sort]) -> Verdict {
    let mut lines = stdout.lines();
    match lines.next().map(str::trim) {
        Some("unsat") => Verdict::Holds,
        Some("sat") => {
            let raw = lines.collect::<Vec<_>>().join("\n");
            Verdict::Counterexample(read_values(&raw, params, sorts))
        }
        Some("unknown" | "timeout") => Verdict::Unknown,
        _ => Verdict::Error(stdout.trim().to_string()),
    }
}

/// Turns `((a 0) (b "x") ((f a b) 5))` into mq literals; keeps `raw` when it cannot.
fn read_values(raw: &str, params: &[String], sorts: &[Sort]) -> Counterexample {
    let mut counterexample = Counterexample {
        inputs: Vec::new(),
        result: None,
        raw: raw.to_string(),
        replay: None,
    };
    let sexps = parse_sexps(raw);
    let [Sexp::List(pairs)] = sexps.as_slice() else {
        return counterexample;
    };
    let values: Option<Vec<String>> = pairs
        .iter()
        .zip(sorts)
        .map(|(pair, sort)| match pair {
            Sexp::List(kv) => kv.get(1).and_then(|v| to_mq(v, sort)),
            _ => None,
        })
        .collect();
    if let Some(values) = values.filter(|v| v.len() == params.len() + 1) {
        let mut values = values.into_iter();
        counterexample.inputs = params.iter().cloned().zip(values.by_ref()).collect();
        counterexample.result = values.next();
    }
    counterexample
}

#[derive(Debug, PartialEq)]
enum Sexp {
    Atom(String),
    Str(String),
    List(Vec<Sexp>),
}

fn parse_sexps(input: &str) -> Vec<Sexp> {
    fn parse(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<Sexp> {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        match chars.peek()? {
            '(' => {
                chars.next();
                let mut items = Vec::new();
                loop {
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }
                    if chars.peek() == Some(&')') {
                        chars.next();
                        return Some(Sexp::List(items));
                    }
                    items.push(parse(chars)?);
                }
            }
            '"' => {
                chars.next();
                let mut text = String::new();
                while let Some(c) = chars.next() {
                    if c == '"' {
                        if chars.peek() == Some(&'"') {
                            chars.next();
                            text.push('"');
                        } else {
                            return Some(Sexp::Str(text));
                        }
                    } else {
                        text.push(c);
                    }
                }
                None
            }
            _ => {
                let mut atom = String::new();
                while let Some(c) = chars.peek() {
                    if c.is_whitespace() || *c == '(' || *c == ')' {
                        break;
                    }
                    atom.push(*c);
                    chars.next();
                }
                Some(Sexp::Atom(atom))
            }
        }
    }
    let mut chars = input.chars().peekable();
    let mut out = Vec::new();
    while let Some(sexp) = parse(&mut chars) {
        out.push(sexp);
    }
    out
}

/// A solver value of the given sort as an mq literal, or `None` for a form with no mq equivalent.
fn to_mq(value: &Sexp, sort: &Sort) -> Option<String> {
    let is_atom = |name: &str| matches!(value, Sexp::Atom(a) if a == name);
    match (sort, value) {
        (Sort::Int, Sexp::Atom(a)) if a.chars().all(|c| c.is_ascii_digit()) => Some(a.clone()),
        (Sort::Int, Sexp::List(items)) => match items.as_slice() {
            [Sexp::Atom(minus), Sexp::Atom(n)] if minus == "-" => Some(format!("-{n}")),
            _ => None,
        },
        (Sort::Bool, Sexp::Atom(a)) if a == "true" || a == "false" => Some(a.clone()),
        (Sort::Str, Sexp::Str(text)) if !text.contains("\\u{") => {
            Some(format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\"")))
        }
        (Sort::Seq(elem), _) => seq_items(value, elem).map(|items| format!("[{}]", items.join(", "))),
        (Sort::Opt(_), _) if is_atom("none") => Some("None".to_string()),
        (Sort::Opt(inner), Sexp::List(items)) => match items.as_slice() {
            [Sexp::Atom(a), Sexp::Atom(n), _] if a == "as" && n == "none" => Some("None".to_string()),
            [Sexp::Atom(some), x] if some == "some" => to_mq(x, inner),
            _ => None,
        },
        (Sort::Record(fields), Sexp::List(items)) if items.len() == fields.len() + 1 => {
            let entries: Option<Vec<String>> = fields
                .iter()
                .zip(&items[1..])
                .map(|((key, sort), v)| to_mq(v, sort).map(|v| format!("\"{key}\": {v}")))
                .collect();
            entries.map(|entries| format!("{{{}}}", entries.join(", ")))
        }
        _ => None,
    }
}

fn seq_items(value: &Sexp, elem: &Sort) -> Option<Vec<String>> {
    match value {
        Sexp::Atom(a) if a == "seq.empty" => Some(Vec::new()),
        Sexp::List(items) => match items.as_slice() {
            [Sexp::Atom(a), Sexp::Atom(s), _] if a == "as" && s == "seq.empty" => Some(Vec::new()),
            [Sexp::Atom(u), x] if u == "seq.unit" => Some(vec![to_mq(x, elem)?]),
            [Sexp::Atom(c), rest @ ..] if c == "seq.++" => {
                let mut all = Vec::new();
                for r in rest {
                    all.extend(seq_items(r, elem)?);
                }
                Some(all)
            }
            _ => None,
        },
        _ => None,
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
                        Verdict::Counterexample(ce) => ("❌", counterexample_text(ce)),
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
                        Verdict::Counterexample(ce) => {
                            ok = false;
                            if ce.inputs.is_empty() {
                                details.push_str(&format!(
                                    "\n### {} `@ensures({})`\n\n```\n{}\n```\n",
                                    o.function, o.expr, ce.raw
                                ));
                            }
                        }
                        Verdict::Error(message) => {
                            ok = false;
                            details.push_str(&format!(
                                "\n### {} `@ensures({})`\n\n```\n{}\n```\n",
                                o.function, o.expr, message
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

fn counterexample_text(ce: &Counterexample) -> String {
    let mut text = String::from("counterexample");
    if !ce.inputs.is_empty() {
        let inputs = ce
            .inputs
            .iter()
            .map(|(name, value)| format!("{name} = {value}"))
            .collect::<Vec<_>>()
            .join(", ");
        text.push_str(&format!(": `{inputs}`"));
        if let Some(result) = &ce.result {
            text.push_str(&format!(" gives `result = {result}`"));
        }
    }
    match &ce.replay {
        Some(Replay::Reproduced) => text.push_str(" (reproduced in mq)"),
        Some(Replay::NotReproduced(why)) => {
            text.push_str(&format!(
                " (**not reproduced in mq**, the translation may differ: {why})"
            ));
        }
        None => {}
    }
    text.replace('|', "\\|")
}
