//! `# @requires(expr)` / `# @ensures(expr)` function contracts.
//!
//! A contracted top-level `def f(a, b)` is renamed in place to `__orig_f` (or a longer unused name), and a wrapper `f`
//! that checks the contracts around a call to `__orig_f` is appended after the file content.
//! Recursive and forward calls resolve `f` at call time, so they go through the wrapper too.
//! Appending (like `@property` does) keeps every original line number intact; only the
//! columns on the renamed `def` line shift.

use mq_lang::{CstNodeKind, CstTrivia};

use super::{TestAnnotation, TestRunner};

/// Prefix of the renamed original function. A suffix of `_` is appended until the name is
/// unused anywhere in the file.
const ORIG_PREFIX: &str = "__orig_";

/// Host function the checking wrapper calls to count contract evaluations.
pub(crate) const CONTRACT_CHECK_FN: &str = "__contract_check";

/// Start of the message of a `@requires` violation raised by a direct call.
const REQUIRES_PREFIX: &str = "@requires violated (caller's responsibility)";

/// One `@requires` / `@ensures` annotation attached to a `def`.
#[derive(Debug, PartialEq)]
pub(super) struct Contract {
    pub(super) expr: String,
    /// One-based line of the annotation, for the violation message.
    pub(super) line: u32,
}

/// A top-level `def` with its contracts.
#[derive(Debug, PartialEq)]
struct Contracted {
    name: String,
    /// Collision-free name the original `def` is renamed to.
    orig_name: String,
    params: Vec<String>,
    /// Parameters as written in the signature, with default values (`n = 1`).
    param_decls: Vec<String>,
    requires: Vec<Contract>,
    ensures: Vec<Contract>,
    /// One-based line and column of the name token.
    name_line: u32,
    name_column: usize,
    /// The name contract stats are keyed by: `module::name` for an imported module.
    stats_name: String,
}

impl TestRunner {
    /// Rewrites `content` so every top-level `def` with `@requires`/`@ensures` annotations
    /// checks them on each call. Returns `content` unchanged when there are none, and a
    /// user-facing message when a contracted `def` cannot be wrapped.
    ///
    /// With `label_nested`, a `@requires` violation escaping
    /// from a contracted function's body is re-labelled "in nested call", so `@small_scope`
    /// can tell it from a violation by the function the test called directly. A violation
    /// that passes through an uncontracted helper first is still seen as direct.
    ///
    /// Every contract is also registered in `stats`, so the report can list those the tests
    /// never evaluated. `module` names the imported module, or is empty for the test file, and
    /// keeps identical contracts of different modules apart.
    pub(crate) fn apply_contracts(
        content: &str,
        label_nested: bool,
        module: &str,
        stats: Option<&ContractStats>,
    ) -> Result<String, String> {
        let (nodes, _) = mq_lang::parse_recovery(content);
        let mut contracted = Vec::new();

        for node in &nodes {
            let CstNodeKind::Def { name, .. } = &node.kind else {
                continue;
            };
            let (requires, ensures) = Self::collect_contracts(&node.leading_trivia);
            if requires.is_empty() && ensures.is_empty() {
                continue;
            }

            let name_text = name.name().map(|n| n.to_string()).unwrap_or_default();
            let (sig, _) = node.split_cond_and_program();
            let mut params = Vec::new();
            let mut param_decls = Vec::new();
            for param in sig.iter().skip(1) {
                let CstNodeKind::Param {
                    asterisk: None,
                    default,
                    ..
                } = &param.kind
                else {
                    return Err(format!(
                        "contracts on '{name_text}': variadic parameters are not supported"
                    ));
                };
                let Some(p) = param.name() else {
                    return Err(format!("contracts on '{name_text}': unnamed parameter"));
                };
                let decl = match default {
                    Some(default) => {
                        let text = Self::source_text(content, default.node_range()).ok_or_else(|| {
                            format!("contracts on '{name_text}': could not read the default of '{p}'")
                        })?;
                        format!("{p} = {text}")
                    }
                    None => p.to_string(),
                };
                params.push(p.to_string());
                param_decls.push(decl);
            }
            if !ensures.is_empty() && params.iter().any(|p| p == "result") {
                return Err(format!(
                    "@ensures on '{name_text}': a parameter named 'result' clashes with the bound return value"
                ));
            }

            let range = name.range();
            let stats_name = if module.is_empty() {
                name_text.clone()
            } else {
                format!("{module}::{name_text}")
            };
            contracted.push(Contracted {
                name: name_text,
                orig_name: String::new(),
                stats_name,
                params,
                param_decls,
                requires,
                ensures,
                name_line: range.start.line,
                name_column: range.start.column,
            });
        }

        if contracted.is_empty() {
            return Ok(content.to_string());
        }
        if let Some(stats) = stats {
            for c in &contracted {
                for r in &c.requires {
                    stats.declare(&c.stats_name, "requires", r);
                }
                for e in &c.ensures {
                    stats.declare(&c.stats_name, "ensures", e);
                }
            }
        }

        let mut taken = std::collections::HashSet::new();
        for c in &mut contracted {
            let mut candidate = format!("{ORIG_PREFIX}{}", c.name);
            while content.contains(&candidate) || taken.contains(&candidate) {
                candidate.push('_');
            }
            taken.insert(candidate.clone());
            c.orig_name = candidate;
        }

        let mut out = Self::rename_originals(content, &contracted)?;
        for c in &contracted {
            out.push('\n');
            out.push_str(&Self::build_contract_wrapper(c, label_nested));
        }
        Ok(out)
    }

