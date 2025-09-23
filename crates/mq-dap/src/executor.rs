use crossbeam_channel::Sender;
use std::{fs, path::PathBuf};
use tracing::{debug, error, info};

use crate::error::MqAdapterError;
use crate::protocol::DebuggerMessage;

type DynResult<T> = miette::Result<T, Box<dyn std::error::Error>>;

/// Execute query in a separate thread
pub fn execute_query(
    mut engine: mq_lang::Engine,
    query: String,
    input_file: Option<String>,
    message_tx: Sender<DebuggerMessage>,
) -> DynResult<()> {
    debug!(query = %query, input_file = ?input_file, "Executing query in background thread");

    let query = fs::read_to_string(&query).map_err(|e| {
        let error_msg = format!("Failed to read query file '{}': {}", query, e);
        error!(error = %error_msg);
        Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
    })?;

    // Prepare input data
    let input_data = if let Some(file_path) = input_file {
        let input = fs::read_to_string(&file_path).map_err(|e| {
            let error_msg = format!("Failed to read input file '{}': {}", file_path, e);
            error!(error = %error_msg);
            Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
        })?;

        parse_input_data(&file_path, &input)?
    } else {
        mq_lang::null_input()
    };

    let result = engine.eval(&query, input_data.into_iter());

    match result {
        Ok(values) => {
            let output = values
                .values()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            info!(output = %output, "Query execution completed successfully");
        }
        Err(e) => {
            let error_msg = format!("Query execution failed: {}", e);
            error!(error = %error_msg);
            // Send terminated message even on error
            let _ = message_tx.send(DebuggerMessage::Terminated);
            return Err(Box::new(MqAdapterError::QueryError(error_msg)));
        }
    }

    if let Err(e) = message_tx.send(DebuggerMessage::Terminated) {
        error!(error = %e, "Failed to send terminated message");
    }

    Ok(())
}

/// Parse input data based on file extension
fn parse_input_data(file_path: &str, input: &str) -> DynResult<Vec<mq_lang::RuntimeValue>> {
    match PathBuf::from(file_path)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
        .as_str()
    {
        "json" | "csv" | "tsv" | "xml" | "toml" | "yaml" | "yml" | "txt" => {
            Ok(mq_lang::raw_input(input))
        }
        "html" | "htm" => mq_lang::parse_html_input(input).map_err(|e| {
            let error_msg = format!("Failed to parse input file '{}': {}", file_path, e);
            error!(error = %error_msg);
            Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
        }),
        "mdx" => mq_lang::parse_mdx_input(input).map_err(|e| {
            let error_msg = format!("Failed to parse input file '{}': {}", file_path, e);
            error!(error = %error_msg);
            Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
        }),
        _ => mq_lang::parse_markdown_input(input).map_err(|e| {
            let error_msg = format!("Failed to parse input file '{}': {}", file_path, e);
            error!(error = %error_msg);
            Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
        }),
    }
}
#[cfg(test)]
mod tests {
    use crossbeam_channel::unbounded;

    use super::*;

    /// Helper to create a dummy engine that echoes input
    fn dummy_engine() -> mq_lang::Engine {
        mq_lang::Engine::default()
    }

    #[test]
    fn test_parse_input_data_json() {
        let input = r#"{"key": "value"}"#;
        let result = parse_input_data("test.json", input).unwrap();
        assert!(!result.is_empty(), "Should parse JSON input as raw");
    }

    #[test]
    fn test_parse_input_data_html() {
        let input = r#"<div>Hello</div>"#;
        let result = parse_input_data("test.html", input);
        assert!(result.is_ok(), "Should parse HTML input");
    }

    #[test]
    fn test_parse_input_data_mdx() {
        let input = r#"# Hello MDX"#;
        let result = parse_input_data("test.mdx", input);
        assert!(result.is_ok(), "Should parse MDX input");
    }

    #[test]
    fn test_parse_input_data_markdown_default() {
        let input = r#"# Hello Markdown"#;
        let result = parse_input_data("test.unknown", input);
        assert!(result.is_ok(), "Should parse unknown extension as Markdown");
    }

    #[test]
    fn test_execute_query_success() {
        let engine = dummy_engine();
        let query_file = "test_query.txt";
        let input_file = None;
        let (tx, rx) = unbounded();

        // Write a dummy query file
        std::fs::write(query_file, ".h").unwrap();

        let result = execute_query(engine, query_file.to_string(), input_file, tx);
        assert!(result.is_ok(), "Query execution should succeed");
        assert!(matches!(rx.recv().unwrap(), DebuggerMessage::Terminated));

        // Clean up
        let _ = std::fs::remove_file(query_file);
    }

    #[test]
    fn test_execute_query_query_file_not_found() {
        let engine = dummy_engine();
        let query_file = "non_existent_query.txt";
        let input_file = None;
        let (tx, _rx) = unbounded();

        let result = execute_query(engine, query_file.to_string(), input_file, tx);
        assert!(result.is_err(), "Should error if query file does not exist");
    }
}
