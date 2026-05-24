use clap::{Parser, Subcommand};
use colored::Colorize;
use miette::IntoDiagnostic;
use miette::miette;
use mq_lang::DefaultEngine;
use rayon::prelude::*;
use std::collections::BTreeMap;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::{fs, path::PathBuf};
use which::which;

use crate::grep;

#[derive(Parser, Debug, Default)]
#[command(name = "mq")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(after_help = "# Examples\n\n\
    mq 'query' file.md\n\
    mq -f 'file' file.md        # read query from file\n\
    mq repl                     # start a REPL session\n\n\
    # Auto-parsing by file extension or -I flag\n\n\
    mq automatically imports the matching module based on the file extension.\n\
    Use -I <format> to force a specific format:\n\n\
    .cbor / -I cbor  import \"cbor\" | cbor::cbor_parse()  (reads as bytes)\n\
    .csv  / -I csv   import \"csv\"  | csv::csv_parse(true)\n\
    .hcl  / -I hcl   import \"hcl\"  | hcl::hcl_parse()\n\
    .json / -I json  import \"json\" | json::json_parse()\n\
    .psv  / -I psv   import \"csv\"  | csv::psv_parse(true)\n\
    .toml / -I toml  import \"toml\" | toml::toml_parse()\n\
    .toon / -I toon  import \"toon\" | toon::toon_parse()\n\
    .tsv  / -I tsv   import \"csv\"  | csv::tsv_parse(true)\n\
    .xml  / -I xml   import \"xml\"  | xml::xml_parse()\n\
    .yaml / -I yaml  import \"yaml\" | yaml::yaml_parse()\n\n\
    Use -I raw   to disable auto-parsing and receive the raw string.\n\
    Use -I bytes to read input as raw bytes without parsing.\n\n\
    # Passing arguments to queries (ARGS)\n\n\
    When --args or --argv is given, ARGS = {\"positional\": [...], \"named\": {...}}\n\n\
    mq -I null 'name' --args name Alice\n\
    mq -I null 'ARGS | .\"named\"' --args name Alice\n\
    # => {\"name\": \"Alice\"}\n\n\
    mq -I null 'ARGS | .\"positional\"' --argv x y z  # must come after query and files\n\
    # => [\"x\", \"y\", \"z\"]\n\n\
    mq -I null 'ARGS' file.md --args name Alice --argv x y z\n\
    # => {\"positional\": [\"x\",\"y\",\"z\"], \"named\": {\"name\": \"Alice\"}}\n")]
#[command(
    about = "mq is a markdown processor that can filter markdown nodes by using jq-like syntax.",
    long_about = None
)]
pub struct Cli {
    #[clap(flatten)]
    input: InputArgs,

    #[clap(flatten)]
    output: OutputArgs,

    #[clap(subcommand)]
    commands: Option<Commands>,

    /// List all available subcommands (built-in and external)
    #[arg(long)]
    list: bool,

    /// Number of files to process before switching to parallel processing
    #[arg(short = 'P', default_value_t = 10)]
    parallel_threshold: usize,

    #[arg(value_name = "QUERY OR FILE")]
    query: Option<String>,
    files: Option<Vec<PathBuf>>,

    /// Positional string arguments, available as ARGS."positional" in queries.
    #[arg(long = "argv", num_args = 0..)]
    argv: Option<Vec<String>>,
}

#[cfg(unix)]
const UNIX_EXECUTABLE_BITS: u32 = 0o111;

/// Represents the input format for processing.
///
/// Native formats (no module import):
/// - Markdown: Standard Markdown parsing.
/// - Mdx: MDX parsing.
/// - Html: HTML parsing.
/// - Text: Treats input as plain text.
/// - Null: No input.
/// - Raw: Treats all input as a single string, without parsing.
/// - Bytes: Reads input as raw bytes (`RuntimeValue::Bytes`), without any parsing.
///
/// Module-backed formats (auto-import and parse, sorted alphabetically):
/// - Cbor: Reads input as raw bytes and parses via the `cbor` module.
/// - Csv/Hcl/Json/Psv/Toml/Toon/Tsv/Xml/Yaml: Auto-import the matching module and parse.
#[derive(Clone, Debug, Default, clap::ValueEnum, PartialEq)]
enum InputFormat {
    #[default]
    Markdown,
    Mdx,
    Html,
    Text,
    Null,
    Raw,
    Bytes,
    Cbor,
    Csv,
    Hcl,
    Json,
    Psv,
    Toml,
    Toon,
    Tsv,
    Xml,
    Yaml,
}

