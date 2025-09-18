use itertools::Itertools;

use super::runtime_value::RuntimeValue;
use crate::ast::node as ast;
use crate::eval::Evaluator;
use crate::eval::env::Env;
use crate::{Shared, SharedCell, Token};

use std::{collections::HashSet, fmt::Debug};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Default, Hash)]
pub enum DebuggerCommand {
    /// Continue normal execution.
    #[default]
    Continue,
    /// Step into the next expression, diving into functions.
    StepInto,
    /// Run to the next expression or statement, stepping over functions.
    StepOver,
    /// Run to the next statement, skipping over functions.
    Next,
    /// Run to the end of the current function call.
    FunctionExit,
    /// Quit the debugger.
    Quit,
}

/// Represents a breakpoint in the debugger.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Breakpoint {
    /// Unique identifier for the breakpoint
    pub id: usize,
    /// Line number where the breakpoint is set
    pub line: usize,
    /// Column number where the breakpoint is set (optional)
    pub column: Option<usize>,
    /// Whether the breakpoint is enabled
    pub enabled: bool,
}

/// Debugger state and context information
#[derive(Debug, Clone)]
pub struct DebugContext {
    /// Current runtime value being evaluated
    pub current_value: RuntimeValue,
    /// Current AST node being evaluated
    pub current_node: Shared<ast::Node>,
    /// Current token being evaluated
    pub token: Shared<Token>,
    /// Call stack of AST nodes representing the current execution path
    pub call_stack: Vec<Shared<ast::Node>>,
    /// Current evaluation environment info
    pub env: Shared<SharedCell<Env>>,
    /// Source code being executed
    pub source_code: String,
}

impl Default for DebugContext {
    fn default() -> Self {
        Self {
            current_value: RuntimeValue::NONE,
            current_node: Shared::new(ast::Node {
                token_id: crate::ast::TokenId::new(0),
                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(0.0.into()))),
            }),
            token: Shared::new(Token {
                kind: crate::TokenKind::Eof,
                range: crate::Range::default(),
                module_id: crate::eval::module::ModuleId::new(0),
            }),
            call_stack: Vec::new(),
            env: Shared::new(SharedCell::new(Env::default())),
            source_code: String::new(),
        }
    }
}

/// The main debugger struct that manages breakpoints and execution state
#[derive(Debug)]
pub struct Debugger {
    /// Set of active breakpoints
    breakpoints: HashSet<Breakpoint>,
    /// Call stack of AST nodes representing the current execution path
    call_stack: Vec<Shared<ast::Node>>,
    /// Next breakpoint ID to assign
    next_breakpoint_id: usize,
    /// Current debugger command
    current_command: DebuggerCommand,
    /// Whether the debugger is currently active
    active: bool,
    /// Current call stack depth for step operations
    step_depth: Option<usize>,
    /// Stores conditional expressions for breakpoints
    handler: Box<dyn DebuggerHandler>,
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}

impl Debugger {
    /// Create a new debugger instance
    pub fn new() -> Self {
        Self {
            breakpoints: HashSet::new(),
            call_stack: Vec::new(),
            next_breakpoint_id: 1,
            current_command: DebuggerCommand::Continue,
            active: false,
            step_depth: None,
            handler: Box::new(DefaultDebuggerHandler {}),
        }
    }

    pub fn set_handler(&mut self, handler: Box<dyn DebuggerHandler>) {
        self.handler = handler;
    }

    /// Activate the debugger
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate the debugger
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if the debugger is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Add a breakpoint at the specified line
    pub fn add_breakpoint(&mut self, line: usize, column: Option<usize>) -> usize {
        let breakpoint = Breakpoint {
            id: self.next_breakpoint_id,
            line,
            column,
            enabled: true,
        };
        let id = breakpoint.id;
        self.breakpoints.insert(breakpoint);
        self.next_breakpoint_id += 1;
        id
    }

    pub fn push_call_stack(&mut self, node: Shared<ast::Node>) {
        self.call_stack.push(node);
    }

    pub fn pop_call_stack(&mut self) {
        self.call_stack.pop();
    }

    pub fn current_call_stack(&self) -> Vec<Shared<ast::Node>> {
        self.call_stack.clone()
    }

    /// Remove a breakpoint by ID
    pub fn remove_breakpoint(&mut self, id: usize) -> bool {
        self.breakpoints.retain(|bp| bp.id != id);
        true
    }

    /// List all breakpoints
    pub fn list_breakpoints(&self) -> Vec<&Breakpoint> {
        self.breakpoints.iter().collect()
    }

