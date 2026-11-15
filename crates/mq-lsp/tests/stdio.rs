//! End-to-end tests driving the `mq-lsp` binary over stdio JSON-RPC.

mod common;

use common::LspClient;
use serde_json::json;

const URI: &str = "file:///test.mq";

#[test]
fn initialize_advertises_capabilities() {
    let client = LspClient::start();
    assert!(!client.init_result["capabilities"]["referencesProvider"].is_null());
    client.shutdown();
}

#[test]
fn diagnostics_reported_on_open() {
    let mut client = LspClient::start();
    client.open(URI, "let x = ");
    let params = client.diagnostics_for(URI, |p| !p["diagnostics"].as_array().unwrap().is_empty());
    assert!(!params["diagnostics"].as_array().unwrap().is_empty());
    client.shutdown();
}

#[test]
fn references_include_declaration_toggle() {
    let mut client = LspClient::start();
    client.open(URI, "let foo = 1 | foo | foo");
    let count = |client: &mut LspClient, include: bool| {
        client
            .at(
                "textDocument/references",
                URI,
                0,
                15,
                json!({ "context": { "includeDeclaration": include } }),
            )
            .as_array()
            .map_or(0, |a| a.len())
    };
    assert_eq!(count(&mut client, true), 3);
    assert_eq!(count(&mut client, false), 2);
    client.shutdown();
}

#[test]
fn goto_definition_after_emoji_uses_utf16_columns() {
    let mut client = LspClient::start();
    // The emoji is 2 UTF-16 units, so `foo` on line 1 starts at column 13 after `"😀" | `.
    client.open(URI, "def foo(): 1;\n\"😀\" | foo()");
    let result = client.at("textDocument/definition", URI, 1, 8, json!({}));
    let range = if result.is_array() {
        &result[0]["range"]
    } else {
        &result["range"]
    };
    assert_eq!(range["start"]["line"], 0, "result: {result}");
    client.shutdown();
}

#[test]
fn hover_on_qualified_module_function() {
    let mut client = LspClient::start();
    client.open(URI, "module m: def f(): 1; end\n| m::f()");
    let result = client.at("textDocument/hover", URI, 1, 5, json!({}));
    assert!(!result.is_null(), "expected hover on module::fn");
    client.shutdown();
}
