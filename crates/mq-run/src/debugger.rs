use colored::*;
use miette::IntoDiagnostic;
use mq_lang::{DebugContext, Shared};
use rustyline::{
    At, Cmd, CompletionType, Config, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers, Movement, Word,
    completion::Completer,
    error::ReadlineError,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    validate::{ValidationContext, ValidationResult, Validator},
};
use std::{borrow::Cow, cmp::max, fmt, fs};
use strum::IntoEnumIterator;

type LineNo = usize;
type BreakpointId = usize;

#[derive(Debug, Clone, PartialEq, strum::EnumIter)]
pub enum Command {
    Backtrace,
    Breakpoint(Option<LineNo>),
    Continue,
    Clear(Option<BreakpointId>),
    Error(String),
    Eval(String),
    Finish,
    Help,
    Info,
    List,
    LongList,
    Next,
    Quit,
    Step,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Backtrace => write!(f, "backtrace"),
            Command::Breakpoint(Some(line)) => write!(f, "breakpoint {}", line),
            Command::Breakpoint(None) => write!(f, "breakpoint"),
            Command::Continue => write!(f, "continue"),
            Command::Clear(Some(id)) => write!(f, "clear {}", id),
            Command::Clear(None) => write!(f, "clear"),
            Command::Error(e) => write!(f, "error {}", e),
            Command::Eval(expr) => write!(f, "eval {}", expr),
            Command::Finish => write!(f, "finish"),
            Command::Help => write!(f, "help"),
            Command::Info => write!(f, "info"),
            Command::List => write!(f, "list"),
            Command::LongList => write!(f, "long-list"),
            Command::Next => write!(f, "next"),
            Command::Quit => write!(f, "quit"),
            Command::Step => write!(f, "step"),
        }
    }
}

impl Command {
    pub fn help(&self) -> String {
        match self {
            Command::Backtrace => {
                format!("{:<20}{}", "backtrace or bt", "Print the current backtrace")
            }
            Command::Breakpoint(_) => format!("{:<20}{}", "b[reakpoint]", "Set a breakpoint at the specified line"),
            Command::Continue => {
                format!("{:<20}{}", "c[ontinue]", "Continue execution")
            }
            Command::Clear(_) => format!("{:<20}{}", "cl[ear]", "Clear breakpoints at a specific identifier"),
            Command::Eval(_) | Command::Error(_) => "".to_string(),
            Command::Finish => format!("{:<20}{}", "f[inish]", "Finish execution and return to the caller"),
            Command::Help => format!("{:<20}{}", "h[elp]", "Print command help"),
            Command::Info => format!("{:<20}{}", "i[nfo]", "Print information about the current context"),
            Command::List => format!("{:<20}{}", "l[ist]", "List source code around the current line"),
            Command::LongList => {
                format!("{:<20}{}", "long-list or ll", "List all source code lines")
            }
            Command::Next => {
                format!("{:<20}{}", "n[ext]", "Step over the next function call")
            }
            Command::Quit => {
                format!("{:<20}{}", "q[uit]", "Quit evaluation and exit")
            }
            Command::Step => {
                format!("{:<20}{}", "s[tep]", "Step into the next function call")
            }
        }
    }
}

impl From<String> for Command {
    fn from(s: String) -> Self {
        match s.as_str().split_whitespace().collect::<Vec<&str>>().as_slice() {
            ["backtrace"] | ["bt"] => Command::Backtrace,
            ["breakpoint", line] | ["b", line] => Command::Breakpoint(line.parse().ok()),
            ["breakpoint"] | ["b"] => Command::Breakpoint(None),
            ["continue"] | ["c"] => Command::Continue,
            ["clear", line] | ["cl", line] => Command::Clear(line.parse().ok()),
            ["clear"] | ["cl"] => Command::Clear(None),
            ["env"] => Command::Error("Use 'info' command instead of 'env'".to_string()),
            ["eval", rest @ ..] | ["e", rest @ ..] => {
                let expr = rest.join(" ");
                if expr.is_empty() {
                    Command::Error("No expression provided for eval".to_string())
                } else {
                    Command::Eval(expr)
                }
            }
            ["finish"] | ["f"] => Command::Finish,
            ["help"] => Command::Help,
            ["info"] | ["i"] => Command::Info,
            ["list"] | ["l"] => Command::List,
            ["long-list"] | ["ll"] => Command::LongList,
            ["next"] | ["n"] => Command::Next,
            ["quit"] | ["q"] => Command::Quit,
            ["step"] | ["s"] => Command::Step,
            _ => Command::Eval(s),
        }
    }
}

