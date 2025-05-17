use std::{fmt::Display, fs, path::PathBuf, str::FromStr};

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
    #[schemars(description = "Extract blockquotes from markdown content.")]
    Blockquote,
    #[schemars(description = "Extract image references from markdown content.")]
    Image,
    #[schemars(description = "Extract link references from markdown content.")]
    Link,
    #[schemars(description = "Extract emphasis (italic) text from markdown content.")]
    Emphasis,
    #[schemars(description = "Extract strong emphasis (bold) text from markdown content.")]
    Strong,
    #[schemars(description = "Extract delete (strikethrough) from markdown content.")]
    Delete,
    #[schemars(description = "Extract horizontal rules from markdown content.")]
    HorizontalRule,
    #[schemars(description = "Extract footnote references from markdown content.")]
    FootnoteReference,
    #[schemars(description = "Extract all text content from markdown, ignoring formatting.")]
    Text,
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
            Selector::Blockquote => write!(f, ".blockquote"),
            Selector::Image => write!(f, ".image"),
            Selector::Link => write!(f, ".link"),
            Selector::Emphasis => write!(f, ".emphasis"),
            Selector::Strong => write!(f, ".strong"),
            Selector::Delete => write!(f, ".delete"),
            Selector::HorizontalRule => write!(f, ".horizontal_rule"),
            Selector::FootnoteReference => write!(f, ".footnote_ref"),
            Selector::Text => write!(f, ".text"),
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
    #[schemars(description = "Converts the given markdown string to plain text.")]
    ToText(String),
    #[schemars(description = "Replaces all occurrences of a substring with another string.")]
    Replace(String, String),
}