    /// Set the current debugger command
    pub fn set_command(&mut self, command: DebuggerCommand) {
        self.current_command = command;
        match command {
            DebuggerCommand::StepInto | DebuggerCommand::StepOver | DebuggerCommand::Next => {
                self.step_depth = None;
            }
            DebuggerCommand::FunctionExit | DebuggerCommand::Quit => {
                // step_depth will be set to current depth - 1 when stepping
            }
            DebuggerCommand::Continue => {
                self.step_depth = None;
            }
        }
    }

    /// Check if execution should pause at the current location and handle callbacks
    pub fn should_break(
        &mut self,
        context: &DebugContext,
        token: Shared<Token>,
    ) -> (bool, Option<DebuggerAction>) {
        if !self.active {
            return (false, None);
        }

        let line = token.range.start.line as usize;
        let column = token.range.start.column;

        if let Some(breakpoint) = self.find_active_breakpoint(line, column) {
            return self.breakpoint_hit(context, &breakpoint);
        }

        let should_break = match self.current_command {
            DebuggerCommand::Continue => false,
            DebuggerCommand::Quit => {
                self.deactivate();
                false
            }
            DebuggerCommand::StepInto => {
                self.current_command = DebuggerCommand::Continue;
                true
            }
            DebuggerCommand::StepOver => {
                if let Some(step_depth) = self.step_depth {
                    if context.call_stack.len() <= step_depth {
                        self.current_command = DebuggerCommand::Continue;
                        self.step_depth = None;
                        true
                    } else {
                        false
                    }
                } else {
                    self.step_depth = Some(context.call_stack.len());
                    self.current_command = DebuggerCommand::Continue;
                    true
                }
            }
            DebuggerCommand::Next => {
                if let Some(step_depth) = self.step_depth {
                    if context.call_stack.len() <= step_depth {
                        self.current_command = DebuggerCommand::Continue;
                        self.step_depth = None;
                        true
                    } else {
                        false
                    }
                } else {
                    self.step_depth = Some(context.call_stack.len());
                    self.current_command = DebuggerCommand::Continue;
                    true
                }
            }
            DebuggerCommand::FunctionExit => {
                // Break when we exit the current function
                if let Some(step_depth) = self.step_depth {
                    if context.call_stack.len() < step_depth {
                        self.current_command = DebuggerCommand::Continue;
                        self.step_depth = None;
                        true
                    } else {
                        false
                    }
                } else {
                    // Set target depth to current - 1
                    self.step_depth = Some(context.call_stack.len().saturating_sub(1));
                    false
                }
            }
        };

        if should_break {
            let action = self.handler.on_step(context);

            self.handle_debugger_action(action.clone());
            self.current_command = action.clone().into();
            (true, Some(action))
        } else {
            (false, None)
        }
    }

    /// Called when execution hits a breakpoint.
    pub fn breakpoint_hit(
        &mut self,
        context: &DebugContext,
        breakpoint: &Breakpoint,
    ) -> (bool, Option<DebuggerAction>) {
        if !self.active {
            return (false, None);
        }

        let action = self.handler.on_breakpoint_hit(breakpoint, context);

        self.handle_debugger_action(action.clone());
        self.current_command = action.clone().into();
        (true, Some(action))
    }

    fn handle_debugger_action(&mut self, action: DebuggerAction) {
        match action {
            DebuggerAction::Breakpoint(Some(line_no)) => {
                self.add_breakpoint(line_no, None);
            }
            DebuggerAction::Breakpoint(None) => {
                println!(
                    "Breakpoints:\n{}",
                    self.breakpoints
                        .iter()
                        .sorted_by_key(|bp| (bp.line, bp.column))
                        .map(|bp| {
                            format!(
                                "  [{}] {}:{}{}",
                                bp.id,
                                bp.line,
                                bp.column
                                    .map(|col| col.to_string())
                                    .unwrap_or_else(|| "-".to_string()),
                                if bp.enabled {
                                    " (enabled)"
                                } else {
                                    " (disabled)"
                                },
                            )
                        })
                        .join("\n")
                );
            }
            DebuggerAction::Clear(Some(breakpoint_id)) => {
                self.remove_breakpoint(breakpoint_id);
            }
            DebuggerAction::Clear(None) => {
                self.clear_breakpoints();
            }
            DebuggerAction::Continue
            | DebuggerAction::Next
            | DebuggerAction::StepInto
            | DebuggerAction::StepOver
            | DebuggerAction::FunctionExit
            | DebuggerAction::Quit => {}
        }
    }

    fn find_active_breakpoint(&self, line: usize, column: usize) -> Option<Breakpoint> {
        for breakpoint in &self.breakpoints {
            if !breakpoint.enabled {
                continue;
            }

            if breakpoint.line == line {
                if let Some(bp_column) = breakpoint.column {
                    if bp_column != column {
                        continue;
                    }
                }

                return Some(breakpoint.clone());
            }
        }

        None
    }

