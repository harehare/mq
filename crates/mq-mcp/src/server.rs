use miette::miette;
use rmcp::{
    ErrorData, ServerHandler, ServiceExt,
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use tokio::io::{stdin, stdout};
type McpResult = Result<CallToolResult, ErrorData>;

#[derive(Debug, Clone, Default)]
pub struct Server {
    pub tool_router: ToolRouter<Self>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct QueryForHtml {
    #[schemars(description = "The HTML to process")]
    html: String,
    #[schemars(
        description = "The mq query to execute. Selectors and functions listed in the available_selectors and available_functions tools can be used."
    )]
    query: Option<String>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct QueryForMarkdown {
    #[schemars(description = "The markdown to process")]
    markdown: String,
    #[schemars(
        description = "The mq query to execute. Selectors and functions listed in the available_selectors and available_functions tools can be used ."
    )]
    query: String,
}

#[derive(Debug, rmcp::serde::Serialize, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct FunctionInfo {
    #[schemars(description = "The function name")]
    name: String,
    #[schemars(description = "The function description")]
    description: String,
    #[schemars(description = "The function parameters")]
    params: Vec<String>,
    #[schemars(description = "Whether this is a built-in function")]
    is_builtin: bool,
}

#[derive(Debug, rmcp::serde::Serialize, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct SelectorInfo {
    #[schemars(description = "The function name")]
    name: String,
    #[schemars(description = "The function description")]
    description: String,
    #[schemars(description = "The function parameters")]
    params: Vec<String>,
}

#[tool_router]
impl Server {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self {
            tool_router: Self::tool_router(),
        })
    }

    #[tool(
        description = "Executes an mq query on the provided HTML content and returns the result as Markdown. Selectors and functions listed in the available_selectors and available_functions tools can be used."
    )]
    fn html_to_markdown(&self, Parameters(QueryForHtml { html, query }): Parameters<QueryForHtml>) -> McpResult {
        let mut engine = mq_lang::DefaultEngine::default();
        engine.load_builtin_module();

        let markdown = mq_markdown::Markdown::from_html_str(&html).map_err(|e| {
            ErrorData::parse_error("Failed to parse html", Some(serde_json::Value::String(e.to_string())))
        })?;
        let values = engine
            .eval(
                &query.unwrap_or("identity()".to_string()),
                markdown.nodes.clone().into_iter().map(mq_lang::RuntimeValue::from),
            )
            .map_err(|e| {
                ErrorData::invalid_request("Failed to query", Some(serde_json::Value::String(e.to_string())))
            })?;

        Ok(CallToolResult::success(
            values
                .into_iter()
                .filter_map(|value| {
                    if value.is_none() || value.is_empty() {
                        None
                    } else {
                        Some(Content::text(value.to_string()))
                    }
                })
                .collect::<Vec<_>>(),
        ))
    }

    #[tool(
        description = "Extract from markdown content. Selectors and functions listed in the available_selectors and available_functions tools can be used."
    )]
    fn extract_markdown(
        &self,
        Parameters(QueryForMarkdown { markdown, query }): Parameters<QueryForMarkdown>,
    ) -> Result<CallToolResult, ErrorData> {
        let mut engine = mq_lang::DefaultEngine::default();
        engine.load_builtin_module();

        let markdown = mq_markdown::Markdown::from_html_str(&markdown).map_err(|e| {
            ErrorData::parse_error(
                "Failed to parse markdown",
                Some(serde_json::Value::String(e.to_string())),
            )
        })?;
        let values = engine
            .eval(
                &query,
                markdown.nodes.clone().into_iter().map(mq_lang::RuntimeValue::from),
            )
            .map_err(|e| {
                ErrorData::invalid_request("Failed to query", Some(serde_json::Value::String(e.to_string())))
            })?;

        Ok(CallToolResult::success(
            values
                .into_iter()
                .filter_map(|value| {
                    if value.is_none() || value.is_empty() {
                        None
                    } else {
                        Some(Content::text(value.to_string()))
                    }
                })
                .collect::<Vec<_>>(),
        ))
    }

    #[tool(description = "Get available selectors that can be used in mq query.")]
    fn available_functions(&self) -> McpResult {
        let hir = mq_hir::Hir::default();
        let mut functions = Vec::with_capacity(256);

        // Get built-in functions
        for (name, builtin_doc) in hir.builtin.functions.iter() {
            functions.push(FunctionInfo {
                name: name.to_string(),
                description: builtin_doc.description.to_string(),
                params: builtin_doc.params.iter().map(|p| p.to_string()).collect(),
                is_builtin: true,
            });
        }

        // Get internal functions
        for (name, builtin_doc) in hir.builtin.internal_functions.iter() {
            functions.push(FunctionInfo {
                name: name.to_string(),
                description: builtin_doc.description.to_string(),
                params: builtin_doc.params.iter().map(|p| p.to_string()).collect(),
                is_builtin: true,
            });
        }

        let output = serde_json::json!({
            "functions": functions,
            "examples": vec![
                r#"select(or(.[], .code, .h)) | upcase() | add(" Hello World")"#.to_string(),
                r#"select(not(.code))"#.to_string(),
                r#".code("js")"#.to_string(),
            ],
        });
        let functions_json = serde_json::to_string(&output).expect("Failed to serialize functions");

        Ok(CallToolResult::success(vec![Content::text(functions_json)]))
    }

    #[tool(description = "Get available selectors that can be used in mq query.")]
    fn available_selectors(&self) -> McpResult {
        let hir = mq_hir::Hir::default();
        let mut selectors = Vec::with_capacity(256);

        // Get selectors
        for (name, selector_doc) in hir.builtin.selectors.iter() {
            selectors.push(SelectorInfo {
                name: name.to_string(),
                description: selector_doc.description.to_string(),
                params: selector_doc.params.iter().map(|p| p.to_string()).collect(),
            });
        }

        let output = serde_json::json!({
            "selectors": selectors,
        });
        let selectors_json = serde_json::to_string(&output).expect("Failed to serialize selectors");

        Ok(CallToolResult::success(vec![Content::text(selectors_json)]))
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            instructions: Some("mq is a tool for processing markdown content with a jq-like syntax.".into()),
            capabilities: ServerCapabilities::builder()
                .enable_logging()
                .enable_tools()
                .enable_tool_list_changed()
                .build(),
            ..Default::default()
        }
    }
}

