//! Writes the nodes mq makes of each input of the corpus as JSON, for `mdast_official/compare.mjs` to
//! compare with the mdast of the official packages. Run it with `just test-mdast-official`.
#![cfg(feature = "json")]

use mq_markdown::Markdown;
use serde_json::{Value, json};

#[test]
#[ignore = "needs the official packages and the specs; run via `just test-mdast-official`"]
fn write_nodes_of_the_corpus() {
    let corpus = std::fs::read_to_string(std::env::var("MDAST_CORPUS").unwrap()).unwrap();
    let corpus: Vec<Value> = serde_json::from_str(&corpus).unwrap();
    let results: Vec<Value> = corpus
        .iter()
        .map(|entry| {
            let input = entry["input"].as_str().unwrap();
            let parsed = match entry["mode"].as_str().unwrap() {
                "mdx" => Markdown::from_mdx_str(input),
                _ => Markdown::from_markdown_str(input),
            };
            match parsed.and_then(|markdown| markdown.to_json()) {
                Ok(nodes) => json!({"error": null, "nodes": serde_json::from_str::<Value>(&nodes).unwrap()}),
                Err(error) => json!({"error": error.to_string(), "nodes": []}),
            }
        })
        .collect();
    std::fs::write(std::env::var("MDAST_OUT").unwrap(), json!(results).to_string()).unwrap();
}
