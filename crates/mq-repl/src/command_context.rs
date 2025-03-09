use std::{fmt, fs, str::FromStr};

use arboard::Clipboard;
use miette::IntoDiagnostic;
use strum::IntoEnumIterator;

#[derive(Debug, Clone)]
pub enum CommandOutput {
    Value(Vec<mq_lang::Value>),
    String(Vec<String>),
    None,
}

#[derive(Debug, Clone, strum::EnumIter)]
pub enum Command {
    Copy,
    Env(String, String),
    Help,
    Quit,
    LoadFile(String),
    Vars,
    Eval(String),
    Version,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Copy => write!(f, ":copy"),
            Command::Env(_, _) => {
                write!(f, ":env")
            }
            Command::Help => write!(f, ":help"),
            Command::Quit => write!(f, ":quit"),
            Command::LoadFile(_) => write!(f, ":load_file"),
            Command::Vars => write!(f, ":vars"),
            Command::Eval(_) => write!(f, ":eval"),
            Command::Version => write!(f, ":version"),
        }
    }
}

impl Command {
    pub fn help(&self) -> String {
        match self {
            Command::Copy => format!(
                "{:<12}{}",
                "copy", "Copy the execution results to the clipboard"
            ),
            Command::Env(_, _) => {
                format!("{:<12}{}", "env", "Set environment variables (key value)")
            }
            Command::Help => format!("{:<12}{}", ":help", "Print command help"),
            Command::Quit => format!("{:<12}{}", ":quit", "Quit evaluation and exit"),
            Command::LoadFile(_) => format!("{:<12}{}", ":load_file", "Load a markdown file"),
            Command::Vars => format!("{:<12}{}", ":vars", "List bound variables"),
            Command::Eval(_) => format!("{:<12}{}", ":eval", ""),
            Command::Version => format!("{:<12}{}", ":version", "Print mq version"),
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
            [":copy"] => Command::Copy,
            [":env", name, value] => Command::Env(name.to_string(), value.to_string()),
            [":help"] => Command::Help,
            [":quit"] => Command::Quit,
            [":load_file", file_path] => Command::LoadFile(file_path.to_string()),
            [":vars"] => Command::Vars,
            [":version"] => Command::Version,
            _ => Command::Eval(s),
        }
    }
}

pub struct CommandContext {
    pub(crate) engine: mq_lang::Engine,
    pub(crate) input: Vec<mq_lang::Value>,
    pub(crate) hir: mq_hir::Hir,
    pub(crate) source_id: mq_hir::SourceId,
    pub(crate) scope_id: mq_hir::ScopeId,
}

impl CommandContext {
    pub fn new(engine: mq_lang::Engine, input: Vec<mq_lang::Value>) -> Self {
        let mut hir = mq_hir::Hir::new();
        let (source_id, scope_id) = hir.add_new_source(None);

        hir.add_builtin();

        Self {
            engine,
            input,
            hir,
            source_id,
            scope_id,
        }
    }

    pub fn completions(&self, line: &str, pos: usize) -> Vec<String> {
        let src = &line[..pos];

        self.hir
            .symbols()
            .filter_map(|(_, symbol)| {
                let name = symbol
                    .name
                    .as_ref()
                    .map(|name| name.to_string())
                    .unwrap_or_default();

                if name.contains(src) { Some(name) } else { None }
            })
            .collect::<Vec<_>>()
    }

