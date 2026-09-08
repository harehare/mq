//! Executes one mq script and emits a stable result marker for differential fuzzing.

use mq_lang::{DefaultEngine, null_input};

const RESULT_PREFIX: &str = "MQ_DIFF_RESULT:";

fn main() {
    let script = std::env::args().nth(1).unwrap_or_default();
    let mut engine = DefaultEngine::default();
    engine.load_builtin_module();
    let result = match engine.eval(&script, null_input().into_iter()) {
        Ok(values) => format!("ok:{values:?}"),
        // The two engines have different internal error representations. Whether evaluation
        // succeeds is the stable, user-visible contract this runner compares.
        Err(_) => "err".to_owned(),
    };

    // Builtins may write to stdout or stderr. A distinct final stderr marker lets the fuzzer
    // isolate this runner's result without treating program output as an engine difference.
    eprintln!("{RESULT_PREFIX}{result}");
}
