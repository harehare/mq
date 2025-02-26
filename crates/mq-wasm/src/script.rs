use std::str::FromStr;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = Script)]
pub struct ExportScript(Script);

#[wasm_bindgen(js_class = Script)]
impl ExportScript {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(Script::new())
    }

    #[wasm_bindgen(js_name = run)]
    pub fn run(&mut self, code: String, content: String) -> Result<String, JsValue> {
        self.0.run(&code, &content)
    }

    #[wasm_bindgen(js_name = runScript)]
    pub fn format(&mut self, code: String) -> Result<String, JsValue> {
        self.0.format(&code)
    }
}

#[derive(Debug)]
pub struct Script {
    engine: mq_lang::Engine,
}

impl Script {
    pub fn new() -> Self {
        let mut engine = mq_lang::Engine::default();
        // engine.load_builtin_module().unwrap();

        Self { engine }
    }

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
        let mut script = ExportScript::new();
        let result = script.run("upcase() | trim()".to_string(), "Hello world".to_string());
        assert_eq!(result.unwrap(), "\nHELLO WORLD\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_invalid_syntax() {
        let mut script = ExportScript::new();
        assert!(
            script
                .run("invalid syntax".to_string(), "test".to_string())
                .is_err()
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format() {
        let mut script = ExportScript::new();
        let result = script
            .format("downcase()|ltrimstr(\"hello\")|upcase()|trim()".to_string())
            .unwrap();
        assert_eq!(
            result,
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format_invalid() {
        let mut script = ExportScript::new();
        assert!(script.format("x=>".to_string()).is_err());
    }
}
