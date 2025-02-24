use std::{fmt, fs, str::FromStr};

use arboard::Clipboard;
use itertools::Itertools;
use miette::IntoDiagnostic;
use strum::IntoEnumIterator;

#[derive(Debug, Clone)]
pub enum CommandOutput {
    Value(Vec<mdq_lang::Value>),
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
            Command::Version => format!("{:<12}{}", ":version", "Print mdq version"),
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
            s => Command::Eval(s.join(" ")),
        }
    }
}

pub struct CommandContext {
    pub(crate) engine: mdq_lang::Engine,
    pub(crate) input: Vec<mdq_lang::Value>,
    pub(crate) hir: mdq_hir::Hir,
    pub(crate) source_id: mdq_hir::SourceId,
    pub(crate) scope_id: mdq_hir::ScopeId,
}

impl CommandContext {
    pub fn new(engine: mdq_lang::Engine, input: Vec<mdq_lang::Value>) -> Self {
        let mut hir = mdq_hir::Hir::new();
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
            .collect_vec()
    }

    pub fn execute(&mut self, to_run: &str) -> miette::Result<CommandOutput> {
        match to_run.to_string().into() {
            Command::Copy => {
                let text = self
                    .input
                    .iter()
                    .map(|runtime_value| runtime_value.to_string())
                    .collect_vec()
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
                    let markdown: mdq_md::Markdown = mdq_md::Markdown::from_str(&markdown_content)?;

                    self.input = markdown
                        .nodes
                        .into_iter()
                        .map(mdq_lang::Value::from)
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
                mdq_lang::Engine::version().to_string(),
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
