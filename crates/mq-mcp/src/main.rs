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
pub enum Selector {
    #[schemars(description = "Extract level 1 headings (h1) from markdown content.")]
    Heading1,
    #[schemars(description = "Extract level 2 headings (h2) from markdown content.")]
    Heading2,
    #[schemars(description = "Extract level 3 headings (h3) from markdown content.")]
    Heading3,
    #[schemars(description = "Extract level 4 headings (h4) from markdown content.")]
    Heading4,
    #[schemars(description = "Extract level 5 headings (h5) from markdown content.")]
    Heading5,
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

impl Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Selector::Heading1 => write!(f, ".h1"),
            Selector::Heading2 => write!(f, ".h2"),
            Selector::Heading3 => write!(f, ".h3"),
            Selector::Heading4 => write!(f, ".h4"),
            Selector::Heading5 => write!(f, ".h5"),
            Selector::List => write!(f, ".[]"),
            Selector::CheckedList => write!(f, ".list.checked"),
            Selector::Table => write!(f, ".[][]"),
            Selector::Code => write!(f, ".code"),
            Selector::InlineCode => write!(f, ".code_inline"),
            Selector::Math => write!(f, ".math"),
            Selector::InlineMath => write!(f, ".math_inline"),
            Selector::Html => write!(f, ".html"),
            Selector::Yaml => write!(f, ".yaml"),
            Selector::Toml => write!(f, ".toml"),
        }
    }
}

#[derive(Debug, rmcp::serde::Deserialize, rmcp::serde::Serialize, schemars::JsonSchema)]
pub enum Function {
    #[schemars(description = "Checks if string contains a substring.")]
    Contains(String),
    #[schemars(description = "Checks if the given string starts with the specified substring.")]
    StartsWith(String),
    #[schemars(description = "Checks if the given string ends with the specified substring.")]
    EndsWith(String),
    #[schemars(description = "Tests if string matches a pattern.")]
    Test(String),
    #[schemars(description = "Converts the given markdown string to HTML.")]
    ToHtml(String),
}

impl Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Function::Contains(s) => write!(f, r#"select(contains("{}"))"#, s),
            Function::StartsWith(s) => write!(f, r#"select(starts_with("{}"))"#, s),
            Function::EndsWith(s) => write!(f, r#"select(ends_with("{}"))"#, s),
            Function::Test(s) => write!(f, r#"select(test("{}"))"#, s),
            Function::ToHtml(s) => write!(f, r#"to_html("{}")"#, s),
        }
    }
}

#[derive(Debug, rmcp::serde::Serialize, rmcp::serde::Deserialize, schemars::JsonSchema)]
pub struct Query {
    #[schemars(
        description = "List of selectors to extract specific elements from markdown content"
    )]
    selectors: Vec<Selector>,
    #[schemars(
        description = "List of functions to filter or transform the extracted markdown elements"
    )]
    functions: Vec<Function>,
}

impl Display for Query {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let selectors = self
            .selectors
            .iter()
            .map(|query| query.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "select(or({})){}",
            selectors,
            if self.functions.is_empty() {
                "".to_string()
            } else {
                format!(
                    " | {}",
                    self.functions
                        .iter()
                        .map(|func| func.to_string())
                        .collect::<Vec<_>>()
                        .join(" | ")
                )
            }
        )
    }
}

#[tool(tool_box)]
impl Server {
    #[tool(description = "Extract from markdown content.")]
    fn extract_from_markdown(
        &self,
        #[tool(param)]
        #[schemars(description = "The markdown contents")]
        markdown_contents: Vec<String>,
        #[tool(param)]
        #[schemars(description = "Query to extract specific elements from markdown content")]
        query: Query,
    ) -> Result<CallToolResult, McpError> {
        if query.selectors.is_empty() {
            return Err(McpError::invalid_request(
                "No selector provided",
                Some(serde_json::Value::String(
                    "Queries cannot be empty".to_string(),
                )),
            ));
        }

        self.execute_query(&markdown_contents.join("\n"), query.to_string().as_str())
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
