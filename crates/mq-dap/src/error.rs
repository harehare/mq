use dap::prelude::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MqAdapterError {
    #[error("Unhandled command: {0:?}")]
    UnhandledCommand(Command),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Failed to deserialize launch arguments: {0}")]
    LaunchArgumentsError(serde_json::Error),
    #[error("Missing launch arguments")]
    MissingLaunchArguments,
    #[error("File I/O error: {0}")]
    FileError(String),
    #[error("Query execution error: {0}")]
    QueryError(String),
    #[error("Evaluation error: {0}")]
    EvaluationError(String),
}
