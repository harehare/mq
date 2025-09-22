use clap::{Parser, Subcommand};
use itertools::Itertools;
use miette::IntoDiagnostic;
use miette::miette;
use mq_lang::Engine;
use rayon::prelude::*;
use std::collections::VecDeque;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::{self, BufWriter, Read, Write};
use std::str::FromStr;
use std::{fs, path::PathBuf};

#[derive(Parser, Debug, Default)]
#[command(name = "mq")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(after_help = "Examples:\n\n\
    To filter markdown nodes:\n\
    $ mq 'query' file.md\n\n\
    To read query from file:\n\
    $ mq -f 'file' file.md\n\n\
    To start a REPL session:\n\
    $ mq repl\n\n\
    To format mq file:\n\
    $ mq fmt --check file.mq")]
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

    /// Number of files to process before switching to parallel processing
    #[arg(short = 'P', default_value_t = 10)]
    parallel_threshold: usize,

    #[arg(value_name = "QUERY OR FILE")]
    query: Option<String>,
    files: Option<Vec<PathBuf>>,
}
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

    #[arg(long = "json", default_value_t = false)]
    include_json: bool,

    /// Include the built-in CSV module
    #[arg(long = "csv", default_value_t = false)]
    include_csv: bool,

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

    /// Update the input markdown
    #[arg(short = 'U', long, default_value_t = false)]
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
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a REPL session for interactive query execution
    Repl,
    /// Start a language server for mq
    Lsp,
    /// Start an MCP server for mq
    Mcp,
    /// Start a TUI for mq
    Tui {
        /// Path to the file to be opened in TUI mode
        #[arg(value_name = "FILE")]
        file_path: PathBuf,
    },
    /// Format mq files based on specified formatting options.
    Fmt {
        /// Number of spaces for indentation
        #[arg(short, long, default_value_t = 2)]
        indent_width: usize,
        /// Check if files are formatted without modifying them
        #[arg(short, long)]
        check: bool,
    },
    /// Show functions documentation for the query
    Docs {
        /// Specify additional module names to load for documentation
        #[arg(short = 'M', long)]
        module_names: Option<Vec<String>>,
    },
    /// Start a debug adapter for mq
    #[cfg(feature = "debugger")]
    Dap,
}

impl Cli {
    pub fn run(&self) -> miette::Result<()> {
        if !matches!(self.input.input_format, Some(InputFormat::Markdown) | None)
            && self.output.update
        {
            return Err(miette!(
                "The output format is not supported for the update option"
            ));
        }

        match &self.commands {
            Some(Commands::Repl) => {
                mq_repl::Repl::new(vec![mq_lang::RuntimeValue::String("".to_string())]).run()
            }
            None if self.query.is_none() => {
                mq_repl::Repl::new(vec![mq_lang::RuntimeValue::String("".to_string())]).run()
            }
            Some(Commands::Tui { file_path }) => {
                let mut app = {
                    let content = fs::read_to_string(file_path).into_diagnostic()?;
                    mq_tui::App::with_file(content, file_path.to_string_lossy().into_owned())
                };

                app.run()?;

                Ok(())
            }
            Some(Commands::Lsp) => {
                tokio::runtime::Runtime::new()
                    .into_diagnostic()?
                    .block_on(async { mq_lsp::start().await });
                Ok(())
            }
            Some(Commands::Mcp) => {
                let _ = tokio::runtime::Runtime::new()
                    .into_diagnostic()?
                    .block_on(async { mq_mcp::start().await });
                Ok(())
            }
            Some(Commands::Fmt {
                indent_width,
                check,
            }) => {
                let mut formatter =
                    mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
                        indent_width: *indent_width,
                    }));
                for (_, content) in self.read_contents()? {
                    let formatted = formatter.format(&content).into_diagnostic()?;

                    if *check && formatted != content {
                        return Err(miette!("The input is not formatted"));
                    } else {
                        println!("{}", formatted);
                    }
                }