    /// Collects the `@requires` and `@ensures` annotations among `trivia`, in source order.
    pub(super) fn collect_contracts(trivia: &[CstTrivia]) -> (Vec<Contract>, Vec<Contract>) {
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        for (line, comment) in Self::merge_annotation_lines(trivia) {
            match Self::parse_annotation(&comment) {
                Some(TestAnnotation::Requires(expr)) => requires.push(Contract { expr, line }),
                Some(TestAnnotation::Ensures(expr)) => ensures.push(Contract { expr, line }),
                _ => {}
            }
        }
        (requires, ensures)
    }

    /// The source text of `range` (one-based line and column), or `None` if it is out of bounds.
    fn source_text(content: &str, range: mq_lang::Range) -> Option<String> {
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let offset = |pos: mq_lang::Position| -> Option<usize> {
            let start = *line_starts.get((pos.line as usize).checked_sub(1)?)?;
            let column = pos.column.checked_sub(1)?;
            match content[start..].char_indices().nth(column) {
                Some((i, _)) => Some(start + i),
                None => Some(content.len()),
            }
        };
        content
            .get(offset(range.start)?..offset(range.end)?)
            .map(str::to_string)
    }

    /// Renames each contracted `def` to its `orig_name`, verifying the name token really
    /// sits at the reported position.
    fn rename_originals(content: &str, contracted: &[Contracted]) -> Result<String, String> {
        let line_starts: Vec<usize> = std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        let mut offsets = Vec::new();
        for c in contracted {
            let offset = line_starts
                .get(c.name_line as usize - 1)
                .and_then(|&start| {
                    let line = &content[start..];
                    line.char_indices().nth(c.name_column - 1).map(|(i, _)| start + i)
                })
                .filter(|&at| content[at..].starts_with(&c.name))
                .ok_or_else(|| format!("could not locate 'def {}' to attach its contracts", c.name))?;
            offsets.push((offset, c));
        }

        let mut out = content.to_string();
        offsets.sort_unstable_by_key(|(offset, _)| *offset);
        for (offset, c) in offsets.into_iter().rev() {
            out.replace_range(offset..offset + c.name.len(), &c.orig_name);
        }
        Ok(out)
    }

    /// The expression of `contract` wrapped so the host function [`CONTRACT_CHECK_FN`] counts
    /// each evaluation and violation, returning the condition unchanged.
    fn contract_check(kind: &str, name: &str, contract: &Contract) -> String {
        format!(
            "{CONTRACT_CHECK_FN}(\"{kind}\", \"{name}\", {}, \"{}\", ({}))",
            contract.line,
            escape_mq_string(&contract.expr),
            contract.expr
        )
    }

