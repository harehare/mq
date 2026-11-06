//! Re-runs a counterexample in the real mq interpreter, which checks the SMT translation
//! against mq's own semantics.

use super::{Counterexample, Obligation, Replay};
use crate::runner::{ContractStats, TestRunner};

/// Calls the function on the counterexample's inputs with its contracts enforced, and reports
/// whether mq raises the same `@ensures` violation.
pub(crate) fn replay(content: &str, obligation: &Obligation, counterexample: &Counterexample) -> Replay {
    if counterexample.inputs.len() != obligation.params.len() {
        return Replay::NotReproduced("the solver's model could not be read".to_string());
    }
    let source = match TestRunner::apply_contracts(content, false, None) {
        Ok(source) => source,
        Err(message) => return Replay::NotReproduced(message),
    };
    let args = counterexample
        .inputs
        .iter()
        .map(|(_, value)| value.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let query = format!("{source}\n| {}({args})", obligation.function);

    let mut engine = mq_lang::Engine::with_io(
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    ContractStats::default().register(&mut engine);
    let marker = format!(
        "@ensures violated (function's responsibility): {}: {} (line {})",
        obligation.function, obligation.expr, obligation.line
    );
    match engine.eval(&query, mq_lang::null_input().into_iter()) {
        Err(e) => {
            let message = format!("{e:?}");
            if message.contains(&marker) {
                match (&counterexample.result, mq_result(&message)) {
                    (Some(model), Some(actual)) if is_scalar(model) && *model != actual => {
                        Replay::NotReproduced(format!("the model says result = {model}, but mq returned {actual}"))
                    }
                    _ => Replay::Reproduced,
                }
            } else {
                let first = message.lines().next().unwrap_or_default();
                Replay::NotReproduced(format!("mq raised another error: {first}"))
            }
        }
        Ok(_) => Replay::NotReproduced("mq returned normally".to_string()),
    }
}

/// Numbers and booleans print the same in the model and in mq's error message.
fn is_scalar(value: &str) -> bool {
    value == "true" || value == "false" || value.trim_start_matches('-').chars().all(|c| c.is_ascii_digit())
}

/// The `result = ...` value of a contract violation message; `None` prints as nothing.
fn mq_result(message: &str) -> Option<String> {
    let rest = message.split("; result = ").nth(1)?;
    let value: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
        .collect();
    Some(if value.is_empty() { "None".to_string() } else { value })
}
