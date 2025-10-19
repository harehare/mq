use std::{
    fmt::Debug,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::SeqCst},
    },
};

use crossbeam_channel::{Receiver, Sender};
use mq_lang::DebuggerAction;
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
    pause_requested: Arc<AtomicBool>,
}

impl DapHandlerWrapper {
    pub fn new(handler: DapDebuggerHandler, command_rx: Receiver<DapCommand>) -> Self {
        Self {
            handler,
            command_rx,
            pause_requested: Arc::new(AtomicBool::new(false)),
        }
    }

    fn next_action(&self, command: DapCommand) -> DebuggerAction {
        match command {
            DapCommand::Continue => mq_lang::DebuggerAction::Continue,
            DapCommand::Next => mq_lang::DebuggerAction::Next,
            DapCommand::StepIn => mq_lang::DebuggerAction::StepInto,
            DapCommand::StepOut => mq_lang::DebuggerAction::FunctionExit,
            DapCommand::Pause => {
                // Set pause flag and step into next statement
                self.pause_requested.store(true, SeqCst);
                mq_lang::DebuggerAction::StepInto
            }
            DapCommand::Terminate => mq_lang::DebuggerAction::Quit,
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
            Ok(command) => self.next_action(command),
            Err(e) => {
                error!(error = %e, "Failed to receive command from DAP server");
                mq_lang::DebuggerAction::Continue
            }
        }
    }

    fn on_step(&self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        debug!(line = context.token.range.start.line + 1, "Step event");

        // Check if pause was requested
        let is_pause = self.pause_requested.swap(false, SeqCst);

        // Send appropriate message to DAP server
        let message = if is_pause {
            DebuggerMessage::Paused {
                thread_id: self.handler.thread_id,
                line: context.token.range.start.line as usize + 1,
                context: context.clone(),
            }
        } else {
            DebuggerMessage::StepCompleted {
                thread_id: self.handler.thread_id,
                line: context.token.range.start.line as usize + 1,
                context: context.clone(),
            }
        };

        if let Err(e) = self.handler.message_tx.send(message) {
            error!(error = %e, "Failed to send step message to DAP server");
            return mq_lang::DebuggerAction::Continue;
        }

        // Wait for command from DAP server
        match self.command_rx.recv() {
            Ok(command) => self.next_action(command),
            Err(e) => {
                error!(error = %e, "Failed to receive command from DAP server");
                mq_lang::DebuggerAction::Continue
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use mq_lang::DebuggerHandler;

    #[test]
    fn test_dap_debugger_handler_new() {
        let (tx, _rx) = unbounded::<DebuggerMessage>();
        let handler = DapDebuggerHandler::new(tx);

        assert_eq!(handler.thread_id, 1);
    }

    #[test]
    fn test_dap_debugger_handler_debug_format() {
        let (tx, _rx) = unbounded::<DebuggerMessage>();
        let handler = DapDebuggerHandler::new(tx);

        let debug_str = format!("{:?}", handler);
        assert!(debug_str.contains("DapDebuggerHandler"));
        assert!(debug_str.contains("thread_id"));
    }

    #[test]
    fn test_dap_handler_wrapper_new() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (_command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let debug_str = format!("{:?}", wrapper);
        assert!(debug_str.contains("DapHandlerWrapper"));
    }

    #[test]
    fn test_on_breakpoint_hit_continue() {
        let (message_tx, message_rx) = unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 10,
            column: Some(5),
            enabled: true,
            source: None,
        };
        let context = mq_lang::DebugContext::default();

        // Send continue command before calling on_breakpoint_hit
        command_tx.send(DapCommand::Continue).unwrap();

        let action = wrapper.on_breakpoint_hit(&breakpoint, &context);

        // Verify the correct action is returned
        assert!(matches!(action, mq_lang::DebuggerAction::Continue));

        // Verify message was sent
        let received_message = message_rx.try_recv().unwrap();
        match received_message {
            DebuggerMessage::BreakpointHit {
                thread_id, line, ..
            } => {
                assert_eq!(thread_id, 1);
                assert_eq!(line, 10);
            }
            _ => panic!("Expected BreakpointHit message"),
        }
    }

    #[test]
    fn test_on_breakpoint_hit_next() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 10,
            column: Some(5),
            enabled: true,
            source: None,
        };
        let context = mq_lang::DebugContext::default();

        command_tx.send(DapCommand::Next).unwrap();

        let action = wrapper.on_breakpoint_hit(&breakpoint, &context);
        assert!(matches!(action, mq_lang::DebuggerAction::Next));
    }

