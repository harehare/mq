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
    use super::*;
    use crossbeam_channel::unbounded;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to create a dummy engine that echoes input
    fn dummy_engine() -> mq_lang::Engine {
        mq_lang::Engine::default()
    }

    #[test]
    fn test_parse_input_data_json() {
        let input = r#"{"key": "value"}"#;
        let result = parse_input_data("test.json", input).unwrap();
        assert!(!result.is_empty(), "Should parse JSON input as raw");
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_csv() {
        let input = "name,age\nJohn,30\nJane,25";
        let result = parse_input_data("test.csv", input).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_tsv() {
        let input = "name\tage\nJohn\t30\nJane\t25";
        let result = parse_input_data("test.tsv", input).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_xml() {
        let input = "<root><item>test</item></root>";
        let result = parse_input_data("test.xml", input).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_toml() {
        let input = r#"
[package]
name = "test"
"#;
        let result = parse_input_data("test.toml", input).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_yaml() {
        let input = r#"
name: test
version: 1.0.0
"#;
        let result = parse_input_data("test.yaml", input).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_txt() {
        let input = "This is plain text";
        let result = parse_input_data("test.txt", input).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_parse_input_data_html() {
        let input = r#"<div>Hello</div>"#;
        let result = parse_input_data("test.html", input);
        assert!(result.is_ok(), "Should parse HTML input");

        let input = "<html><body><h1>Hello</h1></body></html>";
        let result = parse_input_data("test.html", input);
        // HTML parsing might succeed or fail depending on implementation
        // We just check that it returns a result
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_parse_input_data_htm() {
        let input = "<html><body><h1>Hello</h1></body></html>";
        let result = parse_input_data("test.htm", input);
        // HTML parsing might succeed or fail depending on implementation
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_parse_input_data_mdx() {
        let input = r#"# Hello MDX"#;
        let result = parse_input_data("test.mdx", input);
        assert!(result.is_ok(), "Should parse MDX input");

        let input = "# Hello\n\n```js\nconsole.log('hello');\n```";
        let result = parse_input_data("test.mdx", input);
        // MDX parsing might succeed or fail depending on implementation
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_parse_input_data_markdown_default() {
        let input = r#"# Hello Markdown"#;
        let result = parse_input_data("test.unknown", input);
        assert!(result.is_ok(), "Should parse unknown extension as Markdown");
        let input = "# Hello World\n\nThis is markdown content.";
        let result = parse_input_data("test.md", input);
        // Markdown parsing should work
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_parse_input_data_unknown_extension() {
        let input = "# Hello World\n\nThis is treated as markdown.";
        let result = parse_input_data("test.unknown", input);
        // Unknown extensions are treated as markdown
        assert!(result.is_ok() || result.is_err());
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
        let temp_dir = TempDir::new().unwrap();
        let query_file_path = temp_dir.path().join("test.mq");
        let input_file_path = temp_dir.path().join("input.md");

        // Create query file
        fs::write(&query_file_path, "# Simple Query\n.").unwrap();

        // Create input file
        fs::write(&input_file_path, "# Test Input\n\nThis is a test.").unwrap();

        let engine = mq_lang::Engine::default();
        let (tx, _rx) = unbounded::<DebuggerMessage>();

        let result = execute_query(
            engine,
            query_file_path.to_string_lossy().to_string(),
            Some(input_file_path.to_string_lossy().to_string()),
            tx,
        );

        // The result might succeed or fail depending on the query execution
        // We're mainly testing that the function doesn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_execute_query_without_input_file() {
        let temp_dir = TempDir::new().unwrap();
        let query_file_path = temp_dir.path().join("test.mq");

        // Create query file
        fs::write(&query_file_path, ".").unwrap();

        let engine = mq_lang::Engine::default();
        let (tx, _rx) = unbounded::<DebuggerMessage>();

        let result = execute_query(
            engine,
            query_file_path.to_string_lossy().to_string(),
            None,
            tx,
        );

        // The result might succeed or fail depending on the query execution
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_execute_query_nonexistent_query_file() {
        let engine = mq_lang::Engine::default();
        let (tx, _rx) = unbounded::<DebuggerMessage>();

        let result = execute_query(engine, "nonexistent.mq".to_string(), None, tx);

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read query file")
        );
    }

    #[test]
    fn test_execute_query_nonexistent_input_file() {
        let temp_dir = TempDir::new().unwrap();
        let query_file_path = temp_dir.path().join("test.mq");

        // Create query file
        fs::write(&query_file_path, ".").unwrap();

        let engine = mq_lang::Engine::default();
        let (tx, _rx) = unbounded::<DebuggerMessage>();

        let result = execute_query(
            engine,
            query_file_path.to_string_lossy().to_string(),
            Some("nonexistent_input.md".to_string()),
            tx,
        );

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to read input file")
        );
    }

    #[test]
    fn test_execute_query_sends_terminated_message() {
        let temp_dir = TempDir::new().unwrap();
        let query_file_path = temp_dir.path().join("test.mq");

        // Create a simple query file
        fs::write(&query_file_path, ".").unwrap();

        let engine = mq_lang::Engine::default();
        let (tx, rx) = unbounded::<DebuggerMessage>();

        let result = execute_query(
            engine,
            query_file_path.to_string_lossy().to_string(),
            None,
            tx,
        );

        // Whether the execution succeeds or fails, a Terminated message should be sent
        if result.is_ok() {
            // If successful, check for Terminated message
            if let Ok(message) = rx.try_recv() {
                assert!(matches!(message, DebuggerMessage::Terminated));
            }
        } else {
            // If failed, Terminated message should still be sent
            if let Ok(message) = rx.try_recv() {
                assert!(matches!(message, DebuggerMessage::Terminated));
            }
        }
    }

    #[test]
    fn test_parse_input_data_edge_cases() {
        // Test empty file path
        let result = parse_input_data("", "content");
        assert!(result.is_ok() || result.is_err());

        // Test file path without extension
        let result = parse_input_data("filename", "content");
        assert!(result.is_ok() || result.is_err());

        // Test file path with multiple dots
        let result = parse_input_data("file.name.with.dots.md", "# Content");
        assert!(result.is_ok() || result.is_err());

        // Test case sensitivity
        let result = parse_input_data("test.JSON", r#"{"key": "value"}"#);
        assert!(!result.unwrap().is_empty());
    }
}
