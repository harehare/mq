use dap::prelude::*;
use dap::responses::{
    ContinueResponse, EvaluateResponse, ScopesResponse, SetBreakpointsResponse, StackTraceResponse,
    ThreadsResponse, VariablesResponse,
};
use dap::types::Breakpoint;
use serde::Deserialize;
use std::io::{self, BufReader, BufWriter};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::{debug, error, info};

#[derive(Error, Debug)]
enum MqAdapterError {
    #[error("Unhandled command: {0:?}")]
    UnhandledCommand(Command),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Failed to deserialize launch arguments: {0}")]
    LaunchArgumentsError(serde_json::Error),
    #[error("Missing launch arguments")]
    MissingLaunchArguments,
}

type DynResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Messages sent from the debugger handler to the DAP server
#[derive(Debug, Clone)]
pub enum DebuggerMessage {
    /// A breakpoint was hit, should send stopped event
    BreakpointHit {
        thread_id: i64,
        reason: String,
        line: usize,
        breakpoint: mq_lang::Breakpoint,
        context: mq_lang::DebugContext,
    },
    /// A step operation completed, should send stopped event
    StepCompleted {
        thread_id: i64,
        reason: String,
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

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LaunchArgs {
    // Optional arguments to the program.
    args: Option<Vec<String>>,
    // Optional working directory for the program.
    cwd: Option<String>,
}

fn main() -> DynResult<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("mq_dap=debug")),
        )
        .init();

    info!("Starting mq-dap debug adapter");

    let mut adapter = MqAdapter::new();
    let reader = BufReader::new(io::stdin());
    let writer = BufWriter::new(io::stdout());
    let mut server = Server::new(reader, writer);

    // First, the client sends an initialize request
    let req = match server.poll_request()? {
        Some(req) => req,
        None => {
            return Err(Box::new(MqAdapterError::ProtocolError(
                "Missing initialize request".to_string(),
            )));
        }
    };

    // We must respond with an `Initialize` response.
    // This is a good place to return server capabilities.
    if let Command::Initialize(_) = &req.command {
        // In a real adapter, you would adjust these capabilities
        let capabilities = types::Capabilities {
            supports_configuration_done_request: Some(true),
            supports_function_breakpoints: Some(true),
            supports_conditional_breakpoints: Some(true),
            supports_hit_conditional_breakpoints: Some(true),
            supports_evaluate_for_hovers: Some(true),
            ..Default::default()
        };
        let rsp = req.success(ResponseBody::Initialize(capabilities));
        server.respond(rsp)?;

        // Signifies that the adapter is ready to accept configuration requests
        server.send_event(Event::Initialized)?;
    } else {
        return Err(Box::new(MqAdapterError::ProtocolError(
            "Expected initialize request".to_string(),
        )));
    }

    // Take the receiver from adapter to handle messages in the main loop
    let debugger_message_rx = adapter.debugger_message_rx.take();

    // Main loop
    loop {
        // Handle debugger messages if available
        if let Some(ref rx) = debugger_message_rx {
            if let Ok(message) = rx.try_recv() {
                if let Err(e) = adapter.handle_debugger_message(message, &mut server) {
                    error!(error = %e, "Failed to handle debugger message");
                }
            }
        }

        match server.poll_request()? {
            Some(req) => {
                if let Err(e) = adapter.handle_request(req, &mut server) {
                    error!(error = %e, "Failed to handle DAP request");
                    if let Some(MqAdapterError::ProtocolError(msg)) =
                        e.downcast_ref::<MqAdapterError>()
                    {
                        if msg == "Shutdown" {
                            break;
                        }
                    }
                }
            }
            None => {
                info!("Client disconnected or stream ended");
                break;
            }
        }
    }

    Ok(())
}

struct MqAdapter {
    engine: mq_lang::Engine,
    current_program: Option<String>,
    debugger_message_rx: Option<Receiver<DebuggerMessage>>,
    dap_command_tx: Option<Sender<DapCommand>>,
}

struct DapDebuggerHandler {
    current_context: Option<mq_lang::DebugContext>,
    message_tx: Sender<DebuggerMessage>,
    thread_id: i64,
}

unsafe impl Send for DapDebuggerHandler {}
unsafe impl Sync for DapDebuggerHandler {}

impl std::fmt::Debug for DapDebuggerHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DapDebuggerHandler")
            .field("current_context", &self.current_context)
            .field("thread_id", &self.thread_id)
            .finish()
    }
}

