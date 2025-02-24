use clap::CommandFactory;
use clap::{Parser, Subcommand};
use clap_complete::{Shell, generate};
use itertools::Itertools;
use miette::IntoDiagnostic;
use miette::miette;
use std::fmt::{self, Display};
use std::io::{self, BufWriter, Read, Write};
use std::str::FromStr;
use std::{env, fs, path::PathBuf};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::html::highlighted_html_for_string;
use syntect::parsing::SyntaxSet;
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

#[derive(Parser, Debug)]
#[command(name = "mdq")]
#[command(author = "Takahiro Sato. <harehare1110@gmail.com>")]
#[command(version = "0.1.0")]
#[command(after_help = "Examples:\n\n\
    To filter markdown nodes:\n\
    $ mdq 'query' file.md\n\n\
    To start a REPL session:\n\
    $ mdq repl\n\n\
    To format markdown file:\n\
    $ mdq fmt --check file.md")]
#[command(
    about = "mdq is a markdown processor that can filter markdown nodes by using jq-like syntax.",
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

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum Theme {
    #[default]
    SolarizedDark,
    SolarizedLight,
    Base16OceanDark,
    Base16OceanLight,
}

impl Theme {
    fn name(&self) -> String {
        match self {
            Theme::SolarizedDark => "Solarized (dark)".to_owned(),
            Theme::SolarizedLight => "Solarized (light)".to_owned(),
            Theme::Base16OceanDark => "base16-ocean.dark".to_owned(),
            Theme::Base16OceanLight => "base16-ocean.light".to_owned(),
        }
    }
}

impl Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Theme::SolarizedDark => "solarized-dark".to_owned(),
                Theme::SolarizedLight => "solarized-light".to_owned(),
                Theme::Base16OceanDark => "base16-ocean-dark".to_owned(),
                Theme::Base16OceanLight => "base16-ocean-light".to_owned(),
            }
        )
    }
}

#[derive(Clone, Debug, clap::Args)]
struct InputArgs {
    // load filter from the file
    #[arg(short, long)]
    from_file: Option<Vec<PathBuf>>,

    /// Reads each line as a string
    #[arg(short = 'R', long, group = "input")]
    raw_input: bool,

    /// Use empty string as the single input value
    #[arg(short, long, group = "input")]
    null_input: bool,

    // Search modules from the directory
    #[arg(short = 'L', long = "directory")]
    module_directories: Option<Vec<PathBuf>>,
}

#[derive(Clone, Debug, clap::Args)]
struct OutputArgs {
    /// Colorize output
    #[clap(short = 'C', long, default_value = "false")]
    color_output: bool,

    /// pretty print
    #[clap(short, long, default_value = "false")]
    compact_output: bool,

    /// Compact instead of pretty-printed output
    #[arg(short = 'F', long, value_enum, default_value_t)]
    output_format: Format,

    /// Update the input markdown
    #[clap(short = 'U', long, default_value = "false")]
    update: bool,

    /// Set the theme for syntax highlighting
    #[clap(long, default_value_t = Theme::SolarizedDark)]
    theme: Theme,

    /// Unbuffered output
    #[clap(long, default_value_t = false)]
    unbuffered: bool,
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
        if !self.output.color_output {
            unsafe { env::set_var("NO_COLOR", "true") }
        }

        if self.commands.is_none() && self.query.is_none() {
            return Cli::command().print_help().into_diagnostic();
        }

