use std::{fmt, fs, io::Write, process::Command as ProcessCommand};

#[cfg(all(feature = "clipboard", not(target_os = "android")))]
use arboard::Clipboard;
use miette::{IntoDiagnostic, miette};
use strum::IntoEnumIterator;

/// A completion candidate with a display label and the name to insert.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// The actual text to insert.
    pub name: String,
    /// The label shown in the completion list (may include signature or description).
    pub display: String,
}

#[derive(Debug, Clone)]
pub enum CommandOutput {
    Value(Vec<mq_lang::RuntimeValue>),
    String(Vec<String>),
    History,
    None,
}

#[derive(Debug, Clone, strum::EnumIter)]
pub enum Command {
    Clear,
    Copy,
    Edit,
    Env(String, String),
    Eval(String),
    Help,
    History,
    LoadFile(String),
    NotFound(String),
    Quit,
    Reset,
    SaveFile(String),
    Vars,
    Version,
}

/// List of language keywords used for REPL completions.
///
/// This list must be kept in sync with the keyword definitions in
/// `crates/mq-lang/src/lexer.rs` (see the lexer keyword table around
/// lines 207–260).
const KEYWORDS: &[&str; 28] = &[
    "def", "let", "if", "elif", "else", "end", "while", "loop", "foreach", "self", "nodes", "fn", "break", "continue",
    "include", "true", "false", "None", "match", "try", "catch", "import", "module", "do", "var", "macro", "quote",
    "unquote",
];

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Command::Clear => write!(f, "/clear"),
            Command::Copy => write!(f, "/copy"),
            Command::Edit => write!(f, "/edit"),
            Command::Env(_, _) => write!(f, "/env"),
            Command::Help => write!(f, "/help"),
            Command::History => write!(f, "/history"),
            Command::Quit => write!(f, "/quit"),
            Command::LoadFile(_) => write!(f, "/load"),
            Command::SaveFile(_) => write!(f, "/save"),
            Command::Reset => write!(f, "/reset"),
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
            Command::Clear => format!("{:<12}{}", "/clear", "Clear the terminal screen"),
            Command::Copy => format!("{:<12}{}", "/copy", "Copy the execution results to the clipboard"),
            Command::Edit => format!("{:<12}{}", "/edit", "Edit the current buffer in external editor"),
            Command::Env(_, _) => format!("{:<12}{}", "/env", "Set environment variables (key value)"),
            Command::Help => format!("{:<12}{}", "/help", "Print command help"),
            Command::History => format!("{:<12}{}", "/history", "Show command history"),
            Command::Quit => format!("{:<12}{}", "/quit", "Quit evaluation and exit"),
            Command::LoadFile(_) => format!("{:<12}{}", "/load", "Load a markdown file"),
            Command::SaveFile(_) => format!("{:<12}{}", "/save", "Save a current result to a file"),
            Command::Reset => format!("{:<12}{}", "/reset", "Reset REPL state (clear variables and input)"),
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
            ["/clear"] => Command::Clear,
            ["/copy"] => Command::Copy,
            ["/edit"] => Command::Edit,
            ["/env", name, value] => Command::Env(name.to_string(), value.to_string()),
            ["/help"] => Command::Help,
            ["/history"] => Command::History,
            ["/quit"] => Command::Quit,
            ["/load", file_path] => Command::LoadFile(file_path.to_string()),
            ["/save", file_path] => Command::SaveFile(file_path.to_string()),
            ["/reset"] => Command::Reset,
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
    initial_input: Vec<mq_lang::RuntimeValue>,
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
            initial_input: input.clone(),
            input,
            hir,
            source_id,
            scope_id,
        }
    }

    pub fn completions(&self, line: &str, pos: usize) -> (usize, Vec<CompletionItem>) {
        let prefix = &line[..pos];
        let start = prefix
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_alphanumeric() && c != '_' && c != '/' && c != '@' && c != '.')
            .map(|(i, c)| i + c.len_utf8())
            .unwrap_or(0);
        let word = &line[start..pos];

        let mut matches: Vec<CompletionItem> = Vec::new();

        if word.starts_with('/') {
            for cmd in Command::iter() {
                if matches!(cmd, Command::Eval(_) | Command::NotFound(_)) {
                    continue;
                }
                let cmd_str = cmd.to_string();
                if cmd_str.starts_with(word) {
                    let description = cmd.help();
                    let desc_part = description.trim_start_matches(&cmd_str).trim().to_string();
                    matches.push(CompletionItem {
                        name: cmd_str.clone(),
                        display: if desc_part.is_empty() {
                            cmd_str
                        } else {
                            format!("{:<12}{}", cmd_str, desc_part)
                        },
                    });
                }
            }
        } else if word.starts_with('.') {
            for (selector, doc) in mq_lang::BUILTIN_SELECTOR_DOC.iter() {
                if selector.starts_with(word) {
                    matches.push(CompletionItem {
                        name: selector.to_string(),
                        display: format!("{:<20}{}", selector, doc.description),
                    });
                }
            }
        } else {
            let seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut seen_names = seen_names;

            for keyword in KEYWORDS {
                if keyword.starts_with(word) {
                    matches.push(CompletionItem {
                        name: keyword.to_string(),
                        display: keyword.to_string(),
                    });
                    seen_names.insert(keyword.to_string());
                }
            }

            for (_, symbol) in self.hir.symbols() {
                if let Some(name) = &symbol.value {
                    if name.starts_with('_') || seen_names.contains(name.as_str()) {
                        continue;
                    }
                    if name.starts_with(word) {
                        let display = Self::builtin_display(name);
                        seen_names.insert(name.to_string());
                        matches.push(CompletionItem {
                            name: name.to_string(),
                            display,
                        });
                    }
                }
            }
        }

        matches.sort_by(|a, b| a.name.cmp(&b.name));
        (start, matches)
    }

    fn builtin_display(name: &str) -> String {
        if let Some(doc) = mq_lang::BUILTIN_FUNCTION_DOC.get(name) {
            if doc.params.is_empty() {
                format!("{}()", name)
            } else {
                format!("{}({})", name, doc.params.join(", "))
            }
        } else {
            name.to_string()
        }
    }

    pub fn execute(&mut self, to_run: &str) -> miette::Result<CommandOutput> {
        match to_run.to_string().into() {
            Command::Clear => {
                print!("\x1b[2J\x1b[H");
                let _ = std::io::stdout().flush();
                Ok(CommandOutput::None)
            }
            Command::History => Ok(CommandOutput::History),
            Command::Reset => {
                let mut hir = mq_hir::Hir::default();
                let (source_id, scope_id) = hir.add_new_source(None);
                hir.add_builtin();
                let mut engine = mq_lang::DefaultEngine::default();
                engine.load_builtin_module();
                self.hir = hir;
                self.source_id = source_id;
                self.scope_id = scope_id;
                self.engine = engine;
                self.input = self.initial_input.clone();
                Ok(CommandOutput::None)
            }
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
            Command::SaveFile(file_path) => {
                let content = self.input.iter().map(|v| v.to_string()).collect::<Vec<_>>().join("\n");
                fs::write(file_path, content).into_diagnostic()?;
                Ok(CommandOutput::None)
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
        assert!(matches!(Command::from("/clear".to_string()), Command::Clear));
        assert!(matches!(Command::from("/copy".to_string()), Command::Copy));
        assert!(matches!(Command::from("/edit".to_string()), Command::Edit));
        assert!(matches!(Command::from("/help".to_string()), Command::Help));
        assert!(matches!(Command::from("/history".to_string()), Command::History));
        assert!(matches!(Command::from("/quit".to_string()), Command::Quit));
        assert!(matches!(Command::from("/reset".to_string()), Command::Reset));
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
        assert_eq!(format!("{}", Command::Clear), "/clear");
        assert_eq!(format!("{}", Command::Copy), "/copy");
        assert_eq!(format!("{}", Command::Edit), "/edit");
        assert_eq!(format!("{}", Command::Help), "/help");
        assert_eq!(format!("{}", Command::History), "/history");
        assert_eq!(format!("{}", Command::Quit), "/quit");
        assert_eq!(format!("{}", Command::Reset), "/reset");
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
                Command::Clear => assert!(help.contains("/clear")),
                Command::Copy => assert!(help.contains("/copy")),
                Command::Edit => assert!(help.contains("/edit")),
                Command::Help => assert!(help.contains("/help")),
                Command::History => assert!(help.contains("/history")),
                Command::Quit => assert!(help.contains("/quit")),
                Command::Reset => assert!(help.contains("/reset")),
                Command::Vars => assert!(help.contains("/vars")),
                Command::Version => assert!(help.contains("/version")),
                Command::LoadFile(_) => assert!(help.contains("/load")),
                Command::SaveFile(_) => assert!(help.contains("/save")),
                Command::Env(_, _) => assert!(help.contains("/env")),
                Command::Eval(_) => assert!(help.contains("/eval")),
                Command::NotFound(_) => assert!(help.contains("/not_found")),
            }
        }
    }

    fn names(items: &[CompletionItem]) -> Vec<&str> {
        items.iter().map(|i| i.name.as_str()).collect()
    }

    #[test]
    fn test_completions_basic() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let (start, matches) = ctx.completions("ad", 2);
        assert_eq!(start, 0);
        assert!(names(&matches).contains(&"add"));
    }

    #[test]
    fn test_completions_command() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let (start, matches) = ctx.completions("/he", 3);
        assert_eq!(start, 0);
        assert!(names(&matches).contains(&"/help"));
    }

    #[test]
    fn test_completions_middle_of_line() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let (start, matches) = ctx.completions("let x = ad", 10);
        assert_eq!(start, 8);
        assert!(names(&matches).contains(&"add"));
    }

    #[test]
    fn test_completions_internal_filtered() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let (_, matches) = ctx.completions("_", 1);
        assert!(matches.iter().all(|m| !m.name.starts_with('_')));
    }

    #[test]
    fn test_completions_selector() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let (start, matches) = ctx.completions(".h1", 3);
        assert_eq!(start, 0);
        assert!(names(&matches).contains(&".h1"));
    }

    #[test]
    fn test_completions_function_signature() {
        let engine = mq_lang::DefaultEngine::default();
        let ctx = CommandContext::new(engine, Vec::new());

        let (_, matches) = ctx.completions("ad", 2);
        let add_item = matches.iter().find(|i| i.name == "add");
        assert!(add_item.is_some());
        // add has params so display should include parentheses
        assert!(add_item.unwrap().display.contains('('));
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
    fn test_execute_save_file() {
        let engine = mq_lang::DefaultEngine::default();
        let mut ctx = CommandContext::new(
            engine,
            vec!["# Header".to_string().into(), "Paragraph text.".to_string().into()],
        );
        let temp_file_path = std::env::temp_dir().join("test_execute_save_file.md");

        defer! {
            if temp_file_path.exists() {
                std::fs::remove_file(&temp_file_path).expect("Failed to delete temp file");
            }
        }

        let result = ctx.execute(&format!("/save {}", temp_file_path.to_str().unwrap()));
        assert!(matches!(result, Ok(CommandOutput::None)));

        let saved_content = std::fs::read_to_string(&temp_file_path).expect("Failed to read saved file");
        assert_eq!(saved_content, "# Header\nParagraph text.");
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