impl Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Function::Contains(s) => write!(f, r#"select(contains("{}"))"#, s),
            Function::StartsWith(s) => write!(f, r#"select(starts_with("{}"))"#, s),
            Function::EndsWith(s) => write!(f, r#"select(ends_with("{}"))"#, s),
            Function::Test(s) => write!(f, r#"select(test("{}"))"#, s),
            Function::ToHtml(s) => write!(f, r#"to_html("{}")"#, s),
            Function::ToText(s) => write!(f, r#"to_text("{}")"#, s),
            Function::Replace(pattern, replacement) => {
                write!(f, r#"replace("{}", "{}")"#, pattern, replacement)
            }
        }
    }
}

#[derive(Debug, rmcp::serde::Deserialize, schemars::JsonSchema)]
struct Query {
    #[schemars(description = "The markdown file to extract from")]
    file_path: PathBuf,
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

        let functions = if self.functions.is_empty() {
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
        };

        if self.selectors.len() > 1 {
            write!(f, "select(or({})){}", selectors, functions)
        } else {
            write!(f, "select({}){}", selectors, functions)
        }
    }
}

#[tool(tool_box)]
impl Server {
    #[tool(description = "Extract from markdown content.")]
    fn extract_from_markdown(
        &self,
        #[tool(aggr)]
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

        let read_file: String = fs::read_to_string(&query.file_path)
            .map_err(|e| McpError::resource_not_found(e.to_string(), None))?;

        self.execute_query(&read_file, query.to_string().as_str())
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

        let markdown = mq_markdown::Markdown::from_str(markdown).map_err(|e| {
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
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .finish();

    if let Err(err) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("Failed to set global logger: {}", err);
    }

    unsafe { std::env::set_var("NO_COLOR", "1") };
    let transport = (stdin(), stdout());
    let server = Server;

    let service = server.serve(transport).await.map_err(|e| miette!(e))?;
    service.waiting().await.map_err(|e| miette!(e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_display() {
        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![],
            functions: vec![],
        };
        assert_eq!(query.to_string(), "select()");

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![Selector::Heading1],
            functions: vec![],
        };
        assert_eq!(query.to_string(), "select(.h1)");

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![Selector::Heading1, Selector::Heading2],
            functions: vec![],
        };
        assert_eq!(query.to_string(), "select(or(.h1, .h2))");

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![
                Selector::Heading1,
                Selector::Heading2,
                Selector::Heading3,
                Selector::Heading4,
                Selector::Heading5,
                Selector::List,
                Selector::CheckedList,
                Selector::Table,
                Selector::Code,
                Selector::InlineCode,
                Selector::Math,
                Selector::InlineMath,
                Selector::Html,
                Selector::Yaml,
                Selector::Toml,
                Selector::Blockquote,
                Selector::Image,
                Selector::Link,
                Selector::Emphasis,
                Selector::Strong,
                Selector::Delete,
                Selector::HorizontalRule,
                Selector::FootnoteReference,
                Selector::Text,
            ],
            functions: vec![],
        };
        assert_eq!(
            query.to_string(),
            "select(or(.h1, .h2, .h3, .h4, .h5, .[], .list.checked, .[][], .code, .code_inline, .math, .math_inline, .html, .yaml, .toml, .blockquote, .image, .link, .emphasis, .strong, .delete, .horizontal_rule, .footnote_ref, .text))"
        );

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![Selector::Heading1],
            functions: vec![Function::Contains("test".to_string())],
        };
        assert_eq!(
            query.to_string(),
            "select(.h1) | select(contains(\"test\"))"
        );

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![Selector::Heading1],
            functions: vec![
                Function::Contains("test".to_string()),
                Function::StartsWith("start".to_string()),
            ],
        };
        assert_eq!(
            query.to_string(),
            "select(.h1) | select(contains(\"test\")) | select(starts_with(\"start\"))"
        );

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![Selector::Heading1],
            functions: vec![
                Function::Contains("contains".to_string()),
                Function::StartsWith("starts".to_string()),
                Function::EndsWith("ends".to_string()),
                Function::Test("test.*".to_string()),
                Function::ToHtml("<b>html</b>".to_string()),
                Function::ToText("text".to_string()),
                Function::Replace("pattern".to_string(), "replacement".to_string()),
            ],
        };
        assert_eq!(
            query.to_string(),
            r#"select(.h1) | select(contains("contains")) | select(starts_with("starts")) | select(ends_with("ends")) | select(test("test.*")) | to_html("<b>html</b>") | to_text("text") | replace("pattern", "replacement")"#
        );

        let query = Query {
            file_path: PathBuf::from("test.md"),
            selectors: vec![Selector::Heading1, Selector::Code, Selector::List],
            functions: vec![
                Function::Contains("code".to_string()),
                Function::EndsWith("end".to_string()),
            ],
        };
        assert_eq!(
            query.to_string(),
            "select(or(.h1, .code, .[])) | select(contains(\"code\")) | select(ends_with(\"end\"))"
        );
    }

    #[tokio::test]
    async fn test_extract_from_markdown() {
        let (_, temp_file_path) = mq_test::create_file(
            "test1.md",
            "# Heading 1\n\n## Heading 2\n\n- List item 1\n- List item 2\n\n```rust\nfn main() {{}}\n```\n",
        );

        let server = Server;

        let query = Query {
            file_path: temp_file_path.to_path_buf(),
            selectors: vec![Selector::Heading1],
            functions: vec![],
        };
        let result = server.extract_from_markdown(query).unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0]
                .raw
                .as_text()
                .map(|t| t.text.clone())
                .unwrap_or_default(),
            "# Heading 1"
        );

        let query = Query {
            file_path: temp_file_path.to_path_buf(),
            selectors: vec![Selector::Heading1, Selector::Heading2],
            functions: vec![],
        };
        let result = server.extract_from_markdown(query).unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 2);
        assert_eq!(
            result.content[0]
                .raw
                .as_text()
                .map(|t| t.text.clone())
                .unwrap_or_default(),
            "# Heading 1"
        );
        assert_eq!(
            result.content[1]
                .raw
                .as_text()
                .map(|t| t.text.clone())
                .unwrap_or_default(),
            "## Heading 2"
        );

        let query = Query {
            file_path: temp_file_path.to_path_buf(),
            selectors: vec![Selector::List],
            functions: vec![Function::Contains("item 2".to_string())],
        };
        let result = server.extract_from_markdown(query).unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0]
                .raw
                .as_text()
                .map(|t| t.text.clone())
                .unwrap_or_default(),
            "- List item 2"
        );

        let query = Query {
            file_path: temp_file_path.to_path_buf(),
            selectors: vec![Selector::Code],
            functions: vec![],
        };
        let result = server.extract_from_markdown(query).unwrap();
        assert!(!result.is_error.unwrap_or_default());
        assert_eq!(result.content.len(), 1);
        assert_eq!(
            result.content[0]
                .raw
                .as_text()
                .map(|t| t.text.clone())
                .unwrap_or_default(),
            "```rust\nfn main() {{}}\n```"
        );

        let query = Query {
            file_path: temp_file_path.to_path_buf(),
            selectors: vec![],
            functions: vec![],
        };
        let result = server.extract_from_markdown(query);
        assert!(result.is_err());

        let query = Query {
            file_path: PathBuf::from("/non/existent/path"),
            selectors: vec![Selector::Heading1],
            functions: vec![],
        };
        let result = server.extract_from_markdown(query);
        assert!(result.is_err());
    }
}
