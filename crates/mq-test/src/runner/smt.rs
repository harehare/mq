//! SMT-LIB output for `# @requires` / `# @ensures` contracts (`mq-test --smt`).
//!
//! Each contracted top-level `def` whose body fits a small Int/Bool subset is translated to a
//! script that asks a solver (e.g. `z3`) for an input satisfying every `@requires` but
//! violating an `@ensures`. `unsat` means the postcondition holds for all inputs; `sat` prints a
//! counterexample model. mq numbers are modeled as mathematical integers, so `/` and `%` are
//! not translated and the result does not account for floating point. Anything outside the
//! subset is reported in a `; skipped` comment instead of being approximated.

mod replay;
mod term;
mod translate;
mod types;
mod verify;

use std::collections::HashMap;
use std::fmt::Write;

use mq_lang::{Arena, AstExpr, CstNodeKind, Shared, SharedCell};

use self::term::{Result as SmtResult, Signature, Sort, Typed};
use self::translate::{Func, Node, Translator};
use self::types::Partial;
use super::TestRunner;
use super::contract::Contract;

pub(super) use self::replay::replay;
pub(super) use self::verify::{Counterexample, Replay, Verdict, report_markdown, solve};

/// Gives the sorts of each function; production uses `mq-check`, tests can supply their own.
type Signatures = dyn Fn(&str, &HashMap<String, Func>) -> HashMap<String, Partial>;

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
    /// The sorts of the parameters, then of the result, for reading a counterexample.
    sorts: Vec<Sort>,
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
        Self::emit_smt_with(content, &types::inferred)
    }

    fn emit_smt_with(content: &str, signatures: &Signatures) -> Result<String, String> {
        let entries = Self::smt_entries_with(content, signatures)?;
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
        Self::smt_entries_with(content, &types::inferred)
    }

    fn smt_entries_with(content: &str, signatures: &Signatures) -> Result<Vec<Entry>, String> {
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
        let sigs = signatures(content, &funcs);
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
        partials: &HashMap<String, Partial>,
        requires: &[Contract],
        ensures: &[Contract],
    ) -> Result<Vec<Obligation>, String> {
        let target = funcs
            .get(name)
            .ok_or_else(|| "only top-level defs with plain parameters are supported".to_string())?;
        let requires_ast = Self::parse_contracts(requires)?;
        let ensures_ast = Self::parse_contracts(ensures)?;

        // Sorts mq-check left open come from `@requires` predicates. An `@ensures` predicate on a
        // parameter is a claim to verify, so it may only declare `result`.
        let mut declared = types::declared(&requires_ast).map_err(|e| e.to_string())?;
        declared.extend(
            types::declared(&ensures_ast)
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|(var, _)| var == "result"),
        );
        let no_declarations = HashMap::new();
        let sigs: HashMap<String, SmtResult<Signature>> = partials
            .iter()
            .filter_map(|(n, partial)| {
                let func = funcs.get(n)?;
                let declared = if n == name { &declared } else { &no_declarations };
                Some((n.clone(), partial.complete(func, declared)))
            })
            .collect();
        let mut tr = Translator::new(funcs, &sigs);
        let sig = tr.define(name).map_err(|e| e.to_string())?;

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
                .map_err(|e| e.to_string())?;
            let _ = writeln!(assumptions, "(assert {})", t.term);
        }

        // `result` is bound to the call itself, so the contract reads as written.
        let args: Vec<Typed> = target
            .params
            .iter()
            .zip(&sig.params)
            .map(|(p, s)| Typed::var(p, s.clone()))
            .collect();
        let call = Typed::call(name, &sig, args).map_err(|e| e.to_string())?;
        env.insert("result".to_string(), sig.ret.clone());
        let mut violations = Vec::new();
        for (c, ast) in ensures.iter().zip(&ensures_ast) {
            let holds = tr
                .expr(ast, &env)
                .and_then(|t| t.expect(Sort::Bool, &c.expr))
                .map_err(|e| e.to_string())?;
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
        let mut every_sort: Vec<Sort> = sig.params.iter().cloned().chain([sig.ret.clone()]).collect();
        every_sort.extend(tr.sorts_seen().iter().cloned());
        every_sort.extend(
            tr.defs()
                .iter()
                .flat_map(|d| d.params.iter().map(|(_, s)| s.clone()).chain([d.ret.clone()])),
        );
        let mut setup = term::datatype_declarations(&every_sort);
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
                sorts: sig.params.iter().cloned().chain([sig.ret.clone()]).collect(),
                value_terms: value_terms.clone(),
            })
            .collect())
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

/// Test helper: translates `content` with the given sorts, e.g. `("f", "number,string->bool")`.
#[cfg(test)]
pub(super) fn emit_with_sorts(content: &str, spec: &[(&str, &str)]) -> Result<String, String> {
    TestRunner::emit_smt_with(content, &test_sorts(spec))
}

/// Test helper: like [`emit_with_sorts`], returning the obligations.
#[cfg(test)]
pub(super) fn entries_with_sorts(content: &str, spec: &[(&str, &str)]) -> Result<Vec<Entry>, String> {
    TestRunner::smt_entries_with(content, &test_sorts(spec))
}

/// Test helper: classifies solver output for values of the sorts named by `"number"`, `"string"`,
/// `"bool"`, `"[T]"`, `"T?"` and `"{k:T,..}"`.
#[cfg(test)]
pub(super) fn parse_output_with(stdout: &str, params: &[String], sorts: &[&str]) -> Verdict {
    let sorts: Vec<Sort> = sorts.iter().map(|n| test_sort(n)).collect();
    verify::parse_output(stdout, params, &sorts)
}

/// Splits at commas that are not inside brackets or braces.
#[cfg(test)]
fn split_top_level(text: &str) -> Vec<&str> {
    let (mut parts, mut depth, mut start) = (Vec::new(), 0, 0);
    for (i, c) in text.char_indices() {
        match c {
            '[' | '{' => depth += 1,
            ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                parts.push(&text[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

#[cfg(test)]
fn test_sort(text: &str) -> Sort {
    let text = text.trim();
    if let Some(inner) = text.strip_suffix('?') {
        return Sort::Opt(Box::new(test_sort(inner)));
    }
    if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        return Sort::Seq(Box::new(test_sort(inner)));
    }
    if let Some(inner) = text.strip_prefix('{').and_then(|t| t.strip_suffix('}')) {
        let fields = split_top_level(inner)
            .into_iter()
            .map(|field| {
                let (key, sort) = field.split_once(':').unwrap();
                (key.trim().to_string(), test_sort(sort))
            })
            .collect();
        return Sort::record(fields);
    }
    match text {
        "bool" => Sort::Bool,
        "string" => Sort::Str,
        _ => Sort::Int,
    }
}

#[cfg(test)]
fn test_sorts(spec: &[(&str, &str)]) -> impl Fn(&str, &HashMap<String, Func>) -> HashMap<String, Partial> + use<> {
    let sort = test_sort;
    let spec: Vec<(String, String)> = spec.iter().map(|(n, t)| (n.to_string(), t.to_string())).collect();
    move |_, _| {
        spec.iter()
            .map(|(name, text)| {
                let (params, ret) = text.split_once("->").unwrap();
                let params = split_top_level(params)
                    .into_iter()
                    .filter(|p| !p.trim().is_empty())
                    .map(sort)
                    .collect();
                (name.clone(), Partial::known(&Signature { params, ret: sort(ret) }))
            })
            .collect()
    }
}