impl InputFormat {
    fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "md" | "markdown" => Self::Markdown,
            "mdx" => Self::Mdx,
            "html" | "htm" => Self::Html,
            "txt" | "log" => Self::Raw,
            "jsonl" | "ndjson" => Self::Text,
            "cbor" => Self::Cbor,
            "csv" => Self::Csv,
            "hcl" => Self::Hcl,
            "json" => Self::Json,
            "psv" => Self::Psv,
            "toml" => Self::Toml,
            "toon" => Self::Toon,
            "tsv" => Self::Tsv,
            "xml" => Self::Xml,
            "yaml" | "yml" => Self::Yaml,
            _ => Self::Markdown,
        }
    }

    fn needs_binary_read(&self) -> bool {
        matches!(self, Self::Bytes | Self::Cbor)
    }

    fn module_query_prefix(&self) -> Option<&'static str> {
        match self {
            // Module-backed formats (alphabetical order)
            Self::Cbor => Some(r#"import "cbor" | cbor::cbor_parse()"#),
            Self::Csv => Some(r#"import "csv" | csv::csv_parse(true)"#),
            Self::Hcl => Some(r#"import "hcl" | hcl::hcl_parse()"#),
            Self::Json => Some(r#"import "json" | json::json_parse()"#),
            Self::Psv => Some(r#"import "csv" | csv::psv_parse(true)"#),
            Self::Toml => Some(r#"import "toml" | toml::toml_parse()"#),
            Self::Toon => Some(r#"import "toon" | toon::toon_parse()"#),
            Self::Tsv => Some(r#"import "csv" | csv::tsv_parse(true)"#),
            Self::Xml => Some(r#"import "xml" | xml::xml_parse()"#),
            Self::Yaml => Some(r#"import "yaml" | yaml::yaml_parse()"#),
            _ => None,
        }
    }
}

/// Holds file/stdin content as either UTF-8 text or raw bytes.
enum ContentData {
    Text(String),
    Bytes(Vec<u8>),
}

impl ContentData {
    fn empty() -> Self {
        ContentData::Text(String::new())
    }

    fn as_str(&self) -> Option<&str> {
        match self {
            ContentData::Text(s) => Some(s),
            ContentData::Bytes(_) => None,
        }
    }

    fn as_bytes(&self) -> &[u8] {
        match self {
            ContentData::Text(s) => s.as_bytes(),
            ContentData::Bytes(b) => b,
        }
    }
}

impl From<String> for ContentData {
    fn from(s: String) -> Self {
        ContentData::Text(s)
    }
}

impl From<Vec<u8>> for ContentData {
    fn from(b: Vec<u8>) -> Self {
        ContentData::Bytes(b)
    }
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Markdown,
    Html,
    Text,
    Json,
    Table,
    Grep,
    Raw,
    None,
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
pub enum ListStyle {
    #[default]
    Dash,
    Plus,
    Star,
}

#[derive(Debug, Clone, PartialEq, Default, clap::ValueEnum)]
pub enum LinkTitleStyle {
    #[default]
    Double,
    Single,
    Paren,
}

#[derive(Debug, Clone, PartialEq, Default, clap::ValueEnum)]
pub enum LinkUrlStyle {
    #[default]
    None,
    Angle,
}

#[derive(Clone, Debug, clap::Args, Default)]
struct InputArgs {
    /// Aggregate all input files/content into a single array
    #[arg(short = 'A', long, default_value_t = false)]
    aggregate: bool,

    /// load filter from the file
    #[arg(short, long, default_value_t = false)]
    from_file: bool,

    /// Set input format
    #[arg(short = 'I', long, value_enum)]
    input_format: Option<InputFormat>,

    /// Search modules from the directory
    #[arg(short = 'L', long = "directory")]
    module_directories: Option<Vec<PathBuf>>,

    /// Load additional modules from specified files
    #[arg(short = 'M', long)]
    module_names: Option<Vec<String>>,

    /// Import modules by name, making them available as `name::fn()` in queries
    #[arg(short = 'm', long)]
    import_module_names: Option<Vec<String>>,

    /// Sets a named string argument. NAME is accessible directly in queries, and also
    /// via ARGS."named" when --args or --argv is given.
    #[arg(long, num_args = 2, value_names = ["NAME", "VALUE"], aliases = ["arg", "define"])]
    args: Option<Vec<String>>,

    /// Sets file contents that can be referenced at runtime
    #[arg(long="rawfile", num_args = 2, value_names = ["NAME", "FILE"])]
    raw_file: Option<Vec<String>>,

    /// Enable streaming mode for processing large files line by line
    #[arg(long, default_value_t = false)]
    stream: bool,
}

#[derive(Clone, Debug, clap::Args, Default)]
struct OutputArgs {
    /// Set output format
    #[arg(short = 'F', long, value_enum, default_value_t)]
    output_format: OutputFormat,

    /// Update the input markdown (aliases: -i, --in-place, --inplace)
    #[arg(
        short = 'U',
        long = "update",
        short_alias='i',
        aliases=["in-place", "inplace"],
        default_value_t = false
    )]
    update: bool,

    /// Unbuffered output
    #[clap(long, default_value_t = false)]
    unbuffered: bool,

    /// Set the list style for markdown output
    #[clap(long, value_enum, default_value_t = ListStyle::Dash)]
    list_style: ListStyle,

    /// Set the link title surround style for markdown output
    #[clap(long, value_enum, default_value_t = LinkTitleStyle::Double)]
    link_title_style: LinkTitleStyle,

    /// Set the link URL surround style for markdown links
    #[clap(long, value_enum, default_value_t = LinkUrlStyle::None)]
    link_url_style: LinkUrlStyle,

    /// Specify a query to insert between files as a separator
    #[clap(short = 'S', long, value_name = "QUERY")]
    separator: Option<String>,

    /// Output to the specified file
    #[clap(short = 'o', long = "output", value_name = "FILE")]
    output_file: Option<PathBuf>,

    /// Colorize markdown output
    #[arg(short = 'C', long = "color-output", default_value_t = false)]
    color_output: bool,

    /// Show NUM nodes before each match. Only effective with -F grep.
    #[clap(short = 'B', long, value_name = "NUM")]
    before_context: Option<usize>,

    /// Show NUM nodes after each match. Only effective with -F grep.
    #[clap(long, value_name = "NUM")]
    after_context: Option<usize>,

    /// Show NUM nodes before and after each match. Only effective with -F grep.
    #[clap(long, value_name = "NUM")]
    context: Option<usize>,
}

impl OutputArgs {
    /// Returns `(before, after)` node counts for grep context expansion.
    /// `--context N` sets both sides; `--before-context` / `--after-context` override each side.
    fn context_counts(&self) -> (usize, usize) {
        let base = self.context.unwrap_or(0);
        let before = self.before_context.unwrap_or(base);
        let after = self.after_context.unwrap_or(base);
        (before, after)
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a REPL session for interactive query execution
    Repl,
    /// Start a debug adapter for mq
    #[cfg(feature = "debugger")]
    Dap,
}

impl Cli {
    /// Get the path to the external commands directory (~/.local/bin)
    fn get_external_commands_dir() -> Option<PathBuf> {
        let home_dir = dirs::home_dir()?;
        let mq_bin_dir = home_dir.join(".local").join("bin");
        if mq_bin_dir.exists() && mq_bin_dir.is_dir() {
            Some(mq_bin_dir)
        } else {
            None
        }
    }

    /// Find all external commands (mq-* files in ~/.local/bin and PATH)
    fn find_external_commands() -> Vec<String> {
        let mut seen = std::collections::HashSet::new();

        // Search ~/.local/bin first
        if let Some(bin_dir) = Self::get_external_commands_dir() {
            Self::collect_mq_commands_from_dir(&bin_dir, &mut seen);
        }

        // Search PATH directories
        if let Ok(path_var) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path_var) {
                Self::collect_mq_commands_from_dir(&dir, &mut seen);
            }
        }

        let mut commands: Vec<String> = seen.into_iter().collect();
        commands.sort();
        commands
    }

    /// Collect mq-* command names from a directory.
    fn collect_mq_commands_from_dir(dir: &Path, seen: &mut std::collections::HashSet<String>) {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_name) = entry.file_name().into_string()
                    && file_name.starts_with("mq-")
                    && Self::is_executable_file(&entry)
                    && let Some(subcommand) = file_name.strip_prefix("mq-")
                {
                    let subcommand = Self::strip_executable_extension(subcommand);
                    if !subcommand.is_empty() {
                        seen.insert(subcommand);
                    }
                }
            }
        }
    }

    /// Check if a directory entry is an executable file.
    /// On Windows, checks for executable extensions (.exe, .cmd, .bat, .com).
    /// On Unix, checks for the executable bit in file permissions.
    fn is_executable_file(entry: &fs::DirEntry) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            entry
                .metadata()
                .map(|m| m.is_file() && m.permissions().mode() & UNIX_EXECUTABLE_BITS != 0)
                .unwrap_or(false)
        }
        #[cfg(windows)]
        {
            let path = entry.path();
            let is_file = entry.metadata().map(|m| m.is_file()).unwrap_or(false);
            is_file
                && path.extension().and_then(|e| e.to_str()).is_some_and(|ext| {
                    ext.eq_ignore_ascii_case("exe")
                        || ext.eq_ignore_ascii_case("cmd")
                        || ext.eq_ignore_ascii_case("bat")
                        || ext.eq_ignore_ascii_case("com")
                })
        }
        #[cfg(not(any(unix, windows)))]
        {
            entry.metadata().map(|m| m.is_file()).unwrap_or(false)
        }
    }

    /// Strip known executable extensions on Windows. On Unix, returns the name as-is.
    fn strip_executable_extension(name: &str) -> String {
        if cfg!(windows) {
            let path = Path::new(name);
            match path.extension().and_then(|e| e.to_str()) {
                Some("exe" | "cmd" | "bat" | "com") => {
                    path.file_stem().unwrap_or_default().to_string_lossy().to_string()
                }
                _ => name.to_string(),
            }
        } else {
            name.to_string()
        }
    }

    /// Execute an external subcommand
    fn execute_external_command(&self, command_path: PathBuf, args: &[String]) -> miette::Result<()> {
        if args.is_empty() {
            return Err(miette!("No subcommand specified"));
        }

        let subcommand = &args[0];

        // Check if the file is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::metadata(&command_path).into_diagnostic()?;
            let permissions = metadata.permissions();
            if permissions.mode() & 0o111 == 0 {
                return Err(miette!(
                    "External subcommand 'mq-{}' is not executable. Run: chmod +x {}",
                    subcommand,
                    command_path.display()
                ));
            }
        }

        // Execute the external command with remaining arguments
        let status = Command::new(&command_path).args(&args[1..]).status().map_err(|e| {
            miette!(
                "Failed to execute external subcommand 'mq-{}' at {}: {}",
                subcommand,
                command_path.display(),
                e
            )
        })?;

        if !status.success() {
            let code = status.code().unwrap_or(1);
            std::process::exit(code);
        }

        Ok(())
    }

    /// List all available subcommands (built-in and external)
    fn list_commands(&self) -> miette::Result<()> {
        let mut output = vec![
            format!("{}", "Built-in subcommands:".bold().cyan()),
            format!(
                "  {} - Start a REPL session for interactive query execution",
                "repl".green()
            ),
        ];

        #[cfg(feature = "debugger")]
        output.push(format!("  {} - Start a debug adapter for mq", "dap".green()));

        let external_commands = Self::find_external_commands();
        if !external_commands.is_empty() {
            output.push("".to_string());
            output.push(format!(
                "{}",
                "External subcommands (from ~/.local/bin and PATH):".bold().yellow()
            ));
            for cmd in external_commands {
                output.push(format!("  {}", cmd.bright_yellow()));
            }
        }

        println!("{}", output.join("\n"));
        Ok(())
    }

    pub fn run(&self) -> miette::Result<()> {
        if self.list {
            return self.list_commands();
        }

        if (self.output.before_context.is_some()
            || self.output.after_context.is_some()
            || self.output.context.is_some())
            && !matches!(self.output.output_format, OutputFormat::Grep)
        {
            return Err(miette!(
                "--before-context, --after-context, and --context are only valid with -F grep"
            ));
        }

        // Check if query is actually an external subcommand
        // This handles the case where clap parses "mq test arg1" as query="test", files=["arg1"]
        if !self.input.from_file
            && self.commands.is_none()
            && let Some(query_value) = &self.query
        {
            // Only treat as external command if query_value is a valid file name
            if query_value
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                let command_path = {
                    let command_bin = format!("mq-{}", query_value);
                    let command_path = Self::get_external_commands_dir().unwrap_or_default().join(&command_bin);

                    if !command_path.exists() {
                        which(&command_bin).ok()
                    } else {
                        Some(command_path)
                    }
                };

                if let Some(command_path) = command_path {
                    let mut args = vec![query_value.clone()];
                    if let Some(files) = &self.files {
                        args.extend(files.iter().map(|p| p.to_string_lossy().to_string()));
                    }
                    return self.execute_external_command(command_path, &args);
                }
            }
        }

        if !matches!(self.input.input_format, Some(InputFormat::Markdown) | None) && self.output.update {
            return Err(miette!("The output format is not supported for the update option"));
        }

        match &self.commands {
            Some(Commands::Repl) => mq_repl::Repl::new(vec![mq_lang::RuntimeValue::String("".to_string())]).run(),
            None if self.query.is_none() => {
                mq_repl::Repl::new(vec![mq_lang::RuntimeValue::String("".to_string())]).run()
            }
            #[cfg(feature = "debugger")]
            Some(Commands::Dap) => mq_dap::start().map_err(|e| miette!(e.to_string())),
            None => {
                if self.input.stream {
                    self.process_streaming()
                } else {
                    self.process_batch()
                }
            }
        }
    }

    fn create_engine(&self) -> miette::Result<DefaultEngine> {
        let mut engine = mq_lang::DefaultEngine::default();
        engine.load_builtin_module();

        if self.input.aggregate {
            engine.import_module("section").map_err(|e| *e)?;
        }

        if let Some(dirs) = &self.input.module_directories {
            engine.set_search_paths(dirs.clone());
        }

        if let Some(modules) = &self.input.module_names {
            for module_name in modules {
                engine.load_module(module_name).map_err(|e| *e)?;
            }
        }

        if let Some(modules) = &self.input.import_module_names {
            for module_name in modules {
                engine.import_module(module_name).map_err(|e| *e)?;
            }
        }

        if self.input.args.is_some() || self.argv.is_some() {
            let mut named: BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue> = BTreeMap::new();
            if let Some(args) = &self.input.args {
                for v in args.chunks(2) {
                    engine.define_string_value(&v[0], &v[1]);
                    named.insert(mq_lang::Ident::new(&v[0]), mq_lang::RuntimeValue::String(v[1].clone()));
                }
            }
            let positional: Vec<mq_lang::RuntimeValue> = self
                .argv
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|s| mq_lang::RuntimeValue::String(s.clone()))
                .collect();
            let args_map: BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue> = [
                (
                    mq_lang::Ident::new("positional"),
                    mq_lang::RuntimeValue::Array(positional),
                ),
                (mq_lang::Ident::new("named"), mq_lang::RuntimeValue::Dict(named)),
            ]
            .into_iter()
            .collect();
            engine.define_value("ARGS", mq_lang::RuntimeValue::Dict(args_map));
        }

        if let Some(raw_file) = &self.input.raw_file {
            for v in raw_file.chunks(2) {
                let path = PathBuf::from_str(&v[1]).into_diagnostic()?;

                if !path.exists() {
                    return Err(miette!("File not found: {}", path.display()));
                }

                let content = fs::read_to_string(&path).into_diagnostic()?;
                engine.define_string_value(&v[0], &content);
            }
        }

        #[cfg(feature = "debugger")]
        {
            use crate::debugger::DebuggerHandler;
            let handler = DebuggerHandler::new(engine.clone());
            engine.set_debugger_handler(Box::new(handler));
            engine.debugger().write().unwrap().activate();
        }

        Ok(engine)
    }

    fn get_query(&self) -> miette::Result<String> {
        let query = match self.query.as_ref() {
            Some(q) if self.input.from_file => {
                let path = PathBuf::from_str(q).into_diagnostic()?;
                fs::read_to_string(path).into_diagnostic()?
            }
            Some(q) => q.clone(),
            None => return Err(miette!("Query is required")),
        };

        let aggregate = self.input.aggregate.then_some("nodes");
        Ok(aggregate.map(|agg| format!("{} | {}", agg, query)).unwrap_or(query))
    }

    /// Returns a query prefix that auto-imports and parses a module-backed format.
    ///
    /// If an explicit `-I <format>` is given and it is a module-backed format, the prefix
    /// for that format is returned. Otherwise the file extension is used for detection.
    fn auto_query_prefix(&self, file: &Option<PathBuf>) -> Option<String> {
        if let Some(fmt) = &self.input.input_format {
            return fmt.module_query_prefix().map(str::to_string);
        }
        let ext = file.as_ref()?.extension()?.to_string_lossy().to_lowercase();
        InputFormat::from_extension(&ext)
            .module_query_prefix()
            .map(str::to_string)
    }

    fn set_file_vars(&self, engine: &mut mq_lang::DefaultEngine, file: &Path) {
        engine.define_string_value("__FILE__", file.to_string_lossy().as_ref());
        engine.define_string_value(
            "__FILE_NAME__",
            file.file_name().unwrap_or_default().to_string_lossy().as_ref(),
        );
        engine.define_string_value(
            "__FILE_STEM__",
            file.file_stem().unwrap_or_default().to_string_lossy().as_ref(),
        );
    }

    fn resolve_input(
        &self,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<Vec<mq_lang::RuntimeValue>> {
        let text = content.as_str().unwrap_or("");
        Ok(
            match self.input.input_format.as_ref().cloned().unwrap_or_else(|| {
                if let Some(file) = file {
                    InputFormat::from_extension(&file.extension().unwrap_or_default().to_string_lossy())
                } else if io::stdin().is_terminal() {
                    InputFormat::Null
                } else {
                    InputFormat::Markdown
                }
            }) {
                // Native formats
                InputFormat::Markdown => mq_lang::parse_markdown_input(text)?,
                InputFormat::Mdx => mq_lang::parse_mdx_input(text)?,
                InputFormat::Html => mq_lang::parse_html_input(text)?,
                InputFormat::Text => mq_lang::parse_text_input(text)?,
                InputFormat::Null => mq_lang::null_input(),
                InputFormat::Raw => mq_lang::raw_input(text),
                // Bytes: pass raw binary content as RuntimeValue::Bytes with no further parsing.
                InputFormat::Bytes => mq_lang::bytes_input(content.as_bytes()),
                // Module-backed binary format: pass raw bytes; the cbor module handles parsing.
                InputFormat::Cbor => mq_lang::bytes_input(content.as_bytes()),
                // Module-backed text formats (alphabetical): pass raw string; the module handles parsing.
                InputFormat::Csv
                | InputFormat::Hcl
                | InputFormat::Json
                | InputFormat::Psv
                | InputFormat::Toml
                | InputFormat::Toon
                | InputFormat::Tsv
                | InputFormat::Xml
                | InputFormat::Yaml => mq_lang::raw_input(text),
            },
        )
    }

    fn apply_update(
        &self,
        input: Vec<mq_lang::RuntimeValue>,
        results: mq_lang::RuntimeValues,
    ) -> miette::Result<mq_lang::RuntimeValues> {
        let current_values: mq_lang::RuntimeValues = input.into();
        if current_values.len() != results.len() {
            return Err(miette!("The number of input and output values do not match"));
        }
        Ok(current_values.update_with(results))
    }

    fn emit_results(
        &self,
        runtime_values: mq_lang::RuntimeValues,
        grep_input: Option<Vec<mq_lang::RuntimeValue>>,
        file: &Option<PathBuf>,
    ) -> miette::Result<()> {
        if let Some(input) = grep_input {
            let (before, after) = self.output.context_counts();
            grep::print_grep(
                runtime_values,
                &input,
                file,
                &self.output.output_file,
                self.output.unbuffered,
                before,
                after,
            )
        } else {
            self.print(runtime_values)
        }
    }

    fn execute(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        query: &str,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        let effective_query;
        let query = match self.auto_query_prefix(file) {
            Some(prefix) => {
                effective_query = format!("{} | {}", prefix, query);
                effective_query.as_str()
            }
            None => query,
        };

        if let Some(f) = file {
            self.set_file_vars(engine, f);
        }

        let input = self.resolve_input(file, content)?;
        let is_grep = matches!(self.output.output_format, OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| input.clone());

        let runtime_values = if self.output.update {
            let results = engine.eval(query, input.clone().into_iter()).map_err(|e| *e)?;
            self.apply_update(input, results)?
        } else {
            engine.eval(query, input.into_iter()).map_err(|e| *e)?
        };

        if let Some(separator) = &self.output.separator {
            let separator = engine
                .eval(
                    separator,
                    vec![mq_lang::RuntimeValue::String("".to_string())].into_iter(),
                )
                .map_err(|e| *e)?;
            self.print(separator)?;
        }

        self.emit_results(runtime_values, grep_input, file)
    }

    /// Returns the effective query string combining any auto-prefix with the base query.
    fn effective_query(&self, query: &str, file: &Option<PathBuf>) -> String {
        match self.auto_query_prefix(file) {
            Some(prefix) => format!("{} | {}", prefix, query),
            None => query.to_string(),
        }
    }

    /// Returns true if all files would produce the same effective query prefix.
    fn all_files_same_prefix(&self, files: &[(Option<PathBuf>, ContentData)]) -> bool {
        if files.is_empty() {
            return true;
        }
        let first = self.auto_query_prefix(&files[0].0);
        files[1..].iter().all(|(f, _)| self.auto_query_prefix(f) == first)
    }

    fn process_batch(&self) -> Result<(), miette::Error> {
        let query = self.get_query()?;
        let files = self.read_contents()?;

        if files.len() > self.parallel_threshold {
            files.par_iter().try_for_each(|(file, content)| {
                let mut engine = self.create_engine()?;
                self.execute(&mut engine, &query, file, content)
            })?;
        } else {
            let mut engine = self.create_engine()?;

            // Pre-compile query if all files share the same effective query (same prefix)
            if files.len() > 1 && self.all_files_same_prefix(&files) && self.output.separator.is_none() {
                let effective = self.effective_query(&query, &files[0].0);
                let program = engine.compile(&effective).map_err(|e| *e)?;
                for (file, content) in &files {
                    self.execute_compiled(&mut engine, &program, file, content)?;
                }
            } else {
                files
                    .iter()
                    .try_for_each(|(file, content)| self.execute(&mut engine, &query, file, content))?;
            }
        }

        Ok(())
    }

    fn execute_compiled(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        program: &mq_lang::Program,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        if let Some(f) = file {
            self.set_file_vars(engine, f);
        }

        let input = self.resolve_input(file, content)?;
        let is_grep = matches!(self.output.output_format, OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| input.clone());

        let runtime_values = if self.output.update {
            let results = engine
                .eval_compiled(program, input.clone().into_iter())
                .map_err(|e| *e)?;
            self.apply_update(input, results)?
        } else {
            engine.eval_compiled(program, input.into_iter()).map_err(|e| *e)?
        };

        self.emit_results(runtime_values, grep_input, file)
    }

    fn process_streaming(&self) -> miette::Result<()> {
        if self.is_binary_format() {
            return Err(miette!(
                "Streaming mode is not supported for binary input formats (bytes, cbor)"
            ));
        }
        let query = self.get_query()?;
        let mut engine = self.create_engine()?;

        self.process_lines(|file, line| self.execute(&mut engine, &query, &file.cloned(), &line.into()))
    }

    fn process_lines<F>(&self, mut process: F) -> miette::Result<()>
    where
        F: FnMut(Option<&PathBuf>, String) -> miette::Result<()>,
    {
        // If files are specified, process each file line by line
        if let Some(files) = &self.files {
            for file in files {
                let file_handle = fs::File::open(file).into_diagnostic()?;
                let reader = io::BufReader::new(file_handle);
                for line_result in reader.lines() {
                    let line = line_result.into_diagnostic()?;
                    process(Some(file), line)?;
                }
            }
        } else {
            // Otherwise, process stdin line by line
            let stdin = io::stdin();
            let reader = io::BufReader::new(stdin.lock());
            for line_result in reader.lines() {
                let line = line_result.into_diagnostic()?;
                process(None, line)?;
            }
        }
        Ok(())
    }

    fn is_binary_format(&self) -> bool {
        matches!(
            self.input.input_format,
            Some(InputFormat::Bytes) | Some(InputFormat::Cbor)
        )
    }

    fn needs_binary_read_for_file(&self, file: &Path) -> bool {
        self.input
            .input_format
            .as_ref()
            .map(|fmt| fmt.needs_binary_read())
            .unwrap_or_else(|| {
                let ext = file.extension().unwrap_or_default().to_string_lossy().to_lowercase();
                InputFormat::from_extension(&ext).needs_binary_read()
            })
    }

    fn read_contents(&self) -> miette::Result<Vec<(Option<PathBuf>, ContentData)>> {
        if matches!(self.input.input_format, Some(InputFormat::Null)) {
            return Ok(vec![(None, ContentData::empty())]);
        }

        self.files
            .clone()
            .map(|files| {
                let load_contents: miette::Result<Vec<ContentData>> = files
                    .iter()
                    .map(|file| {
                        if self.needs_binary_read_for_file(file) {
                            fs::read(file).map(Into::into).into_diagnostic()
                        } else {
                            fs::read_to_string(file).map(Into::into).into_diagnostic()
                        }
                    })
                    .collect();
                load_contents.map(move |contents| {
                    files
                        .into_iter()
                        .zip(contents)
                        .map(|(file, content)| (Some(file), content))
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_else(|| {
                if io::stdin().is_terminal() {
                    return Ok(vec![(None, ContentData::empty())]);
                }

                if self.is_binary_format() {
                    let mut buf = Vec::new();
                    io::stdin().read_to_end(&mut buf).into_diagnostic()?;
                    Ok(vec![(None, buf.into())])
                } else {
                    let mut input = String::new();
                    io::stdin().read_to_string(&mut input).into_diagnostic()?;
                    Ok(vec![(None, input.into())])
                }
            })
    }

    /// Returns `true` if the `NO_COLOR` environment variable is set and non-empty.
    fn is_no_color() -> bool {
        std::env::var("NO_COLOR").is_ok_and(|v| !v.is_empty())
    }

    #[inline(always)]
    fn write_ignore_pipe<W: Write>(handle: &mut W, data: &[u8]) -> miette::Result<()> {
        match handle.write_all(data) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
            Err(e) => Err(miette!(e)),
        }
    }

    /// Recursively collects Markdown nodes from a `RuntimeValue`.
    ///
    /// Flattens nested `Array` values so that any Markdown nodes contained
    /// within are returned as a flat list.
    fn collect_markdown_nodes(value: &mq_lang::RuntimeValue, nodes: &mut Vec<mq_markdown::Node>) {
        match value {
            mq_lang::RuntimeValue::Markdown(node, _) => nodes.push((**node).clone()),
            mq_lang::RuntimeValue::Array(items) => {
                for item in items {
                    Self::collect_markdown_nodes(item, nodes);
                }
            }
            _ => {}
        }
    }

    /// Returns `true` if the dict is a known expandable typed dict (has `type: :symbol`).
    fn is_typed_dict(map: &std::collections::BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue>) -> bool {
        let type_key = mq_lang::Ident::new("type");
        matches!(
            map.get(&type_key),
            Some(mq_lang::RuntimeValue::Symbol(s)) if matches!(s.as_str().as_str(), "section" | "table")
        )
    }

    /// Expands a typed dict (one with `type: :symbol`) into Markdown nodes.
    ///
    /// Returns `None` if the dict is not a known expandable type.
    /// To add support for a new type, add a match arm for the type name.
    fn expand_typed_dict(
        map: &std::collections::BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue>,
    ) -> Option<Vec<mq_markdown::Node>> {
        let type_key = mq_lang::Ident::new("type");
        match map.get(&type_key) {
            Some(mq_lang::RuntimeValue::Symbol(s)) => match s.as_str().as_str() {
                "section" => {
                    let mut nodes = Vec::new();
                    if let Some(header) = map.get(&mq_lang::Ident::new("header")) {
                        Self::collect_markdown_nodes(header, &mut nodes);
                    }
                    if let Some(children) = map.get(&mq_lang::Ident::new("children")) {
                        Self::collect_markdown_nodes(children, &mut nodes);
                    }
                    Some(nodes)
                }
                "table" => {
                    // Reconstruct table nodes in the same order as table::to_markdown():
                    // header cells + align row + flattened data rows
                    let mut nodes = Vec::new();
                    if let Some(header) = map.get(&mq_lang::Ident::new("header")) {
                        Self::collect_markdown_nodes(header, &mut nodes);
                    }
                    if let Some(align) = map.get(&mq_lang::Ident::new("align")) {
                        Self::collect_markdown_nodes(align, &mut nodes);
                    }
                    if let Some(rows) = map.get(&mq_lang::Ident::new("rows")) {
                        Self::collect_markdown_nodes(rows, &mut nodes);
                    }
                    Some(nodes)
                }
                // To add a new expandable type: add a match arm here.
                _ => None,
            },
            _ => None,
        }
    }

    /// Converts a `RuntimeValue` into a list of Markdown nodes.
    fn runtime_value_to_nodes(runtime_value: &mq_lang::RuntimeValue) -> Vec<mq_markdown::Node> {
        match runtime_value {
            mq_lang::RuntimeValue::Markdown(node, _) => vec![(**node).clone()],
            mq_lang::RuntimeValue::Dict(map) => {
                Self::expand_typed_dict(map).unwrap_or_else(|| vec![runtime_value.to_string().into()])
            }
            mq_lang::RuntimeValue::Array(items) => {
                let has_expandable = items.iter().any(|v| match v {
                    mq_lang::RuntimeValue::Markdown(_, _) => true,
                    mq_lang::RuntimeValue::Dict(m) => Self::is_typed_dict(m),
                    _ => false,
                });
                if has_expandable {
                    items.iter().flat_map(Self::runtime_value_to_nodes).collect()
                } else if items.is_empty() {
                    vec![]
                } else {
                    vec![runtime_value.to_string().into()]
                }
            }
            _ => vec![runtime_value.to_string().into()],
        }
    }

    fn build_markdown(&self, runtime_values: &[mq_lang::RuntimeValue]) -> mq_markdown::Markdown {
        let mut markdown =
            mq_markdown::Markdown::new(runtime_values.iter().flat_map(Self::runtime_value_to_nodes).collect());
        markdown.set_options(mq_markdown::RenderOptions {
            list_style: match self.output.list_style.clone() {
                ListStyle::Dash => mq_markdown::ListStyle::Dash,
                ListStyle::Plus => mq_markdown::ListStyle::Plus,
                ListStyle::Star => mq_markdown::ListStyle::Star,
            },
            link_title_style: match self.output.link_title_style.clone() {
                LinkTitleStyle::Double => mq_markdown::TitleSurroundStyle::Double,
                LinkTitleStyle::Single => mq_markdown::TitleSurroundStyle::Single,
                LinkTitleStyle::Paren => mq_markdown::TitleSurroundStyle::Paren,
            },
            link_url_style: match self.output.link_url_style.clone() {
                LinkUrlStyle::None => mq_markdown::UrlSurroundStyle::None,
                LinkUrlStyle::Angle => mq_markdown::UrlSurroundStyle::Angle,
            },
        });
        markdown
    }

    fn print(&self, runtime_values: mq_lang::RuntimeValues) -> miette::Result<()> {
        let stdout = io::stdout();
        let mut handle: Box<dyn Write> = if let Some(output_file) = &self.output.output_file {
            let file = fs::File::create(output_file).into_diagnostic()?;
            Box::new(BufWriter::new(file))
        } else if self.output.unbuffered {
            Box::new(stdout.lock())
        } else {
            Box::new(BufWriter::new(stdout.lock()))
        };
        let runtime_values = runtime_values.values();

        match self.output.output_format {
            OutputFormat::Raw => {
                for value in runtime_values {
                    match value {
                        mq_lang::RuntimeValue::Bytes(b) => Self::write_ignore_pipe(&mut handle, b)?,
                        _ => Self::write_ignore_pipe(&mut handle, value.to_string().as_bytes())?,
                    }
                }
            }
            OutputFormat::Json => {
                let theme = (self.output.color_output && !Self::is_no_color()).then(mq_markdown::ColorTheme::from_env);
                let json_str = crate::json::runtime_values_to_json(runtime_values, theme.as_ref())?;
                Self::write_ignore_pipe(&mut handle, json_str.as_bytes())?;
            }
            OutputFormat::Html => {
                let markdown = self.build_markdown(runtime_values);
                Self::write_ignore_pipe(&mut handle, markdown.to_html().as_bytes())?;
            }
            OutputFormat::Text => {
                let markdown = self.build_markdown(runtime_values);
                Self::write_ignore_pipe(&mut handle, markdown.to_text().as_bytes())?;
            }
            OutputFormat::Markdown if self.output.color_output && !Self::is_no_color() => {
                let markdown = self.build_markdown(runtime_values);
                let theme = mq_markdown::ColorTheme::from_env();
                Self::write_ignore_pipe(&mut handle, markdown.to_colored_string_with_theme(&theme).as_bytes())?;
            }
            OutputFormat::Markdown => {
                let markdown = self.build_markdown(runtime_values);
                Self::write_ignore_pipe(&mut handle, markdown.to_string().as_bytes())?;
            }
            OutputFormat::Table => {
                let theme = (self.output.color_output && !Self::is_no_color()).then(mq_markdown::ColorTheme::from_env);
                let table = crate::table::runtime_values_to_table(runtime_values, theme.as_ref());
                Self::write_ignore_pipe(&mut handle, format!("{}\n", table).as_bytes())?;
            }
            OutputFormat::Grep => {
                let markdown = self.build_markdown(runtime_values);
                Self::write_ignore_pipe(&mut handle, markdown.to_string().as_bytes())?;
            }
            OutputFormat::None => {}
        }

        if !self.output.unbuffered
            && let Err(e) = handle.flush()
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(miette!(e));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
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
    fn test_cli_null_input() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("self".to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_raw_input() {
        let (_, temp_file_path) = create_file("test1.md", "# test");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Text),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_output_formats() {
        let (_, temp_file_path) = create_file("test2.md", "# test");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        for format in [
            OutputFormat::Markdown,
            OutputFormat::Html,
            OutputFormat::Text,
            OutputFormat::Table,
            OutputFormat::Grep,
        ] {
            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    output_format: format.clone(),
                    ..Default::default()
                },
                commands: None,
                query: Some("self".to_string()),
                files: Some(vec![temp_file_path.clone()]),
                ..Cli::default()
            };

            assert!(cli.run().is_ok());
        }
    }

    #[test]
    fn test_cli_list_styles() {
        let (_, temp_file_path) = create_file("test3.md", "# test");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        for style in [ListStyle::Dash, ListStyle::Plus, ListStyle::Star] {
            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    list_style: style.clone(),
                    ..Default::default()
                },
                commands: None,
                query: Some("self".to_string()),
                files: Some(vec![temp_file_path.clone()]),
                ..Cli::default()
            };

            assert!(cli.run().is_ok());
        }
    }

    #[test]
    fn test_cli_color_output() {
        let (_, temp_file_path) = create_file("test_color.md", "# test");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                color_output: true,
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path.clone()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_update_flag() {
        let (_, temp_file_path) = create_file("test4.md", "# test");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                update: true,
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_with_module_names() {
        let (temp_dir, temp_file_path) = create_file("math.mq", "def math(): 42;");
        let (_, temp_md_file_path) = create_file("test.md", "# test");
        let temp_md_file_path_clone = temp_md_file_path.clone();

        defer! {
            if temp_file_path.exists() {
                std::fs::remove_file(&temp_file_path).expect("Failed to delete temp file");
            }

            if temp_md_file_path_clone.exists() {
                std::fs::remove_file(&temp_md_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let cli = Cli {
            input: InputArgs {
                module_names: Some(vec!["math".to_string()]),
                module_directories: Some(vec![temp_dir.clone()]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("math".to_owned()),
            files: Some(vec![temp_md_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_find_external_commands() {
        // find_external_commands searches ~/.local/bin and PATH for mq-* files
        let commands = Cli::find_external_commands();
        // We can't assert specific commands, but we can check the function works
        assert!(commands.iter().all(|cmd| !cmd.is_empty()));
    }

    #[test]
    fn test_get_external_commands_dir() {
        // This test checks if the function returns a valid path or None
        let dir = Cli::get_external_commands_dir();
        if let Some(path) = dir {
            assert!(path.ends_with(".local/bin") || path.ends_with(".local\\bin"));
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_collect_mq_commands_from_dir() {
        let temp_dir = std::env::temp_dir().join("mq-collect-test");
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        // Create test files: mq-foo, mq-bar, a non-mq file, and a non-executable mq file
        fs::write(temp_dir.join("mq-foo"), "").expect("Failed to write file");
        fs::write(temp_dir.join("mq-bar"), "").expect("Failed to write file");
        fs::write(temp_dir.join("other-cmd"), "").expect("Failed to write file");
        fs::write(temp_dir.join("mq-noexec"), "").expect("Failed to write file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Set executable bit on mq-foo and mq-bar, but not mq-noexec
            fs::set_permissions(temp_dir.join("mq-foo"), fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
            fs::set_permissions(temp_dir.join("mq-bar"), fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
        }

        let mut seen = std::collections::HashSet::new();
        Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);

        assert_eq!(seen.len(), 2);
        assert!(seen.contains("foo"));
        assert!(seen.contains("bar"));
        assert!(!seen.contains("other-cmd"));
        assert!(!seen.contains("noexec"));
    }

    #[test]
    #[cfg(unix)]
    fn test_collect_mq_commands_from_dir_deduplicates() {
        let dir1 = std::env::temp_dir().join("mq-dedup-test-1");
        let dir2 = std::env::temp_dir().join("mq-dedup-test-2");
        fs::create_dir_all(&dir1).expect("Failed to create test directory");
        fs::create_dir_all(&dir2).expect("Failed to create test directory");

        defer! {
            if dir1.exists() {
                std::fs::remove_dir_all(&dir1).ok();
            }
            if dir2.exists() {
                std::fs::remove_dir_all(&dir2).ok();
            }
        }

        // Same command in both directories
        fs::write(dir1.join("mq-dup"), "").expect("Failed to write file");
        fs::write(dir2.join("mq-dup"), "").expect("Failed to write file");
        fs::write(dir2.join("mq-unique"), "").expect("Failed to write file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(dir1.join("mq-dup"), fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
            fs::set_permissions(dir2.join("mq-dup"), fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
            fs::set_permissions(dir2.join("mq-unique"), fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
        }

        let mut seen = std::collections::HashSet::new();
        Cli::collect_mq_commands_from_dir(&dir1, &mut seen);
        Cli::collect_mq_commands_from_dir(&dir2, &mut seen);

        assert_eq!(seen.len(), 2);
        assert!(seen.contains("dup"));
        assert!(seen.contains("unique"));
    }

    #[test]
    fn test_collect_mq_commands_from_nonexistent_dir() {
        let nonexistent = std::env::temp_dir().join("mq-nonexistent-dir");
        let mut seen = std::collections::HashSet::new();
        // Should not panic on nonexistent directory
        Cli::collect_mq_commands_from_dir(&nonexistent, &mut seen);
        assert!(seen.is_empty());
    }

    #[rstest]
    #[case("foo", "foo")]
    #[case("foo.exe", "foo.exe")]
    #[case("foo.cmd", "foo.cmd")]
    #[case("foo.bat", "foo.bat")]
    #[case("foo.sh", "foo.sh")]
    #[cfg(not(windows))]
    fn test_strip_executable_extension_unix(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(Cli::strip_executable_extension(input), expected);
    }

    #[rstest]
    #[case("foo.exe", "foo")]
    #[case("foo.cmd", "foo")]
    #[case("foo.bat", "foo")]
    #[case("foo.com", "foo")]
    #[case("foo", "foo")]
    #[case("foo.sh", "foo.sh")]
    #[case("foo.txt", "foo.txt")]
    #[cfg(windows)]
    fn test_strip_executable_extension_windows(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(Cli::strip_executable_extension(input), expected);
    }

    #[test]
    fn test_external_command_execution() {
        // Create a temporary directory for testing
        let temp_dir = std::env::temp_dir().join("mq-run-test");
        let bin_dir = temp_dir.join(".mq").join("bin");
        fs::create_dir_all(&bin_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        // Create a test external command
        let test_cmd_path = bin_dir.join("mq-testcmd");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&test_cmd_path, "#!/bin/sh\necho 'test output'").expect("Failed to write test command");
            let mut perms = fs::metadata(&test_cmd_path)
                .expect("Failed to get metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&test_cmd_path, perms).expect("Failed to set permissions");
        }
        #[cfg(not(unix))]
        {
            fs::write(&test_cmd_path, "@echo off\necho test output").expect("Failed to write test command");
        }

        // Note: We can't easily test execute_external_command without modifying HOME
        // This test just verifies the command file was created correctly
        assert!(test_cmd_path.exists());
    }

    #[test]
    fn test_input_format_mdx() {
        let (_, temp_file_path) = create_file("test_mdx.mdx", "# MDX test");
        let (_, output_file) = create_file("test_mdx_output.md", "");
        let temp_file_path_clone = temp_file_path.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Mdx),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(output_content.contains("# MDX test"), "Output should contain heading");
    }

    #[test]
    fn test_input_format_html() {
        let (_, temp_file_path) = create_file("test_html.html", "<h1>HTML test</h1>");
        let (_, output_file) = create_file("test_html_output.md", "");
        let temp_file_path_clone = temp_file_path.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Html),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(
            output_content.contains("# HTML test"),
            "Output should contain converted heading"
        );
    }

    #[test]
    fn test_output_format_json_markdown_input() {
        let (_, temp_file_path) = create_file("test_json_md_input.md", "# Test");
        let (_, output_file) = create_file("test_json_md_output.json", "");
        let temp_file_path_clone = temp_file_path.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Json,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
        assert!(parsed.is_array(), "Markdown JSON output should be an array");
        let nodes = parsed.as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["type"], "Heading", "Markdown heading should have type=Heading");
    }

    #[test]
    fn test_output_format_json_with_json_object_input() {
        let (_, temp_file_path) = create_file("test_json_obj_input.json", r#"{"id": 1, "name": "Alice"}"#);
        let (_, output_file) = create_file("test_json_obj_output.json", "");
        let temp_file_path_clone = temp_file_path.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Json,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
        assert!(parsed.is_object(), "JSON object input should output a JSON object");
        assert_eq!(parsed["id"], 1.0, "id field should be preserved");
        assert_eq!(parsed["name"], "Alice", "name field should be preserved");
        assert!(
            parsed.get("type").is_none(),
            "Output should not contain Markdown AST 'type' field"
        );
    }

    #[test]
    fn test_output_format_json_with_json_array_input() {
        let (_, temp_file_path) = create_file(
            "test_json_arr_input.json",
            r#"[{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]"#,
        );
        let (_, output_file) = create_file("test_json_arr_output.json", "");
        let temp_file_path_clone = temp_file_path.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Json,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
        assert!(parsed.is_array(), "JSON array input should output a JSON array");
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2, "Array should have 2 elements");
        assert_eq!(arr[0]["id"], 1.0);
        assert_eq!(arr[0]["name"], "Alice");
        assert_eq!(arr[1]["id"], 2.0);
        assert_eq!(arr[1]["name"], "Bob");
        assert!(
            arr[0].get("type").is_none(),
            "Output should not contain Markdown AST fields"
        );
    }

    #[test]
    fn test_output_format_raw() {
        let (_, output_file) = create_file("test_raw_output.bin", "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some(r#"to_bytes("hello")"#.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_bytes = fs::read(&output_file).expect("Failed to read output");
        assert_eq!(output_bytes, b"hello");
    }

    #[rstest]
    #[case::from_string("raw_from_string", r#"to_bytes("hello")"#, b"hello" as &[u8])]
    #[case::from_number_array("raw_from_array", "to_bytes([104, 101, 108, 108, 111])", b"hello")]
    #[case::binary_data("raw_binary", "to_bytes([0, 255, 128, 1])", &[0u8, 255, 128, 1])]
    #[case::non_bytes_string("raw_string_value", r#""hello""#, b"hello")]
    #[case::utf8("raw_utf8", r#"to_bytes("あ")"#, &[0xe3u8, 0x81, 0x82])]
    #[case::empty("raw_empty", "to_bytes([])", b"")]
    fn test_output_format_raw_bytes(#[case] suffix: &str, #[case] query: &str, #[case] expected: &[u8]) {
        let (_, output_file) = create_file(&format!("test_{}.bin", suffix), "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_bytes = fs::read(&output_file).expect("Failed to read output");
        assert_eq!(output_bytes, expected);
    }

    #[test]
    fn test_output_format_none() {
        let (_, temp_file_path) = create_file("test_none.md", "# Test");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::None,
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_output_format_table_single_column() {
        let (_, temp_file_path) = create_file("test_table.md", "# Test\n\nContent");
        let (_, output_file) = create_file("test_table_output.md", "");
        let temp_file_path_clone = temp_file_path.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Table,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(output_content.contains("value"), "Table should have value header");
        assert!(output_content.contains("Test"), "Table should contain node text");
    }

    #[test]
    fn test_output_format_table_dict() {
        let (_, output_file) = create_file("test_table_dict_output.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Table,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some(r#"{name: "Alice", age: "30"}"#.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(output_content.contains("name"), "Table should contain name column");
        assert!(output_content.contains("age"), "Table should contain age column");
        assert!(output_content.contains("Alice"), "Table should contain Alice");
        assert!(output_content.contains("30"), "Table should contain 30");
    }

    #[test]
    fn test_output_format_table_nested_dict() {
        let (_, output_file) = create_file("test_table_nested_dict_output.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Table,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some(r#"{name: "Alice", addr: {city: "Tokyo", zip: "100"}}"#.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(output_content.contains("addr"), "Table should contain addr column");
        assert!(output_content.contains("name"), "Table should contain name column");
        assert!(output_content.contains("Alice"), "Table should contain Alice");
        assert!(output_content.contains("city"), "Nested table should contain city key");
        assert!(output_content.contains("Tokyo"), "Nested table should contain Tokyo");
        assert!(output_content.contains("zip"), "Nested table should contain zip key");
        assert!(output_content.contains("100"), "Nested table should contain 100");
        assert!(!output_content.contains("addr.city"), "Dot notation must not appear");
    }

    #[test]
    fn test_output_format_table_array_value() {
        let (_, output_file) = create_file("test_table_array_value_output.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Table,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some(r#"{name: "Alice", tags: ["a", "b"]}"#.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(output_content.contains("tags"), "Table should contain tags column");
        assert!(output_content.contains('a'), "Nested table should contain a");
        assert!(output_content.contains('b'), "Nested table should contain b");
        assert!(output_content.contains("Alice"), "Table should contain Alice");
        assert!(!output_content.contains(r#"["a""#), "Raw array repr must not appear");
    }

    #[test]
    fn test_output_format_table_array_input() {
        let (_, output_file) = create_file("test_table_array_input_output.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Table,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some(r#"[{a: "1"}, {a: "2"}]"#.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(output_content.contains('a'), "Table should have column 'a'");
        assert!(output_content.contains('1'), "Row 1 value should appear");
        assert!(output_content.contains('2'), "Row 2 value should appear");
        assert!(
            !output_content.contains("value"),
            "Should not fall back to 'value' column"
        );
    }

    #[test]
    fn test_link_title_styles() {
        let (_, temp_file_path) = create_file("test_link_title.md", "[link](url \"title\")");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
        }

        for (style, expected_char) in [
            (LinkTitleStyle::Double, '"'),
            (LinkTitleStyle::Single, '\''),
            (LinkTitleStyle::Paren, '('),
        ] {
            let (_, output_file) = create_file(&format!("test_link_title_{:?}.md", style), "");
            let output_file_clone = output_file.clone();

            defer! {
                if output_file_clone.exists() {
                    std::fs::remove_file(&output_file_clone).ok();
                }
            }

            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    link_title_style: style.clone(),
                    output_file: Some(output_file.clone()),
                    ..Default::default()
                },
                commands: None,
                query: Some("self".to_string()),
                files: Some(vec![temp_file_path.clone()]),
                ..Cli::default()
            };

            assert!(cli.run().is_ok());
            let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
            if style == LinkTitleStyle::Paren {
                assert!(
                    output_content.contains("(title)"),
                    "Paren style should wrap title with parens"
                );
            } else {
                assert!(
                    output_content.contains(expected_char),
                    "Link title should use {:?} style",
                    style
                );
            }
        }
    }

    #[test]
    fn test_link_url_styles() {
        let (_, temp_file_path) = create_file("test_link_url.md", "[link](https://example.com)");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
        }

        for style in [LinkUrlStyle::None, LinkUrlStyle::Angle] {
            let (_, output_file) = create_file(&format!("test_link_url_{:?}.md", style), "");
            let output_file_clone = output_file.clone();

            defer! {
                if output_file_clone.exists() {
                    std::fs::remove_file(&output_file_clone).ok();
                }
            }

            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    link_url_style: style.clone(),
                    output_file: Some(output_file.clone()),
                    ..Default::default()
                },
                commands: None,
                query: Some("self".to_string()),
                files: Some(vec![temp_file_path.clone()]),
                ..Cli::default()
            };

            assert!(cli.run().is_ok());
            let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
            if style == LinkUrlStyle::Angle {
                assert!(
                    output_content.contains("<https://example.com>"),
                    "Angle style should wrap URL with angle brackets"
                );
            } else {
                assert!(
                    output_content.contains("(https://example.com)"),
                    "None style should not wrap URL"
                );
            }
        }
    }

    #[test]
    fn test_aggregate_flag() {
        let (_, temp_file1) = create_file("test_agg1.md", "# Test 1");
        let (_, temp_file2) = create_file("test_agg2.md", "# Test 2");
        let (_, output_file) = create_file("test_agg_output.md", "");
        let temp_file1_clone = temp_file1.clone();
        let temp_file2_clone = temp_file2.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file1_clone.exists() {
                std::fs::remove_file(&temp_file1_clone).ok();
            }
            if temp_file2_clone.exists() {
                std::fs::remove_file(&temp_file2_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                aggregate: true,
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: OutputFormat::Text,
                ..Default::default()
            },
            commands: None,
            query: Some("len()".to_string()),
            files: Some(vec![temp_file1, temp_file2]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "Aggregated output should not be empty");
    }

    #[test]
    fn test_from_file_flag() {
        let (_, query_file) = create_file("test_query.mq", "self");
        let (_, input_file) = create_file("test_from_file.md", "# Test");
        let query_file_clone = query_file.clone();
        let input_file_clone = input_file.clone();

        defer! {
            if query_file_clone.exists() {
                std::fs::remove_file(&query_file_clone).ok();
            }
            if input_file_clone.exists() {
                std::fs::remove_file(&input_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                from_file: true,
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query_file.to_string_lossy().to_string()),
            files: Some(vec![input_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_separator_flag() {
        let (_, temp_file1) = create_file("test_sep1.md", "# Test 1");
        let (_, temp_file2) = create_file("test_sep2.md", "# Test 2");
        let (_, output_file) = create_file("test_sep_output.md", "");
        let temp_file1_clone = temp_file1.clone();
        let temp_file2_clone = temp_file2.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file1_clone.exists() {
                std::fs::remove_file(&temp_file1_clone).ok();
            }
            if temp_file2_clone.exists() {
                std::fs::remove_file(&temp_file2_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                separator: Some("\"---\"".to_string()),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file1, temp_file2]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "Output should not be empty");
        assert!(output_content.contains("# Test"), "File content should be present");
    }

    #[test]
    fn test_output_file_flag() {
        let (_, temp_input) = create_file("test_input_out.md", "# Test Output");
        let temp_output = std::env::temp_dir().join("test_output_file.md");
        let temp_input_clone = temp_input.clone();
        let temp_output_clone = temp_output.clone();

        defer! {
            if temp_input_clone.exists() {
                std::fs::remove_file(&temp_input_clone).ok();
            }
            if temp_output_clone.exists() {
                std::fs::remove_file(&temp_output_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_file: Some(temp_output.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_input]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        assert!(temp_output.exists(), "Output file should exist");
        let output_content = fs::read_to_string(&temp_output).expect("Failed to read output");
        assert!(
            output_content.contains("# Test Output"),
            "Output content should match input"
        );
    }

    #[test]
    fn test_unbuffered_output() {
        let (_, temp_file) = create_file("test_unbuf.md", "# Test");
        let temp_file_clone = temp_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                unbuffered: true,
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_update_with_non_markdown_input() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Html),
                ..Default::default()
            },
            output: OutputArgs {
                update: true,
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_err());
    }

    #[test]
    fn test_list_commands() {
        let cli = Cli {
            list: true,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_parallel_threshold() {
        let files: Vec<PathBuf> = (0..15)
            .map(|i| {
                let (_, path) = create_file(&format!("test_parallel_{}.md", i), "# Test");
                path
            })
            .collect();

        let files_clone = files.clone();
        defer! {
            for file in &files_clone {
                if file.exists() {
                    std::fs::remove_file(file).ok();
                }
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            commands: None,
            query: Some("self".to_string()),
            files: Some(files),
            parallel_threshold: 10,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[rstest]
    #[case("mq-exec-owner", 0o700, true)]
    #[case("mq-exec-group", 0o010, true)]
    #[case("mq-exec-other", 0o001, true)]
    #[case("mq-exec-all", 0o755, true)]
    #[case("mq-noexec-rw", 0o644, false)]
    #[case("mq-noexec-ro", 0o444, false)]
    #[cfg(unix)]
    fn test_is_executable_file_unix(#[case] filename: &str, #[case] mode: u32, #[case] expected: bool) {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = std::env::temp_dir().join(format!("mq-exec-test-{filename}"));
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        let file_path = temp_dir.join(filename);
        fs::write(&file_path, "#!/bin/sh\necho test").expect("Failed to write file");
        fs::set_permissions(&file_path, fs::Permissions::from_mode(mode)).expect("Failed to set permissions");

        let entry = fs::read_dir(&temp_dir)
            .expect("Failed to read dir")
            .find(|e| e.as_ref().unwrap().file_name().to_str() == Some(filename))
            .unwrap()
            .unwrap();

        assert_eq!(
            Cli::is_executable_file(&entry),
            expected,
            "File with mode {mode:#o} should return {expected}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_is_executable_file_unix_directory() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = std::env::temp_dir().join("mq-dir-test-unix");
        let sub_dir = temp_dir.join("mq-subdir");
        fs::create_dir_all(&sub_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        fs::set_permissions(&sub_dir, fs::Permissions::from_mode(0o755)).expect("Failed to set permissions");

        let entry = fs::read_dir(&temp_dir)
            .expect("Failed to read dir")
            .find(|e| e.as_ref().unwrap().file_name() == "mq-subdir")
            .unwrap()
            .unwrap();

        assert!(!Cli::is_executable_file(&entry), "Directory should return false");
    }

    #[rstest]
    #[case("mq-test.exe", true)]
    #[case("mq-test.cmd", true)]
    #[case("mq-test.bat", true)]
    #[case("mq-test.com", true)]
    #[case("mq-test.EXE", true)]
    #[case("mq-test.Bat", true)]
    #[case("mq-test.txt", false)]
    #[case("mq-test.sh", false)]
    #[case("mq-test", false)]
    #[cfg(windows)]
    fn test_is_executable_file_windows(#[case] filename: &str, #[case] expected: bool) {
        let temp_dir = std::env::temp_dir().join(format!("mq-exec-test-win-{}", filename.replace('.', "-")));
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        let file_path = temp_dir.join(filename);
        fs::write(&file_path, "test").expect("Failed to write file");

        let entry = fs::read_dir(&temp_dir)
            .expect("Failed to read dir")
            .find(|e| e.as_ref().unwrap().file_name().to_str() == Some(filename))
            .unwrap()
            .unwrap();

        assert_eq!(
            Cli::is_executable_file(&entry),
            expected,
            "File '{filename}' should return {expected}"
        );
    }

    #[test]
    #[cfg(windows)]
    fn test_is_executable_file_windows_directory() {
        let temp_dir = std::env::temp_dir().join("mq-dir-test-windows");
        let sub_dir = temp_dir.join("mq-subdir");
        fs::create_dir_all(&sub_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        let entry = fs::read_dir(&temp_dir)
            .expect("Failed to read dir")
            .find(|e| e.as_ref().unwrap().file_name() == "mq-subdir")
            .unwrap()
            .unwrap();

        assert!(!Cli::is_executable_file(&entry), "Directory should return false");
    }

    #[test]
    #[cfg(not(any(unix, windows)))]
    fn test_is_executable_file_other_os() {
        let temp_dir = std::env::temp_dir().join("mq-other-test");
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        let file = temp_dir.join("mq-test");
        fs::write(&file, "test").expect("Failed to write file");

        let entry = fs::read_dir(&temp_dir)
            .expect("Failed to read dir")
            .find(|e| e.as_ref().unwrap().file_name() == "mq-test")
            .unwrap()
            .unwrap();

        assert!(
            Cli::is_executable_file(&entry),
            "Regular file should return true on other OS"
        );
    }

    #[test]
    #[cfg(not(any(unix, windows)))]
    fn test_is_executable_file_other_os_directory() {
        let temp_dir = std::env::temp_dir().join("mq-dir-other-test");
        let sub_dir = temp_dir.join("mq-subdir");
        fs::create_dir_all(&sub_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        let entry = fs::read_dir(&temp_dir)
            .expect("Failed to read dir")
            .find(|e| e.as_ref().unwrap().file_name() == "mq-subdir")
            .unwrap()
            .unwrap();

        assert!(
            !Cli::is_executable_file(&entry),
            "Directory should return false on other OS"
        );
    }

    /// Test that Windows deduplicates commands with different executable extensions.
    /// e.g., mq-foo.bat and mq-foo.exe in the same directory should produce only "foo".
    #[test]
    #[cfg(windows)]
    fn test_collect_mq_commands_deduplicates_windows_extensions() {
        let temp_dir = std::env::temp_dir().join("mq-win-dedup-ext-test");
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        // Create the same subcommand with multiple Windows executable extensions
        fs::write(temp_dir.join("mq-foo.exe"), "test").expect("Failed to write file");
        fs::write(temp_dir.join("mq-foo.bat"), "@echo test").expect("Failed to write file");
        fs::write(temp_dir.join("mq-foo.cmd"), "@echo test").expect("Failed to write file");
        fs::write(temp_dir.join("mq-bar.exe"), "test").expect("Failed to write file");

        let mut seen = std::collections::HashSet::new();
        Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);

        assert_eq!(seen.len(), 2, "Should have exactly 2 unique commands");
        assert!(seen.contains("foo"), "Should contain 'foo'");
        assert!(seen.contains("bar"), "Should contain 'bar'");
    }

    /// Test that Windows deduplicates commands with different extensions across directories.
    /// e.g., mq-foo.bat in dir1 and mq-foo.exe in dir2 should produce only "foo".
    #[test]
    #[cfg(windows)]
    fn test_collect_mq_commands_deduplicates_across_dirs_windows() {
        let dir1 = std::env::temp_dir().join("mq-win-cross-dedup-1");
        let dir2 = std::env::temp_dir().join("mq-win-cross-dedup-2");
        fs::create_dir_all(&dir1).expect("Failed to create test directory");
        fs::create_dir_all(&dir2).expect("Failed to create test directory");

        defer! {
            if dir1.exists() {
                std::fs::remove_dir_all(&dir1).ok();
            }
            if dir2.exists() {
                std::fs::remove_dir_all(&dir2).ok();
            }
        }

        fs::write(dir1.join("mq-foo.bat"), "@echo test").expect("Failed to write file");
        fs::write(dir2.join("mq-foo.exe"), "test").expect("Failed to write file");
        fs::write(dir2.join("mq-unique.cmd"), "@echo test").expect("Failed to write file");

        let mut seen = std::collections::HashSet::new();
        Cli::collect_mq_commands_from_dir(&dir1, &mut seen);
        Cli::collect_mq_commands_from_dir(&dir2, &mut seen);

        assert_eq!(seen.len(), 2, "Should have exactly 2 unique commands");
        assert!(seen.contains("foo"), "Should contain 'foo'");
        assert!(seen.contains("unique"), "Should contain 'unique'");
    }

    /// Test that collect_mq_commands_from_dir handles an empty directory correctly.
    #[test]
    fn test_collect_mq_commands_from_empty_dir() {
        let temp_dir = std::env::temp_dir().join("mq-empty-dir-test");
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        let mut seen = std::collections::HashSet::new();
        Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);
        assert!(seen.is_empty(), "Empty directory should yield no commands");
    }

    /// Test that files without the mq- prefix are ignored even if executable.
    #[test]
    fn test_collect_mq_commands_ignores_non_mq_prefix() {
        let temp_dir = std::env::temp_dir().join("mq-prefix-test");
        fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

        defer! {
            if temp_dir.exists() {
                std::fs::remove_dir_all(&temp_dir).ok();
            }
        }

        // Create files without mq- prefix
        fs::write(temp_dir.join("foo"), "test").expect("Failed to write file");
        fs::write(temp_dir.join("bar-mq"), "test").expect("Failed to write file");
        fs::write(temp_dir.join("mqfoo"), "test").expect("Failed to write file");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for name in &["foo", "bar-mq", "mqfoo"] {
                fs::set_permissions(temp_dir.join(name), fs::Permissions::from_mode(0o755))
                    .expect("Failed to set permissions");
            }
        }

        let mut seen = std::collections::HashSet::new();
        Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);
        assert!(seen.is_empty(), "Files without mq- prefix should be ignored");
    }

    #[rstest]
    #[case("md", InputFormat::Markdown)]
    #[case("MD", InputFormat::Markdown)]
    #[case("markdown", InputFormat::Markdown)]
    #[case("mdx", InputFormat::Mdx)]
    #[case("html", InputFormat::Html)]
    #[case("htm", InputFormat::Html)]
    #[case("txt", InputFormat::Raw)]
    #[case("log", InputFormat::Raw)]
    #[case("csv", InputFormat::Csv)]
    #[case("psv", InputFormat::Psv)]
    #[case("tsv", InputFormat::Tsv)]
    #[case("json", InputFormat::Json)]
    #[case("toml", InputFormat::Toml)]
    #[case("yaml", InputFormat::Yaml)]
    #[case("yml", InputFormat::Yaml)]
    #[case("xml", InputFormat::Xml)]
    #[case("jsonl", InputFormat::Text)]
    #[case("ndjson", InputFormat::Text)]
    #[case("cbor", InputFormat::Cbor)]
    #[case("hcl", InputFormat::Hcl)]
    #[case("unknown", InputFormat::Markdown)] // default fallback
    fn test_from_extension(#[case] ext: &str, #[case] expected: InputFormat) {
        assert_eq!(InputFormat::from_extension(ext), expected);
    }

    #[rstest]
    #[case("file.json", Some(r#"import "json" | json::json_parse()"#))]
    #[case("file.yaml", Some(r#"import "yaml" | yaml::yaml_parse()"#))]
    #[case("file.yml", Some(r#"import "yaml" | yaml::yaml_parse()"#))]
    #[case("file.toml", Some(r#"import "toml" | toml::toml_parse()"#))]
    #[case("file.xml", Some(r#"import "xml" | xml::xml_parse()"#))]
    #[case("file.toon", Some(r#"import "toon" | toon::toon_parse()"#))]
    #[case("file.csv", Some(r#"import "csv" | csv::csv_parse(true)"#))]
    #[case("file.tsv", Some(r#"import "csv" | csv::tsv_parse(true)"#))]
    #[case("file.psv", Some(r#"import "csv" | csv::psv_parse(true)"#))]
    #[case("file.cbor", Some(r#"import "cbor" | cbor::cbor_parse()"#))]
    #[case("file.md", None)]
    #[case("file.txt", None)]
    fn test_auto_query_prefix(#[case] filename: &str, #[case] expected: Option<&str>) {
        let cli = Cli {
            input: InputArgs::default(),
            ..Cli::default()
        };
        let file = Some(PathBuf::from(filename));
        assert_eq!(cli.auto_query_prefix(&file).as_deref(), expected);
    }

    #[test]
    fn test_auto_query_prefix_disabled_when_input_format_set() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            ..Cli::default()
        };
        let file = Some(PathBuf::from("file.json"));
        assert_eq!(cli.auto_query_prefix(&file), None);
    }

    #[test]
    fn test_auto_query_prefix_none_for_no_file() {
        let cli = Cli {
            input: InputArgs::default(),
            ..Cli::default()
        };
        assert_eq!(cli.auto_query_prefix(&None), None);
    }

    #[test]
    fn test_json_auto_parse() {
        let (_, temp_file_path) = create_file("auto_parse_test.json", r#"{"key": "value"}"#);
        let temp_file_path_clone = temp_file_path.clone();
        let (_, output_file) = create_file("auto_parse_output.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(content.contains("value"), "JSON should be parsed automatically");
    }

    #[test]
    fn test_csv_auto_parse() {
        let (_, temp_file_path) = create_file("auto_parse_test.csv", "name,age\nAlice,30\n");
        let temp_file_path_clone = temp_file_path.clone();
        let (_, output_file) = create_file("auto_parse_csv_output.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(content.contains("Alice"), "CSV should be parsed automatically");
        assert!(content.contains("name"), "CSV header should be parsed");
    }

    fn create_binary_file(name: &str, content: &[u8]) -> (PathBuf, PathBuf) {
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(name);
        let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
        file.write_all(content).expect("Failed to write to temp file");
        (temp_dir, temp_file_path)
    }

    #[test]
    fn test_content_data_empty() {
        let c = ContentData::empty();
        assert_eq!(c.as_str(), Some(""));
        assert_eq!(c.as_bytes(), b"");
    }

    #[rstest]
    #[case(ContentData::Text("hello".to_string()), Some("hello"))]
    #[case(ContentData::Text("".to_string()), Some(""))]
    #[case(ContentData::Bytes(vec![0xde, 0xad]), None)]
    #[case(ContentData::Bytes(vec![]), None)]
    fn test_content_data_as_str(#[case] input: ContentData, #[case] expected: Option<&str>) {
        assert_eq!(input.as_str(), expected);
    }

    #[rstest]
    #[case(ContentData::Text("abc".to_string()), b"abc".as_ref())]
    #[case(ContentData::Text("".to_string()), b"".as_ref())]
    #[case(ContentData::Bytes(vec![0xde, 0xad, 0xbe, 0xef]), &[0xde, 0xad, 0xbe, 0xef])]
    #[case(ContentData::Bytes(vec![]), b"".as_ref())]
    fn test_content_data_as_bytes(#[case] input: ContentData, #[case] expected: &[u8]) {
        assert_eq!(input.as_bytes(), expected);
    }

    // --- ContentData::from (Into conversions) ---

    #[rstest]
    #[case("hello".to_string(), Some("hello"))]
    #[case("".to_string(), Some(""))]
    fn test_content_data_from_string(#[case] s: String, #[case] expected_str: Option<&str>) {
        let c: ContentData = s.into();
        assert_eq!(c.as_str(), expected_str);
    }

    #[rstest]
    #[case(vec![0x01, 0x02, 0x03])]
    #[case(vec![])]
    fn test_content_data_from_vec_u8(#[case] bytes: Vec<u8>) {
        let expected = bytes.clone();
        let c: ContentData = bytes.into();
        assert_eq!(c.as_str(), None);
        assert_eq!(c.as_bytes(), expected.as_slice());
    }

    #[rstest]
    #[case(Some(InputFormat::Bytes), true)]
    #[case(Some(InputFormat::Cbor), true)]
    #[case(Some(InputFormat::Json), false)]
    #[case(Some(InputFormat::Yaml), false)]
    #[case(Some(InputFormat::Toml), false)]
    #[case(Some(InputFormat::Markdown), false)]
    #[case(Some(InputFormat::Raw), false)]
    #[case(Some(InputFormat::Text), false)]
    #[case(Some(InputFormat::Null), false)]
    #[case(None, false)]
    fn test_is_binary_format(#[case] fmt: Option<InputFormat>, #[case] expected: bool) {
        let cli = Cli {
            input: InputArgs {
                input_format: fmt,
                ..Default::default()
            },
            ..Cli::default()
        };
        assert_eq!(cli.is_binary_format(), expected);
    }

    #[rstest]
    #[case(InputFormat::Bytes, None)]
    #[case(InputFormat::Cbor, Some(r#"import "cbor" | cbor::cbor_parse()"#))]
    #[case(InputFormat::Json, Some(r#"import "json" | json::json_parse()"#))]
    #[case(InputFormat::Hcl, Some(r#"import "hcl" | hcl::hcl_parse()"#))]
    #[case(InputFormat::Markdown, None)]
    #[case(InputFormat::Raw, None)]
    fn test_module_query_prefix(#[case] fmt: InputFormat, #[case] expected: Option<&str>) {
        assert_eq!(fmt.module_query_prefix(), expected);
    }

    #[rstest]
    #[case(InputFormat::Bytes)]
    #[case(InputFormat::Cbor)]
    fn test_binary_format_streaming_returns_error(#[case] fmt: InputFormat) {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(fmt),
                stream: true,
                ..Default::default()
            },
            query: Some("self".to_string()),
            ..Cli::default()
        };
        assert!(cli.run().is_err());
    }

    #[rstest]
    #[case(&[0x01, 0x02, 0x03, 0xff], "self", "bytes_self")]
    #[case(&[0xca, 0xfe, 0xba, 0xbe], "self", "bytes_self2")]
    fn test_bytes_input_self_roundtrip(#[case] data: &[u8], #[case] query: &str, #[case] suffix: &str) {
        let (_, temp_file_path) = create_binary_file(&format!("test_{suffix}.bin"), data);
        let temp_file_path_clone = temp_file_path.clone();
        let (_, output_file) = create_file(&format!("test_{suffix}_out.md"), "");
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() { std::fs::remove_file(&temp_file_path_clone).ok(); }
            if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Bytes),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(query.to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read(&output_file).expect("Failed to read output");
        assert_eq!(result, data);
    }

    #[rstest]
    #[case(&[0xca, 0xfe, 0xba, 0xbe], "4")]
    #[case(&[0x01], "1")]
    #[case(&[], "0")]
    fn test_bytes_input_len(#[case] data: &[u8], #[case] expected_len: &str) {
        let suffix = format!("bytes_len_{}", data.len());
        let (_, temp_file_path) = create_binary_file(&format!("test_{suffix}.bin"), data);
        let temp_file_path_clone = temp_file_path.clone();
        let (_, output_file) = create_file(&format!("test_{suffix}_out.md"), "");
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() { std::fs::remove_file(&temp_file_path_clone).ok(); }
            if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Bytes),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some("len()".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(result.trim(), expected_len);
    }

    #[rstest]
    // CBOR text string "hello": major 3 (text), len 5
    #[case(&[0x65, 0x68, 0x65, 0x6c, 0x6c, 0x6f], None, "hello", "cbor_auto_hello")]
    // CBOR integer 42: 0x18 0x2a
    #[case(&[0x18, 0x2a], Some(InputFormat::Cbor), "42", "cbor_explicit_42")]
    // CBOR integer 0: 0x00
    #[case(&[0x00], Some(InputFormat::Cbor), "0", "cbor_explicit_0")]
    fn test_cbor_parse(
        #[case] cbor_bytes: &[u8],
        #[case] fmt: Option<InputFormat>,
        #[case] expected: &str,
        #[case] suffix: &str,
    ) {
        let ext = if fmt.is_none() { "cbor" } else { "bin" };
        let (_, temp_file_path) = create_binary_file(&format!("test_{suffix}.{ext}"), cbor_bytes);
        let temp_file_path_clone = temp_file_path.clone();
        let (_, output_file) = create_file(&format!("test_{suffix}_out.md"), "");
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_path_clone.exists() { std::fs::remove_file(&temp_file_path_clone).ok(); }
            if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: fmt,
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(
            result.trim().contains(expected),
            "expected '{}' in output, got '{}'",
            expected,
            result.trim()
        );
    }

    #[rstest]
    #[case::data_only(
        "data_only",
        None,
        Some(vec!["x".to_string(), "y".to_string(), "z".to_string()]),
        "ARGS",
        r#"{"positional": ["x", "y", "z"], "named": {}}"#
    )]
    #[case::args_only(
        "args_only",
        Some(vec!["name".to_string(), "Alice".to_string()]),
        None,
        "ARGS",
        r#"{"positional": [], "named": {"name": "Alice"}}"#
    )]
    #[case::args_and_data(
        "args_and_data",
        Some(vec!["name".to_string(), "Alice".to_string()]),
        Some(vec!["x".to_string(), "y".to_string()]),
        "ARGS",
        r#"{"positional": ["x", "y"], "named": {"name": "Alice"}}"#
    )]
    #[case::positional_access(
        "positional_access",
        None,
        Some(vec!["a".to_string(), "b".to_string()]),
        r#"ARGS | ."positional""#,
        r#"["a", "b"]"#
    )]
    #[case::named_access(
        "named_access",
        Some(vec!["key".to_string(), "val".to_string()]),
        None,
        r#"ARGS | ."named""#,
        r#"{"key": "val"}"#
    )]
    #[case::named_individual_var(
        "named_individual_var",
        Some(vec!["greeting".to_string(), "hello".to_string()]),
        None,
        "greeting",
        "hello"
    )]
    fn test_args_and_data(
        #[case] suffix: &str,
        #[case] args: Option<Vec<String>>,
        #[case] argv: Option<Vec<String>>,
        #[case] query: &str,
        #[case] expected: &str,
    ) {
        let (_, output_file) = create_file(&format!("test_args_data_{suffix}.md"), "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                args,
                ..Default::default()
            },
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(query.to_string()),
            argv,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(result.trim(), expected, "query: {}", query);
    }

    #[test]
    fn test_files_without_data_single_file() {
        let (_, input_file) = create_file("test_files_no_data_single.md", "# hello");
        let (_, output_file) = create_file("test_files_no_data_single_out.md", "");
        let input_file_clone = input_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
            if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Text,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some("self".to_string()),
            files: Some(vec![input_file]),
            argv: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(result.contains("hello"), "file content should be processed");
    }

    #[test]
    fn test_files_without_data_multiple_files() {
        // Verify each file is processed independently; output_file is per-run so check each separately.
        let (_, file1) = create_file("test_files_no_data_multi1.md", "# file1");
        let (_, file2) = create_file("test_files_no_data_multi2.md", "# file2");
        let (_, out1) = create_file("test_files_no_data_multi_out1.md", "");
        let (_, out2) = create_file("test_files_no_data_multi_out2.md", "");
        let file1_clone = file1.clone();
        let file2_clone = file2.clone();
        let out1_clone = out1.clone();
        let out2_clone = out2.clone();

        defer! {
            if file1_clone.exists() { std::fs::remove_file(&file1_clone).ok(); }
            if file2_clone.exists() { std::fs::remove_file(&file2_clone).ok(); }
            if out1_clone.exists() { std::fs::remove_file(&out1_clone).ok(); }
            if out2_clone.exists() { std::fs::remove_file(&out2_clone).ok(); }
        }

        for (input, output, expected) in [(&file1, &out1, "file1"), (&file2, &out2, "file2")] {
            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    output_format: OutputFormat::Text,
                    output_file: Some(output.clone()),
                    ..Default::default()
                },
                query: Some("self".to_string()),
                files: Some(vec![input.clone()]),
                argv: None,
                ..Cli::default()
            };
            assert!(cli.run().is_ok());
            let result = fs::read_to_string(output).expect("Failed to read output");
            assert!(result.contains(expected), "file content '{}' should appear", expected);
        }

        // Also verify multi-file run (no output_file) succeeds without error
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            query: Some("self".to_string()),
            files: Some(vec![file1, file2]),
            argv: None,
            ..Cli::default()
        };
        assert!(cli.run().is_ok(), "multi-file run without --argv should succeed");
    }

    #[test]
    fn test_files_with_data_does_not_mix() {
        // --argv values must not be treated as files, and files must not appear in ARGS
        let (_, input_file) = create_file("test_files_with_data.md", "# content");
        let (_, output_file) = create_file("test_files_with_data_out.md", "");
        let input_file_clone = input_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
            if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Raw,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some("ARGS".to_string()),
            files: Some(vec![input_file]),
            argv: Some(vec!["alpha".to_string(), "beta".to_string()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(result.contains("alpha"), "ARGS.positional should contain 'alpha'");
        assert!(result.contains("beta"), "ARGS.positional should contain 'beta'");
        assert!(!result.contains("content"), "file content must not appear in ARGS");
    }

    #[test]
    fn test_files_without_data_args_undefined() {
        // Without --argv or --args, ARGS must be undefined (runtime error expected)
        let (_, input_file) = create_file("test_files_no_args_undefined.md", "# x");
        let input_file_clone = input_file.clone();

        defer! {
            if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            query: Some("ARGS".to_string()),
            files: Some(vec![input_file]),
            argv: None,
            ..Cli::default()
        };

        assert!(
            cli.run().is_err(),
            "ARGS should be undefined when neither --args nor --argv is given"
        );
    }

    #[test]
    fn test_files_with_data_file_content_processed() {
        // File content is still processed even when --argv is given
        let (_, input_file) = create_file("test_files_data_content.md", "# heading");
        let (_, output_file) = create_file("test_files_data_content_out.md", "");
        let input_file_clone = input_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
            if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: OutputFormat::Text,
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some("self".to_string()),
            files: Some(vec![input_file]),
            argv: Some(vec!["x".to_string()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(
            result.contains("heading"),
            "file content should still be processed when --argv is given"
        );
    }

    #[test]
    fn test_args_pair_works() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                args: Some(vec!["name".to_string(), "Alice".to_string()]),
                ..Default::default()
            },
            query: Some("name".to_string()),
            ..Cli::default()
        };
        assert!(cli.run().is_ok(), "--args with a valid NAME VALUE pair should succeed");
    }
}