    /// Get the current debugger command
    pub fn current_command(&self) -> DebuggerCommand {
        self.current_command
    }

    /// Clear all breakpoints
    pub fn clear_breakpoints(&mut self) {
        self.breakpoints.clear();
    }
}

type LineNo = usize;
type BreakpointId = usize;

/// Result of debugger callback execution
#[derive(Debug, Clone, PartialEq)]
pub enum DebuggerAction {
    /// Set a breakpoint at a specific location.
    Breakpoint(Option<LineNo>),
    /// Continue normal execution
    Continue,
    /// Clear breakpoints at a specific location
    Clear(Option<BreakpointId>),
    /// Step into next expression
    StepInto,
    /// Step over current expression
    StepOver,
    /// Step to next statement
    Next,
    /// Run until function exit
    FunctionExit,
    /// Quit the debugger
    Quit,
}

impl From<DebuggerCommand> for DebuggerAction {
    fn from(command: DebuggerCommand) -> Self {
        match command {
            DebuggerCommand::Continue => DebuggerAction::Continue,
            DebuggerCommand::StepInto => DebuggerAction::StepInto,
            DebuggerCommand::StepOver => DebuggerAction::StepOver,
            DebuggerCommand::Next => DebuggerAction::Next,
            DebuggerCommand::FunctionExit => DebuggerAction::FunctionExit,
            DebuggerCommand::Quit => DebuggerAction::Quit,
        }
    }
}

impl From<DebuggerAction> for DebuggerCommand {
    fn from(action: DebuggerAction) -> Self {
        match action {
            DebuggerAction::Breakpoint(_) => DebuggerCommand::Continue,
            DebuggerAction::Clear(_) => DebuggerCommand::Continue,
            DebuggerAction::Continue => DebuggerCommand::Continue,
            DebuggerAction::StepInto => DebuggerCommand::StepInto,
            DebuggerAction::StepOver => DebuggerCommand::StepOver,
            DebuggerAction::Next => DebuggerCommand::Next,
            DebuggerAction::FunctionExit => DebuggerCommand::FunctionExit,
            DebuggerAction::Quit => DebuggerCommand::Quit,
        }
    }
}

pub trait DebuggerHandler: std::fmt::Debug {
    // Called when a breakpoint is hit.
    fn on_breakpoint_hit(
        &mut self,
        _breakpoint: &Breakpoint,
        _context: &DebugContext,
    ) -> DebuggerAction {
        // Default behavior: continue execution
        DebuggerAction::Continue
    }

    /// Called when stepping through execution.
    fn on_step(&mut self, _context: &DebugContext) -> DebuggerAction {
        DebuggerAction::Continue
    }
}

#[derive(Debug, Default)]
pub struct DefaultDebuggerHandler;

impl DebuggerHandler for DefaultDebuggerHandler {}