    pub fn execute(&mut self, to_run: &str) -> miette::Result<CommandOutput> {
        match to_run.to_string().into() {
            Command::Copy => {
                let text = self
                    .input
                    .iter()
                    .map(|runtime_value| runtime_value.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                let mut clipboard = Clipboard::new().unwrap();

                clipboard.set_text(text).into_diagnostic()?;
                Ok(CommandOutput::None)
            }
            Command::Env(name, value) => {
                unsafe { std::env::set_var(name, value) };
                Ok(CommandOutput::None)
            }
            Command::Help => Ok(CommandOutput::String(
                Command::iter().map(|c| c.help().to_string()).collect(),
            )),
            Command::Quit => {
                std::process::exit(0);
            }
            Command::LoadFile(file_path) => fs::read_to_string(file_path)
                .into_diagnostic()
                .and_then(|markdown_content| {
                    let markdown: mq_markdown::Markdown =
                        mq_markdown::Markdown::from_str(&markdown_content)?;

                    self.input = markdown
                        .nodes
                        .into_iter()
                        .map(mq_lang::Value::from)
                        .collect();
                    Ok(CommandOutput::None)
                }),
            Command::Vars => Ok(CommandOutput::String(
                self.engine
                    .defined_values()
                    .iter()
                    .map(|(ident, runtime_value)| format!("{} = {}", ident, runtime_value))
                    .collect(),
            )),
            Command::Version => Ok(CommandOutput::String(vec![
                mq_lang::Engine::version().to_string(),
            ])),
            Command::Eval(code) => {
                if code.is_empty() {
                    return Ok(CommandOutput::None);
                }

                let result = self.engine.eval(&code, self.input.clone().into_iter());

                result.map(|result| {
                    self.hir
                        .add_line_of_code(self.source_id, self.scope_id, &code);
                    Ok(CommandOutput::Value(result.values().clone()))
                })?
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_from_string() {
        assert!(matches!(Command::from(":copy".to_string()), Command::Copy));
        assert!(matches!(Command::from(":help".to_string()), Command::Help));
        assert!(matches!(Command::from(":quit".to_string()), Command::Quit));
        assert!(matches!(Command::from(":vars".to_string()), Command::Vars));
        assert!(matches!(
            Command::from(":version".to_string()),
            Command::Version
        ));

        if let Command::Eval(code) = Command::from("add(1, 2)".to_string()) {
            assert_eq!(code, "add(1, 2)");
        } else {
            panic!("Expected Eval command");
        }

        if let Command::Env(name, value) = Command::from(":env TEST_VAR test_value".to_string()) {
            assert_eq!(name, "TEST_VAR");
            assert_eq!(value, "test_value");
        } else {
            panic!("Expected Env command");
        }

        if let Command::LoadFile(path) = Command::from(":load_file test.md".to_string()) {
            assert_eq!(path, "test.md");
        } else {
            panic!("Expected LoadFile command");
        }
    }

    #[test]
    fn test_command_display() {
        assert_eq!(format!("{}", Command::Copy), ":copy");
        assert_eq!(format!("{}", Command::Help), ":help");
        assert_eq!(format!("{}", Command::Quit), ":quit");
        assert_eq!(format!("{}", Command::Vars), ":vars");
        assert_eq!(format!("{}", Command::Version), ":version");
        assert_eq!(
            format!("{}", Command::LoadFile("test.md".to_string())),
            ":load_file"
        );
        assert_eq!(
            format!("{}", Command::Env("key".to_string(), "value".to_string())),
            ":env"
        );
        assert_eq!(format!("{}", Command::Eval("code".to_string())), ":eval");
    }

    #[test]
    fn test_command_help() {
        for cmd in Command::iter() {
            let help = cmd.help();
            assert!(!help.is_empty());

            match cmd {
                Command::Copy => assert!(help.contains("copy")),
                Command::Help => assert!(help.contains(":help")),
                Command::Quit => assert!(help.contains(":quit")),
                Command::Vars => assert!(help.contains(":vars")),
                Command::Version => assert!(help.contains(":version")),
                Command::LoadFile(_) => assert!(help.contains(":load_file")),
                Command::Env(_, _) => assert!(help.contains("env")),
                Command::Eval(_) => assert!(help.contains(":eval")),
            }
        }
    }

    #[test]
    fn test_completions() {
        let engine = mq_lang::Engine::default();
        let ctx = CommandContext::new(engine, vec![]);

        let completions = ctx.completions("", 0);
        assert!(!completions.is_empty(), "Completions should not be empty");
    }

    #[test]
    fn test_execute_env() {
        let engine = mq_lang::Engine::default();
        let mut ctx = CommandContext::new(engine, vec![]);

        let result = ctx.execute(":env TEST_VAR test_value");
        assert!(matches!(result, Ok(CommandOutput::None)));
        assert_eq!(std::env::var("TEST_VAR").unwrap(), "test_value");
    }

    #[test]
    fn test_execute_help() {
        let engine = mq_lang::Engine::default();
        let mut ctx = CommandContext::new(engine, vec![]);

        let result = ctx.execute(":help").unwrap();
        if let CommandOutput::String(help_strings) = result {
            assert!(!help_strings.is_empty());
            assert!(help_strings.iter().any(|s| s.contains("copy")));
            assert!(help_strings.iter().any(|s| s.contains("env")));
            assert!(help_strings.iter().any(|s| s.contains(":help")));
        } else {
            panic!("Expected String output");
        }
    }

    #[test]
    fn test_execute_vars() {
        let mut engine = mq_lang::Engine::default();
        engine
            .eval("let x = 42", vec!["".to_string().into()].into_iter())
            .unwrap();
        let mut ctx = CommandContext::new(engine, vec![]);

        let result = ctx.execute(":vars").unwrap();
        if let CommandOutput::String(vars) = result {
            assert!(!vars.is_empty());
            assert!(vars.iter().any(|s| s.contains("x = 42")));
        } else {
            panic!("Expected String output");
        }
    }

    #[test]
    fn test_execute_version() {
        let engine = mq_lang::Engine::default();
        let mut ctx = CommandContext::new(engine, vec![]);

        let result = ctx.execute(":version").unwrap();
        if let CommandOutput::String(version) = result {
            assert_eq!(version.len(), 1);
            assert!(!version[0].is_empty());
        } else {
            panic!("Expected String output");
        }
    }

    #[test]
    fn test_execute_eval() {
        let engine = mq_lang::Engine::default();
        let mut ctx = CommandContext::new(engine, vec!["".to_string().into()]);

        let result = ctx.execute("add(1, 2)").unwrap();
        if let CommandOutput::Value(values) = result {
            assert_eq!(values.len(), 1);
            assert_eq!(values[0].to_string(), "3");
        } else {
            panic!("Expected Value output");
        }

        let result = ctx.execute("").unwrap();
        assert!(matches!(result, CommandOutput::None));
    }
}
