use std::str::FromStr;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
#[derive(Debug)]
pub struct Script {
    engine: mq_lang::Engine,
}

#[wasm_bindgen]
impl Script {
    #[wasm_bindgen]
    pub fn new() -> Self {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module().unwrap();

        Self { engine }
    }

    #[wasm_bindgen]
    pub fn run(&mut self, code: &str, content: &str) -> Result<String, JsValue> {
        mq_md::Markdown::from_str(content)
            .map_err(|e| JsValue::from_str(&e.to_string()))
            .and_then(move |markdown| {
                self.engine
                    .eval(code, markdown.nodes.into_iter().map(mq_lang::Value::from))
                    .map_err(|e| JsValue::from_str(&format!("{:?}", &e)))
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
            .map_err(|e| JsValue::from_str(&format!("{:?}", &e)))
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
        let result = script.run(
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()",
            "Hello world",
        );
        assert_eq!(result.unwrap(), "\nWORLD\n");
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
            .format("downcase()|ltrimstr(\"hello\")|upcase()|trim()")
            .unwrap();
        assert_eq!(
            result,
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format_invalid() {
        let mut script = Script::new();
        assert!(script.format("x=>").is_err());
    }
}
