use clap::CommandFactory;
use clap::{Parser, Subcommand};
use clap_complete::{Shell, generate};
use itertools::Itertools;
use miette::IntoDiagnostic;
use miette::miette;
use mq_lang::Engine;
use std::fmt::{self, Display};
use std::io::{self, BufWriter, Read, Write};
use std::str::FromStr;
use std::{env, fs, path::PathBuf};
use url::Url;

#[derive(Parser, Debug)]
#[command(name = "mq")]
#[command(author = "Takahiro Sato. <harehare1110@gmail.com>")]
#[command(version = "0.1.0")]
#[command(after_help = "Examples:\n\n\
    To filter markdown nodes:\n\
    $ mq 'query' file.md\n\n\
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

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,

    query: Option<String>,
    files: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum Format {
    #[default]
    Markdown,
    Html,
    Text,
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
pub enum ListStyle {
    #[default]
    Dash,
    Plus,
    Star,
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
pub enum FormatTarget {
    #[default]
    Mq,
    Markdown,
}

impl Display for FormatTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FormatTarget::Mq => write!(f, "mq"),
            FormatTarget::Markdown => write!(f, "markdown"),
        }
    }
}

#[derive(Clone, Debug, clap::Args, Default)]
struct InputArgs {
    /// load filter from the file
    #[arg(short, long)]
    from_file: Option<Vec<PathBuf>>,

    /// Reads each line as a string
    #[arg(short = 'R', long, group = "input")]
    raw_input: bool,

    /// Use empty string as the single input value
    #[arg(short, long, group = "input")]
    null_input: bool,

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
}

#[derive(Clone, Debug, clap::Args, Default)]
struct OutputArgs {
    /// pretty print
    #[clap(short, long, default_value = "false")]
    compact_output: bool,

    /// Compact instead of pretty-printed output
    #[arg(short = 'F', long, value_enum, default_value_t)]
    output_format: Format,

    /// Update the input markdown
    #[clap(short = 'U', long, default_value = "false")]
    update: bool,

    /// Unbuffered output
    #[clap(long, default_value_t = false)]
    unbuffered: bool,

    /// Set the list style for markdown output
    #[clap(long, value_enum, default_value_t = ListStyle::Dash)]
    list_style: ListStyle,

    /// Output to the specified file
    #[clap(short = 'o', long = "output", value_name = "FILE")]
    output_file: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a REPL session for interactive query execution
    Repl,
    /// Format mq or markdown files based on specified formatting options.
    Fmt {
        /// Number of spaces for indentation
        #[arg(short, long, default_value_t = 2)]
        indent_width: usize,
        /// Check if files are formatted without modifying them
        #[arg(short, long)]
        check: bool,
        /// Specify the target format type
        #[arg(short = 'T', long, default_value_t = FormatTarget::Mq)]
        target: FormatTarget,
    },
    /// Generate shell completion scripts for supported shells
    Completion {
        #[arg(short, long, value_enum)]
        shell: Shell,
    },
    /// Show functions documentation for the query
    Docs,
}

