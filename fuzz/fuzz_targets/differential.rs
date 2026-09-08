#![no_main]

//! Differential fuzzes the tree-walker and Tarn VM with the same deterministic programs.
//!
//! The engines are compiled into separate helper processes because `mq-lang/tarn` selects the
//! evaluator at compile time. `fuzz/scripts/run-differential.sh` builds those helpers and sets
//! the required environment variables before invoking this target.

use std::process::Command;

use libfuzzer_sys::fuzz_target;
use mq_fuzz::DifferentialContext;

const RESULT_PREFIX: &str = "MQ_DIFF_RESULT:";

fn eval_with_runner(runner_variable: &str, script: &str) -> String {
    let runner = std::env::var(runner_variable).unwrap_or_else(|_| {
        panic!("{runner_variable} is not set; run this target through fuzz/scripts/run-differential.sh")
    });
    let output = Command::new(&runner)
        .arg(script)
        .output()
        .unwrap_or_else(|error| panic!("could not start {runner}: {error}"));

    assert!(
        output.status.success(),
        "{runner} exited with {}; stderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    String::from_utf8_lossy(&output.stderr)
        .lines()
        .rev()
        .find_map(|line| line.strip_prefix(RESULT_PREFIX))
        .unwrap_or_else(|| panic!("{runner} did not emit a differential result marker"))
        .to_owned()
}

fuzz_target!(|context: DifferentialContext| {
    let script = context.to_script();
    let tree_result = eval_with_runner("MQ_DIFF_TREE_RUNNER", &script);
    let vm_result = eval_with_runner("MQ_DIFF_VM_RUNNER", &script);

    assert_eq!(
        tree_result, vm_result,
        "tree-walker and Tarn VM produced different results for:\n{script}"
    );
});
