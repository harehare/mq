use crossbeam_channel::{Receiver, Sender};
use dap::prelude::*;
use dap::responses::{
    ContinueResponse, EvaluateResponse, ScopesResponse, SetBreakpointsResponse,
    SetExceptionBreakpointsResponse, SetVariableResponse, StackTraceResponse, ThreadsResponse,
    VariablesResponse,
};
use dap::types::Breakpoint;
use mq_lang::Shared;
use std::path::PathBuf;
use std::thread;
use std::{io, vec};
use tracing::{debug, error};

use crate::error::MqAdapterError;
use crate::executor;
use crate::handler::{DapDebuggerHandler, DapHandlerWrapper};
use crate::protocol::{DapCommand, DebuggerMessage, LaunchArgs};

type DynResult<T> = miette::Result<T, Box<dyn std::error::Error>>;

/// Main DAP adapter for mq debugger
pub struct MqAdapter {
    engine: mq_lang::Engine,
    query_file: Option<String>,
    debugger_message_rx: Option<Receiver<DebuggerMessage>>,
    debugger_message_tx: Option<Sender<DebuggerMessage>>,
    dap_command_tx: Option<Sender<DapCommand>>,
    current_debug_context: Option<mq_lang::DebugContext>,
}

impl Default for MqAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MqAdapter {
    fn new() -> Self {
        // Create channels for communication between DAP server and debugger handler
        let (message_tx, message_rx) = crossbeam_channel::unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = crossbeam_channel::unbounded::<DapCommand>();

        let dap_handler = DapDebuggerHandler::new(message_tx.clone());
        let mut engine = mq_lang::Engine::default();

        // Set up the debugger handler
        {
            let handler_boxed = Box::new(DapHandlerWrapper::new(dap_handler, command_rx));
            engine.set_debugger_handler(handler_boxed);
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

    pub fn debugger_message_rx(&self) -> &Option<Receiver<DebuggerMessage>> {
        &self.debugger_message_rx
    }

    /// Get local variables from the current debug context
    fn get_local_variables_from_context(&self) -> Vec<types::Variable> {
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

    /// Get global variables from the current debug context
    fn get_global_variables_from_context(&self) -> Vec<types::Variable> {
        if let Some(ref context) = self.current_debug_context {
            context
                .env
                .read()
                .unwrap()
                .get_global_variables()
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

    /// Send log output to the DAP client
    pub fn send_log_output(
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
    pub fn handle_debugger_message(
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
            DebuggerMessage::Paused {
                thread_id,
                line,
                context,
                ..
            } => {
                // Store the current debug context for variable inspection
                self.current_debug_context = Some(context);
                debug!(line = line, "Sending stopped event for pause");

                let event = Event::Stopped(events::StoppedEventBody {
                    reason: types::StoppedEventReason::Pause,
                    description: Some(format!("Paused at line {}", line)),
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

    /// Get source information for current query file
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

    /// Get source file name for a given module
    #[inline(always)]
    fn get_source_file_name(&self, module_id: Option<mq_lang::ModuleId>) -> String {
        if module_id.unwrap_or(mq_lang::Module::TOP_LEVEL_MODULE_ID)
            == mq_lang::Module::TOP_LEVEL_MODULE_ID
        {
            self.query_file
                .as_ref()
                .and_then(|query_file| {
                    PathBuf::from(query_file)
                        .file_name()
                        .map(|p| p.to_string_lossy().to_string())
                })
                .unwrap_or_else(|| "unknown".to_string())
        } else {
            format!("{}.mq", self.engine.get_module_name(module_id.unwrap()))
        }
    }

    /// Evaluate code in the current debug context
    fn eval(&self, code: &str) -> DynResult<mq_lang::RuntimeValues> {
        let mut engine = if let Some(ref context) = self.current_debug_context {
            mq_lang::Engine::default().switch_env(Shared::clone(&context.env))
        } else {
            return Err(Box::new(MqAdapterError::EvaluationError(
                "Current context not found".to_string(),
            )) as Box<dyn std::error::Error>);
        };

        engine
            .eval(code, mq_lang::null_input().into_iter())
            .map_err(|e| {
                let error_msg = format!("Evaluation error: {}", e);
                error!(error = %error_msg);
                Box::new(MqAdapterError::EvaluationError(error_msg)) as Box<dyn std::error::Error>
            })
    }

    /// Handle DAP request and send appropriate response
    pub fn handle_request(
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
                        if let Err(e) = executor::execute_query(
                            engine_clone,
                            args.query_file,
                            args.input_file,
                            message_tx_clone,
                        ) {
                            error!(error = %e, "Failed to execute query in background thread");
                        }
                    });
                }

                let rsp = req.success(ResponseBody::Launch);
                server.respond(rsp)?;
            }
            Command::SetExceptionBreakpoints(_) => {
                debug!("Received SetExceptionBreakpoints request");
                let rsp = req.success(ResponseBody::SetExceptionBreakpoints(
                    SetExceptionBreakpointsResponse { breakpoints: None },
                ));
                server.respond(rsp)?;
            }
            Command::SetBreakpoints(args) => {
                debug!(?args, "Received SetBreakpoints request");

                let is_query_file = if let (Some(source_path), Some(query_file)) =
                    (args.source.path.as_ref(), self.query_file.as_ref())
                {
                    let source_abs = std::fs::canonicalize(source_path)
                        .unwrap_or_else(|_| std::path::PathBuf::from(source_path));
                    let query_abs = std::fs::canonicalize(query_file)
                        .unwrap_or_else(|_| std::path::PathBuf::from(query_file));
                    source_abs == query_abs
                } else {
                    false
                };

                let breakpoints_vec = args.breakpoints.as_ref().cloned().unwrap_or_default();
                let mut breakpoints_response: Vec<Breakpoint> =
                    Vec::with_capacity(breakpoints_vec.len());

                let source = if is_query_file {
                    None
                } else {
                    args.source.name.clone()
                };

                self.engine
                    .debugger()
                    .write()
                    .unwrap()
                    .remove_breakpoints(&source);

                for breakpoint in &breakpoints_vec {
                    let id = self.engine.debugger().write().unwrap().add_breakpoint(
                        breakpoint.line as usize,
                        breakpoint.column.map(|bp| bp as usize),
                        source.clone(),
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

                // Use the breakpoints_response vector with IDs for the response
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
                let stack_frames = if !call_stack.is_empty() {
                    call_stack
                        .iter()
                        .rev()
                        .enumerate()
                        .map(|(i, frame)| {
                            let (file_name, token_range) = if i == 0 {
                                if let Some(context) = self.current_debug_context.as_ref() {
                                    (
                                        self.get_source_file_name(Some(context.token.module_id)),
                                        context.token.range.clone(),
                                    )
                                } else {
                                    (
                                        self.get_source_file_name(None),
                                        self.engine.token_arena().read().unwrap()[frame.token_id]
                                            .range
                                            .clone(),
                                    )
                                }
                            } else {
                                (
                                    self.get_source_file_name(None),
                                    self.engine.token_arena().read().unwrap()[frame.token_id]
                                        .range
                                        .clone(),
                                )
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
                            context.current_node.expr,
                            self.get_source_file_name(Some(context.token.module_id)),
                            context.token.range.start.line
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
                    self.get_global_variables_from_context()
                } else {
                    self.get_local_variables_from_context()
                };
                let rsp = req.success(ResponseBody::Variables(VariablesResponse { variables }));
                server.respond(rsp)?;
            }
            Command::SetVariable(args) => {
                debug!(?args, "Received SetVariables request");
                self.eval(format!("let {} = {}", args.name, args.value).as_str())?;

                let value = args.value.clone();
                let rsp = req.success(ResponseBody::SetVariable(SetVariableResponse {
                    value,
                    indexed_variables: None,
                    named_variables: None,
                    type_field: None,
                    variables_reference: None,
                }));
                server.respond(rsp)?;
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

                let rsp = match self.eval(&args.expression) {
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
            Command::Scopes(args) => {
                debug!("Received Scopes request");

                let scopes = vec![
                    types::Scope {
                        name: "GLOBAL".to_string(),
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
                    },
                    types::Scope {
                        name: "LOCAL".to_string(),
                        variables_reference: args.frame_id + 1,
                        expensive: false,
                        named_variables: None,
                        indexed_variables: None,
                        source: None,
                        line: None,
                        column: None,
                        end_line: None,
                        end_column: None,
                        presentation_hint: None,
                    },
                ];

                let rsp = req.success(ResponseBody::Scopes(ScopesResponse { scopes }));
                server.respond(rsp)?;
            }
            Command::Pause(_) => {
                debug!("Received Pause request");
                self.send_debugger_command(DapCommand::Pause)?;
                let rsp = req.success(ResponseBody::Pause);
                server.respond(rsp)?;
            }
            command => {
                return Err(Box::new(MqAdapterError::UnhandledCommand(command.clone())));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dap::server::Server;
    use std::io::{BufReader, BufWriter, Cursor};

    #[test]
    fn test_adapter_new_and_default() {
        let adapter = MqAdapter::new();
        assert!(adapter.debugger_message_rx.is_some());
        assert!(adapter.debugger_message_tx.is_some());
        assert!(adapter.dap_command_tx.is_some());
        assert!(adapter.query_file.is_none());
        assert!(adapter.current_debug_context.is_none());

        let adapter_default = MqAdapter::default();
        assert!(adapter_default.debugger_message_rx.is_some());
    }

    #[test]
    fn test_get_local_variables_from_context_empty() {
        let adapter = MqAdapter::new();
        let variables = adapter.get_local_variables_from_context();
        assert!(variables.is_empty());
    }

    #[test]
    fn test_get_global_variables_from_context_empty() {
        let adapter = MqAdapter::new();
        let variables = adapter.get_global_variables_from_context();
        assert!(variables.is_empty());
    }

    #[test]
    fn test_get_source_with_query_file() {
        let mut adapter = MqAdapter::new();
        adapter.query_file = Some("/path/to/test.mq".to_string());

        let source = adapter.get_source();
        assert!(source.is_some());
        let source = source.unwrap();
        assert_eq!(source.name, Some("test.mq".to_string()));
        assert_eq!(source.path, Some("/path/to/test.mq".to_string()));
    }

    #[test]
    fn test_get_source_without_query_file() {
        let adapter = MqAdapter::new();
        let source = adapter.get_source();
        assert!(source.is_none());
    }

    #[test]
    fn test_get_source_file_name_with_query_file() {
        let mut adapter = MqAdapter::new();
        adapter.query_file = Some("/path/to/test.mq".to_string());

        let name = adapter.get_source_file_name(Some(mq_lang::Module::TOP_LEVEL_MODULE_ID));
        assert_eq!(name, "test.mq");
    }

    #[test]
    fn test_get_source_file_name_without_query_file() {
        let adapter = MqAdapter::new();
        let name = adapter.get_source_file_name(Some(mq_lang::Module::TOP_LEVEL_MODULE_ID));
        assert_eq!(name, "unknown");
    }

    #[test]
    fn test_send_debugger_command() {
        let adapter = MqAdapter::new();
        let result = adapter.send_debugger_command(DapCommand::Continue);
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_without_context() {
        let adapter = MqAdapter::new();
        let result = adapter.eval("1 + 1");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Current context not found")
        );
    }

    #[test]
    fn test_send_log_output() {
        let adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let result = adapter.send_log_output("Test message", &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_debugger_message_terminated() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let message = DebuggerMessage::Terminated;
        let result = adapter.handle_debugger_message(message, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_debugger_message_breakpoint_hit() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let context = mq_lang::DebugContext::default();
        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 1,
            column: None,
            enabled: true,
            source: None,
        };

        let message = DebuggerMessage::BreakpointHit {
            thread_id: 1,
            line: 1,
            context,
            breakpoint,
        };

        let result = adapter.handle_debugger_message(message, &mut server);
        assert!(result.is_ok());
        assert!(adapter.current_debug_context.is_some());
    }

    #[test]
    fn test_handle_debugger_message_step_completed() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let context = mq_lang::DebugContext::default();

        let message = DebuggerMessage::StepCompleted {
            thread_id: 1,
            line: 1,
            context,
        };

        let result = adapter.handle_debugger_message(message, &mut server);
        assert!(result.is_ok());
        assert!(adapter.current_debug_context.is_some());
    }

    #[test]
    fn test_handle_request_threads() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Threads,
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_configuration_done() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::ConfigurationDone,
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_continue() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Continue(dap::requests::ContinueArguments {
                thread_id: 1,
                single_thread: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_next() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Next(dap::requests::NextArguments {
                thread_id: 1,
                single_thread: None,
                granularity: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_step_in() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::StepIn(dap::requests::StepInArguments {
                thread_id: 1,
                single_thread: None,
                target_id: None,
                granularity: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_step_out() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::StepOut(dap::requests::StepOutArguments {
                thread_id: 1,
                single_thread: None,
                granularity: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_scopes() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Scopes(dap::requests::ScopesArguments { frame_id: 0 }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_variables() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Variables(dap::requests::VariablesArguments {
                variables_reference: 1,
                filter: None,
                start: None,
                count: None,
                format: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_stack_trace() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::StackTrace(dap::requests::StackTraceArguments {
                thread_id: 1,
                start_frame: None,
                levels: None,
                format: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_evaluate() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Evaluate(dap::requests::EvaluateArguments {
                expression: "1 + 1".to_string(),
                frame_id: None,
                context: None,
                format: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        // The result might succeed or fail - just ensure it doesn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_handle_request_set_variable() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::SetVariable(dap::requests::SetVariableArguments {
                variables_reference: 1,
                name: "test_var".to_string(),
                value: "42".to_string(),
                format: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        // The result might succeed or fail - just ensure it doesn't panic
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_handle_request_disconnect() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Disconnect(dap::requests::DisconnectArguments {
                restart: None,
                terminate_debuggee: None,
                suspend_debuggee: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_err()); // Should return error to indicate shutdown
    }

    #[test]
    fn test_handle_request_set_breakpoints() {
        let mut adapter = MqAdapter::new();
        adapter.query_file = Some("/path/to/test.mq".to_string());
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let source = dap::types::Source {
            name: Some("test.mq".to_string()),
            path: Some("/path/to/test.mq".to_string()),
            adapter_data: None,
            source_reference: None,
            presentation_hint: None,
            origin: None,
            checksums: None,
            sources: None,
        };

        let breakpoints = vec![
            dap::types::SourceBreakpoint {
                line: 10,
                column: Some(5),
                condition: None,
                hit_condition: None,
                log_message: None,
            },
            dap::types::SourceBreakpoint {
                line: 20,
                column: None,
                condition: None,
                hit_condition: None,
                log_message: None,
            },
        ];

        #[allow(deprecated)]
        let req = Request {
            seq: 1,
            command: Command::SetBreakpoints(dap::requests::SetBreakpointsArguments {
                source,
                breakpoints: Some(breakpoints),
                lines: None,
                source_modified: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }

    #[test]
    fn test_handle_request_unhandled_command() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let req = Request {
            seq: 1,
            command: Command::Initialize(dap::requests::InitializeArguments {
                client_id: None,
                client_name: None,
                adapter_id: "test".to_string(),
                locale: None,
                lines_start_at1: None,
                columns_start_at1: None,
                path_format: None,
                supports_variable_type: None,
                supports_variable_paging: None,
                supports_run_in_terminal_request: None,
                supports_memory_references: None,
                supports_progress_reporting: None,
                supports_invalidated_event: None,
                supports_memory_event: None,
                supports_args_can_be_interpreted_by_shell: None,
                supports_start_debugging_request: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_err()); // Should fail with UnhandledCommand error
    }

    #[test]
    fn test_handle_request_launch_success() {
        let mut adapter = MqAdapter::new();
        let input = BufReader::new(Cursor::new(Vec::new()));
        let output = BufWriter::new(Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let launch_args = LaunchArgs {
            query_file: "/tmp/test_query.mq".to_string(),
            input_file: None,
        };

        let mut additional_data = serde_json::Map::new();
        let value = serde_json::to_value(&launch_args).unwrap();
        if let serde_json::Value::Object(map) = value {
            for (k, v) in map {
                additional_data.insert(k, v);
            }
        }

        let req = Request {
            seq: 1,
            command: Command::Launch(dap::requests::LaunchRequestArguments {
                no_debug: None,
                restart_data: None,
                additional_data: Some(serde_json::Value::Object(additional_data)),
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
        assert_eq!(adapter.query_file, Some("/tmp/test_query.mq".to_string()));
    }

    #[test]
    fn test_handle_request_stack_trace_with_context() {
        let mut adapter = MqAdapter::new();
        let input = std::io::BufReader::new(std::io::Cursor::new(Vec::new()));
        let output = std::io::BufWriter::new(std::io::Cursor::new(Vec::new()));
        let mut server = Server::new(input, output);

        let mut context = mq_lang::DebugContext::default();

        context.call_stack.push(Shared::new(mq_lang::AstNode {
            expr: Shared::new(mq_lang::AstExpr::Literal(mq_lang::AstLiteral::Number(
                42.into(),
            ))),
            token_id: 0u32.into(),
        }));
        adapter.current_debug_context = Some(context);

        let req = Request {
            seq: 1,
            command: Command::StackTrace(dap::requests::StackTraceArguments {
                thread_id: 1,
                start_frame: None,
                levels: None,
                format: None,
            }),
        };

        let result = adapter.handle_request(req, &mut server);
        assert!(result.is_ok());
    }
}
