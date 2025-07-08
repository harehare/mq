use miette::miette;
use rmcp::{
    Error as McpError, ServerHandler, ServiceExt,
    handler::server::tool::{Parameters, ToolRouter},
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use tokio::io::{stdin, stdout};

type McpResult = Result<CallToolResult, McpError>;

#[derive(Debug, Clone, Default)]
pub struct Server {
    pub tool_router: ToolRouter<Self>,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct Query {
    #[schemars(description = "The HTML to process")]
    html: String,
    #[schemars(description = "The mq query to execute")]
    query: Option<String>,
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
    #[schemars(description = "Usage examples showing how to use this function")]
    examples: Vec<String>,
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
        description = "Executes an mq query on the provided HTML content and returns the result as Markdown."
    )]
    fn html_to_markdown(&self, Parameters(Query { html, query }): Parameters<Query>) -> McpResult {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module();

        let markdown = mq_markdown::Markdown::from_html_str(&html).map_err(|e| {
            McpError::parse_error(
                "Failed to parse markdown",
                Some(serde_json::Value::String(e.to_string())),
            )
        })?;
        let values = engine
            .eval(
                &query.unwrap_or("identity()".to_string()),
                markdown.nodes.clone().into_iter().map(mq_lang::Value::from),
            )
            .map_err(|e| {
                McpError::invalid_request(
                    "Failed to query",
                    Some(serde_json::Value::String(e.to_string())),
                )
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

    #[tool(description = "Get available functions and selectors that can be used in mq query.")]
    fn get_available_functions_and_selectors(
        &self,
        Parameters(user_query): Parameters<Option<String>>,
    ) -> McpResult {
        let mut hir = mq_hir::Hir::default();

        if let Some(query) = user_query {
            hir.add_code(None, &query);
        }

        let mut functions = Vec::with_capacity(256);
        let mut selectors = Vec::with_capacity(256);

        // Get built-in functions
        for (name, builtin_doc) in hir.builtin.functions.iter() {
            functions.push(FunctionInfo {
                name: name.to_string(),
                description: builtin_doc.description.to_string(),
                params: builtin_doc.params.iter().map(|p| p.to_string()).collect(),
                is_builtin: true,
                examples: vec![
                    r#"select(or(.[], .code, .h)) | upcase() | add(" Hello World")"#.to_string(),
                    r#"select(not(.code))"#.to_string(),
                    r#".code("js")"#.to_string(),
                ],
            });
        }

        // Get internal functions
        for (name, builtin_doc) in hir.builtin.internal_functions.iter() {
            functions.push(FunctionInfo {
                name: name.to_string(),
                description: builtin_doc.description.to_string(),
                params: builtin_doc.params.iter().map(|p| p.to_string()).collect(),
                is_builtin: true,
                examples: vec![
                    r#"select(or(.[], .code, .h)) | upcase() | add(" Hello World")"#.to_string(),
                    r#"select(not(.code))"#.to_string(),
                    r#".code("js")"#.to_string(),
                ],
            });
        }

        // Get user-defined functions from symbols
        for (_id, symbol) in hir.symbols() {
            if let mq_hir::SymbolKind::Function(params) = &symbol.kind {
                if let Some(name) = &symbol.value {
                    let doc = symbol
                        .doc
                        .iter()
                        .map(|(_, doc)| doc.clone())
                        .collect::<Vec<_>>()
                        .join("\n");

                    functions.push(FunctionInfo {
                        name: name.to_string(),
                        description: if doc.is_empty() {
                            "User-defined function".to_string()
                        } else {
                            doc
                        },
                        params: params.iter().map(|p| p.to_string()).collect(),
                        is_builtin: false,
                        examples: vec![],
                    });
                }
            }
        }

        // Get selectors
        for (name, selector_doc) in hir.builtin.selectors.iter() {
            selectors.push(SelectorInfo {
                name: name.to_string(),
                description: selector_doc.description.to_string(),
                params: selector_doc.params.iter().map(|p| p.to_string()).collect(),
            });
        }

        let output = serde_json::json!({
            "functions": functions,
            "selectors": selectors,
        });
        let functions_json = serde_json::to_string(&output).map_err(|e| {
            McpError::invalid_request(
                "Failed to serialize functions and selectors",
                Some(serde_json::Value::String(e.to_string())),
            )
        })?;

        Ok(CallToolResult::success(vec![Content::text(functions_json)]))
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            instructions: Some(
                "mq is a tool for processing markdown content with a jq-like syntax.".into(),
            ),
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

    #[tokio::test]
    async fn test_execute_code() {
        let server = Server::new().expect("Failed to create server");
        let query = Query {
            html: "<h1>Test Heading</h1><p>This is a test paragraph.</p>".to_string(),
            query: Some(".h1".to_string()),
        };

        let result = server.html_to_markdown(Parameters(query)).unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0]
                .raw
                .as_text()
                .map(|t| t.text.clone())
                .unwrap_or_default(),
            "# Test Heading"
        );

        let query = Query {
            html: "<h1>Test Heading</h1><p>This is a test paragraph.</p>".to_string(),
            query: Some("a".to_string()),
        };

        let result = server.html_to_markdown(Parameters(query));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_available_functions() {
        let server = Server::new().expect("Failed to create server");
        let result = server
            .get_available_functions_and_selectors(Parameters(None))
            .unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);

        let server = Server::new().expect("Failed to create server");
        let result = server
            .get_available_functions_and_selectors(Parameters(Some("def var(): 1;".to_string())))
            .unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);
    }
}
