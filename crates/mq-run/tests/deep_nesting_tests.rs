//! Deeply nested input must end in an error or a normal result, never a stack overflow abort.

use assert_cmd::cargo;
use rstest::rstest;
use std::fs;

const DEPTH: usize = 100_000;

fn nested_blocks(prefix: &str, suffix: &str) -> String {
    format!("{}x{}\n", prefix.repeat(DEPTH), suffix.repeat(DEPTH))
}

/// Indentation grows with depth, so the input size is quadratic; a smaller depth still exceeds the limit.
const YAML_MAPPING_DEPTH: usize = 2_000;

fn nested_yaml_mapping() -> String {
    let mut s = String::new();
    for i in 0..YAML_MAPPING_DEPTH {
        s.push_str(&" ".repeat(i));
        s.push_str("k:\n");
    }
    s.push_str(&" ".repeat(YAML_MAPPING_DEPTH));
    s.push_str("1\n");
    s
}

fn nested_cbor_base64() -> Vec<u8> {
    let mut bytes = vec![0x81; DEPTH];
    bytes.push(0x00);
    bytes
}

#[rstest]
#[case::markdown_quote("markdown", nested_blocks("> ", "").into_bytes())]
#[case::markdown_list("markdown", nested_blocks("- ", "").into_bytes())]
#[case::mdx("mdx", nested_blocks("> ", "").into_bytes())]
#[case::html("html", nested_blocks("<div>", "</div>").into_bytes())]
#[case::xml("xml", nested_blocks("<a>", "</a>").into_bytes())]
#[case::yaml_mapping("yaml", nested_yaml_mapping().into_bytes())]
#[case::yaml_sequence("yaml", nested_blocks("- ", "").into_bytes())]
#[case::yaml_flow("yaml", nested_blocks("[", "]").into_bytes())]
#[case::json("json", nested_blocks("[", "]").into_bytes())]
#[case::jsonl("jsonl", nested_blocks("[", "]").into_bytes())]
#[case::toml("toml", format!("a = {}1{}\n", "[".repeat(DEPTH), "]".repeat(DEPTH)).into_bytes())]
#[case::cbor("cbor", nested_cbor_base64())]
fn deep_nesting_does_not_abort(#[case] format: &str, #[case] input: Vec<u8>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep_input");
    fs::write(&path, input).unwrap();

    let output = cargo::cargo_bin_cmd!("mq")
        .args(["-I", format, ".", path.to_str().unwrap()])
        .output()
        .unwrap();

    // A signal-terminated process (stack overflow abort) has no exit code.
    assert!(
        output.status.code().is_some(),
        "{format}: terminated by signal ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
            .chars()
            .take(200)
            .collect::<String>()
    );
}
