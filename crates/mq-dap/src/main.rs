use crossbeam_channel::{Receiver, Sender};
use dap::prelude::*;
use dap::responses::{
    ContinueResponse, EvaluateResponse, ScopesResponse, SetBreakpointsResponse,
    SetVariableResponse, StackTraceResponse, ThreadsResponse, VariablesResponse,
};
use dap::types::Breakpoint;
use mq_lang::Shared;
use serde::Deserialize;
use std::io::{self, BufReader, BufWriter};
use std::path::PathBuf;
use std::thread;
use std::{fs, vec};
use tracing::{debug, error, info};

use crate::error::MqAdapterError;
use crate::log::DebugConsoleWriter;

mod error;
mod log;

type DynResult<T> = miette::Result<T, Box<dyn std::error::Error>>;

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
    query_file: String,
    input_file: Option<String>,
}

fn main() -> DynResult<()> {
    let (debug_writer, log_rx) = DebugConsoleWriter::new();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(debug_writer)
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

    if let Command::Initialize(_) = &req.command {
        let capabilities = types::Capabilities {
            supports_set_variable: Some(true),
            supports_exception_options: Some(false),
            ..Default::default()
        };
        let rsp = req.success(ResponseBody::Initialize(capabilities));
        server.respond(rsp)?;
        server.send_event(Event::Initialized)?;
    } else {
        return Err(Box::new(MqAdapterError::ProtocolError(
            "Expected initialize request".to_string(),
        )));
    }

    loop {
        debug!("Checking for log messages");
        // Process all available log messages
        while let Ok(log_message) = log_rx.clone().try_recv() {
            if let Err(e) = adapter.send_log_output(&log_message, &mut server) {
                eprintln!("Failed to send log output: {}", e);
            }
        }

        debug!("Waiting for next request or debugger message");
        if let Some(ref rx) = adapter.debugger_message_rx {
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
    query_file: Option<String>,
    debugger_message_rx: Option<Receiver<DebuggerMessage>>,
    debugger_message_tx: Option<Sender<DebuggerMessage>>,
    dap_command_tx: Option<Sender<DapCommand>>,
    current_debug_context: Option<mq_lang::DebugContext>,
}

struct DapDebuggerHandler {
    current_context: Option<mq_lang::DebugContext>,
    message_tx: Sender<DebuggerMessage>,
    thread_id: i64,
}

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

impl mq_lang::DebuggerHandler for DapDebuggerHandler {}

impl MqAdapter {
    fn get_variables_from_context(&self) -> Vec<types::Variable> {
        if let Some(ref context) = self.current_debug_context {
            context
                .env
                .read()
                .unwrap()
                .get_local_variables()
                .iter()
                .map(|v| types::Variable {
                    name: v.name.to_string(),
                    value: v.value.to_string(),
                    type_field: Some(v.type_field.clone()),
                    variables_reference: 0,
                    named_variables: None,
                    indexed_variables: None,
                    presentation_hint: None,
                    evaluate_name: Some(v.name.to_string()),
                    memory_reference: None,
                })
                .collect()
        } else {
            vec![]
        }
    }

    /// Execute query in a separate thread
    fn execute_query_in_thread(
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

            match PathBuf::from(&file_path)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase()
                .as_str()
            {
                "json" | "csv" | "tsv" | "xml" | "toml" | "yaml" | "yml" | "txt" => {
                    mq_lang::raw_input(&input)
                }
                "html" | "htm" => mq_lang::parse_html_input(&input).map_err(|e| {
                    let error_msg = format!("Failed to parse input file '{}': {}", file_path, e);
                    error!(error = %error_msg);
                    Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
                })?,
                "mdx" => mq_lang::parse_mdx_input(&input).map_err(|e| {
                    let error_msg = format!("Failed to parse input file '{}': {}", file_path, e);
                    error!(error = %error_msg);
                    Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
                })?,
                _ => mq_lang::parse_markdown_input(&input).map_err(|e| {
                    let error_msg = format!("Failed to parse input file '{}': {}", file_path, e);
                    error!(error = %error_msg);
                    Box::new(MqAdapterError::FileError(error_msg)) as Box<dyn std::error::Error>
                })?,
            }
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

    fn new() -> Self {
        // Create channels for communication between DAP server and debugger handler
        let (message_tx, message_rx) = crossbeam_channel::unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = crossbeam_channel::unbounded::<DapCommand>();

        let dap_handler = DapDebuggerHandler::new(message_tx.clone());
        let engine = mq_lang::Engine::default();

        // Set up the debugger handler
        {
            let handler_boxed = Box::new(DapHandlerWrapper {
                handler: dap_handler,
                command_rx,
            });
            engine
                .debugger()
                .write()
                .unwrap()
                .set_handler(handler_boxed);
        }

        Self {
            engine,
            debugger_message_rx: Some(message_rx),
            debugger_message_tx: Some(message_tx),
            dap_command_tx: Some(command_tx),
            query_file: None,
            current_debug_context: None,
        }
    }
}

// Wrapper to implement DebuggerHandler trait for the Arc<Mutex<DapDebuggerHandler>>
#[derive(Debug)]
struct DapHandlerWrapper {
    handler: DapDebuggerHandler,
    command_rx: Receiver<DapCommand>,
}

impl mq_lang::DebuggerHandler for DapHandlerWrapper {
    fn on_breakpoint_hit(
        &mut self,
        breakpoint: &mq_lang::Breakpoint,
        context: &mq_lang::DebugContext,
    ) -> mq_lang::DebuggerAction {
        self.handler.set_context(context.clone());
        debug!(line = breakpoint.line, "Breakpoint hit");

        // Send breakpoint hit message to DAP server
        let message = DebuggerMessage::BreakpointHit {
            thread_id: self.handler.thread_id,
            reason: "breakpoint".to_string(),
            line: breakpoint.line,
            breakpoint: breakpoint.clone(),
            context: context.clone(),
        };

        if let Err(e) = self.handler.message_tx.send(message) {
            error!(error = %e, "Failed to send breakpoint message to DAP server");
            return mq_lang::DebuggerAction::Continue;
        }

        // Wait for command from DAP server
        match self.command_rx.recv() {
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
    }

    fn on_step(&mut self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        self.handler.set_context(context.clone());
        debug!(line = context.token.range.start.line + 1, "Step event");

        // Send step completed message to DAP server
        let message = DebuggerMessage::StepCompleted {
            thread_id: self.handler.thread_id,
            reason: "step".to_string(),
            line: context.token.range.start.line as usize + 1,
            context: context.clone(),
        };

        if let Err(e) = self.handler.message_tx.send(message) {
            error!(error = %e, "Failed to send step message to DAP server");
            return mq_lang::DebuggerAction::Continue;
        }

        // Wait for command from DAP server
        match self.command_rx.recv() {
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
    }
}

impl MqAdapter {
    fn send_log_output(
        &self,
        message: &str,
        server: &mut Server<impl io::Read, impl io::Write>,
    ) -> DynResult<()> {
        let event = Event::Output(events::OutputEventBody {
            output: message.to_string(),
            category: Some(types::OutputEventCategory::Console),
            group: None,
            variables_reference: None,
            source: None,
            line: None,
            column: None,
            data: None,
        });
        server.send_event(event)?;
        Ok(())
    }

    /// Handle debugger messages and send appropriate DAP events
    fn handle_debugger_message(
        &mut self,
        message: DebuggerMessage,
        server: &mut Server<impl io::Read, impl io::Write>,
    ) -> DynResult<()> {
        match message {
            DebuggerMessage::BreakpointHit {
                thread_id,
                line,
                context,
                breakpoint,
                ..
            } => {
                // Store the current debug context for variable inspection
                self.current_debug_context = Some(context);
                debug!(line = line, "Sending stopped event for breakpoint");

                let event = Event::Stopped(events::StoppedEventBody {
                    reason: types::StoppedEventReason::Breakpoint,
                    description: Some(format!("Breakpoint hit at line {}", line)),
                    thread_id: Some(thread_id),
                    preserve_focus_hint: None,
                    text: None,
                    all_threads_stopped: None,
                    hit_breakpoint_ids: Some(vec![breakpoint.id as i64]),
                });
                server.send_event(event)?;
            }
            DebuggerMessage::StepCompleted {
                thread_id,
                line,
                context,
                ..
            } => {
                // Store the current debug context for variable inspection
                self.current_debug_context = Some(context);
                debug!(line = line, "Sending stopped event for step");

                let event = Event::Stopped(events::StoppedEventBody {
                    reason: types::StoppedEventReason::Step,
                    description: Some(format!("Step completed at line {}", line)),
                    thread_id: Some(thread_id),
                    preserve_focus_hint: None,
                    text: None,
                    all_threads_stopped: None,
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

    #[inline(always)]
    fn get_source(&self) -> Option<types::Source> {
        self.query_file.as_ref().map(|query_file| types::Source {
            name: PathBuf::from(query_file)
                .file_name()
                .map(|n| n.to_string_lossy().to_string()),
            path: Some(query_file.clone()),
            adapter_data: None,
            source_reference: None,
            presentation_hint: None,
            origin: None,
            checksums: None,
            sources: None,
        })
    }

    #[inline(always)]
    fn get_source_file_name(&self) -> String {
        self.query_file
            .as_ref()
            .and_then(|query_file| {
                PathBuf::from(query_file)
                    .file_name()
                    .map(|p| p.to_string_lossy().to_string())
            })
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn eval(&self, code: &str) -> DynResult<mq_lang::Values> {
        let mut engine = if let Some(ref context) = self.current_debug_context {
            let eng = self.engine.clone();
            eng.switch_env(Shared::clone(&context.env));
            eng
        } else {
            self.engine.clone()
        };

        engine
            .eval(code, mq_lang::null_input().into_iter())
            .map_err(|e| {
                let error_msg = format!("Evaluation error: {}", e);
                error!(error = %error_msg);
                Box::new(MqAdapterError::EvaluationError(error_msg)) as Box<dyn std::error::Error>
            })
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

                self.engine.debugger().write().unwrap().activate();
                self.engine.load_builtin_module();
                self.query_file = Some(args.query_file.clone());

                let engine_clone = self.engine.clone();

                if let Some(ref message_tx) = self.debugger_message_tx {
                    let message_tx_clone = message_tx.clone();

                    thread::spawn(move || {
                        if let Err(e) = Self::execute_query_in_thread(
                            engine_clone,
                            args.query_file,
                            args.input_file,
                            message_tx_clone,
                        ) {
                            error!(error = %e, "Failed to execute query in background thread");
                        }
                    });
                }

                server.send_event(Event::Initialized)?;

                let rsp = req.success(ResponseBody::Launch);
                server.respond(rsp)?;
            }
            Command::SetBreakpoints(args) => {
                debug!(?args, "Received SetBreakpoints request");

                if let (Some(source_path), Some(query_file)) =
                    (args.source.path.as_ref(), self.query_file.as_ref())
                {
                    let source_abs = std::fs::canonicalize(source_path)
                        .unwrap_or_else(|_| std::path::PathBuf::from(source_path));
                    let query_abs = std::fs::canonicalize(query_file)
                        .unwrap_or_else(|_| std::path::PathBuf::from(query_file));
                    if source_abs != query_abs {
                        let rsp =
                            req.success(ResponseBody::SetBreakpoints(SetBreakpointsResponse {
                                breakpoints: vec![],
                            }));

                        server.respond(rsp)?;
                        return Ok(());
                    }
                } else {
                    error!(
                        "SetBreakpoints request for unknown source: {:?}",
                        args.source.path
                    );
                }

                let breakpoints_vec = args.breakpoints.as_ref().cloned().unwrap_or_default();
                let mut breakpoints_response: Vec<Breakpoint> =
                    Vec::with_capacity(breakpoints_vec.len());

                for breakpoint in &breakpoints_vec {
                    let id = self.engine.debugger().write().unwrap().add_breakpoint(
                        breakpoint.line as usize,
                        breakpoint.column.map(|bp| bp as usize),
                    );
                    breakpoints_response.push(Breakpoint {
                        verified: true,
                        line: Some(breakpoint.line),
                        column: breakpoint.column,
                        end_line: None,
                        end_column: None,
                        source: Some(args.source.clone()),
                        message: None,
                        id: Some(id as i64),
                        instruction_reference: None,
                        offset: None,
                    });
                }

                let breakpoints_response: Vec<Breakpoint> = breakpoints_vec
                    .iter()
                    .map(|bp| Breakpoint {
                        verified: true,
                        line: Some(bp.line),
                        column: bp.column,
                        end_line: None,
                        end_column: None,
                        source: Some(args.source.clone()),
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
                server.respond(req.success(ResponseBody::Threads(ThreadsResponse {
                    threads: vec![types::Thread {
                        id: 1,
                        name: "main".to_string(),
                    }],
                })))?;
            }
            Command::StackTrace(args) => {
                debug!(?args, "Received StackTrace request");

                let call_stack = if let Some(context) = &self.current_debug_context {
                    context.call_stack.clone()
                } else {
                    Vec::new()
                };

                let source = self.get_source();
                let file_name = self.get_source_file_name();

                let stack_frames = if !call_stack.is_empty() {
                    call_stack
                        .iter()
                        .rev()
                        .enumerate()
                        .map(|(i, frame)| {
                            let token_range = if i == 0 {
                                if let Some(context) = self.current_debug_context.as_ref() {
                                    context.token.range.clone()
                                } else {
                                    self.engine.token_arena().read().unwrap()[frame.token_id]
                                        .range
                                        .clone()
                                }
                            } else {
                                self.engine.token_arena().read().unwrap()[frame.token_id]
                                    .range
                                    .clone()
                            };
                            types::StackFrame {
                                id: i as i64 + 1,
                                name: format!(
                                    "{} ({}:{})",
                                    frame.expr, file_name, token_range.start.line,
                                ),
                                line: token_range.start.line as i64,
                                column: token_range.start.column as i64,
                                source: source.clone(),
                                ..Default::default()
                            }
                        })
                        .collect::<Vec<_>>()
                } else if let Some(ref context) = self.current_debug_context {
                    vec![types::StackFrame {
                        id: 0,
                        name: format!(
                            "{} ({}:{})",
                            context.current_node.expr, file_name, context.token.range.start.line
                        ),
                        line: context.token.range.start.line as i64,
                        column: context.token.range.start.column as i64,
                        source: source.clone(),
                        ..Default::default()
                    }]
                } else {
                    vec![types::StackFrame {
                        id: 0,
                        name: "unknown".to_string(),
                        line: 1,
                        column: 1,
                        source: source.clone(),
                        ..Default::default()
                    }]
                };

                let rsp = req.success(ResponseBody::StackTrace(StackTraceResponse {
                    stack_frames: stack_frames.clone(),
                    total_frames: Some(stack_frames.len() as i64),
                }));
                server.respond(rsp)?;
            }
            Command::Variables(args) => {
                debug!(?args, "Received Variables request");
                let variables = if args.variables_reference == 1 {
                    self.get_variables_from_context()
                } else {
                    vec![]
                };
                let rsp = req.success(ResponseBody::Variables(VariablesResponse { variables }));
                server.respond(rsp)?;
            }
            Command::SetVariable(args) => {
                debug!(?args, "Received SetVariables request");

                if let Some(ref context) = self.current_debug_context {
                    let local_variables = context
                        .env
                        .clone()
                        .read()
                        .unwrap()
                        .get_local_variables()
                        .clone();
                    let local_var = local_variables.iter().find(|v| v.name == args.name);

                    if let Some(var) = local_var {
                        // TODO:
                        let value = args.value.clone();
                        let rsp = req.success(ResponseBody::SetVariable(SetVariableResponse {
                            value,
                            indexed_variables: None,
                            named_variables: None,
                            type_field: None,
                            variables_reference: None,
                        }));
                        server.respond(rsp)?;
                    } else {
                        let rsp = req.error("No such variable in the current context");
                        server.respond(rsp)?;
                        return Ok(());
                    }
                } else {
                    let rsp = req.error("No active debug context to set variable");
                    server.respond(rsp)?;
                    return Ok(());
                }
            }
            Command::Continue(_) => {
                debug!("Received Continue request");
                self.send_debugger_command(DapCommand::Continue)?;
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
                debug!("Received Disconnect request");

                // Send terminate command to debugger handler
                let _ = self.send_debugger_command(DapCommand::Terminate);

                // Deactivate the debugger
                self.engine.debugger().write().unwrap().deactivate();

                let rsp = req.success(ResponseBody::Disconnect);
                server.respond(rsp)?;
                return Err(Box::new(MqAdapterError::ProtocolError(
                    "Shutdown".to_string(),
                )));
            }
            Command::Evaluate(args) => {
                debug!(?args, "Received Evaluate request");

                let mut engine = if let Some(ref context) = self.current_debug_context {
                    let eng = self.engine.clone();
                    eng.switch_env(Shared::clone(&context.env));
                    eng
                } else {
                    self.engine.clone()
                };

                let rsp = match engine.eval(&args.expression, mq_lang::null_input().into_iter()) {
                    Ok(values) => {
                        let result = values
                            .values()
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        req.success(ResponseBody::Evaluate(EvaluateResponse {
                            result,
                            type_field: Some("string".to_string()),
                            variables_reference: 0,
                            named_variables: None,
                            indexed_variables: None,
                            presentation_hint: None,
                            memory_reference: None,
                        }))
                    }
                    Err(e) => req.error(&format!("Evaluation error: {}", e)),
                };
                server.respond(rsp)?
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
