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
fn parse_input_data(file_path: &str, input: &str) -> DynResult<Vec<mq_lang::Value>> {
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
