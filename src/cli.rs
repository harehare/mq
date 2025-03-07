use clap::CommandFactory;
use clap::{Parser, Subcommand};
use clap_complete::{Shell, generate};
use itertools::Itertools;
use miette::IntoDiagnostic;
use miette::miette;
use std::io::{self, BufWriter, Read, Write};
use std::str::FromStr;
use std::{env, fs, path::PathBuf};

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

#[derive(Clone, Debug, clap::Args)]
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

    /// Sets  string  that can be referenced at runtime
    #[arg(long = "arg", value_names = ["NAME", "VALUE"])]
    args: Option<Vec<String>>,
}

#[derive(Clone, Debug, clap::Args)]
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
}

#[derive(Debug, Subcommand)]
enum Commands {
    Repl,
    Fmt {
        #[arg(short, long, default_value_t = 2)]
        indent_width: usize,
        #[arg(short, long)]
        check: bool,
    },
    Completion {
        #[arg(short, long, value_enum)]
        shell: Shell,
    },
}

impl Cli {
    pub fn run(&self) -> miette::Result<()> {
        if self.commands.is_none() && self.query.is_none() {
            return Cli::command().print_help().into_diagnostic();
        }

        match &self.commands {
            Some(Commands::Repl) => {
                mq_repl::Repl::new(vec![mq_lang::Value::String("".to_string())]).run()
            }
            Some(Commands::Fmt {
                indent_width,
                check,
            }) => {
                for (_, content) in self.read_contents()? {
                    let formatted =
                        mq_formatter::Formatter::new(Some(mq_formatter::FormatterConfig {
                            indent_width: *indent_width,
                        }))
                        .format(&content)
                        .into_diagnostic()?;

                    if *check && formatted != content {
                        return Err(miette!("The input is not formatted"));
                    } else {
                        println!("{}", formatted);
                    }
                }

                Ok(())
            }
            Some(Commands::Completion { shell }) => {
                generate(*shell, &mut Cli::command(), "mq", &mut std::io::stdout());
                Ok(())
            }
            None => {
                let mut engine = mq_lang::Engine::default();
                engine.load_builtin_module()?;

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

                let query = self
                    .input
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
                    .unwrap_or_else(|| self.query.clone().unwrap_or_default());

                for (file, content) in self.read_contents()? {
                    self.execute(&mut engine, &query, file, &content)?;
                }

                Ok(())
            }
        }
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
                .collect_vec();
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

        self.print(runtime_values)
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
                        .collect_vec()
                })
            })
            .unwrap_or_else(|| {
                let mut input = String::new();
                io::stdin().read_to_string(&mut input).into_diagnostic()?;
                Ok(vec![(None, input)])
            })
    }

    fn print(&self, runtime_values: mq_lang::Values) -> miette::Result<()> {
        let stdout = io::stdout();
        let mut handle: Box<dyn Write> = if self.output.unbuffered {
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
                if self.output.update || !self.output.compact_output {
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
