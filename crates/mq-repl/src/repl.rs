use crate::command_context::{Command, CommandContext, CommandOutput};
use colored::*;
use miette::IntoDiagnostic;
use mq_lang::RuntimeValue;
use rustyline::{
    At, Cmd, CompletionType, Config, Context, EditMode, Editor, Helper, KeyCode, KeyEvent, Modifiers, Movement, Word,
    completion::{Completer, FilenameCompleter, Pair},
    error::ReadlineError,
    highlight::{CmdKind, Highlighter},
    hint::Hinter,
    validate::{ValidationContext, ValidationResult, Validator},
};
use std::{borrow::Cow, cell::RefCell, fs, rc::Rc};

/// Highlight mq syntax with keywords and commands
fn highlight_mq_syntax(line: &str) -> Cow<'_, str> {
    let mut result = line.to_string();

    let commands_pattern = r"^(/clear|/copy|/edit|/env|/help|/history|/quit|/load|/reset|/vars|/version)\b";
    if let Ok(re) = regex_lite::Regex::new(commands_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_green().to_string()
            })
            .to_string();
    }

    let keywords_pattern = r"\b(def|let|if|elif|else|end|while|foreach|self|nodes|fn|break|continue|include|true|false|None|match|import|module|do|var|macro|quote|unquote)\b";
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
    let operators_pattern =
        r"(\/\/=|<<|>>|\|\||\?\?|<=|>=|==|!=|=~|&&|\+=|-=|\*=|\/=|\|=|=|\||:|;|\?|!|\+|-|\*|\/|%|<|>|@)";
    if let Ok(re) = regex_lite::Regex::new(operators_pattern) {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                caps[0].bright_yellow().to_string()
            })
            .to_string();
    }

    Cow::Owned(result)
}

/// Format a markdown node with type-specific colors.
fn format_markdown_node(node: &mq_markdown::Node) -> String {
    let s = node.to_string();
    match node {
        mq_markdown::Node::Heading(_) => s.bold().bright_cyan().to_string(),
        mq_markdown::Node::Code(_) => s.bright_yellow().to_string(),
        mq_markdown::Node::CodeInline(_) => s.yellow().to_string(),
        mq_markdown::Node::Link(_) | mq_markdown::Node::LinkRef(_) => s.bright_blue().to_string(),
        mq_markdown::Node::Strong(_) => s.bold().to_string(),
        mq_markdown::Node::Emphasis(_) => s.italic().to_string(),
        _ => s,
    }
}

/// Format a runtime value with type-appropriate colors.
fn format_runtime_value(value: &mq_lang::RuntimeValue) -> Option<String> {
    if value.is_empty() {
        return None;
    }

    let s = match value {
        RuntimeValue::None => return Some("None".dimmed().to_string()),
        RuntimeValue::Number(n) => n.to_string().bright_magenta().to_string(),
        RuntimeValue::Boolean(b) => b.to_string().bright_yellow().to_string(),
        RuntimeValue::String(s) => format!("\"{}\"", s).bright_green().to_string(),
        RuntimeValue::Markdown(node, _) => format_markdown_node(node),
        _ => {
            let s = value.to_string();
            if s.is_empty() {
                return None;
            }
            s
        }
    };
    Some(s)
}

/// Get the appropriate prompt symbol based on character availability
fn get_prompt() -> &'static str {
    if is_char_available() { "❯ " } else { "> " }
}

fn is_truecolor_supported() -> bool {
    matches!(std::env::var("COLORTERM").as_deref(), Ok("truecolor") | Ok("24bit"))
}

fn logo_primary(s: &str) -> ColoredString {
    if is_truecolor_supported() {
        s.truecolor(133, 212, 255)
    } else {
        s.bright_cyan()
    }
}

fn text_muted(s: &str) -> ColoredString {
    if is_truecolor_supported() {
        s.truecolor(148, 163, 184)
    } else {
        s.white()
    }
}

