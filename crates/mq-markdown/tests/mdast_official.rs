//! Writes the nodes mq makes of each input of the corpus as JSON, for `mdast_official/compare.mjs` to
//! compare with the mdast of the official packages, and checks that what mq writes back reads as the
//! same nodes. Run it with `just test-mdast-official`.
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

/// Normalizes nodes for the comparison of what is written with what was read: positions and the spelling
/// of references, fences and rules are left out, line endings are the same and adjacent text is joined.
fn normalize(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for key in ["position", "label", "fence"] {
                object.remove(key);
            }
            if object.get("type").is_some_and(|kind| kind == "HorizontalRule") {
                object.remove("marker");
            }
            for (key, value) in object.iter_mut() {
                normalize(value);
                if let (Value::String(text), true) = (value, key == "value") {
                    *text = text.replace("\r\n", "\n").replace('\r', "\n");
                }
            }
            if object.get("type").is_some_and(|kind| kind == "Html")
                && let Some(Value::String(text)) = object.get_mut("value")
            {
                *text = text.trim_end_matches('\n').to_string();
            }
        }
        Value::Array(array) => {
            array.iter_mut().for_each(normalize);
            let mut joined: Vec<Value> = Vec::with_capacity(array.len());
            for item in array.drain(..) {
                if let Some(last) = joined.last_mut()
                    && item["type"] == "Text"
                    && last["type"] == "Text"
                {
                    let text = format!("{}{}", last["value"].as_str().unwrap(), item["value"].as_str().unwrap());
                    last["value"] = json!(text);
                    continue;
                }
                joined.push(item);
            }
            *array = joined;
        }
        _ => {}
    }
}

fn parse(mode: &str, input: &str) -> Option<Value> {
    let markdown = match mode {
        "mdx" => Markdown::from_mdx_str(input),
        _ => Markdown::from_markdown_str(input),
    }
    .ok()?;
    let mut nodes: Value = serde_json::from_str(&markdown.to_json().ok()?).unwrap();
    normalize(&mut nodes);
    Some(nodes)
}

/// Inputs whose written form reads as other nodes, by id, which `round_trip_known.json` lists.
#[test]
#[ignore = "needs the corpus; run via `just test-mdast-official`"]
fn the_corpus_round_trips() {
    let corpus = std::fs::read_to_string(std::env::var("MDAST_CORPUS").unwrap()).unwrap();
    let corpus: Vec<Value> = serde_json::from_str(&corpus).unwrap();
    let known: Value = serde_json::from_str(include_str!("mdast_official/round_trip_known.json")).unwrap();
    let mut unexpected = Vec::new();
    let mut fixed = Vec::new();
    for entry in &corpus {
        let (id, mode, input) = (
            entry["id"].as_str().unwrap(),
            entry["mode"].as_str().unwrap(),
            entry["input"].as_str().unwrap(),
        );
        let Some(before) = parse(mode, input) else { continue };
        let written = match mode {
            "mdx" => Markdown::from_mdx_str(input),
            _ => Markdown::from_markdown_str(input),
        }
        .unwrap()
        .to_string();
        let same = parse(mode, &written).as_ref() == Some(&before);
        match (same, known.get(id).is_some()) {
            (false, false) => unexpected.push(format!(
                "{id} {input:?}\n  written: {written:?}\n  before:  {}\n  after:   {}",
                before,
                parse(mode, &written).map_or("error".to_string(), |after| after.to_string())
            )),
            (true, true) => fixed.push(id.to_string()),
            _ => {}
        }
    }
    let ids: Vec<&str> = corpus.iter().map(|entry| entry["id"].as_str().unwrap()).collect();
    for id in known.as_object().unwrap().keys() {
        assert!(
            ids.contains(&id.as_str()),
            "round_trip_known.json has an id that is not in the corpus: {id}"
        );
    }
    println!("{} inputs, {} known", corpus.len(), known.as_object().unwrap().len());
    assert!(
        unexpected.is_empty(),
        "do not read back as the same nodes:\n{}",
        unexpected.join("\n")
    );
    assert!(
        fixed.is_empty(),
        "now the same, remove from round_trip_known.json: {fixed:?}"
    );
}
