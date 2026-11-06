//! SMT-LIB output for `# @requires` / `# @ensures` contracts (`mq-test --smt`).
//!
//! Each contracted top-level `def` whose body fits a small Int/Bool subset is translated to a
//! script that asks a solver (e.g. `z3`) for an input satisfying every `@requires` but
//! violating an `@ensures`. `unsat` means the postcondition holds for all inputs; `sat` prints a
//! counterexample model. mq numbers are modeled as mathematical integers, so `/` and `%` are
//! not translated and the result does not account for floating point. Anything outside the
//! subset is reported in a `; skipped` comment instead of being approximated.

mod replay;
mod signature;
mod term;
mod translate;
mod verify;

use std::collections::HashMap;
use std::fmt::Write;

use mq_lang::{Arena, AstExpr, CstNodeKind, Shared, SharedCell};

use self::signature::parse_signature;
use self::term::{Result as SmtResult, Signature, SmtError, Sort, Typed};
use self::translate::{Func, Node, Translator};
use super::TestRunner;
use super::contract::Contract;

pub(super) use self::replay::replay;
#[cfg(test)]
pub(super) use self::verify::parse_output;
pub(super) use self::verify::{Counterexample, Replay, Verdict, report_markdown, solve};

/// One `@ensures` to check: the solver is asked for an input satisfying `setup` (definitions,
/// declarations and every `@requires`) but violating the `@ensures`.
pub(super) struct Obligation {
    pub(super) function: String,
    pub(super) line: u32,
    pub(super) expr: String,
    /// Commands shared by every obligation of the function.
    setup: std::rc::Rc<str>,
    /// The negated `@ensures`, as an `assert` command.
    violated: String,
    /// The mq parameter names, in order.
    pub(super) params: Vec<String>,
    /// What `get-value` reports for a counterexample: the parameters, then the call itself.
    value_terms: String,
}

impl Obligation {
    /// A self-contained script that ends with `(check-sat)`.
    pub(super) fn script(&self, with_model: bool) -> String {
        let mut s = format!(
            "(set-option :produce-models true)\n{}{}\n(check-sat)\n",
            self.setup, self.violated
        );
        if with_model {
            s.push_str(&format!("(get-value ({}))\n", self.value_terms));
        }
        s
    }
}

/// The outcome of translating one function.
pub(super) enum Entry {
    Checks(Vec<Obligation>),
    Skipped { function: String, reason: String },
}

impl TestRunner {
    /// Renders the SMT-LIB script for every contracted `def` in `content` that has an `@ensures`.
    pub(super) fn emit_smt(content: &str) -> Result<String, String> {
        let entries = Self::smt_entries(content)?;
        let mut out = String::from("(set-option :produce-models true)\n");
        if entries.is_empty() {
            out.push_str("; no function with @ensures\n");
        }
        for entry in &entries {
            out.push('\n');
            match entry {
                Entry::Skipped { function, reason } => {
                    let _ = writeln!(out, "; skipped {function}: {reason}");
                }
                Entry::Checks(obligations) => {
                    let Some(first) = obligations.first() else {
                        continue;
                    };
                    let _ = write!(out, "; {}\n(push 1)\n{}", first.function, first.setup);
                    for o in obligations {
                        let _ = write!(
                            out,
                            "(push 1)\n(echo \"{}: @ensures({}) (line {}): unsat = holds, sat = counterexample\")\n{}\n(check-sat)\n(pop 1)\n",
                            o.function,
                            o.expr.replace('"', "\"\""),
                            o.line,
                            o.violated
                        );
                    }
                    out.push_str("(pop 1)\n");
                }
            }
        }
        Ok(out)
    }

    /// Translates every contracted `def` of `content` that has an `@ensures`.
    pub(super) fn smt_entries(content: &str) -> Result<Vec<Entry>, String> {
        let arena = Shared::new(SharedCell::new(Arena::new(16)));
        let program = mq_lang::parse(content, arena).map_err(|e| e.to_string())?;
        let mut funcs: HashMap<String, Func> = HashMap::new();
        for node in &program {
            let AstExpr::Def(name, params, body) = &node.expr else {
                continue;
            };
            if params.iter().any(|p| p.default.is_some() || p.is_variadic) {
                continue;
            }
            funcs.insert(
                name.name.to_string(),
                Func {
                    params: params.iter().map(|p| p.ident.name.to_string()).collect(),
                    body: body.clone(),
                },
            );
        }

        let (cst, _) = mq_lang::parse_recovery(content);
        let sigs = Self::signatures(&cst, &funcs);
        let mut entries = Vec::new();
        for node in &cst {
            let CstNodeKind::Def { name, .. } = &node.kind else {
                continue;
            };
            let (requires, ensures) = Self::collect_contracts(&node.leading_trivia);
            let Some(name) = name.name().map(|n| n.to_string()) else {
                continue;
            };
            if ensures.is_empty() {
                continue;
            }
            entries.push(match Self::smt_for(&name, &funcs, &sigs, &requires, &ensures) {
                Ok(obligations) => Entry::Checks(obligations),
                Err(reason) => Entry::Skipped { function: name, reason },
            });
        }
        Ok(entries)
    }