#[derive(Debug)]
pub struct DebuggerHandler {
    engine: mq_lang::DefaultEngine,
}

#[cfg(feature = "debugger")]
impl mq_lang::DebuggerHandler for DebuggerHandler {
    // Called when a breakpoint is hit.
    fn on_breakpoint_hit(
        &self,
        _breakpoint: &mq_lang::Breakpoint,
        context: &mq_lang::DebugContext,
    ) -> mq_lang::DebuggerAction {
        self.run_debug(context).unwrap_or(mq_lang::DebuggerAction::Continue)
    }

    /// Called when stepping through execution.
    fn on_step(&self, context: &mq_lang::DebugContext) -> mq_lang::DebuggerAction {
        self.run_debug(context).unwrap_or(mq_lang::DebuggerAction::Continue)
    }
}

pub fn config_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("MQ_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("mq")))
}

impl DebuggerHandler {
    pub fn new(engine: mq_lang::DefaultEngine) -> Self {
        Self { engine }
    }

    pub fn run_debug(&self, context: &mq_lang::DebugContext) -> miette::Result<mq_lang::DebuggerAction> {
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

        let config_dir = config_dir();

        if let Some(config_dir) = &config_dir {
            let history = config_dir.join("dbg_history.txt");
            fs::create_dir_all(config_dir).ok();
            if editor.load_history(&history).is_err() {
                println!("No previous history.");
            }
        }

        let (start, snippet) = self.get_source_code_with_context(context, context.token.range.start.line as usize, 5);
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
                            let range = self.engine.token_arena().read().unwrap()[frame.token_id].range;

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
                Command::Breakpoint(line_opt) => {
                    return Ok(mq_lang::DebuggerAction::Breakpoint(line_opt));
                }
                Command::Clear(line_opt) => {
                    return Ok(mq_lang::DebuggerAction::Clear(line_opt));
                }
                Command::List => {
                    let (start, snippet) =
                        self.get_source_code_with_context(context, context.token.range.start.line as usize, 5);
                    Self::print_source_code(start, context.token.range.start.line as usize + 1, snippet);
                }
                Command::LongList => {
                    let lines: Vec<String> = context.source.code.lines().map(|s| s.to_string()).collect();
                    Self::print_source_code(0, context.token.range.start.line as usize + 1, lines);
                }
                Command::Info => {
                    println!(
                        "{}",
                        context
                            .env
                            .read()
                            .unwrap()
                            .get_local_variables()
                            .iter()
                            .map(|v| v.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    );
                }
                Command::Eval(expr) => {
                    let value: mq_lang::RuntimeValue = context.current_value.clone();
                    let mut engine = self.engine.switch_env(Shared::clone(&context.env));
                    let values = match engine.eval(&expr, vec![value].into_iter()) {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("Error evaluating expression: {}", e);
                            continue;
                        }
                    };

                    editor.add_history_entry(&expr).unwrap();

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
                            if matches!(c, Command::Eval(_) | Command::Error(_)) {
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
                Command::Quit => return Ok(mq_lang::DebuggerAction::Quit),
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
                format!("{:>line_number_width$}| {}", line_number.to_string().blue(), line)
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
        let lines: Vec<&str> = context.source.code.lines().collect();
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
fn highlight_syntax(line: &str) -> Cow<'_, str> {
    let mut result = line.to_string();

    let commands_pattern = r"^(backtrace|bt|step|s|next|n|finish|f|info|i|continue|c|help|quit|env|)\b";
    if let Ok(re) = regex_lite::Regex::new(commands_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_green().to_string()
            })
            .to_string();
    }

    let keywords_pattern =
        r"\b(def|let|if|elif|else|end|while|foreach|self|nodes|fn|break|continue|include|true|false|None)\b";
    if let Ok(re) = regex_lite::Regex::new(keywords_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| caps[0].bright_blue().to_string())
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
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
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
        // If input is empty or ends with newline, consider it valid
        if input.is_empty() || input.ends_with('\n') {
            return Ok(ValidationResult::Valid(None));
        }

        // If input matches a known command, consider it valid (return None)
        let trimmed = input.trim();
        let is_command = matches!(
            trimmed,
            "backtrace"
                | "bt"
                | "breakpoint"
                | "b"
                | "continue"
                | "c"
                | "clear"
                | "cl"
                | "env"
                | "finish"
                | "f"
                | "help"
                | "info"
                | "i"
                | "list"
                | "l"
                | "long-list"
                | "ll"
                | "next"
                | "n"
                | "quit"
                | "q"
                | "step"
                | "s"
        ) || trimmed.starts_with("breakpoint ")
            || trimmed.starts_with("b ")
            || trimmed.starts_with("clear ")
            || trimmed.starts_with("cl ")
            || trimmed.starts_with("eval ")
            || trimmed.starts_with("e ");

        if is_command {
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
#[cfg(test)]
mod tests {
    use mq_lang::ModuleId;
    use mq_lang::{self, DebugContext};

    use super::*;

    #[test]
    fn test_command_from_string_basic() {
        assert!(matches!(Command::from("backtrace".to_string()), Command::Backtrace));
        assert!(matches!(Command::from("bt".to_string()), Command::Backtrace));
        assert!(matches!(Command::from("continue".to_string()), Command::Continue));
        assert!(matches!(Command::from("c".to_string()), Command::Continue));
        assert!(matches!(Command::from("finish".to_string()), Command::Finish));
        assert!(matches!(Command::from("f".to_string()), Command::Finish));
        assert!(matches!(Command::from("help".to_string()), Command::Help));
        assert!(matches!(Command::from("info".to_string()), Command::Info));
        assert!(matches!(Command::from("i".to_string()), Command::Info));
        assert!(matches!(Command::from("list".to_string()), Command::List));
        assert!(matches!(Command::from("l".to_string()), Command::List));
        assert!(matches!(Command::from("long-list".to_string()), Command::LongList));
        assert!(matches!(Command::from("ll".to_string()), Command::LongList));
        assert!(matches!(Command::from("next".to_string()), Command::Next));
        assert!(matches!(Command::from("n".to_string()), Command::Next));
        assert!(matches!(Command::from("quit".to_string()), Command::Quit));
        assert!(matches!(Command::from("q".to_string()), Command::Quit));
        assert!(matches!(Command::from("step".to_string()), Command::Step));
        assert!(matches!(Command::from("s".to_string()), Command::Step));
    }

    #[test]
    fn test_command_from_string_breakpoint_and_clear() {
        assert_eq!(
            Command::from("breakpoint 10".to_string()),
            Command::Breakpoint(Some(10))
        );
        assert_eq!(Command::from("b 20".to_string()), Command::Breakpoint(Some(20)));
        assert_eq!(Command::from("breakpoint".to_string()), Command::Breakpoint(None));
        assert_eq!(Command::from("b".to_string()), Command::Breakpoint(None));
        assert_eq!(Command::from("clear 3".to_string()), Command::Clear(Some(3)));
        assert_eq!(Command::from("cl 4".to_string()), Command::Clear(Some(4)));
        assert_eq!(Command::from("clear".to_string()), Command::Clear(None));
        assert_eq!(Command::from("cl".to_string()), Command::Clear(None));
    }

    #[test]
    fn test_command_from_string_eval_and_error() {
        assert_eq!(
            Command::from("eval foo + 1".to_string()),
            Command::Eval("foo + 1".to_string())
        );
        assert_eq!(Command::from("e bar".to_string()), Command::Eval("bar".to_string()));
        assert_eq!(
            Command::from("eval".to_string()),
            Command::Error("No expression provided for eval".to_string())
        );
        assert_eq!(
            Command::from("e".to_string()),
            Command::Error("No expression provided for eval".to_string())
        );
        assert_eq!(
            Command::from("env".to_string()),
            Command::Error("Use 'info' command instead of 'env'".to_string())
        );
    }

    #[test]
    fn test_command_display() {
        assert_eq!(Command::Backtrace.to_string(), "backtrace");
        assert_eq!(Command::Breakpoint(Some(42)).to_string(), "breakpoint 42");
        assert_eq!(Command::Breakpoint(None).to_string(), "breakpoint");
        assert_eq!(Command::Continue.to_string(), "continue");
        assert_eq!(Command::Clear(Some(1)).to_string(), "clear 1");
        assert_eq!(Command::Clear(None).to_string(), "clear");
        assert_eq!(Command::Error("err".to_string()).to_string(), "error err");
        assert_eq!(Command::Eval("foo".to_string()).to_string(), "eval foo");
        assert_eq!(Command::Finish.to_string(), "finish");
        assert_eq!(Command::Help.to_string(), "help");
        assert_eq!(Command::Info.to_string(), "info");
        assert_eq!(Command::List.to_string(), "list");
        assert_eq!(Command::LongList.to_string(), "long-list");
        assert_eq!(Command::Next.to_string(), "next");
        assert_eq!(Command::Quit.to_string(), "quit");
        assert_eq!(Command::Step.to_string(), "step");
    }

    #[test]
    fn test_highlight_syntax_keywords_and_numbers() {
        let input = r#"let x = 42"#;
        let highlighted = highlight_syntax(input);
        assert!(highlighted.contains("let"));
        assert!(highlighted.contains("42"));
    }

    #[test]
    fn test_highlight_syntax_string_and_operators() {
        let input = r#"foo = "bar" + 1"#;
        let highlighted = highlight_syntax(input);
        assert!(highlighted.contains("\"bar\""));
        assert!(highlighted.contains("+"));
        assert!(highlighted.contains("="));
    }

    #[test]
    fn test_get_source_code_with_context_basic() {
        let context = DebugContext {
            source: mq_lang::Source {
                name: None,
                code: "a\nb\nc\nd\ne\nf\ng\nh\ni\nj".to_string(),
            },
            token: Shared::new(mq_lang::Token {
                range: mq_lang::Range {
                    start: mq_lang::Position { line: 4, column: 0 },
                    end: mq_lang::Position { line: 4, column: 1 },
                },
                kind: mq_lang::TokenKind::Eof,
                module_id: ModuleId::new(0),
            }),
            ..Default::default()
        };
        let handler = DebuggerHandler::new(mq_lang::DefaultEngine::default());
        let (start, snippet) = handler.get_source_code_with_context(&context, 4, 2);
        assert_eq!(start, 2);
        assert_eq!(
            snippet,
            vec![
                "c".to_string(),
                "d".to_string(),
                "e".to_string(),
                "f".to_string(),
                "g".to_string()
            ]
        );
    }

    #[test]
    fn test_get_source_code_with_context_edge_cases() {
        let handler = DebuggerHandler::new(mq_lang::DefaultEngine::default());

        // Empty source code
        let empty_context = DebugContext {
            source: mq_lang::Source {
                name: None,
                code: "".to_string(),
            },
            ..Default::default()
        };
        let (start, snippet) = handler.get_source_code_with_context(&empty_context, 0, 2);
        assert_eq!(start, 0);
        assert!(snippet.is_empty());

        // Single line
        let single_line_context = DebugContext {
            source: mq_lang::Source {
                name: None,
                code: "single line".to_string(),
            },
            ..Default::default()
        };
        let (start, snippet) = handler.get_source_code_with_context(&single_line_context, 0, 2);
        assert_eq!(start, 0);
        assert_eq!(snippet, vec!["single line".to_string()]);

        // Line at beginning
        let context = DebugContext {
            source: mq_lang::Source {
                name: None,
                code: "a\nb\nc".to_string(),
            },
            ..Default::default()
        };
        let (start, snippet) = handler.get_source_code_with_context(&context, 0, 1);
        assert_eq!(start, 0);
        assert_eq!(snippet, vec!["a".to_string(), "b".to_string()]);

        // Line at end
        let (start, snippet) = handler.get_source_code_with_context(&context, 2, 1);
        assert_eq!(start, 1);
        assert_eq!(snippet, vec!["b".to_string(), "c".to_string()]);

        // Large context size
        let (start, snippet) = handler.get_source_code_with_context(&context, 1, 10);
        assert_eq!(start, 0);
        assert_eq!(snippet, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn test_debugger_handler_new() {
        let engine = mq_lang::DefaultEngine::default();
        let handler = DebuggerHandler::new(engine);
        // Just verify it constructs without panic
        assert!(std::mem::size_of_val(&handler) > 0);
    }

    #[test]
    fn test_command_from_string_invalid_line_numbers() {
        // Invalid line number should result in None
        assert_eq!(
            Command::from("breakpoint invalid".to_string()),
            Command::Breakpoint(None)
        );
        assert_eq!(Command::from("clear not_a_number".to_string()), Command::Clear(None));
        assert_eq!(Command::from("b -1".to_string()), Command::Breakpoint(None));
    }

    #[test]
    fn test_command_from_string_multi_word_eval() {
        assert_eq!(
            Command::from("eval foo + bar - baz".to_string()),
            Command::Eval("foo + bar - baz".to_string())
        );
        assert_eq!(
            Command::from("e x = 1; y = 2".to_string()),
            Command::Eval("x = 1; y = 2".to_string())
        );
    }

    #[test]
    fn test_command_from_string_unknown() {
        // Unknown commands should fallback to Eval
        assert_eq!(
            Command::from("unknown_command".to_string()),
            Command::Eval("unknown_command".to_string())
        );
        assert_eq!(
            Command::from("random text".to_string()),
            Command::Eval("random text".to_string())
        );
    }
}
