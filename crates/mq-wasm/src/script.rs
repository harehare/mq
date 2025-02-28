use std::str::FromStr;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name=runScript)]
pub fn run_script(code: &str, content: &str) -> Result<String, JsValue> {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module().unwrap();
    mq_md::Markdown::from_str(content)
        .map_err(|e| JsValue::from_str(&e.to_string()))
        .and_then(move |markdown| {
            engine
                .eval(code, markdown.nodes.into_iter().map(mq_lang::Value::from))
                .map_err(|e| JsValue::from_str(&format!("{}", &e.cause)))
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

#[wasm_bindgen(js_name=formatScript)]
pub fn format_script(code: &str) -> Result<String, JsValue> {
    mq_formatter::Formatter::default()
        .format(code)
        .map_err(|e| JsValue::from_str(&format!("{}", &e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    wasm_bindgen_test_configure!(run_in_browser);

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_simple() {
        let result = run_script(
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()",
            "Hello world",
        );
        assert_eq!(result.unwrap(), "WORLD\n");
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_run_invalid_syntax() {
        assert!(run_script("invalid syntax", "test").is_err());
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format() {
        let result = format_script("downcase()|ltrimstr(\"hello\")|upcase()|trim()").unwrap();
        assert_eq!(
            result,
            "downcase() | ltrimstr(\"hello\") | upcase() | trim()"
        );
    }

    #[allow(unused)]
    #[wasm_bindgen_test]
    fn test_script_format_invalid() {
        assert!(format_script("x=>").is_err());
    }
}