impl Cli {
    pub fn run(&self) -> miette::Result<()> {
        if self.commands.is_none() && self.query.is_none() && self.input.from_file.is_none() {
            return Cli::command().print_help().into_diagnostic();
        }

        match &self.commands {
            Some(Commands::Repl) => {
                mq_repl::Repl::new(vec![mq_lang::Value::String("".to_string())]).run()
            }
            Some(Commands::Fmt {
                indent_width,
                check,
                target,
            }) => {
                match target {
                    FormatTarget::Mq => {
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
                    }
                    FormatTarget::Markdown => {
                        let options = mq_markdown::RenderOptions {
                            list_style: match self.output.list_style.clone() {
                                ListStyle::Dash => mq_markdown::ListStyle::Dash,
                                ListStyle::Plus => mq_markdown::ListStyle::Plus,
                                ListStyle::Star => mq_markdown::ListStyle::Star,
                            },
                        };

                        for (_, content) in self.read_contents()? {
                            let formatted = mq_markdown::pretty_markdown(&content, &options)?;

                            if *check && formatted != content {
                                return Err(miette!("The input is not formatted"));
                            } else {
                                println!("{}", formatted);
                            }
                        }
                    }
                }

                Ok(())
            }
            Some(Commands::Completion { shell }) => {
                generate(*shell, &mut Cli::command(), "mq", &mut std::io::stdout());
                Ok(())
            }
            Some(Commands::Docs) => {
                let query = self.get_query();
                let mut hir = mq_hir::Hir::default();
                let file = Url::parse("file:///").into_diagnostic()?;
                hir.add_code(file, &query);

                let doc_csv = hir
                    .symbols()
                    .filter_map(|(_, symbol)| match symbol {
                        mq_hir::Symbol {
                            kind: mq_hir::SymbolKind::Function(params),
                            name: Some(name),
                            doc,
                            ..
                        } => Some(mq_lang::Value::String(
                            [
                                format!("`{}`", name),
                                doc.iter().map(|(_, d)| d.to_string()).join("\n"),
                                params.iter().map(|p| format!("`{}`", p)).join(", "),
                                format!("{}({})", name, params.join(", ")),
                            ]
                            .join("\t"),
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();

                let mut engine = self.create_engine()?;
                let doc_values = engine.eval("tsv2table()", doc_csv.into_iter())?;
                self.print(
                    Some(
                        "| Function Name | Description | Parameters | Example |
| --- | --- | --- | --- |
",
                    ),
                    doc_values,
                    true,
                )?;

                Ok(())
            }
            None => {
                let mut engine = self.create_engine()?;

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

                let query = self.get_query();

                for (file, content) in self.read_contents()? {
                    self.execute(&mut engine, &query, file, &content)?;
                }

                Ok(())
            }
        }
    }

    fn create_engine(&self) -> miette::Result<Engine> {
        let mut engine = mq_lang::Engine::default();
        engine.load_builtin_module()?;
        engine.set_filter_none(!self.output.update);

        if let Some(dirs) = &self.input.module_directories {
            engine.set_paths(dirs.clone());
        }

        if let Some(modules) = &self.input.module_names {
            for module_name in modules {
                engine.load_module(module_name)?;
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

        Ok(engine)
    }

    fn get_query(&self) -> String {
        self.input
            .from_file
            .as_ref()
            .and_then(|files| {
                files
                    .iter()
                    .map(|file| fs::read_to_string(file).into_diagnostic())
                    .collect::<miette::Result<Vec<String>>>()
                    .map(|r| r.join("|\n"))
                    .ok()
            })
            .unwrap_or_else(|| self.query.clone().unwrap_or_default())
    }

    fn execute(
        &self,
        engine: &mut mq_lang::Engine,
        query: &str,
        file: Option<PathBuf>,
        content: &str,
    ) -> miette::Result<()> {
        if let Some(file) = file {
            unsafe { env::set_var("__FILE__", file.to_string_lossy().to_string()) };
        }

        let runtime_values = if self.input.null_input {
            engine.eval(
                query,
                vec![mq_lang::Value::String("".to_string())].into_iter(),
            )
        } else if self.input.raw_input {
            let runtime_values = content
                .lines()
                .map(|line| mq_lang::Value::String(line.to_string()))
                .collect::<Vec<_>>();
            engine.eval(query, runtime_values.into_iter())
        } else {
            let markdown: mq_markdown::Markdown = mq_markdown::Markdown::from_str(content)?;
            let input = markdown.nodes.into_iter().map(mq_lang::Value::from);

            if self.output.update {
                let results = engine.eval(query, input.clone())?;
                results
                    .values()
                    .iter()
                    .zip(input.into_iter())
                    .flat_map(|(updated_runtime_value, runtime_value)| {
                        if let mq_lang::Value::Markdown(node) = &runtime_value {
                            match updated_runtime_value {
                                mq_lang::Value::None
                                | mq_lang::Value::Function(_, _)
                                | mq_lang::Value::NativeFunction(_) => Ok(vec![runtime_value]),
                                mq_lang::Value::Markdown(_) => {
                                    Ok(vec![updated_runtime_value.clone()])
                                }
                                mq_lang::Value::String(s) => {
                                    Ok(vec![mq_lang::Value::Markdown(node.clone().with_value(s))])
                                }
                                mq_lang::Value::Bool(b) => Ok(vec![mq_lang::Value::Markdown(
                                    node.clone().with_value(b.to_string().as_str()),
                                )]),
                                mq_lang::Value::Number(n) => Ok(vec![mq_lang::Value::Markdown(
                                    node.clone().with_value(n.to_string().as_str()),
                                )]),
                                mq_lang::Value::Array(array) => Ok(array
                                    .iter()
                                    .filter_map(|o| {
                                        if !matches!(o, mq_lang::Value::None) {
                                            Some(mq_lang::Value::Markdown(
                                                node.clone().with_value(o.to_string().as_str()),
                                            ))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()),
                            }
                        } else {
                            Err(miette!("Internal error"))
                        }
                    })
                    .try_fold(
                        Vec::with_capacity(results.values().len()),
                        |mut acc, res| {
                            acc.extend(res);
                            Ok(acc)
                        },
                    )
                    .map(Into::into)
            } else {
                engine.eval(query, input)
            }
        }?;

        self.print(
            None,
            runtime_values,
            self.output.update || !self.output.compact_output,
        )
    }

    fn read_contents(&self) -> miette::Result<Vec<(Option<PathBuf>, String)>> {
        if self.input.null_input {
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
                let mut input = String::new();
                io::stdin().read_to_string(&mut input).into_diagnostic()?;
                Ok(vec![(None, input)])
            })
    }

    fn print(
        &self,
        header: Option<&str>,
        runtime_values: mq_lang::Values,
        pretty: bool,
    ) -> miette::Result<()> {
        let stdout = io::stdout();
        let mut handle: Box<dyn Write> = if let Some(output_file) = &self.output.output_file {
            let file = fs::File::create(output_file).into_diagnostic()?;
            Box::new(BufWriter::new(file))
        } else if self.output.unbuffered {
            Box::new(stdout.lock())
        } else {
            Box::new(BufWriter::new(stdout.lock()))
        };
        let runtime_values = if self.output.update {
            runtime_values.values()
        } else {
            &runtime_values.compact()
        };

        let mut markdown = mq_markdown::Markdown::new(
            runtime_values
                .iter()
                .map(|runtime_value| match runtime_value {
                    mq_lang::Value::Markdown(node) => node.clone(),
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
        });

        if let Some(header) = header {
            handle
                .write_all(header.as_bytes())
                .map_err(|e| miette!(e))?;
        }

        match self.output.output_format {
            Format::Html => handle
                .write_all(markdown.to_html().as_bytes())
                .map_err(|e| miette!(e))?,
            Format::Text => {
                handle
                    .write_all(markdown.to_text().as_bytes())
                    .map_err(|e| miette!(e))?;
            }
            Format::Markdown => {
                if pretty {
                    handle
                        .write_all(markdown.to_pretty_markdown()?.as_bytes())
                        .map_err(|e| miette!(e))?;
                } else {
                    handle
                        .write_all(markdown.to_string().as_bytes())
                        .map_err(|e| miette!(e))?;
                }
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
                null_input: true,
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            verbose: clap_verbosity_flag::Verbosity::new(0, 0),
            query: Some("self".to_string()),
            files: None,
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
                raw_input: true,
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            verbose: clap_verbosity_flag::Verbosity::new(0, 0),
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
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

        for format in [Format::Markdown, Format::Html, Format::Text] {
            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    output_format: format.clone(),
                    ..Default::default()
                },
                commands: None,
                verbose: clap_verbosity_flag::Verbosity::new(0, 0),
                query: Some("self".to_string()),
                files: Some(vec![temp_file_path.clone()]),
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
                verbose: clap_verbosity_flag::Verbosity::new(0, 0),
                query: Some("self".to_string()),
                files: Some(vec![temp_file_path.clone()]),
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
                target: FormatTarget::Mq,
            }),
            verbose: clap_verbosity_flag::Verbosity::new(0, 0),
            query: None,
            files: Some(vec![temp_file_path]),
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
                target: FormatTarget::Mq,
            }),
            verbose: clap_verbosity_flag::Verbosity::new(0, 0),
            query: None,
            files: Some(vec![temp_file_path]),
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
            verbose: clap_verbosity_flag::Verbosity::new(0, 0),
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
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
            verbose: clap_verbosity_flag::Verbosity::new(0, 0),
            query: Some("math".to_owned()),
            files: Some(vec![temp_md_file_path]),
        };

        assert!(cli.run().is_ok());
    }
}
