use std::str::FromStr;

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
                            .collect::<Vec<_>>()
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
#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tower_lsp::lsp_types::ExecuteCommandParams;

    use super::*;

    #[test]
    fn test_run_selected_text_with_valid_text() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "mq/runSelectedText".to_string(),
            arguments: vec![Value::String("add(1, 2)".to_string())],
            work_done_progress_params: Default::default(),
        };

        let response = response(input.clone(), params);
        assert!(response.is_some());
    }

    #[test]
    fn test_run_selected_text_with_invalid_code() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "mq/runSelectedText".to_string(),
            arguments: vec![Value::String("add1, 2)".to_string())],
            work_done_progress_params: Default::default(),
        };

        let response = response(input, params);
        assert!(response.is_some());
        assert!(response.unwrap().contains("Unexpected token"));
    }

    #[test]
    fn test_run_selected_text_with_empty_arguments() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "mq/runSelectedText".to_string(),
            arguments: Vec::new(),
            work_done_progress_params: Default::default(),
        };

        let response = response(input, params);
        assert!(response.is_none());
    }

    #[test]
    fn test_unsupported_command() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "unsupported/command".to_string(),
            arguments: Vec::new(),
            work_done_progress_params: Default::default(),
        };

        let response = response(input, params);
        assert!(response.is_none());
    }

    #[test]
    fn test_run_selected_text_with_non_string_argument() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "mq/runSelectedText".to_string(),
            arguments: vec![Value::Number(42.into())],
            work_done_progress_params: Default::default(),
        };

        let response = response(input, params);
        assert!(response.is_none());
    }
}
