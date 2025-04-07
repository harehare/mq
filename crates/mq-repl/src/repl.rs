use colored::*;
use miette::IntoDiagnostic;
use rustyline::{
    At, Cmd, CompletionType, Config, Context, EditMode, Editor, Helper, KeyCode, KeyEvent,
    Modifiers, Movement, Word,
    completion::{Completer, FilenameCompleter, Pair},
    error::ReadlineError,
    highlight::{CmdKind, Highlighter, MatchingBracketHighlighter},
    hint::Hinter,
    validate::{ValidationContext, ValidationResult, Validator},
};
use std::{borrow::Cow, cell::RefCell, fs, rc::Rc};

use crate::command_context::{Command, CommandContext, CommandOutput};

const PROMPT: &str = "> ";

pub struct MqLineHelper {
    command_context: Rc<RefCell<CommandContext>>,
    file_completer: FilenameCompleter,
    matching_bracket_highlighter: MatchingBracketHighlighter,
}

impl MqLineHelper {
    pub fn new(command_context: Rc<RefCell<CommandContext>>) -> Self {
        Self {
            command_context,
            file_completer: FilenameCompleter::new(),
            matching_bracket_highlighter: MatchingBracketHighlighter::default(),
        }
    }
}

impl Hinter for MqLineHelper {
    type Hint = String;
}

impl Highlighter for MqLineHelper {
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        prompt.cyan().to_string().into()
    }

    fn highlight_char(&self, line: &str, pos: usize, kind: CmdKind) -> bool {
        self.matching_bracket_highlighter
            .highlight_char(line, pos, kind)
    }
}

impl Validator for MqLineHelper {
    fn validate(&self, ctx: &mut ValidationContext<'_>) -> Result<ValidationResult, ReadlineError> {
        let input = ctx.input();
        if input.is_empty() || input.ends_with("\n") || input.starts_with(":") {
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
    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<Pair>), ReadlineError> {
        let mut completions = self
            .command_context
            .borrow()
            .completions(line, pos)
            .iter()
            .map(|cmd| Pair {
                display: cmd.clone(),
                replacement: format!("{}{}", cmd, &line[pos..]),
            })
            .collect::<Vec<_>>();

        if line.starts_with(Command::LoadFile("".to_string()).to_string().as_str()) {
            let (_, file_completions) = self.file_completer.complete_path(line, pos)?;
            completions.extend(file_completions);
        }

        Ok((0, completions))
    }
}

impl Helper for MqLineHelper {}

pub struct Repl {
    command_context: Rc<RefCell<CommandContext>>,
}

impl Repl {
    pub fn new(input: Vec<mq_lang::Value>) -> Self {
        let mut engine = mq_lang::Engine::default();

        engine.load_builtin_module();

        Self {
            command_context: Rc::new(RefCell::new(CommandContext::new(engine, input))),
        }
    }

    pub fn config_dir() -> Option<std::path::PathBuf> {
        std::env::var_os("MDQ_CONFIG_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| dirs::config_dir().map(|d| d.join("mq")))
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

        let config_dir = Self::config_dir();

        if let Some(config_dir) = &config_dir {
            let history = config_dir.join("history.txt");
            fs::create_dir_all(config_dir).ok();
            if editor.load_history(&history).is_err() {
                println!("No previous history.");
            }
        }

        println!("Welcome to mq. For help, type :help");

        loop {
            let prompt = format!("{}", PROMPT.cyan());
            let readline = editor.readline(&prompt);

            match readline {
                Ok(line) => match self.command_context.borrow_mut().execute(&line) {
                    Ok(CommandOutput::String(s)) => println!("{}", s.join("\n")),
                    Ok(CommandOutput::Value(runtime_values)) => {
                        let lines = runtime_values
                            .iter()
                            .filter_map(|runtime_value| {
                                let s = runtime_value.to_string();
                                if s.is_empty() { None } else { Some(s) }
                            })
                            .collect::<Vec<_>>();

                        if !lines.is_empty() {
                            println!("{}", lines.join("\n"))
                        }

                        editor.add_history_entry(&line).unwrap();
                    }
                    Ok(CommandOutput::None) => (),
                    Err(e) => {
                        eprintln!("{:?}", e)
                    }
                },
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
                editor
                    .save_history(&history.to_string_lossy().to_string())
                    .unwrap();
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
        unsafe { std::env::set_var("MDQ_CONFIG_DIR", "/tmp/test_mq_config") };
        let config_dir = Repl::config_dir();
        assert_eq!(
            config_dir,
            Some(std::path::PathBuf::from("/tmp/test_mq_config"))
        );

        unsafe { std::env::remove_var("MDQ_CONFIG_DIR") };
        let config_dir = Repl::config_dir();
        assert!(config_dir.is_some());
        if let Some(dir) = config_dir {
            assert!(dir.ends_with("mq"));
        }
    }
}