impl DapDebuggerHandler {
    fn new(message_tx: Sender<DebuggerMessage>) -> Self {
        Self {
            current_context: None,
            message_tx,
            thread_id: 1, // Main thread ID
        }
    }

    fn set_context(&mut self, context: mq_lang::DebugContext) {
        self.current_context = Some(context);
    }
}

// This implementation is no longer used as the logic moved to DapHandlerWrapper
impl mq_lang::DebuggerHandler for DapDebuggerHandler {}

impl MqAdapter {
    fn new() -> Self {
        // Create channels for communication between DAP server and debugger handler
        let (message_tx, message_rx) = mpsc::channel::<DebuggerMessage>();
        let (command_tx, command_rx) = mpsc::channel::<DapCommand>();

        let dap_handler = Arc::new(Mutex::new(DapDebuggerHandler::new(message_tx)));
        let engine = mq_lang::Engine::default();

        // Set up the debugger handler
        {
            let handler_clone = dap_handler.clone();
            let handler_boxed = Box::new(DapHandlerWrapper {
                handler: handler_clone,
                command_rx: Arc::new(Mutex::new(command_rx)),
            });
            engine
                .debugger()
                .write()
                .unwrap()
                .set_handler(handler_boxed);
        }

        Self {
            engine,
            current_program: None,
            debugger_message_rx: Some(message_rx),
            dap_command_tx: Some(command_tx),
        }
    }
}

// Wrapper to implement DebuggerHandler trait for the Arc<Mutex<DapDebuggerHandler>>
#[derive(Debug)]
struct DapHandlerWrapper {
    handler: Arc<Mutex<DapDebuggerHandler>>,
    command_rx: Arc<Mutex<Receiver<DapCommand>>>,
}

impl mq_lang::DebuggerHandler for DapHandlerWrapper {
    fn on_breakpoint_hit(
        &mut self,
        breakpoint: &mq_lang::Breakpoint,
        context: &mq_lang::DebugContext,
    ) -> mq_lang::DebuggerAction {
        if let (Ok(mut handler), Ok(command_rx)) = (self.handler.lock(), self.command_rx.lock()) {
            handler.set_context(context.clone());
            debug!(line = breakpoint.line, "Breakpoint hit");

            // Send breakpoint hit message to DAP server
            let message = DebuggerMessage::BreakpointHit {
                thread_id: handler.thread_id,
                reason: "breakpoint".to_string(),
                line: breakpoint.line,
                breakpoint: breakpoint.clone(),
                context: context.clone(),
            };

            if let Err(e) = handler.message_tx.send(message) {
                error!(error = %e, "Failed to send breakpoint message to DAP server");
                return mq_lang::DebuggerAction::Continue;
            }

            // Wait for command from DAP server
            match command_rx.recv() {
                Ok(DapCommand::Continue) => mq_lang::DebuggerAction::Continue,
                Ok(DapCommand::Next) => mq_lang::DebuggerAction::Next,
                Ok(DapCommand::StepIn) => mq_lang::DebuggerAction::StepInto,
                Ok(DapCommand::StepOut) => mq_lang::DebuggerAction::FunctionExit,
                Ok(DapCommand::Terminate) => mq_lang::DebuggerAction::Quit,
                Err(e) => {
                    error!(error = %e, "Failed to receive command from DAP server");
                    mq_lang::DebuggerAction::Continue
                }
            }
        } else {
            mq_lang::DebuggerAction::Continue
        }
    }

    fn on_step(&mut self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        if let (Ok(mut handler), Ok(command_rx)) = (self.handler.lock(), self.command_rx.lock()) {
            handler.set_context(context.clone());
            debug!(line = context.token.range.start.line + 1, "Step event");

            // Send step completed message to DAP server
            let message = DebuggerMessage::StepCompleted {
                thread_id: handler.thread_id,
                reason: "step".to_string(),
                line: context.token.range.start.line as usize + 1,
                context: context.clone(),
            };

            if let Err(e) = handler.message_tx.send(message) {
                error!(error = %e, "Failed to send step message to DAP server");
                return mq_lang::DebuggerAction::Continue;
            }

            // Wait for command from DAP server
            match command_rx.recv() {
                Ok(DapCommand::Continue) => mq_lang::DebuggerAction::Continue,
                Ok(DapCommand::Next) => mq_lang::DebuggerAction::Next,
                Ok(DapCommand::StepIn) => mq_lang::DebuggerAction::StepInto,
                Ok(DapCommand::StepOut) => mq_lang::DebuggerAction::FunctionExit,
                Ok(DapCommand::Terminate) => mq_lang::DebuggerAction::Quit,
                Err(e) => {
                    error!(error = %e, "Failed to receive command from DAP server");
                    mq_lang::DebuggerAction::Continue
                }
            }
        } else {
            mq_lang::DebuggerAction::Continue
        }
    }
}

