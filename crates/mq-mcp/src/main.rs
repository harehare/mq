use std::fmt::Display;

use miette::miette;
use rmcp::{
    Error as McpError, ServerHandler, ServiceExt,
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool,
};
use tokio::io::{stdin, stdout};

#[derive(Debug, Clone, Default)]
pub struct Server;

#[derive(Debug, rmcp::serde::Deserialize, rmcp::serde::Serialize, schemars::JsonSchema)]
pub enum Query {
    #[schemars(description = "Extract headings from markdown content.")]
    Heading(Option<u8>),
    #[schemars(description = "Extract list items from markdown content.")]
    List,
    #[schemars(description = "Extract checked list items from markdown content.")]
    CheckedList,
    #[schemars(description = "Extract table from markdown content.")]
    Table,
    #[schemars(description = "Extract code blocks from markdown content.")]
    Code,
    #[schemars(description = "Extract inline code from markdown content.")]
    InlineCode,
    #[schemars(description = "Extract math blocks from markdown content.")]
    Math,
    #[schemars(description = "Extract inline math from markdown content.")]
    InlineMath,
    #[schemars(description = "Extract HTML blocks from markdown content.")]
    Html,
    #[schemars(description = "Extract YAML frontmatter from markdown content.")]
    Yaml,
    #[schemars(description = "Extract TOML frontmatter from markdown content.")]
    Toml,
}

#[derive(Debug, rmcp::serde::Serialize, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct Queries {
    queries: Vec<Query>,
}

impl Display for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Query::Heading(level) => {
                if let Some(level) = level {
                    write!(f, ".h({})", level)
                } else {
                    write!(f, ".h")
                }
            }
            Query::List => write!(f, ".[]"),
            Query::CheckedList => write!(f, ".list.checked"),
            Query::Table => write!(f, ".[][]"),
            Query::Code => write!(f, ".code"),
            Query::InlineCode => write!(f, ".code_inline"),
            Query::Math => write!(f, ".math"),
            Query::InlineMath => write!(f, ".math_inline"),
            Query::Html => write!(f, ".html"),
            Query::Yaml => write!(f, ".yaml"),
            Query::Toml => write!(f, ".toml"),
        }
    }
}

impl Display for Queries {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let queries = self
            .queries
            .iter()
            .map(|query| query.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "or({})", queries)
    }
}

#[tool(tool_box)]
impl Server {
    #[tool(description = "Extract from markdown content.")]
    fn extract_from_markdown(
        &self,
        #[tool(param)]
        #[schemars(description = "The markdown content to extract headings from")]
        markdown: String,
        #[tool(param)]
        #[schemars(description = "Queries to extract specific elements from markdown content")]
        queries: Queries,
    ) -> Result<CallToolResult, McpError> {
        if queries.queries.is_empty() {
            return Err(McpError::invalid_request(
                "No queries provided",
                Some(serde_json::Value::String(
                    "Queries cannot be empty".to_string(),
                )),
            ));
        }

        self.execute_query(&markdown, queries.to_string().as_str())
            .map_err(|e| {
                McpError::invalid_request(
                    "Failed to execute query",
                    Some(serde_json::Value::String(e.to_string())),
                )
            })
    }

    fn execute_query(&self, markdown: &str, query: &str) -> Result<CallToolResult, McpError> {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module();

        let markdown = mq_markdown::Markdown::from_mdx_str(&markdown).map_err(|e| {
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
                .map(|value| Content::text(value.to_string()))
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

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    unsafe { std::env::set_var("NO_COLOR", "1") };
    let transport = (stdin(), stdout());
    let server = Server;

    let service = server.serve(transport).await.map_err(|e| miette!(e))?;
    service.waiting().await.map_err(|e| miette!(e))?;

    Ok(())
}
