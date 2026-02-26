use clap::{Parser, Subcommand};
use colored::Colorize;
use glob::glob;
use itertools::Itertools;
use miette::IntoDiagnostic;
use miette::miette;
use mq_lang::DefaultEngine;
use rayon::prelude::*;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::{fs, path::PathBuf};
use which::which;

#[derive(Parser, Debug, Default)]
#[command(name = "mq")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(after_help = "# Examples:\n\n\
    ## To filter markdown nodes:\n\
    mq 'query' file.md\n\n\
    ## To read query from file:\n\
    mq -f 'file' file.md\n\n\
    ## To start a REPL session:\n\
    mq repl\n\n\
    ## To format mq file:\n\
    mq fmt --check file.mq")]
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
}

#[cfg(unix)]
const UNIX_EXECUTABLE_BITS: u32 = 0o111;

/// Represents the input format for processing.
/// - Markdown: Standard Markdown parsing.
/// - Mdx: MDX parsing.
/// - Html: HTML parsing.
/// - Text: Treats input as plain text.
/// - Null: No input.
/// - Raw: Treats all input as a single string, without parsing.
#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum InputFormat {
    #[default]
    Markdown,
    Mdx,
    Html,
    Text,
    Null,
    Raw,
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum OutputFormat {
    #[default]
    Markdown,
    Html,
    Text,
    Json,
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

    /// Sets string that can be referenced at runtime
    #[arg(long, value_names = ["NAME", "VALUE"])]
    args: Option<Vec<String>>,

    /// Sets file contents that can be referenced at runtime
    #[arg(long="rawfile", value_names = ["NAME", "FILE"])]
    raw_file: Option<Vec<String>>,

    /// Enable streaming mode for processing large files line by line
    #[arg(long, default_value_t = false)]
    stream: bool,

    /// Include the built-in JSON module
    #[arg(long = "json", default_value_t = false)]
    include_json: bool,

    /// Include the built-in CSV module
    #[arg(long = "csv", default_value_t = false)]
    include_csv: bool,

    /// Include the built-in Fuzzy module
    #[arg(long = "fuzzy", default_value_t = false)]
    include_fuzzy: bool,

    /// Include the built-in YAML module
    #[arg(long = "yaml", default_value_t = false)]
    include_yaml: bool,

    /// Include the built-in TOML module
    #[arg(long = "toml", default_value_t = false)]
    include_toml: bool,

    /// Include the built-in XML module
    #[arg(long = "xml", default_value_t = false)]
    include_xml: bool,

    /// Include the built-in test module
    #[arg(long = "test", default_value_t = false)]
    include_test: bool,
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
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a REPL session for interactive query execution
    Repl,
    /// Format mq files based on specified formatting options.
    Fmt {
        /// Number of spaces for indentation
        #[arg(short, long, default_value_t = 2)]
        indent_width: usize,
        /// Check if files are formatted without modifying them
        #[arg(short, long)]
        check: bool,
        /// Sort imports
        #[arg(long, default_value_t = false)]
        sort_imports: bool,
        /// Sort functions
        #[arg(long, default_value_t = false)]
        sort_functions: bool,
        /// Sort fields
        #[arg(long, default_value_t = false)]
        sort_fields: bool,
        /// Path to the mq file to format
        files: Option<Vec<PathBuf>>,
    },
    /// Start a debug adapter for mq
    #[cfg(feature = "debugger")]
    Dap,
}

impl Cli {
    /// Get the path to the external commands directory (~/.mq/bin)
    fn get_external_commands_dir() -> Option<PathBuf> {
        let home_dir = dirs::home_dir()?;
        let mq_bin_dir = home_dir.join(".mq").join("bin");
        if mq_bin_dir.exists() && mq_bin_dir.is_dir() {
            Some(mq_bin_dir)
        } else {
            None
        }
    }

    /// Find all external commands (mq-* files in ~/.mq/bin and PATH)
    fn find_external_commands() -> Vec<String> {
        let mut seen = std::collections::HashSet::new();

        // Search ~/.mq/bin first
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
            format!(
                "  {} - Format mq files based on specified formatting options",
                "fmt".green()
            ),
        ];

