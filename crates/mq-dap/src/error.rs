use std::borrow::Cow;

use dap::prelude::Command;
use thiserror::Error;

#[derive(Error, Debug)]
/// Errors raised by the DAP adapter.
pub enum MqAdapterError {
    #[error("Unhandled command: {0:?}")]
    /// The adapter has no handler for the command.
    UnhandledCommand(Box<Command>),
    #[error("Protocol error: {0}")]
    /// A DAP message violated the protocol.
    ProtocolError(Cow<'static, str>),
    #[error("Failed to deserialize launch arguments: {0}")]
    /// Launch arguments could not be parsed.
    LaunchArgumentsError(serde_json::Error),
    #[error("Missing launch arguments")]
    /// The launch request carried no arguments.
    MissingLaunchArguments,
    #[error("File I/O error: {0}")]
    /// Reading a file failed.
    FileError(String),
    #[error("Query execution error: {0}")]
    /// The query failed to compile or run.
    QueryError(String),
    #[error("Evaluation error: {0}")]
    /// Evaluating an expression failed.
    EvaluationError(Cow<'static, str>),
}