    #[test]
    fn test_on_breakpoint_hit_step_in() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 10,
            column: Some(5),
            enabled: true,
            source: None,
        };
        let context = mq_lang::DebugContext::default();

        command_tx.send(DapCommand::StepIn).unwrap();

        let action = wrapper.on_breakpoint_hit(&breakpoint, &context);
        assert!(matches!(action, mq_lang::DebuggerAction::StepInto));
    }

    #[test]
    fn test_on_breakpoint_hit_step_out() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 10,
            column: Some(5),
            enabled: true,
            source: None,
        };
        let context = mq_lang::DebugContext::default();

        command_tx.send(DapCommand::StepOut).unwrap();

        let action = wrapper.on_breakpoint_hit(&breakpoint, &context);
        assert!(matches!(action, mq_lang::DebuggerAction::FunctionExit));
    }

    #[test]
    fn test_on_breakpoint_hit_terminate() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 10,
            column: Some(5),
            enabled: true,
            source: None,
        };
        let context = mq_lang::DebugContext::default();

        command_tx.send(DapCommand::Terminate).unwrap();

        let action = wrapper.on_breakpoint_hit(&breakpoint, &context);
        assert!(matches!(action, mq_lang::DebuggerAction::Quit));
    }

    #[test]
    fn test_on_breakpoint_hit_recv_error() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (_command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let breakpoint = mq_lang::Breakpoint {
            id: 1,
            line: 10,
            column: Some(5),
            enabled: true,
            source: None,
        };
        let context = mq_lang::DebugContext::default();

        // Don't send any command, so recv will fail when command_tx is dropped
        drop(_command_tx);

        let action = wrapper.on_breakpoint_hit(&breakpoint, &context);
        assert!(matches!(action, mq_lang::DebuggerAction::Continue));
    }

    #[test]
    fn test_on_step_continue() {
        let (message_tx, message_rx) = unbounded::<DebuggerMessage>();
        let (command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let context = mq_lang::DebugContext::default();

        command_tx.send(DapCommand::Continue).unwrap();

        let action = wrapper.on_step(&context);
        assert!(matches!(action, mq_lang::DebuggerAction::Continue));

        // Verify message was sent
        let received_message = message_rx.try_recv().unwrap();
        match received_message {
            DebuggerMessage::StepCompleted {
                thread_id, line, ..
            } => {
                assert_eq!(thread_id, 1);
                assert_eq!(line, 2); // context.token.range.start.line is 1, so +1 = 2
            }
            _ => panic!("Expected StepCompleted message"),
        }
    }

    #[test]
    fn test_on_step_all_commands() {
        let commands_and_actions = vec![
            (DapCommand::Continue, mq_lang::DebuggerAction::Continue),
            (DapCommand::Next, mq_lang::DebuggerAction::Next),
            (DapCommand::StepIn, mq_lang::DebuggerAction::StepInto),
            (DapCommand::StepOut, mq_lang::DebuggerAction::FunctionExit),
            (DapCommand::Terminate, mq_lang::DebuggerAction::Quit),
        ];

        for (command, expected_action) in commands_and_actions {
            let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
            let (command_tx, command_rx) = unbounded::<DapCommand>();

            let handler = DapDebuggerHandler::new(message_tx);
            let wrapper = DapHandlerWrapper::new(handler, command_rx);

            let context = mq_lang::DebugContext::default();

            command_tx.send(command).unwrap();

            let action = wrapper.on_step(&context);

            // Compare debug strings since DebuggerAction doesn't implement PartialEq
            assert_eq!(format!("{:?}", action), format!("{:?}", expected_action));
        }
    }

    #[test]
    fn test_on_step_recv_error() {
        let (message_tx, _message_rx) = unbounded::<DebuggerMessage>();
        let (_command_tx, command_rx) = unbounded::<DapCommand>();

        let handler = DapDebuggerHandler::new(message_tx);
        let wrapper = DapHandlerWrapper::new(handler, command_rx);

        let context = mq_lang::DebugContext::default();

        // Don't send any command, so recv will fail when command_tx is dropped
        drop(_command_tx);

        let action = wrapper.on_step(&context);
        assert!(matches!(action, mq_lang::DebuggerAction::Continue));
    }
}
