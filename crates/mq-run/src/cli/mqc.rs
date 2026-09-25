//! `mq compile` and `mq run`: saving a query as `.mqc` bytecode and running it.
use super::{Cli, InputFormat};
use clap::ValueEnum;
use miette::{IntoDiagnostic, WrapErr, miette};
use std::fs;
use std::path::PathBuf;

/// `.mqc` metadata keys recording how `mq compile` shaped the query.
const MQC_QUERY_PREFIX: &str = "mq-run.query-prefix";
const MQC_INPUT_FORMAT: &str = "mq-run.input-format";
const MQC_AGGREGATE: &str = "mq-run.aggregate";

/// Subcommands recognized from the first positional argument, so every query flag still applies.
#[derive(Clone, Copy)]
pub(super) enum BytecodeCommand {
    Compile,
    Run,
}

impl Cli {
    pub(super) fn bytecode_command(&self) -> Option<BytecodeCommand> {
        if self.input.from_file || self.commands.is_some() {
            return None;
        }
        match self.query.as_deref()? {
            "compile" => Some(BytecodeCommand::Compile),
            "run" => Some(BytecodeCommand::Run),
            _ => None,
        }
    }

    /// `mq compile QUERY_FILE -o OUTPUT.mqc`
    pub(super) fn compile_bytecode(&self) -> miette::Result<()> {
        let [query_file] = self.files.as_deref().unwrap_or_default() else {
            return Err(miette!("Usage: mq compile QUERY_FILE -o OUTPUT.mqc"));
        };
        let output = self
            .output
            .output_file
            .as_ref()
            .ok_or_else(|| miette!("mq compile requires -o/--output for the .mqc file"))?;
        let query = fs::read_to_string(query_file)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to read {}", query_file.display()))?;
        let query = if self.input.aggregate {
            format!("nodes | {query}")
        } else {
            query
        };
        let prefix = self.auto_query_prefix(&None).unwrap_or_default();
        let effective_query = if prefix.is_empty() {
            query
        } else {
            format!("{prefix} | {query}")
        };
        let input_format = self
            .explicit_input_format()
            .and_then(|format| format.to_possible_value())
            .map(|value| value.get_name().to_string())
            .unwrap_or_default();

        let mut engine = self.create_engine()?;
        let bytes = engine
            .compile_to_mqc(
                &effective_query,
                &[
                    (MQC_QUERY_PREFIX, &prefix),
                    (MQC_INPUT_FORMAT, &input_format),
                    (MQC_AGGREGATE, if self.input.aggregate { "true" } else { "false" }),
                ],
            )
            .map_err(miette::Report::new)?;
        fs::write(output, bytes)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to write {}", output.display()))
    }

    /// Reads and validates the `mq run` program; the remaining positionals are its input files.
    pub(super) fn load_bytecode(&self) -> miette::Result<()> {
        let Some(path) = self.files.as_deref().and_then(<[PathBuf]>::first) else {
            return Err(miette!("Usage: mq run PROGRAM.mqc [FILES]..."));
        };
        for (flag, given) in [
            ("-M/--module-names", self.input.module_names.is_some()),
            ("-m/--import-module-names", self.input.import_module_names.is_some()),
            ("-L/--directory", self.input.module_directories.is_some()),
        ] {
            if given {
                return Err(miette!(
                    "{flag} has no effect on mq run: modules are compiled into the program. Pass it to `mq compile` instead."
                ));
            }
        }
        let bytes = fs::read(path)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to read {}", path.display()))?;
        let program = self
            .create_engine()?
            .load_mqc(&bytes)
            .map_err(miette::Report::new)
            .wrap_err_with(|| format!("Failed to load {}", path.display()))?;
        let compiled_aggregate = program.metadata(MQC_AGGREGATE) == Some("true");
        if compiled_aggregate != self.input.aggregate {
            return Err(miette!(
                "-A/--aggregate must match between `mq compile` and `mq run` (the program was compiled {} it)",
                if compiled_aggregate { "with" } else { "without" }
            ));
        }
        self.bytecode
            .set(bytes)
            .map_err(|_| miette!("The mq run program was already loaded"))
    }

    /// Loads the `mq run` program, checking it was compiled for `file`'s input handling.
    pub(super) fn load_mqc_program(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        bytes: &[u8],
        file: &Option<PathBuf>,
    ) -> miette::Result<mq_lang::CompiledProgram> {
        let program = engine.load_mqc(bytes).map_err(miette::Report::new)?;
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
                help = "Pass the same -I (and --csv-delimiter/--no-header) to `mq compile` and `mq run`.",
                "The program was compiled for {compiled_format} input, but {target} is read as {effective_format} input"
            ));
        }
        Ok(program.program().clone())
    }

    /// The input format `file` is read as, when it matters to the `mq run` program.
    pub(super) fn mqc_input_format(&self, file: &Option<PathBuf>) -> Option<String> {
        self.bytecode.get().is_some().then(|| self.input_format_name(file))
    }

    #[cfg(feature = "watch")]
    pub(super) fn ensure_watch_supported(&self) -> miette::Result<()> {
        if self.bytecode.get().is_some() {
            return Err(miette!(
                "--watch does not support `mq run PROGRAM.mqc`: the compiled program is loaded once and never reloaded"
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