impl MqAdapter {
    /// Handle debugger messages and send appropriate DAP events
    fn handle_debugger_message(
        &mut self,
        message: DebuggerMessage,
        server: &mut Server<impl io::Read, impl io::Write>,
    ) -> DynResult<()> {
        match message {
            DebuggerMessage::BreakpointHit {
                thread_id, line, ..
            } => {
                debug!(line = line, "Sending stopped event for breakpoint");

                let event = Event::Stopped(events::StoppedEventBody {
                    reason: types::StoppedEventReason::Breakpoint,
                    description: Some(format!("Breakpoint hit at line {}", line)),
                    thread_id: Some(thread_id),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: None,
                });
                server.send_event(event)?;
            }
            DebuggerMessage::StepCompleted {
                thread_id, line, ..
            } => {
                debug!(line = line, "Sending stopped event for step");

                let event = Event::Stopped(events::StoppedEventBody {
                    reason: types::StoppedEventReason::Step,
                    description: Some(format!("Step completed at line {}", line)),
                    thread_id: Some(thread_id),
                    preserve_focus_hint: Some(false),
                    text: None,
                    all_threads_stopped: Some(true),
                    hit_breakpoint_ids: None,
                });
                server.send_event(event)?;
            }
            DebuggerMessage::Terminated => {
                debug!("Sending terminated event");

                let event = Event::Terminated(Some(events::TerminatedEventBody {
                    restart: Some(serde_json::Value::Bool(false)),
                }));
                server.send_event(event)?;
            }
        }
        Ok(())
    }

    /// Send a command to the debugger handler
    fn send_debugger_command(&self, command: DapCommand) -> DynResult<()> {
        if let Some(ref tx) = self.dap_command_tx {
            tx.send(command)
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        }
        Ok(())
    }

