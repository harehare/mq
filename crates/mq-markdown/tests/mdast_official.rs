//! Writes what mq makes of each input of `mdx_official/corpus.json`, for `mdx_official/compare.mjs` to
//! compare with the official MDX packages. Run it with `just test-mdx-official`.
#![cfg(feature = "json")]

use mq_markdown::{Markdown, Node};
use serde_json::{Value, json};

fn attribute(attribute: &Value) -> String {
    let value = &attribute["value"];
    if attribute["type"] == "expression" {
        return format!("...{{{}}}", value.as_str().unwrap());
    }
    let name = value["name"].as_str().unwrap();
    match &value["value"] {
        Value::Null => name.to_string(),
        literal if literal["type"] == "literal" => {
            format!("{name}=lit:{}", json!(literal["value"].as_str().unwrap()))
        }
        expression => format!("{name}=expr:{{{}}}", expression["value"].as_str().unwrap()),
    }
}

fn attributes(attributes: &impl serde::Serialize) -> String {
    let attributes = serde_json::to_value(attributes).unwrap();
    attributes
        .as_array()
        .unwrap()
        .iter()
        .map(|a| format!(" {}", attribute(a)))
        .collect()
}

fn element(kind: &str, name: Option<&str>, attributes: String, children: &[Node], out: &mut Vec<String>) {
    out.push(format!("{kind} <{}{attributes}>(", name.unwrap_or_default()));
    children.iter().for_each(|child| describe(child, out));
    out.push(")".into());
}

/// The flat description `compare.mjs` makes of the official nodes: MDX nodes with their children, and
/// the text, code and HTML around them. Other nodes are looked through.
fn describe(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::MdxJsxFlowElement(e) => element("flow", e.name.as_deref(), attributes(&e.attributes), &e.children, out),
        Node::MdxJsxTextElement(e) => element("text", e.name.as_deref(), attributes(&e.attributes), &e.children, out),
        Node::MdxFlowExpression(e) => out.push(format!("flowexpr {{{}}}", e.value)),
        Node::MdxTextExpression(e) => out.push(format!("textexpr {{{}}}", e.value)),
        Node::MdxJsEsm(e) => out.push(format!("esm {}", json!(e.value))),
        Node::Text(t) => out.push(format!("T{}", json!(t.value))),
        Node::CodeInline(t) => out.push(format!("T{}", json!(t.value))),
        Node::Code(t) => out.push(format!("T{}", json!(t.value))),
        Node::Html(t) => out.push(format!("html {}", json!(t.value))),
        Node::Yaml(_) => out.push("yaml".into()),
        Node::HorizontalRule(_) => out.push("hr".into()),
        other => other.children().iter().for_each(|child| describe(child, out)),
    }
}

#[test]
#[ignore = "needs the official MDX packages; run via `just test-mdx-official`"]
fn write_nodes_of_the_corpus() {
    let corpus = include_str!("mdx_official/corpus.json");
    let corpus: Vec<String> = serde_json::from_str(corpus).unwrap();
    let results: Vec<Value> = corpus
        .iter()
        .map(|input| match Markdown::from_mdx_str(input) {
            Ok(markdown) => {
                let mut items = Vec::new();
                markdown.nodes.iter().for_each(|node| describe(node, &mut items));
                json!({"error": null, "items": items})
            }
            Err(error) => json!({"error": error.to_string(), "items": []}),
        })
        .collect();
    std::fs::write(std::env::var("MDX_OFFICIAL_OUT").unwrap(), json!(results).to_string()).unwrap();
}