impl Evaluator {
    pub fn debugger(&self) -> Shared<SharedCell<Debugger>> {
        Shared::clone(&self.debugger)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{Arena, Range, TokenKind, ast::TokenId, eval::module::ModuleId};

    use super::*;

    fn make_token(line: usize, column: usize) -> Shared<Token> {
        Shared::new(Token {
            kind: TokenKind::Ident("dummy".into()),
            range: Range {
                start: crate::Position {
                    line: line as u32,
                    column,
                },
                end: crate::Position {
                    line: line as u32,
                    column: column + 1,
                },
            },
            module_id: ModuleId::new(0),
        })
    }

    fn make_node(token_id: TokenId) -> Shared<ast::Node> {
        Shared::new(ast::Node {
            token_id,
            expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(42.0.into()))),
        })
    }

    fn make_debug_context(line: usize, column: usize) -> DebugContext {
        let mut arena = Arena::new(10);
        let token = make_token(line, column);
        let token_id = arena.alloc(Shared::clone(&token));
        let node = make_node(token_id);
        let env = Shared::new(SharedCell::new(Env::default()));
        DebugContext {
            current_value: RuntimeValue::NONE,
            current_node: node,
            token: Shared::clone(&token),
            call_stack: Vec::new(),
            env,
            source_code: String::new(),
        }
    }

    #[rstest]
    #[case(10, Some(5), 10, Some(5), true)]
    #[case(10, None, 10, Some(5), true)]
    #[case(10, None, 11, Some(5), false)]
    #[case(10, Some(5), 10, None, false)]
    #[case(10, Some(5), 11, Some(5), false)]
    #[case(10, None, 11, None, false)]
    fn test_breakpoint_matching(
        #[case] bp_line: usize,
        #[case] bp_col: Option<usize>,
        #[case] node_line: usize,
        #[case] node_col: Option<usize>,
        #[case] should_match: bool,
    ) {
        let mut dbg = Debugger::new();
        dbg.activate();
        dbg.add_breakpoint(bp_line, bp_col);

        let col = node_col.unwrap_or(0);
        let ctx = make_debug_context(node_line, col);
        let (hit, _) = dbg.should_break(&ctx, make_token(node_line, node_col.unwrap_or(0)));
        assert_eq!(hit, should_match);
    }

    #[rstest]
    #[case(DebuggerCommand::Continue, false)]
    #[case(DebuggerCommand::StepInto, true)]
    #[case(DebuggerCommand::StepOver, true)]
    #[case(DebuggerCommand::Next, true)]
    #[case(DebuggerCommand::FunctionExit, false)]
    fn test_should_break_on_command(#[case] command: DebuggerCommand, #[case] should_break: bool) {
        let mut dbg = Debugger::new();
        dbg.activate();
        dbg.set_command(command);

        let ctx = make_debug_context(1, 1);
        let (hit, _) = dbg.should_break(&ctx, make_token(1, 1));
        assert_eq!(hit, should_break);
    }

    #[rstest]
    #[case(
        DebuggerCommand::StepOver,
        None,
        vec![],
        true,
        "StepOver: step_depth None, call_stack empty"
    )]
    #[case(
        DebuggerCommand::StepOver,
        Some(1),
        vec![0, 1],
        false,
        "StepOver: step_depth Some(1), call_stack.len()=2"
    )]
    #[case(
        DebuggerCommand::StepOver,
        Some(2),
        vec![0],
        true,
        "StepOver: step_depth Some(2), call_stack.len()=1"
    )]
    #[case(
        DebuggerCommand::Next,
        None,
        vec![],
        true,
        "Next: step_depth None, call_stack empty"
    )]
    #[case(
        DebuggerCommand::Next,
        Some(1),
        vec![0, 1],
        false,
        "Next: step_depth Some(1), call_stack.len()=2"
    )]
    #[case(
        DebuggerCommand::Next,
        Some(2),
        vec![0],
        true,
        "Next: step_depth Some(2), call_stack.len()=1"
    )]
    #[case(
        DebuggerCommand::FunctionExit,
        None,
        vec![],
        false,
        "FunctionExit: step_depth None, call_stack empty"
    )]
    #[case(
        DebuggerCommand::FunctionExit,
        Some(2),
        vec![0],
        true,
        "FunctionExit: step_depth Some(2), call_stack.len()=1"
    )]
    #[case(
        DebuggerCommand::FunctionExit,
        Some(1),
        vec![0, 1],
        false,
        "FunctionExit: step_depth Some(1), call_stack.len()=2"
    )]
    fn test_should_break_with_step_depth(
        #[case] command: DebuggerCommand,
        #[case] step_depth: Option<usize>,
        #[case] call_stack_indices: Vec<usize>,
        #[case] expected_hit: bool,
        #[case] _desc: &str,
    ) {
        let mut dbg = Debugger::new();
        dbg.activate();
        dbg.set_command(command);
        dbg.step_depth = step_depth;

        let ctx = make_debug_context(1, 1);
        let mut ctx = ctx;
        ctx.call_stack = call_stack_indices
            .into_iter()
            .map(|i| make_node(TokenId::new(i as u32)))
            .collect();

        let (hit, _) = dbg.should_break(&ctx, make_token(1, 1));
        assert_eq!(hit, expected_hit);
    }

    #[test]
    fn test_add_and_remove_breakpoint() {
        let mut dbg = Debugger::new();
        let id = dbg.add_breakpoint(1, Some(2));
        assert_eq!(dbg.list_breakpoints().len(), 1);
        assert!(dbg.remove_breakpoint(id));
        assert_eq!(dbg.list_breakpoints().len(), 0);
    }

    #[test]
    fn test_clear_breakpoints() {
        let mut dbg = Debugger::new();
        dbg.add_breakpoint(1, None);
        dbg.add_breakpoint(2, Some(3));
        assert_eq!(dbg.list_breakpoints().len(), 2);
        dbg.clear_breakpoints();
        assert_eq!(dbg.list_breakpoints().len(), 0);
    }

    #[test]
    fn test_debugger_activate_deactivate() {
        let mut dbg = Debugger::new();
        assert!(!dbg.is_active());
        dbg.activate();
        assert!(dbg.is_active());
        dbg.deactivate();
        assert!(!dbg.is_active());
    }
}
