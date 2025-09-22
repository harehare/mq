use crossbeam_channel::{Receiver, Sender};
use tracing::{debug, error};

use crate::protocol::{DapCommand, DebuggerMessage};

/// DAP debugger handler that communicates with the DAP server
pub struct DapDebuggerHandler {
    message_tx: Sender<DebuggerMessage>,
    thread_id: i64,
}

impl std::fmt::Debug for DapDebuggerHandler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DapDebuggerHandler")
            .field("thread_id", &self.thread_id)
            .finish()
    }
}

impl DapDebuggerHandler {
    pub fn new(message_tx: Sender<DebuggerMessage>) -> Self {
        Self {
            message_tx,
            thread_id: 1, // Main thread ID
        }
    }
}

impl mq_lang::DebuggerHandler for DapDebuggerHandler {}

/// Wrapper to implement DebuggerHandler trait
#[derive(Debug)]
pub struct DapHandlerWrapper {
    handler: DapDebuggerHandler,
    command_rx: Receiver<DapCommand>,
}

impl DapHandlerWrapper {
    pub fn new(handler: DapDebuggerHandler, command_rx: Receiver<DapCommand>) -> Self {
        Self {
            handler,
            command_rx,
        }
    }
}

impl mq_lang::DebuggerHandler for DapHandlerWrapper {
    fn on_breakpoint_hit(
        &self,
        breakpoint: &mq_lang::Breakpoint,
        context: &mq_lang::DebugContext,
    ) -> mq_lang::DebuggerAction {
        debug!(line = breakpoint.line, "Breakpoint hit");

        // Send breakpoint hit message to DAP server
        let message = DebuggerMessage::BreakpointHit {
            thread_id: self.handler.thread_id,
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

    fn on_step(&self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        debug!(line = context.token.range.start.line + 1, "Step event");

        // Send step completed message to DAP server
        let message = DebuggerMessage::StepCompleted {
            thread_id: self.handler.thread_id,
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
