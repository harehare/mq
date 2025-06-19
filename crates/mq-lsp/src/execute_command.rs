use std::borrow::Cow;

use tower_lsp::jsonrpc::Result;

pub fn response(
    params: tower_lsp::lsp_types::ExecuteCommandParams,
) -> Result<Option<serde_json::Value>> {
    if params.arguments.is_empty() {
        return Err(tower_lsp::jsonrpc::Error {
            code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
            message: Cow::Owned("No arguments provided".to_string()),
            data: None,
        });
    }

    match params.command.as_str() {
        "mq/run" => match params
            .arguments
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>()
            .as_slice()
        {
            [Some(command), Some(input)] => {
                let mut engine = mq_lang::Engine::default();
                let input = mq_markdown::Markdown::from_markdown_str(input)
                    .map(|markdown| {
                        markdown
                            .nodes
                            .into_iter()
                            .map(mq_lang::Value::from)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_else(|_| vec![mq_lang::Value::String(input.to_string())]);

                engine.load_builtin_module();
                let result = engine.eval(command, input.into_iter());

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

                        Ok(Some(markdown.to_string().into()))
                    }
                    Err(e) => Err(tower_lsp::jsonrpc::Error {
                        code: tower_lsp::jsonrpc::ErrorCode::InternalError,
                        message: Cow::Owned(format!("Error: {}", e)),
                        data: None,
                    }),
                }
            }
            _ => Err(tower_lsp::jsonrpc::Error {
                code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
                message: Cow::Owned("Invalid arguments".to_string()),
                data: None,
            }),
        },
        _ => Err(tower_lsp::jsonrpc::Error {
            code: tower_lsp::jsonrpc::ErrorCode::InvalidParams,
            message: Cow::Owned("Invalid arguments".to_string()),
            data: None,
        }),
    }
}
#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tower_lsp::lsp_types::ExecuteCommandParams;

    use super::*;

    #[test]
    fn test_run_with_valid_text() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "mq/run".to_string(),
            arguments: vec![Value::String("add(1, 2)".to_string()), input.into()],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_ok());
    }
    #[test]
    fn test_no_arguments() {
        let params = ExecuteCommandParams {
            command: "mq/run".to_string(),
            arguments: vec![],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
            assert_eq!(e.message, "No arguments provided");
        }
    }

    #[test]
    fn test_invalid_command() {
        let params = ExecuteCommandParams {
            command: "mq/invalid".to_string(),
            arguments: vec![
                Value::String("query".to_string()),
                Value::String("input".to_string()),
            ],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
            assert_eq!(e.message, "Invalid arguments");
        }
    }

    #[test]
    fn test_run_with_insufficient_arguments() {
        let params = ExecuteCommandParams {
            command: "mq/run".to_string(),
            arguments: vec![Value::String("query".to_string())],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, tower_lsp::jsonrpc::ErrorCode::InvalidParams);
            assert_eq!(e.message, "Invalid arguments");
        }
    }

    #[test]
    fn test_run_with_invalid_query() {
        let input = "# Test\nThis is a test".to_string();
        let params = ExecuteCommandParams {
            command: "mq/run".to_string(),
            arguments: vec![
                Value::String("invalid_function()".to_string()),
                input.into(),
            ],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, tower_lsp::jsonrpc::ErrorCode::InternalError);
            assert!(e.message.contains("Error:"));
        }
    }
}