    /// Builds the checking wrapper `def f(...)` that calls the renamed original.
    fn build_contract_wrapper(c: &Contracted, label_nested: bool) -> String {
        let name = &c.name;
        let params = c.params.join(", ");
        let arg_names = c.params.join(", ");
        let args_value = format!("to_string([{params}])");

        let mut steps = Vec::new();
        for r in &c.requires {
            let msg = format!(
                "\"@requires violated (caller's responsibility): {name}: {} (line {}); arguments ({arg_names}) = \" + {args_value}",
                escape_mq_string(&r.expr),
                r.line
            );
            let check = Self::contract_check("requires", &c.stats_name, r);
            steps.push(format!("(if (not({check})): error({msg}) else: None)"));
        }
        let orig = &c.orig_name;
        if label_nested {
            steps.push(format!(
                "let result = (try: {orig}({params}) catch(e): (error(if (starts_with(e[\"message\"], \"{REQUIRES_PREFIX}\")): \"@requires violated in nested call: \" + e[\"message\"] else: e[\"message\"])))"
            ));
        } else {
            steps.push(format!("let result = {orig}({params})"));
        }
        for e in &c.ensures {
            let msg = format!(
                "\"@ensures violated (function's responsibility): {name}: {} (line {}); arguments ({arg_names}) = \" + {args_value} + \"; result = \" + to_string(result)",
                escape_mq_string(&e.expr),
                e.line
            );
            let check = Self::contract_check("ensures", &c.stats_name, e);
            steps.push(format!("(if (not({check})): error({msg}) else: None)"));
        }
        steps.push("result".to_string());

        let decls = c.param_decls.join(", ");
        format!("def {name}({decls}) do\n    {}\nend\n", steps.join("\n    | "))
    }
}

/// Escapes `s` for use inside an mq double-quoted string literal.
fn escape_mq_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

/// How often each contract was evaluated and how often it did not hold, keyed by
/// (function, kind, line, expression). Fed by the [`CONTRACT_CHECK_FN`] host function.
#[derive(Clone, Default, Debug)]
pub(crate) struct ContractStats(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<ContractKey, (u64, u64)>>>);

type ContractKey = (String, String, u32, String);

impl ContractStats {
    /// Records `contract` with zero evaluations unless it already has some.
    fn declare(&self, name: &str, kind: &str, contract: &Contract) {
        self.0
            .lock()
            .unwrap()
            .entry((name.to_string(), kind.to_string(), contract.line, contract.expr.clone()))
            .or_default();
    }

    /// Registers [`CONTRACT_CHECK_FN`] on `engine`.
    pub(crate) fn register<IO: mq_lang::Io>(
        &self,
        engine: &mut mq_lang::Engine<crate::resolver::TestModuleResolver, IO>,
    ) {
        let stats = self.clone();
        engine.register_fn(
            CONTRACT_CHECK_FN,
            move |kind: String,
                  name: String,
                  line: i64,
                  expr: String,
                  holds: bool|
                  -> Result<bool, mq_lang::HostFunctionError> {
                let mut map = stats.0.lock().unwrap();
                let entry = map.entry((name, kind, line.max(0) as u32, expr)).or_default();
                entry.0 += 1;
                if !holds {
                    entry.1 += 1;
                }
                Ok(holds)
            },
        );
    }

    /// A Markdown table of every contract that was evaluated, or `None` if there were none.
    ///
    /// A violated `@ensures` is a failure; an unmet `@requires` only means the input was
    /// outside the function's domain.
    pub(crate) fn markdown(&self) -> Option<String> {
        let map = self.0.lock().unwrap();
        if map.is_empty() {
            return None;
        }
        let mut out = String::from(
            "## Contracts\n\n| | Function | Contract | Evaluated | Not held |\n| :-: | :-- | :-- | --: | --: |\n",
        );
        for ((name, kind, line, expr), (evaluated, violated)) in map.iter() {
            let icon = match (*violated, kind.as_str()) {
                (0, _) if *evaluated == 0 => "⚪",
                (0, _) => "✅",
                (_, "ensures") => "❌",
                _ => "↷",
            };
            out.push_str(&format!(
                "| {icon} | `{name}` | `@{kind}({})` (line {line}) | {evaluated} | {violated} |\n",
                expr.replace('|', "\\|")
            ));
        }
        out.push_str("\n`↷`: precondition not met, so the input was outside the function's domain.");
        if map.values().any(|(evaluated, _)| *evaluated == 0) {
            out.push_str(" `⚪`: never evaluated by these tests.");
        }
        out.push('\n');
        Some(out)
    }

    /// The same data for `--format json`.
    pub(crate) fn json(&self) -> serde_json::Value {
        let map = self.0.lock().unwrap();
        serde_json::Value::Array(
            map.iter()
                .map(|((name, kind, line, expr), (evaluated, violated))| {
                    serde_json::json!({
                        "function": name, "kind": kind, "line": line, "expression": expr,
                        "evaluated": evaluated, "violated": violated,
                    })
                })
                .collect(),
        )
    }
}
