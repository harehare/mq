use miette::miette;
use rmcp::{
    Error as McpError, ServerHandler, ServiceExt,
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool,
};
use tokio::io::{stdin, stdout};

#[derive(Debug, Clone, Default)]
pub struct Server;

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct Query {
    #[schemars(description = "The markdown content to process")]
    content: String,
    #[schemars(description = "The mq query to execute")]
    query: String,
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct AstQuery {
    #[schemars(description = "The markdown content to process")]
    content: String,
    #[schemars(description = "The mq query AST to execute")]
    ast_json: String,
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

#[tool(tool_box)]
impl Server {
    #[tool(description = "Execute mq query on markdown content.")]
    fn execute(
        &self,
        #[tool(aggr)]
        #[schemars(description = "Execution query")]
        query: Query,
    ) -> Result<CallToolResult, McpError> {
        self.execute_query(&query.content, &query.query)
    }

    #[tool(description = "Execute mq query on AST JSON.")]
    fn execute_from_ast(
        &self,
        #[tool(aggr)]
        #[schemars(description = "Execution query")]
        query: AstQuery,
    ) -> Result<CallToolResult, McpError> {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module();

        let values = engine
            .eval_ast(
                serde_json::from_str(&query.ast_json).map_err(|e| {
                    McpError::parse_error(
                        "Failed to parse AST JSON",
                        Some(serde_json::Value::String(e.to_string())),
                    )
                })?,
                mq_lang::parse_markdown_input(&query.content)
                    .unwrap()
                    .into_iter(),
            )
            .map_err(|e| {
                McpError::invalid_request(
                    "Failed to execute code on AST",
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
        #[tool(aggr)]
        #[schemars(description = "User defined query")]
        user_query: Option<String>,
    ) -> Result<CallToolResult, McpError> {
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

    fn execute_query(&self, markdown: &str, query: &str) -> Result<CallToolResult, McpError> {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module();

        let markdown = mq_markdown::Markdown::from_markdown_str(markdown).map_err(|e| {
            McpError::parse_error(
                "Failed to parse markdown",
                Some(serde_json::Value::String(e.to_string())),
            )
        })?;
        let values = engine
            .eval(
                query,
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
}

#[tool(tool_box)]
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
    let server = Server;

    let service = server.serve(transport).await.map_err(|e| miette!(e))?;
    service.waiting().await.map_err(|e| miette!(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    #[tokio::test]
    async fn test_execute_code() {
        let server = Server;
        let query = Query {
            content: "# Test Heading\n\nThis is a test paragraph.\n\n- Item 1\n- Item 2"
                .to_string(),
            query: ".h1".to_string(),
        };

        let result = server.execute(query).unwrap();
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
            content: "# Test Heading\n\nThis is a test paragraph.\n\n- Item 1\n- Item 2"
                .to_string(),
            query: "a".to_string(),
        };

        let result = server.execute(query);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_ast() {
        let server = Server;

        // Generate AST JSON from markdown
        let content = "# Test Heading\n\nThis is a test paragraph.".to_string();
        let token_arena = Rc::new(RefCell::new(mq_lang::Arena::new(100)));

        let query = AstQuery {
            content,
            ast_json: mq_lang::parse(".h", token_arena)
                .map(|json| serde_json::to_string(&json).unwrap())
                .unwrap(),
        };

        let result = server.execute_from_ast(query);
        match result {
            Ok(result) => {
                if result.is_error.unwrap_or_default() {
                    println!("Error result: {:?}", result.content);
                    panic!("Query resulted in error");
                }
            }
            Err(e) => {
                println!("Failed to execute query: {:?}", e);
                panic!("Query execution failed");
            }
        }

        // Generate AST JSON from markdown
        let content = "# Test Heading\n\nThis is a test paragraph.".to_string();
        let token_arena = Rc::new(RefCell::new(mq_lang::Arena::new(100)));

        let query = AstQuery {
            content,
            ast_json: mq_lang::parse("a", token_arena)
                .map(|json| serde_json::to_string(&json).unwrap())
                .unwrap(),
        };

        assert!(server.execute_from_ast(query).is_err());
    }

    #[tokio::test]
    async fn test_get_available_functions() {
        let server = Server;
        let result = server.get_available_functions_and_selectors(None).unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);

        let server = Server;
        let result = server
            .get_available_functions_and_selectors(Some("def var(): 1;".to_string()))
            .unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);
    }
}
