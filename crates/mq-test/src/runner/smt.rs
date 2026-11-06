//! SMT-LIB output for `# @requires` / `# @ensures` contracts (`mq-test --smt`).
//!
//! Each contracted top-level `def` whose body fits a small Int/Bool subset is translated to a
//! script that asks a solver (e.g. `z3`) for an input satisfying every `@requires` but
//! violating an `@ensures`. `unsat` means the postcondition holds for all inputs; `sat` prints a
//! counterexample model. mq numbers are modeled as mathematical integers, so `/` and `%` are
//! not translated and the result does not account for floating point. Anything outside the
//! subset is reported in a `; skipped` comment instead of being approximated.

mod term;
mod translate;

use std::collections::HashMap;
use std::fmt::Write;

use mq_lang::{Arena, AstExpr, CstNodeKind, Shared, SharedCell};

use self::term::{Sort, Typed};
use self::translate::{Func, Node, Translator};
use super::TestRunner;
use super::contract::Contract;

impl TestRunner {
    /// Renders the SMT-LIB script for every contracted `def` in `content` that has an `@ensures`.
    pub(super) fn emit_smt(content: &str) -> Result<String, String> {
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
        let mut out = String::from("(set-option :produce-models true)\n");
        let mut any = false;
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
            any = true;
            out.push('\n');
            match Self::smt_for(&name, &funcs, &requires, &ensures) {
                Ok(script) => out.push_str(&script),
                Err(reason) => {
                    let _ = writeln!(out, "; skipped {name}: {reason}");
                }
            }
        }
        if !any {
            out.push_str("; no function with @ensures\n");
        }
        Ok(out)
    }

    /// The script checking every `@ensures` of `name` under its `@requires`.
    fn smt_for(
        name: &str,
        funcs: &HashMap<String, Func>,
        requires: &[Contract],
        ensures: &[Contract],
    ) -> Result<String, String> {
        let target = funcs
            .get(name)
            .ok_or_else(|| "only top-level defs with plain parameters are supported".to_string())?;
        let requires_ast = Self::parse_contracts(requires)?;
        let ensures_ast = Self::parse_contracts(ensures)?;
        let hints: Vec<Node> = requires_ast.iter().chain(&ensures_ast).cloned().collect();

        let mut tr = Translator::new(funcs);
        let sig = tr.define(name, &hints).map_err(|e| e.to_string())?;

        let mut env: translate::Env = target.params.iter().cloned().zip(sig.params.iter().copied()).collect();
        let mut script = String::new();
        for (p, s) in target.params.iter().zip(&sig.params) {
            let _ = writeln!(script, "(declare-const {} {s})", term::symbol(p));
        }
        for (c, ast) in requires.iter().zip(&requires_ast) {
            let t = tr
                .expr(ast, &env)
                .and_then(|t| t.expect(Sort::Bool, &c.expr))
                .map_err(|e| e.to_string())?;
            let _ = writeln!(script, "(assert {})", t.term);
        }

        // `result` is bound to the call itself, so the contract reads as written.
        let args: Vec<Typed> = target
            .params
            .iter()
            .zip(&sig.params)
            .map(|(p, s)| Typed::var(p, *s))
            .collect();
        let call = Typed::call(name, &sig, args).map_err(|e| e.to_string())?;
        env.insert("result".to_string(), sig.ret);
        for (c, ast) in ensures.iter().zip(&ensures_ast) {
            let holds = tr
                .expr(ast, &env)
                .and_then(|t| t.expect(Sort::Bool, &c.expr))
                .map_err(|e| e.to_string())?;
            let violated = Typed::not(holds).bind("result", call.clone());
            let _ = write!(
                script,
                "(push 1)\n(echo \"{name}: @ensures({}) (line {}): unsat = holds, sat = counterexample\")\n(assert {})\n(check-sat)\n(get-model)\n(pop 1)\n",
                c.expr.replace('"', "\"\""),
                c.line,
                violated.term
            );
        }

        let mut out = format!("; {name}\n(push 1)\n");
        for d in tr.defs() {
            let _ = writeln!(out, "{d}");
        }
        out.push_str(&script);
        out.push_str("(pop 1)\n");
        Ok(out)
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
