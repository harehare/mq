use colored::*;
use miette::IntoDiagnostic;
use rustyline::{At, Cmd, DefaultEditor, KeyCode, KeyEvent, Modifiers, Movement, Word};
use std::{cmp::max, fmt};
use strum::IntoEnumIterator;

#[derive(Debug, Clone, strum::EnumIter)]
pub enum Command {
    Backtrace,
    Info,
    Eval(String),
    Help,
    Continue,
    Step,
    Next,
    Finish,
    Quit,
    Error(String),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Backtrace => write!(f, "backtrace"),
            Command::Eval(expr) => write!(f, "eval {}", expr),
            Command::Error(e) => write!(f, "error {}", e),
            Command::Help => write!(f, "help"),
            Command::Continue => write!(f, "continue"),
            Command::Step => write!(f, "step"),
            Command::Info => write!(f, "info"),
            Command::Next => write!(f, "next"),
            Command::Finish => write!(f, "finish"),
            Command::Quit => write!(f, "quit"),
        }
    }
}

impl Command {
    pub fn help(&self) -> String {
        match self {
            Command::Backtrace => "Print the current backtrace".to_string(),
            Command::Eval(_) | Command::Error(_) => "".to_string(),
            Command::Help => "Print command help".to_string(),
            Command::Continue => "Continue execution".to_string(),
            Command::Info => "Print information about the current context".to_string(),
            Command::Step => "Step into the next function call".to_string(),
            Command::Next => "Step over the next function call".to_string(),
            Command::Finish => "Finish execution and return to the caller".to_string(),
            Command::Quit => "Quit evaluation and exit".to_string(),
        }
    }
}

impl From<String> for Command {
    fn from(s: String) -> Self {
        match s
            .as_str()
            .split_whitespace()
            .collect::<Vec<&str>>()
            .as_slice()
        {
            ["backtrace"] | ["bt"] => Command::Backtrace,
            ["step"] | ["s"] => Command::Step,
            ["next"] | ["n"] => Command::Next,
            ["finish"] | ["f"] => Command::Finish,
            ["info"] | ["i"] => Command::Info,
            ["continue"] | ["c"] => Command::Continue,
            ["help"] => Command::Help,
            ["quit"] => Command::Quit,
            ["env"] => Command::Error("Use 'info' command instead of 'env'".to_string()),
            ["eval", rest @ ..] | ["e", rest @ ..] => {
                let expr = rest.join(" ");
                if expr.is_empty() {
                    Command::Error("No expression provided for eval".to_string())
                } else {
                    Command::Eval(expr)
                }
            }
            _ => Command::Eval(s),
        }
    }
}

#[derive(Debug)]
pub struct DebuggerHandler {
    engine: mq_lang::Engine,
}

#[cfg(feature = "debugger")]
impl mq_lang::DebuggerHandler for DebuggerHandler {
    // Called when a breakpoint is hit.
    fn on_breakpoint_hit(
        &mut self,
        _breakpoint: &mq_lang::Breakpoint,
        context: &mq_lang::DebugContext,
    ) -> mq_lang::DebuggerAction {
        self.run_debug(context)
            .unwrap_or(mq_lang::DebuggerAction::Continue)
    }

    /// Called when stepping through execution.
    fn on_step(&mut self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        self.run_debug(context)
            .unwrap_or(mq_lang::DebuggerAction::Continue)
    }
}

impl DebuggerHandler {
    pub fn new(engine: mq_lang::Engine) -> Self {
        Self { engine }
    }