    /// The obligations for the `@ensures` of `name` under its `@requires`.
    fn smt_for(
        name: &str,
        funcs: &HashMap<String, Func>,
        sigs: &HashMap<String, SmtResult<Signature>>,
        requires: &[Contract],
        ensures: &[Contract],
    ) -> Result<Vec<Obligation>, String> {
        let target = funcs
            .get(name)
            .ok_or_else(|| "only top-level defs with plain parameters are supported".to_string())?;
        let requires_ast = Self::parse_contracts(requires)?;
        let ensures_ast = Self::parse_contracts(ensures)?;

        let mut tr = Translator::new(funcs, sigs);
        let sig = tr.define(name).map_err(|e| explain(&e))?;

        let mut env: translate::Env = target.params.iter().cloned().zip(sig.params.iter().cloned()).collect();
        let mut declarations = String::new();
        for (p, s) in target.params.iter().zip(&sig.params) {
            let _ = writeln!(declarations, "(declare-const {} {s})", term::symbol(p));
        }
        let mut assumptions = String::new();
        for (c, ast) in requires.iter().zip(&requires_ast) {
            let t = tr
                .expr(ast, &env)
                .and_then(|t| t.expect(Sort::Bool, &c.expr))
                .map_err(|e| explain(&e))?;
            let _ = writeln!(assumptions, "(assert {})", t.term);
        }

        // `result` is bound to the call itself, so the contract reads as written.
        let args: Vec<Typed> = target
            .params
            .iter()
            .zip(&sig.params)
            .map(|(p, s)| Typed::var(p, s.clone()))
            .collect();
        let call = Typed::call(name, &sig, args).map_err(|e| explain(&e))?;
        env.insert("result".to_string(), sig.ret.clone());
        let mut violations = Vec::new();
        for (c, ast) in ensures.iter().zip(&ensures_ast) {
            let holds = tr
                .expr(ast, &env)
                .and_then(|t| t.expect(Sort::Bool, &c.expr))
                .map_err(|e| explain(&e))?;
            let violated = Typed::not(holds).bind("result", call.clone());
            violations.push((c, format!("(assert {})", violated.term)));
        }

        let value_terms = target
            .params
            .iter()
            .map(|p| term::symbol(p))
            .chain(std::iter::once(call.term.to_string()))
            .collect::<Vec<_>>()
            .join(" ");
        let mut setup = String::new();
        for d in tr.defs() {
            let _ = writeln!(setup, "{d}");
        }
        setup.push_str(&declarations);
        setup.push_str(&assumptions);
        let setup: std::rc::Rc<str> = setup.into();
        Ok(violations
            .into_iter()
            .map(|(c, violated)| Obligation {
                function: name.to_string(),
                line: c.line,
                expr: c.expr.clone(),
                setup: setup.clone(),
                violated,
                params: target.params.clone(),
                value_terms: value_terms.clone(),
            })
            .collect())
    }

    /// The `@type` of every top-level `def`; an unannotated one takes and returns numbers.
    fn signatures(
        cst: &[Shared<mq_lang::CstNode>],
        funcs: &HashMap<String, Func>,
    ) -> HashMap<String, SmtResult<Signature>> {
        let mut sigs = HashMap::new();
        for node in cst {
            let CstNodeKind::Def { name, .. } = &node.kind else {
                continue;
            };
            let Some(name) = name.name().map(|n| n.to_string()) else {
                continue;
            };
            let Some(func) = funcs.get(&name) else {
                continue;
            };
            let annotation = Self::merge_annotation_lines(&node.leading_trivia)
                .into_iter()
                .find_map(|(_, comment)| match Self::parse_annotation(&comment) {
                    Some(super::TestAnnotation::Type(text)) => Some(text),
                    _ => None,
                });
            let sig = match annotation {
                None => Ok(Signature {
                    params: vec![Sort::Int; func.params.len()],
                    ret: Sort::Int,
                }),
                Some(text) => parse_signature(&text).and_then(|sig| {
                    if sig.params.len() == func.params.len() {
                        Ok(sig)
                    } else {
                        Err(SmtError::Invalid(format!(
                            "@type lists {} parameters but `{name}` has {}",
                            sig.params.len(),
                            func.params.len()
                        )))
                    }
                }),
            };
            sigs.insert(name, sig);
        }
        sigs
    }

    fn parse_contracts(contracts: &[Contract]) -> Result<Vec<Node>, String> {
        contracts
            .iter()
            .map(|c| {
                let arena = Shared::new(SharedCell::new(Arena::new(8)));
                let program = mq_lang::parse(&c.expr, arena).map_err(|e| e.to_string())?;
                match program.as_slice() {
                    [node] => Ok(node.clone()),
                    _ => Err(format!("contract `{}` is not a single expression", c.expr)),
                }
            })
            .collect()
    }
}

/// An error message, with a hint when a sort mismatch may come from a missing `@type`.
fn explain(error: &SmtError) -> String {
    match error {
        SmtError::Mismatch { .. } => {
            format!("{error} (parameters and results are numbers unless a `# @type(...)` comment says otherwise)")
        }
        _ => error.to_string(),
    }
}

/// Whether `text` is a valid `@type` signature.
#[cfg(test)]
pub(super) fn signature_is_valid(text: &str) -> bool {
    parse_signature(text).is_ok()
}