        #[cfg(feature = "debugger")]
        output.push(format!("  {} - Start a debug adapter for mq", "dap".green()));

        let external_commands = Self::find_external_commands();
        if !external_commands.is_empty() {
            output.push("".to_string());
            output.push(format!(
                "{}",
                "External subcommands (from ~/.mq/bin and PATH):".bold().yellow()
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
            Some(Commands::Fmt {
                indent_width,
                check,
                files,
                sort_imports,
                sort_fields,
                sort_functions,
            }) => {
                let mut formatter = mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
                    indent_width: *indent_width,
                    sort_imports: *sort_imports,
                    sort_fields: *sort_fields,
                    sort_functions: *sort_functions,
                }));
                let files = match files {
                    Some(f) => f,
                    None => &glob("./**/*.mq")
                        .into_diagnostic()?
                        .collect::<Result<Vec<_>, _>>()
                        .into_diagnostic()?,
                };

                for file in files {
                    if !file.exists() {
                        return Err(miette!("File not found: {}", file.display()));
                    }

                    let content = fs::read_to_string(file).into_diagnostic()?;
                    let formatted = formatter
                        .format(&content)
                        .map_err(|e| miette!("{}: {e}", file.display()))?;

                    if *check && formatted != content {
                        return Err(miette!("The input is not formatted"));
                    } else if formatted != content {
                        fs::write(file, formatted).into_diagnostic()?;
                    }
                }

                Ok(())
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

        if let Some(dirs) = &self.input.module_directories {
            engine.set_search_paths(dirs.clone());
        }

        if let Some(modules) = &self.input.module_names {
            for module_name in modules {
                engine.load_module(module_name).map_err(|e| *e)?;
            }
        }

        if let Some(args) = &self.input.args {
            args.chunks(2).for_each(|v| {
                engine.define_string_value(&v[0], &v[1]);
            });
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

        let includes = [
            ("csv", self.input.include_csv),
            ("fuzzy", self.input.include_fuzzy),
            ("json", self.input.include_json),
            ("toml", self.input.include_toml),
            ("yaml", self.input.include_yaml),
            ("xml", self.input.include_xml),
            ("test", self.input.include_test),
        ]
        .iter()
        .filter(|(_, enabled)| *enabled)
        .map(|(name, _)| format!(r#"include "{}""#, name))
        .join(" | ");

        let aggregate = self.input.aggregate.then_some(r#"nodes | import "section""#);

        let query = match (includes.is_empty(), query.is_empty()) {
            (true, false) => query,
            (false, true) => includes,
            (false, false) => format!("{} | {}", includes, query),
            (true, true) => String::new(),
        };

        Ok(aggregate.map(|agg| format!("{} | {}", agg, query)).unwrap_or(query))
    }

    fn execute(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        query: &str,
        file: &Option<PathBuf>,
        content: &str,
    ) -> miette::Result<()> {
        if let Some(file) = file {
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

        let input = match self.input.input_format.as_ref().unwrap_or_else(|| {
            if let Some(file) = file {
                match file
                    .extension()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_lowercase()
                    .as_str()
                {
                    "md" | "markdown" => &InputFormat::Markdown,
                    "mdx" => &InputFormat::Mdx,
                    "html" | "htm" => &InputFormat::Html,
                    "txt" | "csv" | "tsv" | "json" | "toml" | "yaml" | "yml" | "xml" => &InputFormat::Raw,
                    _ => &InputFormat::Markdown,
                }
            } else if io::stdin().is_terminal() {
                &InputFormat::Null
            } else {
                &InputFormat::Markdown
            }
        }) {
            InputFormat::Markdown => mq_lang::parse_markdown_input(content)?,
            InputFormat::Mdx => mq_lang::parse_mdx_input(content)?,
            InputFormat::Text => mq_lang::parse_text_input(content)?,
            InputFormat::Html => mq_lang::parse_html_input(content)?,
            InputFormat::Null => mq_lang::null_input(),
            InputFormat::Raw => mq_lang::raw_input(content),
        };

        let runtime_values = if self.output.update {
            let results = engine.eval(query, input.clone().into_iter()).map_err(|e| *e)?;
            let current_values: mq_lang::RuntimeValues = input.clone().into();

            if current_values.len() != results.len() {
                return Err(miette!("The number of input and output values do not match"));
            }

            current_values.update_with(results)
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

        self.print(runtime_values)
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
            files
                .iter()
                .try_for_each(|(file, content)| self.execute(&mut engine, &query, file, content))?;
        }

        Ok(())
    }

    fn process_streaming(&self) -> miette::Result<()> {
        let query = self.get_query()?;
        let mut engine = self.create_engine()?;

        self.process_lines(|file, line| self.execute(&mut engine, &query, &file.cloned(), line))
    }

    fn process_lines<F>(&self, mut process: F) -> miette::Result<()>
    where
        F: FnMut(Option<&PathBuf>, &str) -> miette::Result<()>,
    {
        // If files are specified, process each file line by line
        if let Some(files) = &self.files {
            for file in files {
                let file_handle = fs::File::open(file).into_diagnostic()?;
                let reader = io::BufReader::new(file_handle);
                for line_result in reader.lines() {
                    let line = line_result.into_diagnostic()?;
                    process(Some(file), &line)?;
                }
            }
        } else {
            // Otherwise, process stdin line by line
            let stdin = io::stdin();
            let reader = io::BufReader::new(stdin.lock());
            for line_result in reader.lines() {
                let line = line_result.into_diagnostic()?;
                process(None, &line)?;
            }
        }
        Ok(())
    }

    fn read_contents(&self) -> miette::Result<Vec<(Option<PathBuf>, String)>> {
        if matches!(self.input.input_format, Some(InputFormat::Null)) {
            return Ok(vec![(None, "".to_string())]);
        }

        self.files
            .clone()
            .map(|files| {
                let load_contents: miette::Result<Vec<String>> = files
                    .iter()
                    .map(|file| fs::read_to_string(file).into_diagnostic())
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
                    return Ok(vec![(None, "".to_string())]);
                }

                let mut input = String::new();
                io::stdin().read_to_string(&mut input).into_diagnostic()?;
                Ok(vec![(None, input)])
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
        let mut markdown = mq_markdown::Markdown::new(
            runtime_values
                .iter()
                .map(|runtime_value| match runtime_value {
                    mq_lang::RuntimeValue::Markdown(node, _) => node.clone(),
                    _ => runtime_value.to_string().into(),
                })
                .collect(),
        );
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

        match self.output.output_format {
            OutputFormat::Html => Self::write_ignore_pipe(&mut handle, markdown.to_html().as_bytes())?,
            OutputFormat::Text => {
                Self::write_ignore_pipe(&mut handle, markdown.to_text().as_bytes())?;
            }
            OutputFormat::Markdown if self.output.color_output && !Self::is_no_color() => {
                let theme = mq_markdown::ColorTheme::from_env();
                Self::write_ignore_pipe(&mut handle, markdown.to_colored_string_with_theme(&theme).as_bytes())?;
            }
            OutputFormat::Markdown => {
                Self::write_ignore_pipe(&mut handle, markdown.to_string().as_bytes())?;
            }
            OutputFormat::Json => {
                Self::write_ignore_pipe(&mut handle, markdown.to_json()?.as_bytes())?;
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

        for format in [OutputFormat::Markdown, OutputFormat::Html, OutputFormat::Text] {
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
    fn test_cli_fmt_command() {
        let (_, temp_file_path) = create_file("test1.mq", "def math(): 42;");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            commands: Some(Commands::Fmt {
                indent_width: 2,
                check: false,
                files: Some(vec![temp_file_path.clone()]),
                sort_functions: false,
                sort_fields: false,
                sort_imports: false,
            }),
            query: None,
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_fmt_command_with_check() {
        let (_, temp_file_path) = create_file("test2.mq", "def math(): 42;");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            commands: Some(Commands::Fmt {
                indent_width: 2,
                check: true,
                files: Some(vec![temp_file_path.clone()]),
                sort_functions: false,
                sort_fields: false,
                sort_imports: false,
            }),
            query: None,
            files: Some(vec![temp_file_path]),
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
        // find_external_commands searches ~/.mq/bin and PATH for mq-* files
        let commands = Cli::find_external_commands();
        // We can't assert specific commands, but we can check the function works
        assert!(commands.iter().all(|cmd| !cmd.is_empty()));
    }

    #[test]
    fn test_get_external_commands_dir() {
        // This test checks if the function returns a valid path or None
        let dir = Cli::get_external_commands_dir();
        if let Some(path) = dir {
            assert!(path.ends_with(".mq/bin") || path.ends_with(".mq\\bin"));
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
    fn test_output_format_json() {
        let (_, temp_file_path) = create_file("test_json.md", "# Test");
        let (_, output_file) = create_file("test_json_output.json", "");
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
        assert!(!output_content.is_empty(), "JSON output should not be empty");
        assert!(
            output_content.starts_with('{') || output_content.starts_with('['),
            "JSON output should be valid JSON"
        );
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
    fn test_include_csv_module() {
        let (_, temp_file) = create_file("test_csv.csv", "a,b\n1,2\n3,4");
        let (_, output_file) = create_file("test_csv_output.txt", "");
        let temp_file_clone = temp_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                include_csv: true,
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: OutputFormat::Text,
                ..Default::default()
            },
            commands: None,
            query: Some("csv_parse(true) | len()".to_string()),
            files: Some(vec![temp_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "CSV output should not be empty");
    }

    #[test]
    fn test_include_json_module() {
        let (_, temp_file) = create_file("test_json_module.json", r#"{"key": "value", "num": 42}"#);
        let (_, output_file) = create_file("test_json_module_output.txt", "");
        let temp_file_clone = temp_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                include_json: true,
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: OutputFormat::Text,
                ..Default::default()
            },
            commands: None,
            query: Some("json_parse()".to_string()),
            files: Some(vec![temp_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "JSON output should not be empty");
    }

    #[test]
    fn test_include_yaml_module() {
        let (_, temp_file) = create_file("test_yaml.yaml", "key: value\nnum: 42");
        let (_, output_file) = create_file("test_yaml_output.txt", "");
        let temp_file_clone = temp_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                include_yaml: true,
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: OutputFormat::Text,
                ..Default::default()
            },
            commands: None,
            query: Some("yaml_parse()".to_string()),
            files: Some(vec![temp_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "YAML output should not be empty");
    }

    #[test]
    fn test_include_toml_module() {
        let (_, temp_file) = create_file("test_toml.toml", "key = \"value\"\nnum = 42");
        let (_, output_file) = create_file("test_toml_output.txt", "");
        let temp_file_clone = temp_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                include_toml: true,
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: OutputFormat::Text,
                ..Default::default()
            },
            commands: None,
            query: Some("toml_parse()".to_string()),
            files: Some(vec![temp_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "TOML output should not be empty");
    }

    #[test]
    fn test_include_xml_module() {
        let (_, temp_file) = create_file("test_xml.xml", "<root><key>value</key><num>42</num></root>");
        let (_, output_file) = create_file("test_xml_output.txt", "");
        let temp_file_clone = temp_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                include_xml: true,
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: OutputFormat::Text,
                ..Default::default()
            },
            commands: None,
            query: Some("xml_parse()".to_string()),
            files: Some(vec![temp_file]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(!output_content.is_empty(), "XML output should not be empty");
    }

    #[test]
    fn test_fmt_file_not_found() {
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            commands: Some(Commands::Fmt {
                indent_width: 2,
                check: false,
                files: Some(vec![PathBuf::from("nonexistent.mq")]),
                sort_functions: false,
                sort_fields: false,
                sort_imports: false,
            }),
            query: None,
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_err());
    }

    #[test]
    fn test_fmt_check_unformatted_file() {
        let (_, temp_file) = create_file("test_unformatted.mq", "def   math():    42;");
        let temp_file_clone = temp_file.clone();

        defer! {
            if temp_file_clone.exists() {
                std::fs::remove_file(&temp_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            commands: Some(Commands::Fmt {
                indent_width: 2,
                check: true,
                files: Some(vec![temp_file]),
                sort_functions: false,
                sort_fields: false,
                sort_imports: false,
            }),
            query: None,
            files: None,
            ..Cli::default()
        };

        assert!(cli.run().is_err());
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
}