    fn handle_request(
        &mut self,
        req: Request,
        server: &mut Server<impl io::Read, impl io::Write>,
    ) -> DynResult<()> {
        match &req.command {
            Command::Launch(raw_args) => {
                let additional_data = raw_args
                    .additional_data
                    .as_ref()
                    .ok_or(MqAdapterError::MissingLaunchArguments)?;

                let args: LaunchArgs = serde_json::from_value(additional_data.clone())
                    .map_err(MqAdapterError::LaunchArgumentsError)?;

                debug!(?args, "Received launch request");

                // Store the program information
                if let Some(program) = additional_data.get("program").and_then(|v| v.as_str()) {
                    self.current_program = Some(program.to_string());
                }

                self.engine.debugger().write().unwrap().activate();

                server.send_event(Event::Initialized)?;

                let rsp = req.success(ResponseBody::Launch);
                server.respond(rsp)?;
            }
            Command::SetBreakpoints(args) => {
                debug!(?args, "Received SetBreakpoints request");

                let breakpoints_vec = args.breakpoints.as_ref().cloned().unwrap_or_default();

                for breakpoint in &breakpoints_vec {
                    self.engine.debugger().write().unwrap().add_breakpoint(
                        breakpoint.line as usize,
                        breakpoint.column.map(|bp| bp as usize),
                    );
                }

                let breakpoints_response: Vec<Breakpoint> = breakpoints_vec
                    .iter()
                    .map(|bp| Breakpoint {
                        verified: true,
                        line: Some(bp.line),
                        column: bp.column,
                        end_line: None,
                        end_column: None,
                        source: None,
                        message: None,
                        id: None,
                        instruction_reference: None,
                        offset: None,
                    })
                    .collect();

                let rsp = req.success(ResponseBody::SetBreakpoints(SetBreakpointsResponse {
                    breakpoints: breakpoints_response,
                }));

                server.respond(rsp)?;
            }
            Command::Threads => {
                debug!("Received Threads request");
                let thread = types::Thread {
                    id: 1,
                    name: "main".to_string(),
                };
                let rsp = req.success(ResponseBody::Threads(ThreadsResponse {
                    threads: vec![thread],
                }));
                server.respond(rsp)?;
            }
            Command::StackTrace(args) => {
                debug!(?args, "Received StackTrace request");

                let call_stack = self.engine.debugger().read().unwrap().current_call_stack();
                let stack_frames: Vec<types::StackFrame> = call_stack
                    .iter()
                    .enumerate()
                    .map(|(i, frame)| {
                        let token_range = self.engine.token_arena().read().unwrap()[frame.token_id]
                            .range
                            .clone();
                        types::StackFrame {
                            id: i as i64,
                            name: format!("Frame {}", i),
                            source: self.current_program.as_ref().map(|path| types::Source {
                                name: Some(
                                    std::path::Path::new(path)
                                        .file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("unknown")
                                        .to_string(),
                                ),
                                path: Some(path.clone()),
                                ..Default::default()
                            }),
                            line: token_range.start.line as i64 + 1,
                            column: token_range.start.column as i64 + 1,
                            ..Default::default()
                        }
                    })
                    .collect();

                let rsp = req.success(ResponseBody::StackTrace(StackTraceResponse {
                    stack_frames,
                    total_frames: Some(call_stack.len() as i64),
                }));
                server.respond(rsp)?;
            }

            Command::Variables(args) => {
                debug!(?args, "Received Variables request");
                let rsp = req.success(ResponseBody::Variables(VariablesResponse {
                    variables: vec![],
                }));
                server.respond(rsp)?;
            }
            Command::Continue(_) => {
                debug!("Received Continue request");
                self.send_debugger_command(DapCommand::Continue)?;
                self.engine
                    .debugger()
                    .write()
                    .unwrap()
                    .set_command(mq_lang::DebuggerCommand::Continue);
                let rsp = req.success(ResponseBody::Continue(ContinueResponse {
                    all_threads_continued: Some(true),
                }));
                server.respond(rsp)?;
            }
            Command::Next(_) => {
                debug!("Received Next request");
                self.send_debugger_command(DapCommand::Next)?;
                let rsp = req.success(ResponseBody::Next);
                server.respond(rsp)?;
            }
            Command::StepIn(_) => {
                debug!("Received StepIn request");
                self.send_debugger_command(DapCommand::StepIn)?;
                let rsp = req.success(ResponseBody::StepIn);
                server.respond(rsp)?;
            }
            Command::StepOut(_) => {
                debug!("Received StepOut request");
                self.send_debugger_command(DapCommand::StepOut)?;
                let rsp = req.success(ResponseBody::StepOut);
                server.respond(rsp)?;
            }
            Command::ConfigurationDone => {
                debug!("Received ConfigurationDone request");
                let rsp = req.success(ResponseBody::ConfigurationDone);
                server.respond(rsp)?;
            }
            Command::Disconnect(_) => {
                info!("Received Disconnect request");

                // Send terminate command to debugger handler
                let _ = self.send_debugger_command(DapCommand::Terminate);

                // Deactivate the debugger
                self.engine.debugger().write().unwrap().deactivate();
                self.current_program = None;

                let rsp = req.success(ResponseBody::Disconnect);
                server.respond(rsp)?;
                return Err(Box::new(MqAdapterError::ProtocolError(
                    "Shutdown".to_string(),
                )));
            }
            Command::Evaluate(args) => {
                debug!(?args, "Received Evaluate request");

                // Try to evaluate the expression using the mq engine
                let result = match self.engine.eval(&args.expression, std::iter::empty()) {
                    Ok(values) => {
                        let result_str = values
                            .values()
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        result_str
                    }
                    Err(e) => format!("Error: {}", e),
                };

                let rsp = req.success(ResponseBody::Evaluate(EvaluateResponse {
                    result,
                    type_field: Some("string".to_string()),
                    variables_reference: 0,
                    named_variables: None,
                    indexed_variables: None,
                    presentation_hint: None,
                    memory_reference: None,
                }));
                server.respond(rsp)?;
            }
            Command::Scopes(_) => {
                debug!("Received Scopes request");

                let scopes = vec![types::Scope {
                    name: "Local".to_string(),
                    variables_reference: 1,
                    expensive: false,
                    named_variables: None,
                    indexed_variables: None,
                    source: None,
                    line: None,
                    column: None,
                    end_line: None,
                    end_column: None,
                    presentation_hint: None,
                }];

                let rsp = req.success(ResponseBody::Scopes(ScopesResponse { scopes }));
                server.respond(rsp)?;
            }
            command => {
                return Err(Box::new(MqAdapterError::UnhandledCommand(command.clone())));
            }
        }
        Ok(())
    }
}
