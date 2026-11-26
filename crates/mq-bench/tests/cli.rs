//! End-to-end tests that run the `mq-bench` binary.

use std::{fs, process::Command};

fn run(name: &str, content: &str) -> std::process::Output {
    let dir = std::env::temp_dir().join(format!("mq_bench_cli_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("b.mq"), content).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mq-bench"))
        .args(["b.mq", "-n", "1", "--warmup", "0"])
        .current_dir(&dir)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&dir);
    output
}

#[test]
fn oversized_bench_name_fails_the_run() {
    let output = run("oversized", &format!("def bench_{}():\n  1\nend\n", "a".repeat(1025)));
    assert!(!output.status.success());
}

#[test]
fn valid_file_without_benches_still_succeeds() {
    let output = run("none", "def helper(): 1 end\n");
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