                Ok(())
            }
            Some(Commands::Docs { module_names }) => {
                let mut hir = mq_hir::Hir::default();

                if let Some(module_names) = module_names {
                    hir.builtin.disabled = true;

                    for module_name in module_names {
                        hir.add_code(None, &format!("include \"{}\"", module_name));
                    }
                } else {
                    hir.add_code(None, "");
                }

                let mut doc_csv = hir
                    .symbols()
                    .sorted_by_key(|(_, symbol)| symbol.value.clone())
                    .filter_map(|(_, symbol)| match symbol {
                        mq_hir::Symbol {
                            kind: mq_hir::SymbolKind::Function(params),
                            value: Some(value),
                            doc,
                            ..
                        } if !symbol.is_internal_function() => Some(mq_lang::RuntimeValue::String(
                            [
                                format!("`{}`", value),
                                doc.iter().map(|(_, d)| d.to_string()).join("\n"),
                                params.iter().map(|p| format!("`{}`", p)).join(", "),
                                format!("{}({})", value, params.join(", ")),
                            ]
                            .join("\t"),
                        )),
                        _ => None,
                    })
                    .collect::<VecDeque<_>>();

                doc_csv.push_front(mq_lang::RuntimeValue::String(
                    ["Function Name", "Description", "Parameters", "Example"]
                        .iter()
                        .join("\t"),
                ));

                let mut engine = self.create_engine()?;
                let doc_values = engine
                    .eval(
                        r#"include "csv" | tsv_parse(false) | csv_to_markdown_table()"#,
                        mq_lang::raw_input(&doc_csv.iter().join("\n")).into_iter(),
                    )
                    .map_err(|e| *e)?;
                self.print(doc_values)?;

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

    fn create_engine(&self) -> miette::Result<Engine> {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module();

        if let Some(dirs) = &self.input.module_directories {
            engine.set_paths(dirs.clone());
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
            engine
                .debugger()
                .write()
                .unwrap()
                .set_handler(Box::new(handler));
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

        Ok(match (includes.is_empty(), query.is_empty()) {
            (true, false) => query,
            (false, true) => includes,
            (false, false) => format!("{} | {}", includes, query),
            (true, true) => String::new(),
        })
    }

    fn execute(
        &self,
        engine: &mut mq_lang::Engine,
        query: &str,
        file: &Option<PathBuf>,
        content: &str,
    ) -> miette::Result<()> {
        if let Some(file) = file {
            engine.define_string_value("__FILE__", file.to_string_lossy().as_ref());
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
                    "txt" | "csv" | "tsv" | "json" | "toml" | "yaml" | "yml" | "xml" => {
                        &InputFormat::Raw
                    }
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
            let results = engine
                .eval(query, input.clone().into_iter())
                .map_err(|e| *e)?;
            let current_values: mq_lang::RuntimeValues = input.clone().into();

            if current_values.len() != results.len() {
                return Err(miette!(
                    "The number of input and output values do not match"
                ));
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
            OutputFormat::Html => handle
                .write_all(markdown.to_html().as_bytes())
                .map_err(|e| miette!(e))?,
            OutputFormat::Text => {
                handle
                    .write_all(markdown.to_text().as_bytes())
                    .map_err(|e| miette!(e))?;
            }
            OutputFormat::Markdown => {
                handle
                    .write_all(markdown.to_string().as_bytes())
                    .map_err(|e| miette!(e))?;
            }
            OutputFormat::Json => {
                handle
                    .write_all(markdown.to_json()?.as_bytes())
                    .map_err(|e| miette!(e))?;
            }
        }

        if !self.output.unbuffered {
            handle.flush().expect("Error flushing");
        }

        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use mq_test::defer;

    use super::*;

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
        let (_, temp_file_path) = mq_test::create_file("test1.md", "# test");
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
        let (_, temp_file_path) = mq_test::create_file("test2.md", "# test");
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
        let (_, temp_file_path) = mq_test::create_file("test3.md", "# test");
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
    fn test_cli_fmt_command() {
        let (_, temp_file_path) = mq_test::create_file("test1.mq", "def math(): 42;");
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
            }),
            query: None,
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_fmt_command_with_check() {
        let (_, temp_file_path) = mq_test::create_file("test2.mq", "def math(): 42;");
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
            }),
            query: None,
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }

    #[test]
    fn test_cli_update_flag() {
        let (_, temp_file_path) = mq_test::create_file("test4.md", "# test");
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
        let (temp_dir, temp_file_path) = mq_test::create_file("math.mq", "def math(): 42;");
        let (_, temp_md_file_path) = mq_test::create_file("test.md", "# test");
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
}
