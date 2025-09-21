use crossbeam_channel::{Receiver, Sender};
use dap::prelude::*;
use dap::responses::{
    ContinueResponse, EvaluateResponse, ScopesResponse, SetBreakpointsResponse,
    SetVariableResponse, StackTraceResponse, ThreadsResponse, VariablesResponse,
};
use dap::types::Breakpoint;
use mq_lang::Shared;
use std::io;
use std::path::PathBuf;
use std::thread;
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
        let engine = mq_lang::Engine::default();

        // Set up the debugger handler
        {
            let handler_boxed = Box::new(DapHandlerWrapper::new(dap_handler, command_rx));
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
    fn eval(&self, code: &str) -> DynResult<mq_lang::Values> {
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

                for breakpoint in &breakpoints_vec {
                    let id = self.engine.debugger().try_write().unwrap().add_breakpoint(
                        breakpoint.line as usize,
                        breakpoint.column.map(|bp| bp as usize),
                        if is_query_file {
                            None
                        } else {
                            args.source.name.clone()
                        },
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
            command => {
                return Err(Box::new(MqAdapterError::UnhandledCommand(command.clone())));
            }
        }
        Ok(())
    }
}
