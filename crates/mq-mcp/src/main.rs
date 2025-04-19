use miette::miette;
use rmcp::{
    Error as McpError, ServerHandler,
    model::{CallToolResult, Content, ProtocolVersion, ServerCapabilities, ServerInfo},
    serve_server, tool,
};
use tokio::io::{stdin, stdout};

#[derive(Debug, Clone, Default)]
pub struct Server;

#[tool(tool_box)]
impl Server {
    #[tool(description = "Extract headings from markdown content.")]
    fn extract_headings(
        &self,
        #[tool(param)]
        #[schemars(description = "The markdown content to extract headings from")]
        markdown: String,
        #[tool(param)]
        #[schemars(
            description = "Optional level filter (1-6) to extract only headings of specific levels"
        )]
        level: Option<u8>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            "Extracting headings from markdown, level filter: {:?}",
            level
        );

        self.execute_query(
            &markdown,
            &format!(
                ".h{}",
                level
                    .as_ref()
                    .map(|level| level.to_string())
                    .unwrap_or_default()
            ),
        )
    }

    #[tool(description = "Extract code blocks from markdown content.")]
    fn extract_code_blocks(
        &self,
        #[tool(param)]
        #[schemars(description = "The markdown content to extract code blocks from")]
        markdown: String,
        #[tool(param)]
        #[schemars(
            description = "Optional language filter to extract only code blocks of specific language"
        )]
        language: Option<String>,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!(
            "Extracting code blocks from markdown, language filter: {:?}",
            language
        );

        let query = if let Some(lang) = language {
            format!(".code[lang=\"{}\"]", lang)
        } else {
            ".code".to_string()
        };

        self.execute_query(&markdown, &query)
    }

    #[tool(description = "Extract lists from markdown content.")]
    fn extract_lists(
        &self,
        #[tool(param)]
        #[schemars(description = "The markdown content to extract lists from")]
        markdown: String,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!("Extracting list from markdown");
        self.execute_query(&markdown, ".[]")
    }

    #[tool(description = "Extract tables from markdown content.")]
    fn extract_tables(
        &self,
        #[tool(param)]
        #[schemars(description = "The markdown content to extract tables from")]
        markdown: String,
    ) -> Result<CallToolResult, McpError> {
        tracing::info!("Extracting tables from markdown");
        self.execute_query(&markdown, ".[][]")
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
    serve_server(server, transport)
        .await
        .map_err(|e| miette!(e))?;

    Ok(())
}
