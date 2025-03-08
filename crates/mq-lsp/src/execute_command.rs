use std::str::FromStr;

use itertools::Itertools;

pub fn response(
    input: String,
    params: tower_lsp::lsp_types::ExecuteCommandParams,
) -> Option<String> {
    if params.arguments.is_empty() {
        return None;
    }

    match params.command.as_str() {
        "mq/runSelectedText" => {
            if let Some(text) = params.arguments[0].as_str() {
                let mut engine = mq_lang::Engine::default();

                let input = mq_markdown::Markdown::from_str(&input)
                    .map(|markdown| {
                        markdown
                            .nodes
                            .into_iter()
                            .map(mq_lang::Value::from)
                            .collect_vec()
                    })
                    .unwrap_or_else(|_| vec![mq_lang::Value::String(input)]);

                engine.load_builtin_module().unwrap();
                let result = engine.eval(text, input.into_iter());

                match result {
                    Ok(values) => {
                        let markdown = mq_markdown::Markdown::new(
                            values
                                .into_iter()
                                .map(|value| match value {
                                    mq_lang::Value::Markdown(node) => node.clone(),
                                    _ => value.to_string().into(),
                                })
                                .collect(),
                        );

                        Some(markdown.to_string())
                    }
                    Err(e) => Some(e.cause.to_string()),
                }
            } else {
                None
            }
        }
        _ => None,
    }
}
