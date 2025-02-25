use std::str::FromStr;

use wasm_bindgen::prelude::*;

pub struct Script {
    engine: mq_lang::Engine,
}

impl Script {
    pub fn new() -> Self {
        Self {
            engine: mq_lang::Engine::default(),
        }
    }

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
}
