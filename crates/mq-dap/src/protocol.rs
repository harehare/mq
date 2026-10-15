use serde::{Deserialize, Serialize};

/// Messages sent from the debugger handler to the DAP server
#[derive(Debug, Clone)]
pub enum DebuggerMessage {
    /// A breakpoint was hit, should send stopped event
    BreakpointHit {
        /// Id of the debugged thread.
        thread_id: i64,
        /// Line number of the stop.
        line: usize,
        /// The breakpoint that was hit.
        breakpoint: mq_lang::Breakpoint,
        /// Debugger context at the stop.
        context: mq_lang::DebugContext,
    },
    /// A step operation completed, should send stopped event
    StepCompleted {
        /// Id of the debugged thread.
        thread_id: i64,
        /// Line number of the stop.
        line: usize,
        /// Debugger context at the stop.
        context: mq_lang::DebugContext,
    },
    /// Program was paused by user request, should send stopped event
    Paused {
        /// Id of the debugged thread.
        thread_id: i64,
        /// Line number of the stop.
        line: usize,
        /// Debugger context at the stop.
        context: mq_lang::DebugContext,
    },
    /// A logpoint breakpoint fired; should send an output event without stopping execution.
    LogPoint {
        /// The interpolated log message.
        message: String,
    },
    /// An uncaught error was raised while the "Uncaught Exceptions" filter is enabled,
    /// should send a stopped event with reason `exception`.
    ExceptionPaused {
        /// Id of the debugged thread.
        thread_id: i64,
        /// Message text.
        message: String,
        /// Debugger context at the stop.
        context: mq_lang::DebugContext,
    },
    /// Program has terminated
    Terminated,
}

/// Messages sent from the DAP server to the debugger handler
#[derive(Debug, Clone)]
pub enum DapCommand {
    /// Continue execution
    Continue,
    /// Step to next line
    Next,
    /// Step into function
    StepIn,
    /// Step out of function
    StepOut,
    /// Pause execution
    Pause,
    /// Terminate debugging session
    Terminate,
}

/// Launch arguments for DAP launch configuration
#[derive(Deserialize, Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LaunchArgs {
    /// Path of the mq query file to debug.
    pub query_file: String,
    /// Path of the input file, if any.
    pub input_file: Option<String>,
}
