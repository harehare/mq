//! `mq compile` and running `.mqc` programs.
use super::{Cli, InputArgs, InputFormat, ProgramArgs};
use clap::ValueEnum;
use miette::{IntoDiagnostic, WrapErr, miette};
use std::fs;
use std::path::{Path, PathBuf};

/// `.mqc` metadata keys recording how `mq compile` shaped the query.
const MQC_QUERY_PREFIX: &str = "mq-run.query-prefix";
const MQC_INPUT_FORMAT: &str = "mq-run.input-format";
const MQC_AGGREGATE: &str = "mq-run.aggregate";

impl Cli {
    /// `mq compile QUERY_FILE [-o OUTPUT.mqc]`
    pub(super) fn compile_bytecode(
        query_file: &Path,
        output: Option<&Path>,
        program: &ProgramArgs,
    ) -> miette::Result<()> {
        let output = match output {
            Some(output) => output.to_path_buf(),
            None => {
                let output = query_file.with_extension("mqc");
                if output == query_file {
                    return Err(miette!(
                        "{} already has the .mqc extension; pass -o/--output to choose the output file",
                        query_file.display()
                    ));
                }
                output
            }
        };
        if let (Ok(input), Ok(output)) = (fs::canonicalize(query_file), fs::canonicalize(&output))
            && input == output
        {
            return Err(miette!(
                "{} is the query file; choose a different -o/--output",
                output.display()
            ));
        }
        let cli = Cli {
            input: InputArgs {
                program: program.clone(),
                ..Default::default()
            },
            ..Default::default()
        };
        cli.validate_csv_options()?;
        let query = fs::read_to_string(query_file)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to read {}", query_file.display()))?;
        let query = if cli.input.program.aggregate {
            format!("nodes | {query}")
        } else {
            query
        };
        let prefix = cli.auto_query_prefix(&None).unwrap_or_default();
        let effective_query = if prefix.is_empty() {
            query
        } else {
            format!("{prefix} | {query}")
        };
        let input_format = cli
            .explicit_input_format()
            .and_then(|format| format.to_possible_value())
            .map(|value| value.get_name().to_string())
            .unwrap_or_default();

        let mut engine = cli.create_engine()?;
        let bytes = engine
            .compile_to_mqc(
                &effective_query,
                &[
                    (MQC_QUERY_PREFIX, &prefix),
                    (MQC_INPUT_FORMAT, &input_format),
                    (
                        MQC_AGGREGATE,
                        if cli.input.program.aggregate { "true" } else { "false" },
                    ),
                ],
            )
            .map_err(miette::Report::new)?;
        fs::write(&output, bytes)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to write {}", output.display()))
    }

    /// Loads the query as the program when it is a `.mqc` file path.
    pub(super) fn load_bytecode(&self) -> miette::Result<()> {
        let Some(query) = self.query.as_deref().filter(|_| self.commands.is_none()) else {
            return Ok(());
        };
        let path = Path::new(query);
        if path.extension().is_none_or(|ext| ext != "mqc") {
            return Ok(());
        }
        if self.input.from_file {
            return Err(miette!(
                help = format!("Run it without -f: mq {} [FILES]...", path.display()),
                "-f does not accept .mqc files"
            ));
        }
        if !path.is_file() {
            return Ok(());
        }
        let bytes = fs::read(path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to read {}", path.display()))?;
        let modules = &self.input.program;
        if modules.module_names.is_some()
            || modules.import_module_names.is_some()
            || modules.module_directories.is_some()
        {
            return Err(miette!(
                "-L/-M/-m have no effect on a .mqc program: modules are compiled into it. Pass them to `mq compile` instead."
            ));
        }
        let metadata = mq_lang::read_mqc_metadata(&bytes)
            .map_err(miette::Report::new)
            .wrap_err_with(|| format!("Failed to load {}", path.display()))?;
        let compiled_aggregate = metadata
            .iter()
            .any(|(key, value)| key == MQC_AGGREGATE && value == "true");
        if compiled_aggregate != self.input.program.aggregate {
            return Err(miette!(
                "-A/--aggregate must match between `mq compile` and running the .mqc program (it was compiled {} it)",
                if compiled_aggregate { "with" } else { "without" }
            ));
        }
        self.bytecode
            .set(bytes)
            .map_err(|_| miette!("The .mqc program was already loaded"))
    }

    /// Loads the `.mqc` program, checking it was compiled for `file`'s input handling.
    pub(super) fn load_mqc_program(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        bytes: &[u8],
        file: &Option<PathBuf>,
    ) -> miette::Result<mq_lang::CompiledProgram> {
        let program = engine
            .load_mqc(bytes)
            .map_err(miette::Report::new)
            .wrap_err_with(|| format!("Failed to load {}", self.query.as_deref().unwrap_or_default()))?;
        let compiled_prefix = program.metadata(MQC_QUERY_PREFIX).unwrap_or_default();
        let effective_format = self.input_format_name(file);
        // Native formats (raw, markdown, html, ...) share an empty prefix, so also compare format.
        let format_mismatch = match program.metadata(MQC_INPUT_FORMAT) {
            Some(format) if !format.is_empty() => format != effective_format,
            _ => false,
        };
        if compiled_prefix != self.auto_query_prefix(file).unwrap_or_default() || format_mismatch {
            let compiled_format = match program.metadata(MQC_INPUT_FORMAT) {
                Some(format) if !format.is_empty() => format,
                _ => "markdown",
            };
            let target = file
                .as_ref()
                .map_or_else(|| "stdin".to_string(), |path| path.display().to_string());
            return Err(miette!(
                help = "Pass the same -I (and --csv-delimiter/--no-header) to `mq compile` and when running the .mqc program.",
                "The program was compiled for {compiled_format} input, but {target} is read as {effective_format} input"
            ));
        }
        Ok(program.program().clone())
    }

    /// The input format `file` is read as, when it matters to the `.mqc` program.
    pub(super) fn mqc_input_format(&self, file: &Option<PathBuf>) -> Option<String> {
        self.bytecode.get().is_some().then(|| self.input_format_name(file))
    }

    #[cfg(feature = "watch")]
    pub(super) fn ensure_watch_supported(&self) -> miette::Result<()> {
        if self.bytecode.get().is_some() {
            return Err(miette!(
                "--watch does not support .mqc programs: they are loaded once and never reloaded"
            ));
        }
        Ok(())
    }

    fn input_format_name(&self, file: &Option<PathBuf>) -> String {
        let format = self
            .explicit_input_format()
            .or_else(|| file.as_deref().map(InputFormat::from_path))
            .unwrap_or_default();
        format
            .to_possible_value()
            .map_or_else(|| format!("{format:?}"), |value| value.get_name().to_string())
    }
}