pub async fn start() -> miette::Result<()> {
    let transport = (stdin(), stdout());
    let server = Server::new().expect("Failed to create server");

    let service = server.serve(transport).await.map_err(|e| miette!(e))?;
    service.waiting().await.map_err(|e| miette!(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(
        QueryForHtml {
            html: "<h1>Test Heading</h1><p>This is a test paragraph.</p>".to_string(),
            query: Some(".h1".to_string()),
        },
        Ok("# Test Heading")
    )]
    #[case(
        QueryForHtml {
            html: "<h1>Test Heading</h1><p>This is a test paragraph.</p>".to_string(),
            query: Some(".text".to_string()),
        },
        Ok("Test Heading\n\nThis is a test paragraph.")
    )]
    #[case(
        QueryForHtml {
            html: "<h1>Test Heading</h1><p>This is a test paragraph.</p>".to_string(),
            query: None,
        },
        Ok("# Test Heading\n\nThis is a test paragraph.")
    )]
    #[case(
        QueryForHtml {
            html: "<h1>Test Heading".to_string(), // malformed HTML
            query: Some(".h1".to_string()),
        },
        Ok("# Test Heading")
    )]
    #[case(
        QueryForHtml {
            html: "<h1>Test Heading</h1>".to_string(),
            query: Some("not_a_function(".to_string()), // invalid query
        },
        Err("Failed to query")
    )]
    fn test_html_to_markdown(#[case] query: QueryForHtml, #[case] expected: Result<&'static str, &'static str>) {
        let server = Server::new().expect("Failed to create server");
        let result = server.html_to_markdown(Parameters(query));
        match expected {
            Ok(expected_text) => {
                let result = result.expect("Expected Ok result");
                assert!(!result.is_error.unwrap_or_default());
                let actual = result
                    .content
                    .into_iter()
                    .map(|c| c.as_text().map(|t| t.text.clone()).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("\n\n");

                assert_eq!(actual, expected_text);
            }
            Err(expected_err) => {
                let err = result.expect_err("Expected error result");
                let msg = format!("{err}");
                assert!(
                    msg.contains(expected_err),
                    "Error message '{msg}' does not contain expected '{expected_err}'"
                );
            }
        }
    }

    #[rstest]
    #[case(
        QueryForMarkdown {
            markdown: "# Test Heading".to_string(),
            query: ".h1".to_string(),
        },
        Ok("# Test Heading")
    )]
    #[case(
        QueryForMarkdown {
            markdown: "# Test Heading\n\nThis is a test paragraph.".to_string(),
            query: ".text".to_string(),
        },
        Ok("Test Heading\n\nThis is a test paragraph.")
    )]
    #[case(
        QueryForMarkdown {
            markdown: "# Test Heading\n\nThis is a test paragraph.".to_string(),
            query: "identity()".to_string(),
        },
        Ok("# Test Heading\n\nThis is a test paragraph.")
    )]
    #[case(
        QueryForMarkdown {
            markdown: "# Test Heading".to_string(),
            query: "not_a_function(".to_string(), // invalid query
        },
        Err("Failed to query")
    )]
    #[case(
        QueryForMarkdown {
            markdown: "".to_string(),
            query: ".h1".to_string(),
        },
        Ok("")
    )]
    fn test_extract_markdown(#[case] query: QueryForMarkdown, #[case] expected: Result<&'static str, &'static str>) {
        let server = Server::new().expect("Failed to create server");
        let result = server.extract_markdown(Parameters(query));
        match expected {
            Ok(expected_text) => {
                let result = result.expect("Expected Ok result");
                assert!(!result.is_error.unwrap_or_default());
                let actual = result
                    .content
                    .into_iter()
                    .map(|c| c.as_text().map(|c| c.text.clone()).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("\n\n");
                assert_eq!(actual, expected_text);
            }
            Err(expected_err) => {
                let err = result.expect_err("Expected error result");
                let msg = format!("{err}");
                assert!(
                    msg.contains(expected_err),
                    "Error message '{msg}' does not contain expected '{expected_err}'"
                );
            }
        }
    }

    #[test]
    fn test_available_functions() {
        let server = Server::new().expect("Failed to create server");
        let result = server.available_functions().unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.into_iter().len(), 1);
    }

    #[test]
    fn test_available_selectors() {
        let server = Server::new().expect("Failed to create server");
        let result = server.available_selectors().unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.into_iter().len(), 1);
    }

    #[test]
    fn test_get_info() {
        let server = Server::new().expect("Failed to create server");
        let info = server.get_info();
        assert_eq!(info.protocol_version, ProtocolVersion::V_2024_11_05);
        assert!(info.instructions.is_some());
        let instructions = info.instructions.unwrap();
        assert!(
            instructions.contains("mq is a tool for processing markdown content"),
            "Instructions should mention mq"
        );
    }
}