    pub fn run_debug(
        &mut self,
        context: &mq_lang::DebugContext,
    ) -> miette::Result<mq_lang::DebuggerAction> {
        let mut editor = DefaultEditor::new().into_diagnostic()?;
        editor.bind_sequence(
            KeyEvent(KeyCode::Left, Modifiers::CTRL),
            Cmd::Move(Movement::BackwardWord(1, Word::Big)),
        );
        editor.bind_sequence(
            KeyEvent(KeyCode::Right, Modifiers::CTRL),
            Cmd::Move(Movement::ForwardWord(1, At::AfterEnd, Word::Big)),
        );

        let (start, snippet) = self.get_source_code_with_context(
            context.token.module_id,
            context.token.range.start.line as usize,
            5,
        );
        Self::print_source_code(start, context.token.range.start.line as usize + 1, snippet);

        loop {
            let prompt = format!("{}", "(mqdbg) ".yellow());
            let readline = editor.readline(&prompt).into_diagnostic()?;

            if readline.trim().is_empty() {
                continue;
            }

            let command = Command::from(readline);
            match command {
                Command::Backtrace => {
                    let bt = context
                        .call_stack
                        .iter()
                        .filter_map(|frame| {
                            let range = self.engine.token_arena().borrow()[frame.token_id]
                                .range
                                .clone();

                            match &*frame.expr {
                                mq_lang::AstExpr::Call(ident, _) => Some(format!(
                                    "{} at {}:{}",
                                    ident,
                                    range.start.line + 1,
                                    range.start.column + 1
                                )),
                                _ => None,
                            }
                        })
                        .collect::<Vec<String>>();

                    if !bt.is_empty() {
                        println!("{}", bt.join("\n"));
                    }
                }
                Command::Info => {
                    println!("{}", context.env.borrow());
                }
                Command::Eval(expr) => {
                    let value: mq_lang::Value = context.current_value.clone().into();
                    let values = self
                        .engine
                        .eval(&expr, vec![value].into_iter())
                        .map_err(|e| {
                            miette::miette!("Failed to evaluate expression '{}': {}", expr, e)
                        })?;

                    let lines = values
                        .values()
                        .iter()
                        .filter_map(|runtime_value| {
                            if runtime_value.is_none() {
                                return Some("None".to_string());
                            }

                            let s = runtime_value.to_string();
                            if s.is_empty() { None } else { Some(s) }
                        })
                        .collect::<Vec<_>>();

                    if !lines.is_empty() {
                        println!("{}", lines.join("\n"))
                    }
                }
                Command::Help => {
                    let commands: Vec<String> = Command::iter()
                        .filter_map(|c| {
                            if matches!(c, Command::Eval(_)) {
                                None
                            } else {
                                Some(c.help().to_string())
                            }
                        })
                        .collect::<Vec<_>>();

                    println!("{}", commands.join("\n"))
                }
                Command::Continue => return Ok(mq_lang::DebuggerAction::Continue),
                Command::Step => return Ok(mq_lang::DebuggerAction::StepOver),
                Command::Next => return Ok(mq_lang::DebuggerAction::Next),
                Command::Finish => return Ok(mq_lang::DebuggerAction::FunctionExit),
                Command::Quit => return Ok(mq_lang::DebuggerAction::Terminate),
                Command::Error(e) => {
                    eprintln!("{}", e);
                    continue;
                }
            }
        }
    }

    fn print_source_code(start: usize, current_line: usize, snippet: Vec<String>) {
        // The width of the line number column is increased to account for the "=>" marker
        let line_number_width = max(current_line.to_string().len() + 4, 7);
        let display_source_code = snippet.iter().enumerate().map(|(i, line)| {
            let line_number = start + i + 1;

            if line_number == current_line {
                format!(
                    "=>{:>line_number_width$}| {}",
                    line_number.to_string().bold(),
                    line.bold(),
                    line_number_width = line_number_width - 2
                )
            } else {
                format!(
                    "{:>line_number_width$}| {}",
                    line_number.to_string().blue(),
                    line
                )
            }
        });

        println!("{}", display_source_code.collect::<Vec<_>>().join("\n"));
    }

    fn get_source_code_with_context(
        &self,
        module_id: mq_lang::ModuleId,
        line: usize,
        context: usize,
    ) -> (usize, Vec<String>) {
        let source = self
            .engine
            .get_source_code_for_debug(module_id)
            .unwrap_or_default();
        let lines: Vec<&str> = source.lines().collect();
        if lines.is_empty() {
            return (0, vec![]);
        }
        let total_lines = lines.len();
        let start = line.saturating_sub(context);
        let end = (line + context + 1).min(total_lines);
        let snippet = lines[start..end].iter().map(|s| s.to_string()).collect();
        (start, snippet)
    }
}