/// Check if a Unicode character is available in the current environment
fn is_char_available() -> bool {
    // Check environment variables that might indicate character support
    if let Ok(term) = std::env::var("TERM") {
        // Most modern terminals support Unicode
        if term.contains("xterm") || term.contains("screen") || term.contains("tmux") {
            return true;
        }
    }

    // Check if we're in a UTF-8 locale
    if let Ok(lang) = std::env::var("LANG")
        && (lang.to_lowercase().contains("utf-8") || lang.to_lowercase().contains("utf8"))
    {
        return true;
    }

    // Check LC_ALL and LC_CTYPE for UTF-8 support
    for var in ["LC_ALL", "LC_CTYPE"] {
        if let Ok(locale) = std::env::var(var)
            && (locale.to_lowercase().contains("utf-8") || locale.to_lowercase().contains("utf8"))
        {
            return true;
        }
    }

    // Default to false for safety if we can't determine character support
    false
}

pub struct MqLineHelper {
    command_context: Rc<RefCell<CommandContext>>,
    file_completer: FilenameCompleter,
    /// Tracks whether the current input spans multiple lines (continuation mode).
    is_continuation: Rc<RefCell<bool>>,
}

impl MqLineHelper {
    pub fn new(command_context: Rc<RefCell<CommandContext>>) -> Self {
        Self {
            command_context,
            file_completer: FilenameCompleter::new(),
            is_continuation: Rc::new(RefCell::new(false)),
        }
    }
}

impl Hinter for MqLineHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        // Update continuation state based on whether the buffer has newlines.
        *self.is_continuation.borrow_mut() = line.contains('\n');

        if pos < line.len() || line.is_empty() || line.starts_with('/') {
            return None;
        }

        let (start, completions) = self.command_context.borrow().completions(line, pos);
        let word = &line[start..pos];

        // Completion hint takes priority when a single match extends the current word.
        if !word.is_empty() && completions.len() == 1 && completions[0].name.len() > word.len() {
            return Some(completions[0].name[word.len()..].to_string());
        }

        // Bracket closing hint: show the matching close bracket right after an open bracket.
        if word.is_empty() {
            let closing = match line.chars().last() {
                Some('(') => Some(")"),
                Some('[') => Some("]"),
                Some('{') => Some("}"),
                _ => None,
            };
            if let Some(c) = closing {
                return Some(c.to_string());
            }
        }

        None
    }
}

impl Highlighter for MqLineHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(&'s self, prompt: &'p str, _default: bool) -> Cow<'b, str> {
        prompt.cyan().to_string().into()
    }

    fn highlight_hint<'h>(&self, hint: &'h str) -> std::borrow::Cow<'h, str> {
        hint.dimmed().to_string().into()
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _kind: CmdKind) -> bool {
        true
    }

    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        highlight_mq_syntax(line)
    }
}

impl Validator for MqLineHelper {
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

impl Completer for MqLineHelper {
    type Candidate = Pair;

    fn complete(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let (start, matches) = self.command_context.borrow().completions(line, pos);

        let mut completions = matches
            .iter()
            .map(|item| Pair {
                display: item.display.clone(),
                replacement: format!("{}{}", item.name, &line[pos..]),
            })
            .collect::<Vec<_>>();

        if line.starts_with(Command::LoadFile("".to_string()).to_string().as_str()) {
            let (_, file_completions) = self.file_completer.complete_path(line, pos)?;
            completions.extend(file_completions);
        }

        Ok((start, completions))
    }
}

impl Helper for MqLineHelper {}

pub struct Repl {
    command_context: Rc<RefCell<CommandContext>>,
}

pub fn config_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("MQ_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| dirs::config_dir().map(|d| d.join("mq")))
}

impl Repl {
    pub fn new(input: Vec<mq_lang::RuntimeValue>) -> Self {
        let mut engine = mq_lang::DefaultEngine::default();

        engine.load_builtin_module();

        Self {
            command_context: Rc::new(RefCell::new(CommandContext::new(engine, input))),
        }
    }

