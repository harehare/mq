use std::{fmt, fs, io::Write, process::Command as ProcessCommand};

#[cfg(all(feature = "clipboard", not(target_os = "android")))]
use arboard::Clipboard;
use miette::{IntoDiagnostic, miette};
use strum::IntoEnumIterator;

#[derive(Debug, Clone)]
pub enum CommandOutput {
    Value(Vec<mq_lang::RuntimeValue>),
    String(Vec<String>),
    None,
}

#[derive(Debug, Clone, strum::EnumIter)]
pub enum Command {
    Copy,
    Edit,
    Env(String, String),
    Help,
    Quit,
    LoadFile(String),
    Vars,
    Eval(String),
    Version,
    NotFound(String),
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Copy => write!(f, "/copy"),
            Command::Edit => write!(f, "/edit"),
            Command::Env(_, _) => {
                write!(f, "/env")
            }
            Command::Help => write!(f, "/help"),
            Command::Quit => write!(f, "/quit"),
            Command::LoadFile(_) => write!(f, "/load"),
            Command::Vars => write!(f, "/vars"),
            Command::Eval(_) => write!(f, "/eval"),
            Command::Version => write!(f, "/version"),
            Command::NotFound(_) => write!(f, "/not_found"),
        }
    }
}

impl Command {
    pub fn help(&self) -> String {
        match self {
            Command::Copy => format!("{:<12}{}", "/copy", "Copy the execution results to the clipboard"),
            Command::Edit => format!("{:<12}{}", "/edit", "Edit the current buffer in external editor"),
            Command::Env(_, _) => {
                format!("{:<12}{}", "/env", "Set environment variables (key value)")
            }
            Command::Help => format!("{:<12}{}", "/help", "Print command help"),
            Command::Quit => format!("{:<12}{}", "/quit", "Quit evaluation and exit"),
            Command::LoadFile(_) => format!("{:<12}{}", "/load", "Load a markdown file"),
            Command::Vars => format!("{:<12}{}", "/vars", "List bound variables"),
            Command::Eval(_) => format!("{:<12}{}", "/eval", ""),
            Command::NotFound(_) => format!("{:<12}{}", "/not_found", ""),
            Command::Version => format!("{:<12}{}", "/version", "Print mq version"),
        }
    }
}

impl From<String> for Command {
    fn from(s: String) -> Self {
        match s.as_str().split_whitespace().collect::<Vec<&str>>().as_slice() {
            ["/copy"] => Command::Copy,
            ["/edit"] => Command::Edit,
            ["/env", name, value] => Command::Env(name.to_string(), value.to_string()),
            ["/help"] => Command::Help,
            ["/quit"] => Command::Quit,
            ["/load", file_path] => Command::LoadFile(file_path.to_string()),
            ["/vars"] => Command::Vars,
            ["/version"] => Command::Version,
            _ if s.starts_with("/") => Command::NotFound(s),
            _ => Command::Eval(s),
        }
    }
}

pub struct CommandContext {
    pub(crate) engine: mq_lang::DefaultEngine,
    pub(crate) input: Vec<mq_lang::RuntimeValue>,
    pub(crate) hir: mq_hir::Hir,
    pub(crate) source_id: mq_hir::SourceId,
    pub(crate) scope_id: mq_hir::ScopeId,
}

