use colored::*;
use miette::IntoDiagnostic;
use mq_lang::DebugContext;
use rustyline::{
    At, Cmd, CompletionType, Config, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers,
    Movement, Word,
    completion::Completer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    validate::{ValidationContext, ValidationResult, Validator},
};
use std::{borrow::Cow, cmp::max, fmt, rc::Rc};
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
        let config = Config::builder()
            .history_ignore_space(true)
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .color_mode(rustyline::ColorMode::Enabled)
            .build();
        let mut editor = Editor::with_config(config).into_diagnostic()?;
        let helper = DebuggerLineHelper;

        editor.set_helper(Some(helper));
        editor.bind_sequence(
            KeyEvent(KeyCode::Left, Modifiers::CTRL),
            Cmd::Move(Movement::BackwardWord(1, Word::Big)),
        );
        editor.bind_sequence(
            KeyEvent(KeyCode::Right, Modifiers::CTRL),
            Cmd::Move(Movement::ForwardWord(1, At::AfterEnd, Word::Big)),
        );

        let (start, snippet) =
            self.get_source_code_with_context(context, context.token.range.start.line as usize, 5);
        Self::print_source_code(start, context.token.range.start.line as usize + 1, snippet);

        loop {
            let readline = editor.readline("(mqdbg) ").into_diagnostic()?;

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
                    let mut engine = self.engine.switch_env(Rc::clone(&context.env));
                    let values = match engine.eval(&expr, vec![value].into_iter()) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Error evaluating expression: {}", e);
                            continue;
                        }
                    };

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
                    line_number.to_string().yellow().bold(),
                    line.yellow().bold(),
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
        context: &DebugContext,
        line: usize,
        context_lines: usize,
    ) -> (usize, Vec<String>) {
        let lines: Vec<&str> = context.source_code.lines().collect();
        if lines.is_empty() {
            return (0, vec![]);
        }
        let total_lines = lines.len();
        let start = line.saturating_sub(context_lines);
        let end = (line + context_lines + 1).min(total_lines);
        let snippet = lines[start..end].iter().map(|s| s.to_string()).collect();
        (start, snippet)
    }
}

/// Highlight mq syntax with keywords and commands
fn highlight_syntax(line: &str) -> Cow<str> {
    let mut result = line.to_string();

    let commands_pattern =
        r"^(backtrace|bt|step|s|next|n|finish|f|info|i|continue|c|help|quit|env|)\b";
    if let Ok(re) = regex_lite::Regex::new(commands_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_green().to_string()
            })
            .to_string();
    }

    let keywords_pattern = r"\b(def|let|if|elif|else|end|while|foreach|until|self|nodes|fn|break|continue|include|true|false|None)\b";
    if let Ok(re) = regex_lite::Regex::new(keywords_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_blue().to_string()
            })
            .to_string();
    }

    // Highlight strings
    if let Ok(re) = regex_lite::Regex::new(r#""([^"\\]|\\.)*""#) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_green().to_string()
            })
            .to_string();
    }

    // Highlight numbers
    if let Ok(re) = regex_lite::Regex::new(r"\b\d+\b") {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_magenta().to_string()
            })
            .to_string();
    }

    // Highlight operators (after other highlighting to avoid conflicts)
    let operators_pattern = r"(->|<=|>=|==|!=|&&|[=|:;?!+\-*/%<>])";
    if let Ok(re) = regex_lite::Regex::new(operators_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_yellow().to_string()
            })
            .to_string();
    }

    Cow::Owned(result)
}

pub struct DebuggerLineHelper;

impl Hinter for DebuggerLineHelper {
    type Hint = String;
}

impl Helper for DebuggerLineHelper {}
impl Completer for DebuggerLineHelper {
    type Candidate = String;
}

impl Highlighter for DebuggerLineHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        prompt.cyan().to_string().into()
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        true
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        highlight_syntax(line)
    }
}

impl Validator for DebuggerLineHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> Result<ValidationResult, ReadlineError> {
        let input = ctx.input();
        if input.is_empty() || input.ends_with("\n") || input.starts_with("/") {
            return Ok(ValidationResult::Valid(None));
        }

        if mq_lang::parse_recovery(input).1.has_errors() {
            Ok(ValidationResult::Incomplete)
        } else {
            Ok(ValidationResult::Valid(None))
        }
    }

    fn validate_while_typing(&self) -> bool {
        false
    }
}