    fn print_welcome() {
        let version = mq_lang::DefaultEngine::version();

        println!();
        println!("  {} {}", logo_primary("mq").bold(), text_muted(&format!("v{version}")));
        println!("  {}", text_muted("Query. Filter. Transform Markdown."));
        println!();
        println!("  Type {} to see available commands.", logo_primary("/help"));
        println!();
    }

    pub fn run(&self) -> miette::Result<()> {
        let config = Config::builder()
            .history_ignore_space(true)
            .completion_type(CompletionType::List)
            .edit_mode(EditMode::Emacs)
            .color_mode(rustyline::ColorMode::Enabled)
            .build();
        let mut editor = Editor::with_config(config).into_diagnostic()?;
        let helper = MqLineHelper::new(Rc::clone(&self.command_context));

        editor.set_helper(Some(helper));
        editor.bind_sequence(
            KeyEvent(KeyCode::Left, Modifiers::CTRL),
            Cmd::Move(Movement::BackwardWord(1, Word::Big)),
        );
        editor.bind_sequence(
            KeyEvent(KeyCode::Right, Modifiers::CTRL),
            Cmd::Move(Movement::ForwardWord(1, At::AfterEnd, Word::Big)),
        );
        // Bind Esc+C (Alt+C) to clear all input lines
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('c'), Modifiers::ALT),
            Cmd::Kill(Movement::WholeBuffer),
        );
        // Bind Esc+O (Alt+O) to open editor
        editor.bind_sequence(
            KeyEvent(KeyCode::Char('o'), Modifiers::ALT),
            Cmd::Insert(1, "/edit\n".to_string()),
        );

        let config_dir = config_dir();

        if let Some(config_dir) = &config_dir {
            let history = config_dir.join("history.txt");
            fs::create_dir_all(config_dir).ok();
            if editor.load_history(&history).is_err() {
                println!("No previous history.");
            }
        }

        Self::print_welcome();