impl CommandContext {
    pub fn new(engine: mq_lang::DefaultEngine, input: Vec<mq_lang::RuntimeValue>) -> Self {
        let mut hir = mq_hir::Hir::default();
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
                let name = symbol.value.as_ref().map(|name| name.to_string()).unwrap_or_default();

                if name.contains(src) { Some(name) } else { None }
            })
            .collect::<Vec<_>>()
    }

    pub fn execute(&mut self, to_run: &str) -> miette::Result<CommandOutput> {
        match to_run.to_string().into() {
            Command::Copy => {
                #[cfg(all(feature = "clipboard", not(target_os = "android")))]
                {
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
                #[cfg(any(not(feature = "clipboard"), target_os = "android"))]
                {
                    Err(miette!("Clipboard functionality is not available on this platform"))
                }
            }
            Command::Edit => {
                // Get editor from environment variables
                let editor = std::env::var("EDITOR")
                    .or_else(|_| std::env::var("VISUAL"))
                    .unwrap_or_else(|_| "vi".to_string());

                // Create a temporary file
                let mut temp_file = tempfile::Builder::new()
                    .prefix("mq-edit-")
                    .suffix(".mq")
                    .tempfile()
                    .into_diagnostic()?;

                // Write current buffer to temp file (empty for now)
                temp_file.write_all(b"").into_diagnostic()?;
                temp_file.flush().into_diagnostic()?;

                let temp_path = temp_file.path().to_path_buf();

                // Close the file before opening in editor
                drop(temp_file);

                // Launch the editor
                let status = ProcessCommand::new(&editor)
                    .arg(&temp_path)
                    .status()
                    .into_diagnostic()?;

                if !status.success() {
                    return Err(miette!("Editor exited with non-zero status"));
                }

                // Read the edited content
                let edited_content = fs::read_to_string(&temp_path).into_diagnostic()?;

                // Clean up temp file
                fs::remove_file(&temp_path).ok();

                // Evaluate the edited content
                let code = edited_content.trim();
                if code.is_empty() {
                    Ok(CommandOutput::None)
                } else {
                    let eval_result = self.engine.eval(code, self.input.clone().into_iter()).map_err(|e| *e)?;

                    self.hir.add_line_of_code(self.source_id, self.scope_id, code);
                    self.input = eval_result.values().clone();

                    Ok(CommandOutput::Value(eval_result.values().clone()))
                }
            }
            Command::Env(name, value) => {
                unsafe { std::env::set_var(name, value) };
                Ok(CommandOutput::None)
            }
            Command::Help => {
                let mut help_lines = vec![];
                help_lines.push("".to_string());
                help_lines.push("Available commands:".to_string());
                help_lines.push("".to_string());

                let commands: Vec<String> = Command::iter()
                    .filter_map(|c| {
                        if matches!(c, Command::Eval(_)) || matches!(c, Command::NotFound(_)) {
                            None
                        } else {
                            Some(c.help().to_string())
                        }
                    })
                    .collect();

                help_lines.extend(commands);
                help_lines.push("".to_string());

                Ok(CommandOutput::String(help_lines))
            }
            Command::Quit => {
                std::process::exit(0);
            }
            Command::NotFound(s) => Err(miette!(format!("Command not found: {}", s))),
            Command::LoadFile(file_path) => {
                fs::read_to_string(file_path)
                    .into_diagnostic()
                    .and_then(|markdown_content| {
                        let markdown: mq_markdown::Markdown =
                            mq_markdown::Markdown::from_markdown_str(&markdown_content)?;

                        self.input = markdown.nodes.into_iter().map(mq_lang::RuntimeValue::from).collect();
                        Ok(CommandOutput::None)
                    })
            }
            Command::Vars => Ok(CommandOutput::String(
                self.hir
                    .symbols()
                    .filter_map(|(_, symbol)| {
                        if self.hir.is_builtin_symbol(symbol) {
                            None
                        } else {
                            match &symbol.kind {
                                mq_hir::SymbolKind::Function(_) if symbol.parent.is_none() => {
                                    let name = symbol.value.as_ref().map(|name| name.to_string()).unwrap_or_default();
                                    Some(format!("{}: {}", name, symbol))
                                }
                                mq_hir::SymbolKind::Call
                                | mq_hir::SymbolKind::Function(_)
                                | mq_hir::SymbolKind::String
                                | mq_hir::SymbolKind::Number
                                | mq_hir::SymbolKind::Boolean
                                | mq_hir::SymbolKind::None => symbol.parent.and_then(|parent| {
                                    self.hir
                                        .symbol(parent)
                                        .and_then(|parent_symbol| match parent_symbol.kind {
                                            mq_hir::SymbolKind::Variable => {
                                                let name = parent_symbol
                                                    .value
                                                    .as_ref()
                                                    .map(|name| name.to_string())
                                                    .unwrap_or_default();
                                                Some(format!("{}: {}", name, symbol))
                                            }
                                            _ => None,
                                        })
                                }),
                                _ => None,
                            }
                        }
                    })
                    .collect(),
            )),
            Command::Version => Ok(CommandOutput::String(vec![
                mq_lang::DefaultEngine::version().to_string(),
            ])),
            Command::Eval(code) => {
                if code.is_empty() {
                    return Ok(CommandOutput::None);
                }

                let result = self.engine.eval(&code, self.input.clone().into_iter());

                result
                    .map(|result| {
                        self.hir.add_line_of_code(self.source_id, self.scope_id, &code);
                        self.input = result.values().clone();
                        Ok(CommandOutput::Value(result.values().clone()))
                    })
                    .map_err(|e| *e)?
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use scopeguard::defer;
    use std::io::Write;
    use std::{fs::File, path::PathBuf};

    use super::*;

    fn create_file(name: &str, content: &str) -> (PathBuf, PathBuf) {
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(name);
        let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");

        (temp_dir, temp_file_path)
    }

    #[test]
    fn test_command_from_string() {
        assert!(matches!(Command::from("/copy".to_string()), Command::Copy));
        assert!(matches!(Command::from("/edit".to_string()), Command::Edit));
        assert!(matches!(Command::from("/help".to_string()), Command::Help));
        assert!(matches!(Command::from("/quit".to_string()), Command::Quit));
        assert!(matches!(Command::from("/vars".to_string()), Command::Vars));
        assert!(matches!(Command::from("/version".to_string()), Command::Version));

        if let Command::Eval(code) = Command::from("add(1, 2)".to_string()) {
            assert_eq!(code, "add(1, 2)");
        } else {
            panic!("Expected Eval command");
        }

        if let Command::Env(name, value) = Command::from("/env TEST_VAR test_value".to_string()) {
            assert_eq!(name, "TEST_VAR");
            assert_eq!(value, "test_value");
        } else {
            panic!("Expected Env command");
        }

        if let Command::LoadFile(path) = Command::from("/load test.md".to_string()) {
            assert_eq!(path, "test.md");
        } else {
            panic!("Expected LoadFile command");
        }
    }

    #[test]
    fn test_command_display() {
        assert_eq!(format!("{}", Command::Copy), "/copy");
        assert_eq!(format!("{}", Command::Edit), "/edit");
        assert_eq!(format!("{}", Command::Help), "/help");
        assert_eq!(format!("{}", Command::Quit), "/quit");
        assert_eq!(format!("{}", Command::Vars), "/vars");
        assert_eq!(format!("{}", Command::Version), "/version");
        assert_eq!(format!("{}", Command::LoadFile("test.md".to_string())), "/load");
        assert_eq!(
            format!("{}", Command::Env("key".to_string(), "value".to_string())),
            "/env"
        );
        assert_eq!(format!("{}", Command::Eval("code".to_string())), "/eval");
    }

    #[test]
    fn test_command_help() {
        for cmd in Command::iter() {
            let help = cmd.help();
            assert!(!help.is_empty());

            match cmd {
                Command::Copy => assert!(help.contains("/copy")),
                Command::Edit => assert!(help.contains("/edit")),
                Command::Help => assert!(help.contains("/help")),
                Command::Quit => assert!(help.contains("/quit")),
                Command::Vars => assert!(help.contains("/vars")),
                Command::Version => assert!(help.contains("/version")),
                Command::LoadFile(_) => assert!(help.contains("/load")),
                Command::Env(_, _) => assert!(help.contains("/env")),
                Command::Eval(_) => assert!(help.contains("/eval")),
                Command::NotFound(_) => assert!(help.contains("/not_found")),
            }
        }
    }

    #[test]
    fn test_completions() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let completions = ctx.completions("", 0);
        assert!(!completions.is_empty(), "Completions should not be empty");
    }

    #[test]
    fn test_execute_env() {
        let engine = mq_lang::DefaultEngine::default();
        let mut ctx = CommandContext::new(engine, Vec::new());

        let result = ctx.execute("/env TEST_VAR test_value");
        assert!(matches!(result, Ok(CommandOutput::None)));
        assert_eq!(std::env::var("TEST_VAR").unwrap(), "test_value");
    }

    #[test]
    fn test_execute_help() {
        let engine = mq_lang::DefaultEngine::default();
        let mut ctx = CommandContext::new(engine, Vec::new());

        let result = ctx.execute("/help").unwrap();
        if let CommandOutput::String(help_strings) = result {
            assert!(!help_strings.is_empty());
            assert!(help_strings.iter().any(|s| s.contains("/copy")));
            assert!(help_strings.iter().any(|s| s.contains("/env")));
            assert!(help_strings.iter().any(|s| s.contains("/help")));
        } else {
            panic!("Expected String output");
        }
    }

    #[test]
    fn test_execute_vars() {
        let mut ctx = CommandContext::new(mq_lang::DefaultEngine::default(), Vec::new());

        ctx.execute("let x = 42 | let x2 = def fun1(x): add(x, 2); | def fun(): 1;")
            .unwrap();

        let result = ctx.execute("/vars").unwrap();
        if let CommandOutput::String(vars) = result {
            assert!(!vars.is_empty());
            assert!(vars.iter().any(|s| s.contains("x: 42")));
            assert!(vars.iter().any(|s| s.contains("x2: function(x)")));
            assert!(vars.iter().any(|s| s.contains("fun: function()")));
        } else {
            panic!("Expected String output");
        }
    }

    #[test]
    fn test_execute_version() {
        let engine = mq_lang::DefaultEngine::default();
        let mut ctx = CommandContext::new(engine, Vec::new());

        let result = ctx.execute("/version").unwrap();
        if let CommandOutput::String(version) = result {
            assert_eq!(version.len(), 1);
            assert!(!version[0].is_empty());
        } else {
            panic!("Expected String output");
        }
    }

    #[test]
    fn test_execute_load_file() {
        let engine = mq_lang::DefaultEngine::default();
        let mut ctx = CommandContext::new(engine, vec!["".to_string().into()]);
        let (_, temp_file_path) = create_file(
            "test_execute_load_file.md",
            "# Header\n\nParagraph text.\n\n- List item 1\n- List item 2",
        );
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let result = ctx.execute(&format!("/load {}", temp_file_path.to_str().unwrap()));
        assert!(matches!(result, Ok(CommandOutput::None)));

        let list_items = ctx.input.iter().filter(|v| v.to_string().contains("List item")).count();
        assert_eq!(list_items, 2);
    }

    #[test]
    fn test_execute_eval() {
        let engine = mq_lang::DefaultEngine::default();
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
