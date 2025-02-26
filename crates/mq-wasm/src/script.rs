use std::str::FromStr;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct Script {
    engine: mq_lang::Engine,
}

#[wasm_bindgen]
impl Script {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Script::default()
    }

    #[wasm_bindgen]
    pub fn run(&mut self, code: &str, content: &str) -> Result<String, JsValue> {
        mq_md::Markdown::from_str(content)
            .map_err(|e| JsValue::from_str(&e.to_string()))
            .and_then(move |markdown| {
                self.engine
                    .eval(code, markdown.nodes.into_iter().map(mq_lang::Value::from))
                    .map_err(|e| JsValue::from_str(&e.to_string()))
                    .map(|r| {
                        let markdown = mq_md::Markdown::new(
                            r.into_iter()
                                .map(|runtime_value| match runtime_value {
                                    mq_lang::Value::Markdown(node) => node.clone(),
                                    _ => runtime_value.to_string().into(),
                                })
                                .collect(),
                        );
                        markdown.to_string()
                    })
            })
    }

    #[wasm_bindgen]
    pub fn format(&mut self, code: &str) -> Result<String, JsValue> {
        mq_formatter::Formatter::default()
            .format(code)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_new() {
        let script = Script::new();
        assert!(matches!(script, Script { .. }));
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_simple() {
        let mut script = Script::new();
        let result = script
            .run(
                "downcase() | ltrinstr(\"hello\") | upcase() | trim()",
                "Hello world",
            )
            .unwrap();
        assert_eq!(result, "WORLD");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_invalid_syntax() {
        let mut script = Script::new();
        assert!(script.run("invalid syntax", "test").is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format() {
        let mut script = Script::new();
        let result = script
            .format("downcase()|ltrinstr(\"hello\")|upcase()|trim()")
            .unwrap();
        assert_eq!(
            result,
            "downcase() | ltrinstr(\"hello\") | upcase() | trim()"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format_invalid() {
        let mut script = Script::new();
        assert!(script.format("x=>").is_err());
    }
}
