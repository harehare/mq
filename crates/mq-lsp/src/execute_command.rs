use std::borrow::Cow;
use tower_lsp_server::jsonrpc;
use tower_lsp_server::lsp_types::ExecuteCommandParams;

pub fn response(params: ExecuteCommandParams) -> jsonrpc::Result<Option<serde_json::Value>> {
    if params.arguments.is_empty() {
        return Err(jsonrpc::Error {
            code: jsonrpc::ErrorCode::InvalidParams,
            message: Cow::Borrowed("No arguments provided"),
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
            [Some(code), Some(input), Some(input_format)] => {
                execute(code, input, Some(input_format))
            }
            [Some(code), Some(input)] => execute(code, input, None),
            _ => Err(jsonrpc::Error {
                code: jsonrpc::ErrorCode::InvalidParams,
                message: Cow::Borrowed("Invalid arguments"),
                data: None,
            }),
        },
        "mq/to_ast_json" => match params
            .arguments
            .iter()
            .map(|v| v.as_str())
            .collect::<Vec<_>>()
            .as_slice()
        {
            [Some(code)] => {
                let token_arena =
                    mq_lang::Shared::new(mq_lang::SharedCell::new(mq_lang::Arena::new(1024)));
                let program = mq_lang::parse(code, token_arena).map_err(|e| jsonrpc::Error {
                    code: jsonrpc::ErrorCode::InvalidParams,
                    message: Cow::Owned(format!("Error: {}", e)),
                    data: None,
                })?;
                let ast_json = mq_lang::ast_to_json(&program).map_err(|e| jsonrpc::Error {
                    code: jsonrpc::ErrorCode::InvalidParams,
                    message: Cow::Owned(format!("Error: {}", e)),
                    data: None,
                })?;
                Ok(Some(serde_json::to_value(ast_json).unwrap()))
            }
            _ => Err(jsonrpc::Error {
                code: jsonrpc::ErrorCode::InvalidParams,
                message: Cow::Borrowed("Invalid arguments"),
                data: None,
            }),
        },
        _ => Err(jsonrpc::Error {
            code: jsonrpc::ErrorCode::InvalidParams,
            message: Cow::Borrowed("Invalid arguments"),
            data: None,
        }),
    }
}

fn execute(
    code: &str,
    input: &str,
    input_format: Option<&str>,
) -> jsonrpc::Result<Option<serde_json::Value>> {
    let mut engine = mq_lang::Engine::default();
    let input = match input_format.unwrap_or("markdown") {
        "markdown" => mq_lang::parse_markdown_input(input)
            .unwrap_or_else(|_| vec![mq_lang::RuntimeValue::String(input.to_string())]),
        "mdx" => mq_lang::parse_mdx_input(input)
            .unwrap_or_else(|_| vec![mq_lang::RuntimeValue::String(input.to_string())]),
        "html" => mq_lang::parse_html_input(input)
            .unwrap_or_else(|_| vec![mq_lang::RuntimeValue::String(input.to_string())]),
        "text" => mq_lang::parse_text_input(input)
            .unwrap_or_else(|_| vec![mq_lang::RuntimeValue::String(input.to_string())]),
        _ => {
            return Err(jsonrpc::Error {
                code: jsonrpc::ErrorCode::InvalidParams,
                message: Cow::Owned(format!(
                    "Unsupported input format: {}",
                    input_format.unwrap_or("unknown")
                )),
                data: None,
            });
        }
    };

    engine.load_builtin_module();
    let result = engine.eval(code, input.into_iter());

    match result {
        Ok(values) => {
            let markdown = mq_markdown::Markdown::new(
                values
                    .into_iter()
                    .map(|value| match value {
                        mq_lang::RuntimeValue::Markdown(node, _) => node.clone(),
                        _ => value.to_string().into(),
                    })
                    .collect(),
            );

            Ok(Some(markdown.to_string().into()))
        }
        Err(e) => Err(jsonrpc::Error {
            code: jsonrpc::ErrorCode::InternalError,
            message: Cow::Owned(format!("Error: {}", e)),
            data: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use tower_lsp_server::lsp_types::ExecuteCommandParams;

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
            assert_eq!(e.code, jsonrpc::ErrorCode::InvalidParams);
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
            assert_eq!(e.code, jsonrpc::ErrorCode::InvalidParams);
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
            assert_eq!(e.code, jsonrpc::ErrorCode::InvalidParams);
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
            assert_eq!(e.code, jsonrpc::ErrorCode::InternalError);
            assert!(e.message.contains("Error:"));
        }
    }

    #[test]
    fn test_to_ast_json_with_valid_code() {
        let code = "add(1, 2)";
        let params = ExecuteCommandParams {
            command: "mq/to_ast_json".to_string(),
            arguments: vec![Value::String(code.to_string())],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_ok());
        let value = response.unwrap();
        assert!(value.is_some());
        let json = value.unwrap();
        assert!(json.to_string().contains("expr"));
    }

    #[test]
    fn test_to_ast_json_with_invalid_code() {
        let code = "add(";
        let params = ExecuteCommandParams {
            command: "mq/to_ast_json".to_string(),
            arguments: vec![Value::String(code.to_string())],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, jsonrpc::ErrorCode::InvalidParams);
            assert!(e.message.contains("Error:"));
        }
    }

    #[test]
    fn test_to_ast_json_with_no_arguments() {
        let params = ExecuteCommandParams {
            command: "mq/to_ast_json".to_string(),
            arguments: vec![],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, jsonrpc::ErrorCode::InvalidParams);
            assert_eq!(e.message, "No arguments provided");
        }
    }

    #[test]
    fn test_to_ast_json_with_extra_arguments() {
        let params = ExecuteCommandParams {
            command: "mq/to_ast_json".to_string(),
            arguments: vec![
                Value::String("add(1, 2)".to_string()),
                Value::String("extra".to_string()),
            ],
            work_done_progress_params: Default::default(),
        };

        let response = response(params);
        assert!(response.is_err());
        if let Err(e) = response {
            assert_eq!(e.code, jsonrpc::ErrorCode::InvalidParams);
            assert_eq!(e.message, "Invalid arguments");
        }
    }
}
