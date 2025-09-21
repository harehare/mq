use serde::Deserialize;

/// Messages sent from the debugger handler to the DAP server
#[derive(Debug, Clone)]
pub enum DebuggerMessage {
    /// A breakpoint was hit, should send stopped event
    BreakpointHit {
        thread_id: i64,
        line: usize,
        breakpoint: mq_lang::Breakpoint,
        context: mq_lang::DebugContext,
    },
    /// A step operation completed, should send stopped event
    StepCompleted {
        thread_id: i64,
        line: usize,
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
    /// Terminate debugging session
    Terminate,
}

/// Launch arguments for DAP launch configuration
#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LaunchArgs {
    pub query_file: String,
    pub input_file: Option<String>,
}
