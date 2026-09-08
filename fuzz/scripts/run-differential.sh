#!/usr/bin/env bash
set -euo pipefail

workspace_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
build_dir="$workspace_dir/fuzz/target"
tree_target="${MQ_DIFF_TREE_TARGET_DIR:-$build_dir/differential-tree}"
vm_target="${MQ_DIFF_VM_TARGET_DIR:-$build_dir/differential-vm}"

cd "$workspace_dir"
CARGO_TARGET_DIR="$tree_target" cargo build --package mq-fuzz --bin differential-runner --release
CARGO_TARGET_DIR="$vm_target" cargo build --package mq-fuzz --bin differential-runner --release --features tarn

export MQ_DIFF_TREE_RUNNER="$tree_target/release/differential-runner"
export MQ_DIFF_VM_RUNNER="$vm_target/release/differential-runner"

run_count="${MQ_DIFF_RUNS:-1000}"
cargo +nightly fuzz run differential -- -runs="$run_count" "$@"