        match &self.commands {
            Some(Commands::Repl) => {
                mdq_repl::Repl::new(vec![mdq_lang::Value::String("".to_string())]).run()
            }
            Some(Commands::Fmt {
                indent_width,
                check,
            }) => {
                for (_, content) in self.read_contents()? {
                    let formatted =
                        mdq_formatter::Formatter::new(Some(mdq_formatter::FormatterConfig {
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
                generate(*shell, &mut Cli::command(), "mdq", &mut std::io::stdout());
                Ok(())
            }
            None => {
                let mut engine = mdq_lang::Engine::default();
                engine.load_builtin_module()?;

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
        engine: &mut mdq_lang::Engine,
        query: &str,
        file: Option<PathBuf>,
        content: &str,
    ) -> miette::Result<()> {
        if let Some(file) = file {
            unsafe { env::set_var("__FILE__", file.to_string_lossy().to_string()) };
        }

        let runtime_values = if self.input.null_input {
            engine.eval(
                &query,
                vec![mdq_lang::Value::String("".to_string())].into_iter(),
            )
        } else {
            if self.input.raw_input {
                let runtime_values = content
                    .lines()
                    .map(|line| mdq_lang::Value::String(line.to_string()))
                    .collect_vec();
                engine.eval(&query, runtime_values.into_iter())
            } else {
                let markdown: mdq_md::Markdown = mdq_md::Markdown::from_str(&content)?;
                let input = markdown.nodes.into_iter().map(mdq_lang::Value::from);

                if self.output.update {
                    let results = engine.eval(&query, input.clone())?;
                    results
                        .values()
                        .iter()
                        .zip(input.into_iter())
                        .flat_map(|(updated_runtime_value, runtime_value)| {
                            if let mdq_lang::Value::Markdown(node) = &runtime_value {
                                match updated_runtime_value {
                                    mdq_lang::Value::None
                                    | mdq_lang::Value::Function(_, _)
                                    | mdq_lang::Value::NativeFunction(_) => Ok(vec![runtime_value]),
                                    mdq_lang::Value::Markdown(_) => {
                                        Ok(vec![updated_runtime_value.clone()])
                                    }
                                    mdq_lang::Value::String(s) => {
                                        Ok(vec![mdq_lang::Value::Markdown(
                                            node.clone().with_value(s),
                                        )])
                                    }
                                    mdq_lang::Value::Bool(b) => {
                                        Ok(vec![mdq_lang::Value::Markdown(
                                            node.clone().with_value(b.to_string().as_str()),
                                        )])
                                    }
                                    mdq_lang::Value::Number(n) => {
                                        Ok(vec![mdq_lang::Value::Markdown(
                                            node.clone().with_value(n.to_string().as_str()),
                                        )])
                                    }
                                    mdq_lang::Value::Array(array) => Ok(array
                                        .iter()
                                        .filter_map(|o| {
                                            if !matches!(o, mdq_lang::Value::None) {
                                                Some(mdq_lang::Value::Markdown(
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
                    engine.eval(&query, input)
                }
            }
        }?;

        self.print(runtime_values)
    }

    fn read_contents(&self) -> miette::Result<Vec<(Option<PathBuf>, String)>> {
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

    fn print(&self, runtime_values: mdq_lang::Values) -> miette::Result<()> {
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

        let markdown = mdq_md::Markdown::new(
            runtime_values
                .iter()
                .map(|runtime_value| match runtime_value {
                    mdq_lang::Value::Markdown(node) => node.clone(),
                    _ => runtime_value.to_string().into(),
                })
                .collect(),
        );

        if self.output.color_output {
            let ps = SyntaxSet::load_defaults_newlines();
            let ts = ThemeSet::load_defaults();

            match self.output.output_format {
                Format::Text => {
                    let syntax = ps.find_syntax_by_extension("md").unwrap();
                    let mut h = HighlightLines::new(syntax, &ts.themes[&self.output.theme.name()]);
                    let markdown_text = markdown.to_text();
                    let lines = LinesWithEndings::from(markdown_text.as_str());
                    let mut text_lines = Vec::with_capacity(lines.count());

                    for line in LinesWithEndings::from(&markdown.to_text()) {
                        let ranges: Vec<(Style, &str)> =
                            h.highlight_line(line, &ps).into_diagnostic()?;
                        text_lines.push(as_24_bit_terminal_escaped(&ranges[..], true));
                    }

                    handle
                        .write_all(text_lines.join("").as_bytes())
                        .into_diagnostic()?;
                }
                Format::Markdown => {
                    let syntax = ps.find_syntax_by_extension("md").unwrap();
                    let mut h = HighlightLines::new(syntax, &ts.themes[&self.output.theme.name()]);
                    let s = if self.output.update || !self.output.compact_output {
                        markdown.to_pretty_markdown()?
                    } else {
                        markdown.to_string()
                    };
                    let lines = LinesWithEndings::from(&s);
                    let mut text_lines = Vec::with_capacity(lines.count());

                    for line in LinesWithEndings::from(&s) {
                        let ranges: Vec<(Style, &str)> =
                            h.highlight_line(line, &ps).into_diagnostic()?;

                        text_lines.push(as_24_bit_terminal_escaped(&ranges[..], true));
                    }

                    handle
                        .write_all(text_lines.join("").as_bytes())
                        .into_diagnostic()?;
                }
                Format::Html => {
                    let syntax = ps.find_syntax_by_extension("html").unwrap();
                    let html = highlighted_html_for_string(
                        &markdown.to_html(),
                        &ps,
                        syntax,
                        &ts.themes["base16-ocean.dark"],
                    )
                    .into_diagnostic()?;

                    handle.write_all(html.as_bytes()).into_diagnostic()?;
                }
            }
        } else {
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
        }

        if !self.output.unbuffered {
            handle.flush().expect("Error flushing");
        }

        Ok(())
    }
}