        loop {
            let prompt = format!("{}", get_prompt().cyan());
            let readline = editor.readline(&prompt);

            match readline {
                Ok(line) => {
                    editor.add_history_entry(&line).unwrap();

                    match self.command_context.borrow_mut().execute(&line) {
                        Ok(CommandOutput::String(s)) => {
                            if !s.is_empty() {
                                println!("{}", s.join("\n"))
                            }
                        }
                        Ok(CommandOutput::Value(runtime_values)) => {
                            let lines: Vec<String> = runtime_values.iter().filter_map(format_runtime_value).collect();
                            if !lines.is_empty() {
                                println!("{}", lines.join("\n"))
                            }
                        }
                        Ok(CommandOutput::History) => {
                            let entries: Vec<String> = editor
                                .history()
                                .iter()
                                .enumerate()
                                .map(|(i, entry)| format!("  {:>4}  {}", i + 1, entry.dimmed()))
                                .collect();
                            if entries.is_empty() {
                                println!("  No history.");
                            } else {
                                println!("{}", entries.join("\n"));
                            }
                        }
                        Ok(CommandOutput::None) => (),
                        Err(e) => {
                            eprintln!("{:?}", e)
                        }
                    }
                }
                Err(ReadlineError::Interrupted) => {
                    continue;
                }
                Err(ReadlineError::Eof) => {
                    break;
                }
                Err(err) => {
                    eprintln!("Error: {:?}", err);
                    break;
                }
            }

            if let Some(config_dir) = &config_dir {
                let history = config_dir.join("history.txt");
                editor.save_history(&history.to_string_lossy().to_string()).unwrap();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir() {
        unsafe { std::env::set_var("MQ_CONFIG_DIR", "/tmp/test_mq_config") };
        assert_eq!(config_dir(), Some(std::path::PathBuf::from("/tmp/test_mq_config")));

        unsafe { std::env::remove_var("MQ_CONFIG_DIR") };
        let config_dir = config_dir();
        assert!(config_dir.is_some());
        if let Some(dir) = config_dir {
            assert!(dir.ends_with("mq"));
        }
    }

    #[test]
    fn test_highlight_mq_syntax() {
        // Test keyword highlighting
        let result = highlight_mq_syntax("let x = 42");
        assert!(result.contains("let"));

        // Test command highlighting
        let result = highlight_mq_syntax("/help");
        assert!(result.contains("help"));

        // Test operator highlighting
        let result = highlight_mq_syntax("x = 1 + 2");
        assert!(result.contains("="));
        assert!(result.contains("+"));

        // Test string highlighting
        let result = highlight_mq_syntax(r#""hello world""#);
        assert!(result.contains("hello world"));

        // Test number highlighting
        let result = highlight_mq_syntax("42");
        assert!(result.contains("42"));
    }

    #[test]
    fn test_format_runtime_value_number() {
        let v = mq_lang::RuntimeValue::Number(42.into());
        let s = format_runtime_value(&v).unwrap();
        assert!(s.contains("42"));
    }

    #[test]
    fn test_format_runtime_value_boolean() {
        let v = mq_lang::RuntimeValue::Boolean(true);
        let s = format_runtime_value(&v).unwrap();
        assert!(s.contains("true"));
    }

    #[test]
    fn test_format_runtime_value_string() {
        let v = mq_lang::RuntimeValue::String("hello".to_string());
        let s = format_runtime_value(&v).unwrap();
        assert!(s.contains("hello"));
        assert!(s.contains('"'));
    }

    #[test]
    fn test_format_runtime_value_none() {
        let v = mq_lang::RuntimeValue::None;
        let s = format_runtime_value(&v).unwrap();
        assert!(s.contains("None"));
    }

    #[test]
    fn test_is_char_available_utf8_env() {
        // Save original env vars
        let orig_term = std::env::var("TERM").ok();
        let orig_lang = std::env::var("LANG").ok();
        let orig_lc_all = std::env::var("LC_ALL").ok();
        let orig_lc_ctype = std::env::var("LC_CTYPE").ok();

        // TERM contains xterm
        unsafe { std::env::set_var("TERM", "xterm-256color") };
        assert!(is_char_available());

        // LANG contains utf-8
        unsafe { std::env::remove_var("TERM") };
        unsafe { std::env::set_var("LANG", "en_US.UTF-8") };
        assert!(is_char_available());

        // LC_ALL contains utf8
        unsafe { std::env::remove_var("LANG") };
        unsafe { std::env::set_var("LC_ALL", "ja_JP.utf8") };
        assert!(is_char_available());

        // LC_CTYPE contains utf-8
        unsafe { std::env::remove_var("LC_ALL") };
        unsafe { std::env::set_var("LC_CTYPE", "fr_FR.UTF-8") };
        assert!(is_char_available());

        // No relevant env vars
        unsafe { std::env::remove_var("LC_CTYPE") };
        assert!(!is_char_available());

        // Restore original env vars
        if let Some(val) = orig_term {
            unsafe { std::env::set_var("TERM", val) };
        } else {
            unsafe { std::env::remove_var("TERM") };
        }
        if let Some(val) = orig_lang {
            unsafe { std::env::set_var("LANG", val) };
        } else {
            unsafe { std::env::remove_var("LANG") };
        }
        if let Some(val) = orig_lc_all {
            unsafe { std::env::set_var("LC_ALL", val) };
        } else {
            unsafe { std::env::remove_var("LC_ALL") };
        }
        if let Some(val) = orig_lc_ctype {
            unsafe { std::env::set_var("LC_CTYPE", val) };
        } else {
            unsafe { std::env::remove_var("LC_CTYPE") };
        }
    }
}
