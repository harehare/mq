use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use miette::IntoDiagnostic;
use miette::WrapErr;
use miette::miette;
use mq_lang::DefaultEngine;
use mq_lang::DictMap;
use mq_lang::Shared;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, path::PathBuf};
use which::which;

use crate::atomic_output::{AtomicOutput, ClobberMode, OutputSink};
use crate::grep;
use exit_status::{BEST_OUTCOME, ExitReason, Outcome, ReadFailure};
use mq_help as help;

#[cfg(feature = "debug-trace")]
mod bytecode;
mod exit_status;
mod mqc;

/// A file's query prefix and, for a `.mqc` program, its input format.
type ProgramKey = (Option<String>, Option<String>);

/// Programs prepared for one engine, reused across its inputs.
#[derive(Default)]
struct ProgramCache {
    /// One program per [`ProgramKey`]; a run usually has one or two.
    programs: Vec<(ProgramKey, mq_lang::CompiledProgram)>,
    separator: Option<mq_lang::CompiledProgram>,
    /// Whether a file's `__FILE__` globals are installed and need resetting for stdin.
    file_vars_set: bool,
}

fn parse_timeout(value: &str) -> Result<Duration, String> {
    let secs = value
        .parse::<f64>()
        .map_err(|_| "expected a number of seconds".to_string())?;
    if !secs.is_finite() || secs <= 0.0 {
        return Err("must be a finite number greater than 0".to_string());
    }
    let timeout = Duration::try_from_secs_f64(secs).map_err(|_| "timeout is too large".to_string())?;
    // `Instant` ranges differ per OS (i64 secs on Unix, u64 on Windows), so cap at the smaller one.
    if timeout.as_secs() > i64::MAX as u64 || std::time::Instant::now().checked_add(timeout).is_none() {
        return Err("timeout is too large".to_string());
    }
    Ok(timeout)
}

#[derive(Parser, Debug, Default)]
#[command(name = "mq")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(after_help = "# Examples\n\n\
    mq 'query' file.md\n\n\
    mq compile -f query.mq\n\
    mq query.mqc file.md\n\n\
    Compiled .mqc programs are experimental. Recompile them after upgrading mq.\n\n\
    Run `mq help examples` for more usage examples, or `mq help <name>` for\n\
    function/selector/module docs.\n")]
#[command(
    about = "mq is a markdown processor that can filter markdown nodes by using jq-like syntax.",
    long_about = None,
    disable_help_subcommand = true
)]
/// The `mq` command-line interface.
pub struct Cli {
    /// Suppress normal query output while preserving diagnostics, exit status, and query side effects.
    #[arg(short = 'q', long, global = true, default_value_t = false)]
    quiet: bool,

    #[clap(flatten)]
    input: InputArgs,

    #[clap(flatten)]
    output: OutputArgs,

    /// Set both input and output format at once (shorthand for `-I FORMAT -F FORMAT`).
    /// An explicit `-I`/`-F` overrides this for that side. Only accepts formats valid
    /// on both sides; e.g. `-I mdx` or `-F table` still require the dedicated flag.
    #[arg(short = 'T', long = "format", value_enum)]
    format: Option<IoFormat>,

    #[clap(subcommand)]
    commands: Option<Commands>,

    /// Number of files to process before switching to parallel processing
    #[arg(short = 'P', long, default_value_t = 10)]
    parallel_threshold: usize,

    #[arg(value_name = "QUERY OR FILE")]
    query: Option<String>,
    files: Option<Vec<PathBuf>>,

    /// Positional string arguments, available as ARGS."positional" in queries.
    #[arg(long = "args", num_args = 0..)]
    args: Option<Vec<String>>,

    /// Maximum time in seconds allowed for query evaluation before aborting (e.g. 0.5, 5).
    /// Must be a finite positive number. No timeout by default.
    #[arg(long, value_name = "SECONDS", value_parser = parse_timeout, allow_negative_numbers = true)]
    timeout: Option<Duration>,

    /// Include VM frames in uncaught runtime errors.
    #[arg(long, default_value_t = false)]
    stack_trace: bool,

    /// Format for the final uncaught error. Combines with `--stack-trace`.
    #[arg(long, value_enum, default_value_t = ErrorFormat::Human)]
    error_format: ErrorFormat,

    /// Enter the interactive debugger when an uncaught error occurs (mq-dbg only).
    #[cfg(feature = "debugger")]
    #[arg(long = "stop-on-error", default_value_t = false)]
    stop_on_error: bool,

    /// Print the Tarn VM operand stack whenever the debugger stops (mq-dbg `debug-trace` build only).
    #[cfg(feature = "debug-trace")]
    #[arg(long = "dump-stack", default_value_t = false)]
    dump_stack: bool,

    /// Print Tarn VM instruction execution counts to stderr (mq-dbg profile builds only).
    #[cfg(feature = "vm-profile")]
    #[arg(long = "vm-profile", default_value_t = false)]
    vm_profile: bool,

    /// Print the Tarn VM bytecode to stdout and exit without running the program or reading
    /// input. Pass a format as `--dump-bytecode=json` (mq-dbg `debug-trace` build only).
    #[cfg(feature = "debug-trace")]
    #[arg(
        long = "dump-bytecode",
        value_name = "FORMAT",
        value_enum,
        num_args = 0..=1,
        require_equals = true,
        default_missing_value = "text"
    )]
    dump_bytecode: Option<bytecode::BytecodeFormat>,

    /// The `.mqc` program, which replaces the query.
    #[arg(skip)]
    bytecode: std::sync::OnceLock<mq_lang::Mqc>,
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
/// - Csv/Gron/Json/Psv/Toml/Toon/Tsv/Xml/Yaml: Auto-import the matching module and parse.
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
    Gron,
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
            "gron" => Self::Gron,
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

    fn is_gzip_path(path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gz"))
    }

    fn from_path(path: &Path) -> Self {
        if Self::is_gzip_path(path) {
            let inner_ext = Path::new(path.file_stem().unwrap_or_default())
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default();
            Self::from_extension(inner_ext)
        } else {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
            Self::from_extension(ext)
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
            Self::Gron => Some(r#"import "gron" | gron::gron_parse()"#),
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

/// How the final uncaught error is rendered.
#[derive(Clone, Copy, Debug, Default, PartialEq, clap::ValueEnum)]
pub enum ErrorFormat {
    /// Colored, human-readable diagnostic (default).
    #[default]
    Human,
    /// Single-line JSON object via `miette::JSONReportHandler`, for CI/tooling.
    Json,
}

/// Preset combination of `--allow-*` flags, set via `--sandbox`.
#[derive(Clone, Debug, clap::ValueEnum, PartialEq)]
enum SandboxProfile {
    /// Deny everything (same as no flags).
    Strict,
    /// Unrestricted filesystem reads only.
    ReadOnly,
    /// Unrestricted network access (`http()` and HTTP imports) only.
    Networked,
    /// Everything, equivalent to `--allow-all`.
    Unsafe,
}

#[derive(Clone, Debug, Default, clap::ValueEnum, PartialEq)]
enum OutputFormat {
    #[default]
    Markdown,
    Html,
    Text,
    Json,
    Table,
    Grep,
    Gron,
    Raw,
    Csv,
    Toml,
    Toon,
    Xml,
    Yaml,
    Shell,
    None,
}

impl OutputFormat {
    fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "html" | "htm" => Self::Html,
            "txt" | "log" => Self::Raw,
            "json" => Self::Json,
            "csv" => Self::Csv,
            "toml" => Self::Toml,
            "toon" => Self::Toon,
            "xml" => Self::Xml,
            "yaml" | "yml" => Self::Yaml,
            "gron" => Self::Gron,
            _ => Self::Markdown,
        }
    }

    fn from_path(path: &Path) -> Self {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
        Self::from_extension(ext)
    }
}

#[derive(Clone, Debug, clap::ValueEnum, PartialEq)]
enum IoFormat {
    Markdown,
    Html,
    Text,
    Json,
    Gron,
    Raw,
    Csv,
    Toml,
    Toon,
    Xml,
    Yaml,
}

impl From<IoFormat> for InputFormat {
    fn from(fmt: IoFormat) -> Self {
        match fmt {
            IoFormat::Markdown => Self::Markdown,
            IoFormat::Html => Self::Html,
            IoFormat::Text => Self::Text,
            IoFormat::Json => Self::Json,
            IoFormat::Gron => Self::Gron,
            IoFormat::Raw => Self::Raw,
            IoFormat::Csv => Self::Csv,
            IoFormat::Toml => Self::Toml,
            IoFormat::Toon => Self::Toon,
            IoFormat::Xml => Self::Xml,
            IoFormat::Yaml => Self::Yaml,
        }
    }
}

impl From<IoFormat> for OutputFormat {
    fn from(fmt: IoFormat) -> Self {
        match fmt {
            IoFormat::Markdown => Self::Markdown,
            IoFormat::Html => Self::Html,
            IoFormat::Text => Self::Text,
            IoFormat::Json => Self::Json,
            IoFormat::Gron => Self::Gron,
            IoFormat::Raw => Self::Raw,
            IoFormat::Csv => Self::Csv,
            IoFormat::Toml => Self::Toml,
            IoFormat::Toon => Self::Toon,
            IoFormat::Xml => Self::Xml,
            IoFormat::Yaml => Self::Yaml,
        }
    }
}

#[derive(Debug, Clone, Default, clap::ValueEnum)]
/// Bullet marker for list output.
// Variant docs would change `--help` output.
#[allow(missing_docs)]
pub enum ListStyle {
    #[default]
    Dash,
    Plus,
    Star,
}

#[derive(Debug, Clone, PartialEq, Default, clap::ValueEnum)]
/// Quote style for link titles.
// Variant docs would change `--help` output.
#[allow(missing_docs)]
pub enum LinkTitleStyle {
    #[default]
    Double,
    Single,
    Paren,
}

#[derive(Debug, Clone, PartialEq, Default, clap::ValueEnum)]
/// Wrapping style for link URLs.
// Variant docs would change `--help` output.
#[allow(missing_docs)]
pub enum LinkUrlStyle {
    #[default]
    None,
    Angle,
}

/// Flags that shape the compiled program or its permissions, shared by queries and `mq compile`.
#[derive(Clone, Debug, clap::Args, Default, PartialEq)]
struct ProgramArgs {
    /// Aggregate all input files/content into a single array
    #[arg(short = 'A', long, default_value_t = false)]
    aggregate: bool,

    /// Set input format
    #[arg(short = 'I', long, value_enum)]
    input_format: Option<InputFormat>,

    /// Custom delimiter for `-I csv` input (a single ASCII character). Has no effect on
    /// `-I tsv`/`-I psv`, which use a fixed tab/pipe delimiter by design; pass `-I csv`
    /// with this flag instead if you need a different delimiter (e.g. `;`)
    #[arg(long = "csv-delimiter", value_name = "CHAR")]
    csv_delimiter: Option<char>,

    /// Treat csv/tsv/psv input as headerless: each row becomes an array of values
    /// instead of a dict keyed by header names. Applies to `-I csv`, `-I tsv`, and
    /// `-I psv`
    #[arg(long = "no-header", default_value_t = false)]
    no_header: bool,

    /// Do not read frontmatter at the start of Markdown or MDX input: the lines between `---` or
    /// `+++` are then text, a heading or a rule. Applies to `-I markdown` and `-I mdx`, and to
    /// `.md` and `.mdx` files
    #[arg(long = "no-frontmatter", default_value_t = false)]
    no_frontmatter: bool,

    /// Search modules from the directory
    #[arg(short = 'L', long = "directory")]
    module_directories: Option<Vec<PathBuf>>,

    /// Load additional modules from specified files
    #[arg(short = 'M', long)]
    module_names: Option<Vec<String>>,

    /// Import modules by name, making them available as `name::fn()` in queries
    #[arg(short = 'm', long)]
    import_module_names: Option<Vec<String>>,

    /// Allow `import`/`include` to fetch modules over HTTP(S). Disabled by default.
    /// `--allow-http-import=DOMAIN` (repeat the flag, or comma-separate) also allows those
    /// domains beyond the default. Use `github.com/{user}/{repo}` for one repository
    /// (expanded automatically), or a plain domain like `example.com` for any path under
    /// that host. The `=` is required so a bare domain isn't taken as a query/file positional.
    #[cfg(feature = "http-import")]
    #[arg(long = "allow-http-import", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "DOMAIN")]
    allow_http_import: Option<Vec<String>>,

    /// Same as `--allow-http-import=DOMAIN`, except it does not enable HTTP imports by itself.
    #[cfg(feature = "http-import")]
    #[arg(long = "allowed-domain", hide = true)]
    allowed_domains: Option<Vec<String>>,

    /// Force re-fetch of mutable-ref (HEAD/branch) HTTP-imported modules, ignoring the local cache.
    /// Versioned (tagged) modules are never re-fetched regardless of this flag.
    #[cfg(feature = "http-import")]
    #[arg(long = "refresh-modules", default_value_t = false)]
    refresh_modules: bool,

    /// Remove all HTTP module cache including versioned (tagged) modules and lock files.
    /// Use this to fully reset the cache when something goes wrong.
    #[cfg(feature = "http-import")]
    #[arg(long = "clear-cache", default_value_t = false)]
    clear_cache: bool,

    /// Disable the mq.lock integrity check for HTTP imports.
    /// By default a fetched URL's content is checked against mq.lock, and a mismatch is
    /// rejected unless --refresh-modules is also passed.
    #[cfg(feature = "http-import")]
    #[arg(long = "no-lockfile", default_value_t = false, conflicts_with_all = ["lockfile_path", "frozen"])]
    no_lockfile: bool,

    /// Fail instead of recording a new mq.lock entry.
    /// Use in CI so a new module's content is only trusted during a reviewable local run whose mq.lock diff gets committed.
    #[cfg(feature = "http-import")]
    #[arg(long = "frozen", default_value_t = false)]
    frozen: bool,

    /// Path to the mq.lock file used for HTTP import integrity checks.
    /// Defaults to ./mq.lock (relative to the current directory).
    #[cfg(feature = "http-import")]
    #[arg(long = "lockfile", value_name = "PATH")]
    lockfile_path: Option<PathBuf>,

    /// Allow the `http` function to make outbound HTTPS requests. Disabled by default;
    /// requests are HTTPS-only and blocked from reaching loopback/private/link-local
    /// addresses regardless of this flag. Pass with no value to allow any domain, or
    /// `--allow-net=DOMAIN` (repeat the flag, or comma-separate, to add more) to restrict
    /// requests to just those domains (and any path under them). The `=` is required so a
    /// bare domain after the flag isn't swallowed as a query/file positional instead.
    #[cfg(feature = "http-import")]
    #[arg(short = 'N', long = "allow-net", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "DOMAIN")]
    allow_net: Option<Vec<String>>,

    /// Allow the `read_file`/`read_file_bytes`/`open_file`/`collection`/`walk_files`/`file_exists`/`file_info`/`embed_images`
    /// functions to read from the filesystem. Disabled by default. Pass with no value to
    /// allow reading anywhere, or `--allow-read=PATH` (files or directories; repeat the
    /// flag, or comma-separate, to add more) to restrict reads to just those paths and
    /// their descendants. The `=` is required so a bare path after the flag isn't
    /// swallowed as a query/file positional instead.
    #[arg(short = 'R', long = "allow-read", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "PATH")]
    allow_read: Option<Vec<PathBuf>>,

    /// Allow the `write_file`/`extract_images` functions to write to the filesystem.
    /// Under `--sandbox`, this also gates `-o`/`--output` (and `--append`, `--no-clobber`,
    /// `--atomic-output`), which are rejected unless the target is allowed. Without
    /// `--sandbox`, `-o` is not restricted. Disabled by default. Pass with no value to allow writing anywhere, or
    /// `--allow-write=PATH` (files or directories; repeat the flag, or comma-separate, to
    /// add more) to restrict writes to just those paths and their descendants. The `=` is
    /// required so a bare path after the flag isn't swallowed as a query/file positional
    /// instead.
    #[arg(short = 'W', long = "allow-write", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "PATH")]
    allow_write: Option<Vec<PathBuf>>,

    /// Allow the `system` function to execute external commands. Disabled by default.
    /// Commands run directly (never through a shell), so shell metacharacters in arguments
    /// are never interpreted. Pass with no value to allow any command, or
    /// `--allow-run=COMMAND` (repeat the flag, or comma-separate, to add more) to restrict
    /// execution to just those commands. The `=` is required so a bare command after the
    /// flag isn't swallowed as a query/file positional instead.
    #[arg(long = "allow-run", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "COMMAND")]
    allow_run: Option<Vec<String>>,

    /// Allow `$VAR`/`${$VAR}` interpolation and debugger logpoints to read environment
    /// variables. Disabled by default. Pass with no value to allow reading any variable, or
    /// `--allow-env=NAME` (repeat the flag, or comma-separate, to add more) to restrict
    /// access to just those names. The `=` is required so a bare name after the flag isn't
    /// swallowed as a query/file positional instead.
    #[arg(short = 'E', long = "allow-env", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "NAME")]
    allow_env: Option<Vec<String>>,

    /// Grant every sandboxed permission at once (read/write/net/run/env), and also enable
    /// HTTP module imports as if --allow-http-import were passed. Disabled by default.
    /// Cannot be combined with the individual --allow-* flags above.
    #[arg(
        short = 'a',
        long = "allow-all",
        default_value_t = false,
        conflicts_with_all = ["allow_net", "allow_read", "allow_write", "allow_run", "allow_env", "allow_http_import"]
    )]
    allow_all: bool,

    /// Named preset of sandboxed capabilities. Cannot be combined with --allow-* flags,
    /// except --allow-write, which adds write access (including `-o`/`--output`) on top of
    /// the preset. Under a preset, `-o`/`--output` is denied unless write access is granted.
    #[arg(
        long = "sandbox",
        value_enum,
        conflicts_with_all = ["allow_net", "allow_read", "allow_run", "allow_env", "allow_http_import", "allow_all"]
    )]
    sandbox: Option<SandboxProfile>,
}

#[derive(Clone, Debug, clap::Args, Default)]
struct InputArgs {
    #[clap(flatten)]
    program: ProgramArgs,

    /// load filter from the file
    #[arg(short, long, default_value_t = false)]
    from_file: bool,

    /// Sets a named string argument. NAME is accessible directly in queries, and also
    /// via ARGS."named" when --arg or --args is given.
    #[arg(long, num_args = 2, value_names = ["NAME", "VALUE"], alias = "define")]
    arg: Option<Vec<String>>,

    /// Sets a named JSON argument. NAME is accessible directly in queries
    #[arg(long, num_args = 2, value_names = ["NAME", "JSON_VALUE"])]
    argjson: Option<Vec<String>>,

    /// Sets file contents that can be referenced at runtime
    #[arg(long="rawfile", num_args = 2, value_names = ["NAME", "FILE"])]
    raw_file: Option<Vec<String>>,

    /// Sets a named argument from a JSON file. NAME is bound to an array of every JSON
    /// value found in FILE (jq --slurpfile compatible), so a file containing a single
    /// JSON value becomes a one-element array.
    #[arg(long = "slurpfile", num_args = 2, value_names = ["NAME", "FILE"])]
    slurp_file: Option<Vec<String>>,

    /// Enable streaming mode for processing large files line by line
    #[arg(long, default_value_t = false)]
    stream: bool,

    /// Watch the input file(s) for changes and automatically re-run the query whenever
    /// they change. Requires at least one input file (stdin cannot be watched). With
    /// --from-file, the query file is watched too. Runs until interrupted (Ctrl-C); a
    /// query error is printed to stderr and watching continues rather than exiting.
    #[cfg(feature = "watch")]
    #[arg(long, default_value_t = false)]
    watch: bool,

    /// Evaluate the query once against all input files combined (like yq's `eval-all`),
    /// instead of once per file. Enables cross-file aggregation in a single query.
    #[arg(
        long = "eval-all",
        default_value_t = false,
        conflicts_with_all = ["update", "count", "stream", "separator"]
    )]
    eval_all: bool,
}

impl ProgramArgs {
    /// Builds sandboxed IO permissions from `--sandbox` or the individual `--allow-*` flags.
    fn build_sandboxed_io(&self) -> mq_lang::SandboxedIo {
        let sandboxed_io = mq_lang::SandboxedIo::new(mq_lang::NativeIo::default());
        match &self.sandbox {
            Some(SandboxProfile::Strict) => sandboxed_io.allow_write(self.allow_write.clone()),
            Some(SandboxProfile::ReadOnly) => sandboxed_io.allow_read(true).allow_write(self.allow_write.clone()),
            Some(SandboxProfile::Networked) => sandboxed_io.allow_net(true).allow_write(self.allow_write.clone()),
            Some(SandboxProfile::Unsafe) => sandboxed_io.allow_all(),
            None if self.allow_all => sandboxed_io.allow_all(),
            None => sandboxed_io
                .allow_read(self.allow_read.clone())
                .allow_write(self.allow_write.clone())
                .allow_net(self.allow_net.clone())
                .allow_run(self.allow_run.clone())
                .allow_env(self.allow_env.clone()),
        }
    }

    /// Whether `--sandbox` enables HTTP module imports.
    #[cfg(feature = "http-import")]
    fn sandbox_enables_http_import(&self) -> bool {
        matches!(
            self.sandbox,
            Some(SandboxProfile::Networked) | Some(SandboxProfile::Unsafe)
        )
    }

    /// Whether HTTP module imports are enabled by any of `--allow-http-import`, `--allow-all` or `--sandbox`.
    #[cfg(feature = "http-import")]
    fn http_import_enabled(&self) -> bool {
        self.allow_http_import.is_some() || self.allow_all || self.sandbox_enables_http_import()
    }

    /// Extra domains allowed for HTTP imports, from `--allow-http-import=DOMAIN` and `--allowed-domain`.
    #[cfg(feature = "http-import")]
    fn http_import_domains(&self) -> Vec<String> {
        self.allow_http_import
            .iter()
            .chain(&self.allowed_domains)
            .flatten()
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, clap::Args, Default)]
struct OutputArgs {
    /// Set output format. When omitted, inferred from the `-o`/`--output` file
    /// extension if given (e.g. `.json` -> json, `.csv` -> csv), else defaults to
    /// markdown.
    #[arg(short = 'F', long, value_enum, conflicts_with = "quiet")]
    output_format: Option<OutputFormat>,

    /// Update matching Markdown nodes and write the result to stdout
    #[arg(short = 'U', long = "update", default_value_t = false)]
    update: bool,

    /// With --update, print a unified diff instead of the transformed content;
    /// nothing is written. Multiple files are diffed one at a time with their path
    /// in the headers; stdin is labeled `<stdin>`. Exits 1 if anything would change.
    #[arg(long = "diff", default_value_t = false, requires = "update")]
    diff: bool,

    /// Unbuffered output
    #[clap(long, default_value_t = false, conflicts_with = "quiet")]
    unbuffered: bool,

    /// Set the list style for markdown output (default: keep the original)
    #[clap(long, value_enum, conflicts_with = "quiet")]
    list_style: Option<ListStyle>,

    /// Set the link title surround style for markdown output
    #[clap(long, value_enum, default_value_t = LinkTitleStyle::Double, conflicts_with = "quiet")]
    link_title_style: LinkTitleStyle,

    /// Set the link URL surround style for markdown links
    #[clap(long, value_enum, default_value_t = LinkUrlStyle::None, conflicts_with = "quiet")]
    link_url_style: LinkUrlStyle,

    /// Specify a query to insert between files as a separator
    #[clap(short = 'S', long, value_name = "QUERY", conflicts_with = "quiet")]
    separator: Option<String>,

    /// Output to the specified file
    #[clap(short = 'o', long = "output", value_name = "FILE", conflicts_with = "quiet")]
    output_file: Option<PathBuf>,

    /// Write `-o`/`--output` atomically via a same-directory temp file + fsync
    /// + rename, so a crash or full disk mid-write can't truncate the target.
    #[clap(long, value_enum, default_value_t = AtomicOutput::Auto, requires = "output_file", conflicts_with = "quiet")]
    atomic_output: AtomicOutput,

    /// Fail instead of overwriting `-o`/`--output` if the target already exists.
    #[clap(long, default_value_t = false, requires = "output_file", conflicts_with_all = ["append", "quiet"])]
    no_clobber: bool,

    /// Add to `-o`/`--output` instead of replacing it, creating it if missing.
    /// Use `--atomic-output never` for concurrent-safe appends across processes.
    #[clap(
        long,
        default_value_t = false,
        requires = "output_file",
        conflicts_with_all = ["no_clobber", "quiet"]
    )]
    append: bool,

    /// Whether this process has already opened `output_file` once.
    #[clap(skip)]
    output_claimed: std::sync::Arc<AtomicBool>,

    /// Colorize markdown output
    #[arg(
        short = 'C',
        long = "color-output",
        default_value_t = false,
        conflicts_with = "quiet"
    )]
    color_output: bool,

    /// Show NUM nodes before each match. Only effective with -F grep.
    #[clap(short = 'B', long, value_name = "NUM", conflicts_with = "quiet")]
    before_context: Option<usize>,

    /// Show NUM nodes after each match. Only effective with -F grep.
    #[clap(long, value_name = "NUM", conflicts_with = "quiet")]
    after_context: Option<usize>,

    /// Show NUM nodes before and after each match. Only effective with -F grep.
    #[clap(long, value_name = "NUM", conflicts_with = "quiet")]
    context: Option<usize>,

    /// Set the exit code from the result, like jq's -e: 0 truthy, 1 false/null,
    /// 4 no result. Errors exit 2 (unreadable file), 3 (syntax) or 5 (runtime).
    #[arg(short = 'e', long = "exit-status", default_value_t = false)]
    exit_status: bool,

    /// Output only the count of matching (non-None) results.
    /// With multiple files, prints "filename: N" per file and "total: N" at the end.
    #[arg(long = "count", default_value_t = false, conflicts_with_all = ["update", "stream", "quiet"])]
    count: bool,

    /// Skip the first N matching results before outputting.
    #[arg(long, value_name = "N", conflicts_with_all = ["update", "quiet"])]
    skip: Option<usize>,

    /// Limit output to at most N results.
    #[arg(long, value_name = "N", conflicts_with_all = ["update", "quiet"])]
    limit: Option<usize>,

    /// Omit Markdown node position information from structured output
    /// (json, table, gron, csv, toml, toon, xml, yaml). Reduces output size
    /// when source line/column spans aren't needed.
    #[arg(long, default_value_t = false, conflicts_with = "quiet")]
    no_position: bool,

    /// Print JSON on a single line, without pretty-printing. Only valid with -F json.
    #[arg(short = 'c', long, default_value_t = false, conflicts_with = "quiet")]
    compact: bool,

    /// Number of spaces per indent level in pretty-printed output (0-7), jq's
    /// `--indent`. Only valid with -F json or -F xml; default is 2.
    #[arg(
        long,
        value_name = "N",
        value_parser = clap::value_parser!(u8).range(0..=7),
        conflicts_with_all = ["tab", "compact", "quiet"]
    )]
    indent: Option<u8>,

    /// Indent pretty-printed output with tabs instead of spaces, jq's `--tab`.
    /// Only valid with -F json or -F xml.
    #[arg(long, default_value_t = false, conflicts_with_all = ["indent", "compact", "quiet"])]
    tab: bool,
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

    /// Applies skip/limit pagination to a vector of values.
    ///
    /// Call this on compact (non-empty) values so that N refers to visible results.
    fn paginate<T>(&self, values: Vec<T>) -> Vec<T> {
        let skip = self.skip.unwrap_or(0);
        let values: Vec<T> = values.into_iter().skip(skip).collect();
        if let Some(limit) = self.limit {
            values.into_iter().take(limit).collect()
        } else {
            values
        }
    }

    /// The per-level indent string for pretty-printed JSON output, honoring `--tab`/`--indent`.
    fn json_indent_unit(&self) -> String {
        if self.tab {
            "\t".to_string()
        } else {
            " ".repeat(self.indent.unwrap_or(2) as usize)
        }
    }

    /// The `(indent_char, indent_size)` pair for pretty-printed XML output, honoring `--tab`/`--indent`.
    fn xml_indent(&self) -> (u8, usize) {
        if self.tab {
            (b'\t', 1)
        } else {
            (b' ', self.indent.unwrap_or(2) as usize)
        }
    }
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Start a REPL session for interactive query execution. Optional FILES are
    /// combined as the initial input (same file/format handling as `mq QUERY FILES...`).
    Repl {
        /// Markdown (or other supported format) files to load as the REPL's initial input
        files: Option<Vec<PathBuf>>,
    },
    /// Start a debug adapter for mq
    #[cfg(feature = "debugger")]
    Dap,
    /// List built-in and external subcommands
    #[command(name = "commands")]
    Subcommands {
        /// Print machine-readable JSON instead of formatted text
        #[arg(long)]
        json: bool,
    },
    /// Generate a shell completion script and print it to stdout
    Completion {
        /// Shell to generate the completion script for
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    /// Show documentation for a builtin function, selector, standard module, standard-module
    /// function, or the `examples` topic.
    Help {
        /// Name of a function, selector, module, or `examples`, e.g. `map`, `.h1`, `csv_parse`,
        /// `section`, `examples`
        name: Option<String>,
        /// Print machine-readable JSON instead of formatted text
        #[arg(long, conflicts_with = "markdown")]
        json: bool,
        /// Print Markdown instead of formatted text, e.g. queryable with mq itself:
        /// `mq help section --markdown | mq 'select(.code.lang == "mq")'`
        #[arg(long)]
        markdown: bool,
    },
    /// Compile a query to experimental `.mqc` bytecode.
    #[command(
        long_about = "Compile a query to a .mqc program. Run it with `mq PROGRAM.mqc [FILES]...`.\n\nThe .mqc format and CLI are experimental and may change. Recompile programs after upgrading mq."
    )]
    Compile {
        /// Query to compile, or a query file with -f
        #[arg(value_name = "QUERY OR FILE")]
        query: String,
        /// Load the query from the file
        #[arg(short, long)]
        from_file: bool,
        /// Output file [default: the query file with a `.mqc` extension; required for a query string]
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[clap(flatten)]
        program: Box<ProgramArgs>,
    },
}

/// Shell targets supported by the `completion` subcommand.
#[derive(Clone, Debug, clap::ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    Nushell,
    #[value(name = "powershell")]
    PowerShell,
    Zsh,
}

/// Output mode for the `help` subcommand.
#[derive(Clone, Copy)]
enum HelpFormat {
    Human,
    Json,
    Markdown,
}

impl Cli {
    /// Reserved `mq help` topic name for general CLI usage examples (not a function,
    /// selector, or module).
    const EXAMPLES_TOPIC: &'static str = "examples";

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

    /// Resolve `mq-NAME` in `~/.local/bin` first, then `PATH`.
    fn external_command_path(name: &str) -> Option<PathBuf> {
        let command_bin = format!("mq-{}", name);
        let local_path = Self::get_external_commands_dir().unwrap_or_default().join(&command_bin);
        if local_path.exists() {
            Some(local_path)
        } else {
            which(&command_bin).ok()
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

    /// Print built-in subcommands (from the clap definition) and external `mq-*` commands.
    fn run_commands(json: bool) -> miette::Result<()> {
        let builtin: Vec<(String, String)> = Self::command()
            .get_subcommands()
            .filter(|command| !command.is_hide_set())
            .map(|command| {
                let about = command.get_about().map(|about| about.to_string()).unwrap_or_default();
                let summary = about.split_once(". ").map_or(about.as_str(), |(first, _)| first);
                (
                    command.get_name().to_string(),
                    summary.trim_end_matches('.').to_string(),
                )
            })
            .collect();
        let external: Vec<(String, Option<PathBuf>)> = Self::find_external_commands()
            .into_iter()
            .map(|name| {
                let path = Self::external_command_path(&name);
                (name, path)
            })
            .collect();

        if json {
            let value = serde_json::json!({
                "builtin": builtin
                    .iter()
                    .map(|(name, description)| serde_json::json!({ "name": name, "description": description }))
                    .collect::<Vec<_>>(),
                "external": external
                    .iter()
                    .map(|(name, path)| serde_json::json!({ "name": name, "path": path }))
                    .collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&value).into_diagnostic()?);
            return Ok(());
        }

        let width = builtin
            .iter()
            .map(|(name, _)| name.len())
            .chain(external.iter().map(|(name, _)| name.len()))
            .max()
            .unwrap_or(0);
        let mut output = vec![format!("{}", "Built-in subcommands:".bold().cyan())];
        for (name, description) in &builtin {
            output.push(format!("  {:<width$}  {}", name.green(), description));
        }
        if !external.is_empty() {
            output.push(String::new());
            output.push(format!(
                "{}",
                "External subcommands (from ~/.local/bin and PATH):".bold().yellow()
            ));
            for (name, path) in &external {
                let path = path.as_ref().map(|p| p.display().to_string()).unwrap_or_default();
                output.push(format!("  {:<width$}  {}", name.bright_yellow(), path));
            }
        }

        println!("{}", output.join("\n"));
        Ok(())
    }

    /// Generate a shell completion script for the given shell and print it to stdout.
    fn new_repl(engine: mq_lang::DefaultEngine, input: Vec<mq_lang::RuntimeValue>) -> mq_repl::Repl {
        let repl = mq_repl::Repl::with_engine(engine, input);
        if cfg!(feature = "debugger") {
            repl.with_name("mq-dbg")
        } else {
            repl
        }
    }

    fn generate_completion(shell: &CompletionShell) -> miette::Result<()> {
        let mut command = Cli::command();
        let name = command.get_name().to_string();

        match shell {
            CompletionShell::Bash => {
                clap_complete::generate(clap_complete::Shell::Bash, &mut command, name, &mut io::stdout())
            }
            CompletionShell::Elvish => {
                clap_complete::generate(clap_complete::Shell::Elvish, &mut command, name, &mut io::stdout())
            }
            CompletionShell::Fish => {
                clap_complete::generate(clap_complete::Shell::Fish, &mut command, name, &mut io::stdout())
            }
            CompletionShell::Nushell => {
                clap_complete::generate(clap_complete_nushell::Nushell, &mut command, name, &mut io::stdout())
            }
            CompletionShell::PowerShell => {
                clap_complete::generate(clap_complete::Shell::PowerShell, &mut command, name, &mut io::stdout())
            }
            CompletionShell::Zsh => {
                clap_complete::generate(clap_complete::Shell::Zsh, &mut command, name, &mut io::stdout())
            }
        }

        Ok(())
    }

    /// Shows documentation for a single function/selector/module, or lists everything known
    /// when `name` is `None`. Writes the whole output in one call, not one `println!` per
    /// line: hundreds of lines otherwise means hundreds of flushed syscalls.
    fn run_help(name: Option<&str>, json: bool, markdown: bool) -> miette::Result<()> {
        let format = if json {
            HelpFormat::Json
        } else if markdown {
            HelpFormat::Markdown
        } else {
            HelpFormat::Human
        };

        let stdout = io::stdout();
        let mut handle = BufWriter::new(stdout.lock());

        let Some(name) = name else {
            let out = match format {
                HelpFormat::Json => serde_json::to_string_pretty(&help::all_names()).into_diagnostic()?,
                HelpFormat::Markdown => Self::help_index_markdown(),
                HelpFormat::Human => Self::help_index_text(),
            };
            Self::write_ignore_pipe(&mut handle, out.as_bytes())?;
            Self::write_ignore_pipe(&mut handle, b"\n")?;
            return handle.flush().into_diagnostic();
        };

        // `examples` is a reserved topic name (not a function/selector/module): general CLI
        // usage that clap's own `--help`/`after_help` intentionally keeps short.
        if name == Self::EXAMPLES_TOPIC {
            let out = match format {
                HelpFormat::Json => serde_json::to_string_pretty(&serde_json::json!({
                    "topic": Self::EXAMPLES_TOPIC,
                    "content": Self::examples_topic_markdown(),
                }))
                .into_diagnostic()?,
                HelpFormat::Markdown => Self::examples_topic_markdown(),
                HelpFormat::Human => Self::examples_topic_text(),
            };
            Self::write_ignore_pipe(&mut handle, out.as_bytes())?;
            Self::write_ignore_pipe(&mut handle, b"\n")?;
            return handle.flush().into_diagnostic();
        }

        // A bare name that's both a module and a same-named function within it (e.g.
        // `section`) resolves to the module overview; `module::function` disambiguates.
        if !name.contains("::")
            && let Some(module) = help::lookup_module(name)
        {
            // A selector can also share a module's bare name (e.g. `.table`/`table`,
            // `.toml`/`toml`, `.yaml`/`yaml`); append it after the module overview instead
            // of leaving it unreachable without the leading dot. JSON keeps the module
            // alone for output stability; pass the leading dot (`.table`) for the
            // selector's own JSON.
            let selector_entries: Vec<_> = help::lookup(name)
                .into_iter()
                .filter(|e| e.kind == "selector")
                .collect();

            let out = match format {
                HelpFormat::Json => serde_json::to_string_pretty(&module).into_diagnostic()?,
                HelpFormat::Markdown => {
                    let mut s = help::render_module_markdown(&module);
                    for entry in &selector_entries {
                        s.push('\n');
                        s.push_str(&help::render_markdown(entry));
                    }
                    s
                }
                HelpFormat::Human => {
                    let mut s = help::render_module_human(&module);
                    for entry in &selector_entries {
                        s.push('\n');
                        s.push_str(&help::render_human(entry));
                    }
                    s
                }
            };
            Self::write_ignore_pipe(&mut handle, out.as_bytes())?;
            Self::write_ignore_pipe(&mut handle, b"\n")?;
            return handle.flush().into_diagnostic();
        }

        let entries = help::lookup(name);
        if entries.is_empty() {
            return Err(match help::suggest(name) {
                Some(suggestion) => {
                    miette!("no function, selector, or module named `{name}` — did you mean `{suggestion}`?")
                }
                None => miette!("no function, selector, or module named `{name}`"),
            });
        }

        let out = match format {
            HelpFormat::Json => serde_json::to_string_pretty(&entries).into_diagnostic()?,
            HelpFormat::Markdown => entries.iter().map(help::render_markdown).collect::<Vec<_>>().join("\n"),
            HelpFormat::Human => entries.iter().map(help::render_human).collect::<Vec<_>>().join("\n"),
        };
        Self::write_ignore_pipe(&mut handle, out.as_bytes())?;
        Self::write_ignore_pipe(&mut handle, b"\n")?;
        handle.flush().into_diagnostic()
    }

    /// Sorted, deduped selector, top-level function, and keyword names for the `mq help`
    /// index. Cheap: uses `top_level_entries`, which never parses standard-module sources.
    fn help_index_names() -> (Vec<String>, Vec<String>, Vec<String>) {
        let entries = help::top_level_entries();

        let mut selectors: Vec<String> = entries
            .iter()
            .filter(|e| e.kind == "selector")
            .map(|e| e.name.clone())
            .collect();
        selectors.sort_unstable();
        selectors.dedup();

        let mut functions: Vec<String> = entries
            .iter()
            .filter(|e| e.kind == "function")
            .map(|e| e.name.clone())
            .collect();
        functions.sort_unstable();
        functions.dedup();

        let mut keywords: Vec<String> = entries
            .iter()
            .filter(|e| e.kind == "keyword")
            .map(|e| e.name.clone())
            .collect();
        keywords.sort_unstable();
        keywords.dedup();

        (selectors, functions, keywords)
    }

    /// Builds the grouped `mq help` index text: selectors, top-level functions, and modules
    /// (each with a one-line summary), for a name-less human-readable lookup.
    fn help_index_text() -> String {
        let (selectors, functions, keywords) = Self::help_index_names();
        let mut out = String::new();

        let _ = writeln!(out, "{}", "Selectors:".bold().cyan());
        for s in selectors {
            let _ = writeln!(out, "  {s}");
        }

        let _ = writeln!(out, "\n{}", "Functions:".bold().cyan());
        for f in functions {
            let _ = writeln!(out, "  {f}");
        }

        let _ = writeln!(out, "\n{}", "Keywords:".bold().cyan());
        for k in keywords {
            let _ = writeln!(out, "  {k}");
        }

        let _ = writeln!(out, "\n{}", "Modules:".bold().cyan());
        for module in help::all_modules() {
            let summary = module.description.split(". ").next().unwrap_or_default();
            let padded_name = format!("{:<10}", module.name);
            let _ = writeln!(out, "  {} {}", padded_name.green(), summary);
        }

        let _ = write!(
            out,
            "\nRun `mq help <name>` for a function or selector, `mq help <module>` for a module \
            overview, `mq help examples` for CLI usage examples."
        );

        out
    }

    /// Same content as [`Self::help_index_text`], as Markdown: queryable with mq itself.
    fn help_index_markdown() -> String {
        let (selectors, functions, keywords) = Self::help_index_names();
        let mut out = String::new();

        let _ = writeln!(out, "# mq help");

        let _ = writeln!(out, "\n## Selectors\n");
        for s in selectors {
            let _ = writeln!(out, "- `{s}`");
        }

        let _ = writeln!(out, "\n## Functions\n");
        for f in functions {
            let _ = writeln!(out, "- `{f}`");
        }

        let _ = writeln!(out, "\n## Keywords\n");
        for k in keywords {
            let _ = writeln!(out, "- `{k}`");
        }

        let _ = writeln!(out, "\n## Modules\n");
        for module in help::all_modules() {
            let summary = module.description.split(". ").next().unwrap_or_default();
            if summary.is_empty() {
                let _ = writeln!(out, "- `{}`", module.name);
            } else {
                let _ = writeln!(out, "- `{}` — {}", module.name, summary);
            }
        }

        let _ = write!(
            out,
            "\nRun `mq help <name>` for a function or selector, `mq help <module>` for a module \
            overview, `mq help examples` for CLI usage examples."
        );

        out
    }

    /// Text for the `mq help examples` topic, kept out of clap's own `after_help` so
    /// `--help` stays short; this is the fuller reference it points to.
    fn examples_topic_text() -> String {
        let mut out = String::new();

        let _ = writeln!(out, "{}", "Basic usage:".bold().cyan());
        let _ = writeln!(out, "  mq 'query' file.md");
        let _ = writeln!(out, "  mq -f 'file' file.md        # read query from file");
        let _ = writeln!(out, "  mq repl                     # start a REPL session");

        let _ = writeln!(out, "\n{}", "Auto-parsing by file extension or -I flag:".bold().cyan());
        let _ = writeln!(
            out,
            "  mq automatically imports the matching module based on the file extension."
        );
        let _ = writeln!(out, "  Use -I <format> to force a specific format:\n");
        let _ = writeln!(
            out,
            "  .cbor / -I cbor  import \"cbor\" | cbor::cbor_parse()  (reads as bytes)"
        );
        let _ = writeln!(out, "  .csv  / -I csv   import \"csv\"  | csv::csv_parse(true)");
        let _ = writeln!(out, "  .gron / -I gron  import \"gron\" | gron::gron_parse()");
        let _ = writeln!(out, "  .json / -I json  import \"json\" | json::json_parse()");
        let _ = writeln!(out, "  .psv  / -I psv   import \"csv\"  | csv::psv_parse(true)");
        let _ = writeln!(out, "  .toml / -I toml  import \"toml\" | toml::toml_parse()");
        let _ = writeln!(out, "  .toon / -I toon  import \"toon\" | toon::toon_parse()");
        let _ = writeln!(out, "  .tsv  / -I tsv   import \"csv\"  | csv::tsv_parse(true)");
        let _ = writeln!(out, "  .xml  / -I xml   import \"xml\"  | xml::xml_parse()");
        let _ = writeln!(out, "  .yaml / -I yaml  import \"yaml\" | yaml::yaml_parse()\n");
        let _ = writeln!(
            out,
            "  Use -I raw   to disable auto-parsing and receive the raw string."
        );
        let _ = writeln!(out, "  Use -I bytes to read input as raw bytes without parsing.");

        let _ = writeln!(out, "\n{}", "Output formats (-F):".bold().cyan());
        let _ = writeln!(out, "  mq -F json '.h' file.md          # headings as JSON nodes");
        let _ = writeln!(
            out,
            "  mq -F csv 'to_text()' file.md    # every node's text, one per CSV row"
        );
        let _ = writeln!(
            out,
            "  mq -o out.json 'self' file.md    # -F inferred from -o's extension"
        );
        let _ = writeln!(
            out,
            "  mq -T json 'self' file.txt       # sets both -I and -F at once (-I/-F still override)"
        );

        let _ = writeln!(out, "\n{}", "Updating markdown (-U):".bold().cyan());
        let _ = writeln!(
            out,
            "  mq -U '.h1 | update(\"New title\")' file.md         # whole document to stdout; the file is not touched"
        );
        let _ = writeln!(
            out,
            "  mq -U --diff '.h1 | update(\"New title\")' file.md  # preview the change as a unified diff"
        );
        let _ = writeln!(out, "  (-U never edits in place; unmatched content is reformatted too)");

        let _ = writeln!(out, "\n{}", "Filtering with context (-F grep):".bold().cyan());
        let _ = writeln!(
            out,
            "  mq -F grep --context 1 '.h' file.md  # matches with 1 node of context on each side"
        );
        let _ = writeln!(out, "  (also: -B/--before-context, --after-context)");

        let _ = writeln!(out, "\n{}", "Passing arguments to queries (ARGS):".bold().cyan());
        let _ = writeln!(
            out,
            "  When --arg or --args is given, ARGS = {{\"positional\": [...], \"named\": {{...}}}}\n"
        );
        let _ = writeln!(out, "  mq -I null 'name' --arg name Alice");
        let _ = writeln!(out, "  mq -I null 'ARGS | .\"named\"' --arg name Alice");
        let _ = writeln!(out, "  # => {{\"name\": \"Alice\"}}\n");
        let _ = writeln!(
            out,
            "  mq -I null 'ARGS | .\"positional\"' --args x y z  # must come after query and files"
        );
        let _ = writeln!(out, "  # => [\"x\", \"y\", \"z\"]\n");
        let _ = writeln!(out, "  mq -I null 'ARGS' file.md --arg name Alice --args x y z");
        let _ = writeln!(
            out,
            "  # => {{\"positional\": [\"x\",\"y\",\"z\"], \"named\": {{\"name\": \"Alice\"}}}}"
        );

        let _ = writeln!(
            out,
            "\n{}",
            "Sandboxed capabilities (--allow-read/write/net/run/env):".bold().cyan()
        );
        let _ = writeln!(
            out,
            "  Disabled by default; each takes an optional allowlist (comma-separated)."
        );
        let _ = writeln!(out, "  mq --allow-read=. -I null 'read_file(\"notes.md\")'");
        let _ = writeln!(out, "  mq --allow-run=echo -I null 'system(\"echo\", [\"hello\"])'");
        let _ = writeln!(out, "  mq --allow-env=MY_VAR -I null '$MY_VAR'");
        let _ = writeln!(
            out,
            "  mq --allow-net=api.example.com 'http(\"get\", \"https://api.example.com/data\")' file.md"
        );
        let _ = writeln!(
            out,
            "  mq --sandbox=read-only -I null 'read_file(\"notes.md\")'  # strict/read-only/networked/unsafe presets"
        );

        #[cfg(feature = "http-import")]
        {
            let _ = writeln!(out, "\n{}", "HTTP module imports (--allow-http-import):".bold().cyan());
            let _ = writeln!(
                out,
                "  Separate from --allow-net above; disabled by default regardless of it."
            );
            let _ = writeln!(
                out,
                "  mq --allow-http-import 'import \"github.com/harehare/kdl.mq\"' file.md"
            );
        }

        let _ = writeln!(out, "\n{}", "Working with multiple files:".bold().cyan());
        let _ = writeln!(
            out,
            "  mq --eval-all '.h1' a.md b.md          # one query over all files combined"
        );
        let _ = writeln!(
            out,
            "  mq -S '\"---\"' '.h1' a.md b.md          # insert a separator between files"
        );

        let _ = writeln!(out, "\n{}", "Streaming large files (--stream):".bold().cyan());
        let _ = write!(out, "  mq --stream -I text 'select(contains(\"ERROR\"))' huge.log");

        out
    }

    /// Same content as [`Self::examples_topic_text`], as Markdown: queryable with mq itself
    /// via `mq help examples --markdown | mq '...'`.
    fn examples_topic_markdown() -> String {
        let mut out = String::new();

        let _ = writeln!(out, "# mq help examples");

        let _ = writeln!(out, "\n## Basic usage\n");
        let _ = writeln!(
            out,
            "```sh\nmq 'query' file.md\nmq -f 'file' file.md        # read query from file\nmq repl                     # start a REPL session\n```"
        );

        let _ = writeln!(out, "\n## Auto-parsing by file extension or -I flag\n");
        let _ = writeln!(
            out,
            "mq automatically imports the matching module based on the file extension. \
            Use `-I <format>` to force a specific format:\n"
        );
        let _ = writeln!(out, "| Extension / `-I` | Query prefix |");
        let _ = writeln!(out, "|---|---|");
        let _ = writeln!(
            out,
            "| `.cbor` / `-I cbor` | `import \"cbor\" \\| cbor::cbor_parse()` (reads as bytes) |"
        );
        let _ = writeln!(out, "| `.csv` / `-I csv` | `import \"csv\" \\| csv::csv_parse(true)` |");
        let _ = writeln!(
            out,
            "| `.gron` / `-I gron` | `import \"gron\" \\| gron::gron_parse()` |"
        );
        let _ = writeln!(
            out,
            "| `.json` / `-I json` | `import \"json\" \\| json::json_parse()` |"
        );
        let _ = writeln!(out, "| `.psv` / `-I psv` | `import \"csv\" \\| csv::psv_parse(true)` |");
        let _ = writeln!(
            out,
            "| `.toml` / `-I toml` | `import \"toml\" \\| toml::toml_parse()` |"
        );
        let _ = writeln!(
            out,
            "| `.toon` / `-I toon` | `import \"toon\" \\| toon::toon_parse()` |"
        );
        let _ = writeln!(out, "| `.tsv` / `-I tsv` | `import \"csv\" \\| csv::tsv_parse(true)` |");
        let _ = writeln!(out, "| `.xml` / `-I xml` | `import \"xml\" \\| xml::xml_parse()` |");
        let _ = writeln!(
            out,
            "| `.yaml` / `-I yaml` | `import \"yaml\" \\| yaml::yaml_parse()` |"
        );
        let _ = writeln!(
            out,
            "\nUse `-I raw` to disable auto-parsing and receive the raw string. Use `-I bytes` to \
            read input as raw bytes without parsing."
        );

        let _ = writeln!(out, "\n## Output formats (-F)\n");
        let _ = writeln!(
            out,
            "```sh\nmq -F json '.h' file.md          # headings as JSON nodes\nmq -F csv 'to_text()' file.md    # every node's text, one per CSV row\n\
            mq -o out.json 'self' file.md    # -F inferred from -o's extension\n\
            mq -T json 'self' file.txt       # sets both -I and -F at once (-I/-F still override)\n```"
        );

        let _ = writeln!(out, "\n## Updating markdown (-U)\n");
        let _ = writeln!(
            out,
            "```sh\nmq -U '.h1 | update(\"New title\")' file.md         # whole document to stdout; the file is not touched\n\
            mq -U --diff '.h1 | update(\"New title\")' file.md  # preview the change as a unified diff\n```\n\n\
            `-U` never edits in place; unmatched content is reformatted too."
        );

        let _ = writeln!(out, "\n## Filtering with context (-F grep)\n");
        let _ = writeln!(
            out,
            "```sh\nmq -F grep --context 1 '.h' file.md  # matches with 1 node of context on each side\n```\n\n\
            (also: `-B`/`--before-context`, `--after-context`)"
        );

        let _ = writeln!(out, "\n## Passing arguments to queries (ARGS)\n");
        let _ = writeln!(
            out,
            "When `--arg` or `--args` is given, `ARGS = {{\"positional\": [...], \"named\": {{...}}}}`\n"
        );
        let _ = writeln!(
            out,
            "```sh\nmq -I null 'name' --arg name Alice\nmq -I null 'ARGS | .\"named\"' --arg name Alice\n\
            # => {{\"name\": \"Alice\"}}\n\n\
            mq -I null 'ARGS | .\"positional\"' --args x y z  # must come after query and files\n\
            # => [\"x\", \"y\", \"z\"]\n\n\
            mq -I null 'ARGS' file.md --arg name Alice --args x y z\n\
            # => {{\"positional\": [\"x\",\"y\",\"z\"], \"named\": {{\"name\": \"Alice\"}}}}\n```"
        );

        let _ = writeln!(out, "\n## Sandboxed capabilities (--allow-read/write/net/run/env)\n");
        let _ = writeln!(
            out,
            "Disabled by default; each takes an optional allowlist (comma-separated).\n"
        );
        let _ = writeln!(
            out,
            "```sh\nmq --allow-read=. -I null 'read_file(\"notes.md\")'\n\
            mq --allow-run=echo -I null 'system(\"echo\", [\"hello\"])'\n\
            mq --allow-env=MY_VAR -I null '$MY_VAR'\n\
            mq --allow-net=api.example.com 'http(\"get\", \"https://api.example.com/data\")' file.md\n\
            mq --sandbox=read-only -I null 'read_file(\"notes.md\")'  # strict/read-only/networked/unsafe presets\n```"
        );

        #[cfg(feature = "http-import")]
        {
            let _ = writeln!(out, "\n## HTTP module imports (--allow-http-import)\n");
            let _ = writeln!(
                out,
                "Separate from --allow-net above; disabled by default regardless of it.\n"
            );
            let _ = writeln!(
                out,
                "```sh\nmq --allow-http-import 'import \"github.com/harehare/kdl.mq\"' file.md\n```"
            );
        }

        let _ = writeln!(out, "\n## Working with multiple files\n");
        let _ = writeln!(
            out,
            "```sh\nmq --eval-all '.h1' a.md b.md          # one query over all files combined\n\
            mq -S '\"---\"' '.h1' a.md b.md          # insert a separator between files\n```"
        );

        let _ = writeln!(out, "\n## Streaming large files (--stream)\n");
        let _ = write!(
            out,
            "```sh\nmq --stream -I text 'select(contains(\"ERROR\"))' huge.log\n```"
        );

        out
    }

    /// The exit code requested by `halt(code)`, if `err` came from it.
    pub fn halt_exit_code(err: &miette::Report) -> Option<i32> {
        err.downcast_ref::<mq_lang::Error>().and_then(mq_lang::Error::exit_code)
    }

    /// The process exit code for an uncaught error. `halt(code)` always wins. Otherwise it is 1,
    /// or with `--exit-status` the jq codes: 2 for an I/O error, 3 for a syntax or module error
    /// and 5 for a runtime error.
    pub fn error_exit_code(&self, err: &miette::Report) -> i32 {
        if let Some(code) = Self::halt_exit_code(err) {
            return code;
        }
        if !self.output.exit_status {
            return ExitReason::Failure.code();
        }
        if err.downcast_ref::<ReadFailure>().is_some() {
            return ExitReason::IoError.code();
        }
        if err
            .downcast_ref::<mq_lang::Error>()
            .is_some_and(mq_lang::Error::is_compile_error)
        {
            ExitReason::SyntaxError.code()
        } else {
            ExitReason::RuntimeError.code()
        }
    }

    /// Prints an uncaught error to stderr per `--error-format`.
    pub fn report_error(&self, err: &miette::Report) {
        match self.error_format {
            ErrorFormat::Human => {
                eprintln!("Error: {err:?}");
                if let Some(hint) = Self::capability_hint(&err.to_string()) {
                    eprintln!("  hint: {hint}");
                }
            }
            ErrorFormat::Json => match Self::render_error_json(err) {
                Some(json) => eprintln!("{json}"),
                None => eprintln!("Error: {err:?}"),
            },
        }
    }

    #[cold]
    fn capability_hint(message: &str) -> Option<String> {
        fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
            let rest = text.split_once(start)?.1;
            Some(rest.split_once(end)?.0.trim())
        }
        let broad = |flag: &str, scoped: &str| {
            Some(format!(
                "the sandbox blocked this. Pass `{flag}` (or `{scoped}` to limit it), or `--allow-all`."
            ))
        };
        let scoped = |flag: &str, value: &str| {
            Some(format!(
                "the sandbox blocked this. Add it with `{flag}={value}`, or pass `--allow-all`."
            ))
        };

        if message.contains("network access is disabled") {
            return broad("--allow-net", "--allow-net=DOMAIN");
        }
        if message.contains("filesystem reads are disabled") {
            return broad("--allow-read", "--allow-read=PATH");
        }
        if message.contains("filesystem writes are disabled") {
            return broad("--allow-write", "--allow-write=PATH");
        }
        if message.contains("environment variable access is disabled") {
            return broad("--allow-env", "--allow-env=NAME");
        }
        if message.contains("process execution is disabled") {
            return broad("--allow-run", "--allow-run=COMMAND");
        }
        if let Some(url) = between(
            message,
            "network access to ",
            " is not allowed (outside the allowed domains)",
        ) {
            let host = url.split("://").last().unwrap_or(url).split('/').next().unwrap_or(url);
            return scoped("--allow-net", host);
        }
        if let Some(path) = between(message, " of ", " is not allowed (outside the allowed paths)") {
            let action = message.split(" of ").next()?.split_whitespace().last()?;
            let flag = if action == "write" {
                "--allow-write"
            } else {
                "--allow-read"
            };
            return scoped(flag, path);
        }
        if let Some(name) = between(message, "environment variable `", "` is not allowed") {
            return scoped("--allow-env", name);
        }
        if let Some(command) = between(
            message,
            "execution of ",
            " is not allowed (outside the allowed commands)",
        ) {
            return scoped("--allow-run", command);
        }
        None
    }

    /// Renders `err` as a single-line JSON diagnostic object (`None` on a render failure).
    fn render_error_json(err: &miette::Report) -> Option<String> {
        let mut json = String::new();
        miette::JSONReportHandler::new().render_report(&mut json, &**err).ok()?;
        Some(json)
    }

    /// Runs the command or query.
    pub fn run(&self) -> miette::Result<()> {
        if let Some(result) = self.run_standalone_command() {
            return result;
        }

        self.load_bytecode()?;

        if let Some(result) = self.run_external_command() {
            return result;
        }

        self.validate_csv_options()?;
        self.validate_frontmatter_option()?;

        match &self.commands {
            Some(Commands::Repl { files }) => return self.run_repl(files.as_deref()),
            None if self.query.is_none() => return self.run_repl(None),
            _ => {}
        }

        self.validate_output_options()?;
        self.run_query()
    }

    /// Runs the subcommands that take no query or input document, so none of the query
    /// input/output flag validation applies to them.
    fn run_standalone_command(&self) -> Option<miette::Result<()>> {
        match self.commands.as_ref()? {
            Commands::Subcommands { json } => Some(Self::run_commands(*json)),
            Commands::Completion { shell } => Some(Self::generate_completion(shell)),
            Commands::Help { name, json, markdown } => Some(Self::run_help(name.as_deref(), *json, *markdown)),
            Commands::Compile {
                query,
                from_file,
                output,
                program,
            } => {
                if self.format.is_some() {
                    return Some(Err(miette!(
                        "-T/--format has no effect on `mq compile`; pass -I/--input-format after `compile` instead"
                    )));
                }
                if self.input.program != ProgramArgs::default() || self.input.from_file {
                    return Some(Err(miette!(
                        "Pass compile options after `compile`: mq compile [OPTIONS] QUERY"
                    )));
                }
                Some(Self::compile_bytecode(query, *from_file, output.as_deref(), program))
            }
            #[cfg(feature = "debugger")]
            Commands::Dap => Some(mq_dap::start().map_err(|e| miette!(e.to_string()))),
            Commands::Repl { .. } => None,
        }
    }

    /// Runs `mq-NAME` when the query is the bare name of an external subcommand.
    ///
    /// clap parses `mq test arg1` as query="test", files=["arg1"], so this is detected here.
    fn run_external_command(&self) -> Option<miette::Result<()>> {
        if self.input.from_file || self.commands.is_some() || self.bytecode.get().is_some() {
            return None;
        }
        let query_value = self.query.as_ref()?;
        if !query_value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return None;
        }
        let command_path = Self::external_command_path(query_value)?;
        let mut args = vec![query_value.clone()];
        if let Some(files) = &self.files {
            args.extend(files.iter().map(|p| p.to_string_lossy().to_string()));
        }
        Some(self.execute_external_command(command_path, &args))
    }

    /// Checks the output flags against the resolved output format.
    fn validate_output_options(&self) -> miette::Result<()> {
        if (self.output.before_context.is_some()
            || self.output.after_context.is_some()
            || self.output.context.is_some())
            && !matches!(self.resolved_output_format(), OutputFormat::Grep)
        {
            return Err(miette!(
                "--before-context, --after-context, and --context are only valid with -F grep"
            ));
        }

        if !matches!(self.explicit_input_format(), Some(InputFormat::Markdown) | None) && self.output.update {
            return Err(miette!("The output format is not supported for the update option"));
        }

        if self.output.diff && matches!(self.resolved_output_format(), OutputFormat::Grep) {
            return Err(miette!("--diff is not supported with -F grep"));
        }

        if self.output.compact && !matches!(self.resolved_output_format(), OutputFormat::Json) {
            return Err(miette!("--compact is only valid with -F json"));
        }

        if (self.output.indent.is_some() || self.output.tab)
            && !matches!(self.resolved_output_format(), OutputFormat::Json | OutputFormat::Xml)
        {
            return Err(miette!("--indent/--tab are only valid with -F json or -F xml"));
        }

        self.check_output_permitted()
    }

    fn run_repl(&self, files: Option<&[PathBuf]>) -> miette::Result<()> {
        let engine = self.create_engine()?;
        let input = match files {
            Some(files) if !files.is_empty() => {
                let files = Self::expand_glob_patterns(files)?;
                if files.iter().any(|f| Self::is_stdin_path(f)) {
                    return Err(miette!("`-` (stdin) is not supported by the repl command"));
                }
                let contents = self.read_files_content(&files)?;
                let mut combined = Vec::new();
                for (file, content) in &contents {
                    combined.extend(self.resolve_input(file, content)?);
                }
                combined
            }
            _ => vec![mq_lang::RuntimeValue::String(Shared::new("".to_string()))],
        };
        Self::new_repl(engine, input).run()
    }

    fn run_query(&self) -> miette::Result<()> {
        #[cfg(feature = "watch")]
        {
            if self.input.watch {
                return self.run_watch();
            }
        }

        let result = self.execute_once();

        // --diff: exit 1 if anything would change, for CI gating. Takes precedence over -e.
        if self.output.diff && exit_status::had_diff() {
            result?;
            std::process::exit(ExitReason::Failure.code());
        }

        // --exit-status / -e: exit with code 1 if no truthy value was
        // produced. Mirrors jq's behaviour: false and null are falsy;
        // everything else (including empty string, 0, [], {}) is truthy.
        if self.output.exit_status
            && let Some(reason) = BEST_OUTCOME.get().exit_reason()
        {
            result?;
            std::process::exit(reason.code());
        }

        result
    }

    fn create_engine(&self) -> miette::Result<DefaultEngine> {
        let sandboxed_io = self.input.program.build_sandboxed_io();
        let mut engine = mq_lang::DefaultEngine::default();
        engine.set_io(Shared::new(sandboxed_io));
        engine.load_builtin_module();
        // Empty defaults for stdin; `--arg` etc. below may override them.
        Self::define_file_vars(&engine, None);

        if self.input.program.aggregate {
            engine.import_module("section").map_err(|e| *e)?;
        }

        if let Some(dirs) = &self.input.program.module_directories {
            engine.set_search_paths(dirs.clone());
        }

        if let Some(modules) = &self.input.program.module_names {
            for module_name in modules {
                engine.load_module(module_name).map_err(|e| *e)?;
            }
        }

        if let Some(modules) = &self.input.program.import_module_names {
            for module_name in modules {
                engine.import_module(module_name).map_err(|e| *e)?;
            }
        }

        if self.input.arg.is_some()
            || self.args.is_some()
            || self.input.argjson.is_some()
            || self.input.slurp_file.is_some()
        {
            let mut named: DictMap = DictMap::default();
            if let Some(args) = &self.input.arg {
                for v in args.chunks(2) {
                    engine.define_string_value(&v[0], &v[1]);
                    named.insert(
                        mq_lang::Ident::new(&v[0]),
                        mq_lang::RuntimeValue::String(Shared::new(v[1].clone())),
                    );
                }
            }

            if let Some(argjson) = &self.input.argjson {
                for v in argjson.chunks(2) {
                    let json_value: serde_json::Value = serde_json::from_str(&v[1]).into_diagnostic()?;
                    let runtime_value: mq_lang::RuntimeValue = json_value.into();
                    engine.define_value(&v[0], runtime_value.clone()).into_diagnostic()?;
                    named.insert(mq_lang::Ident::new(&v[0]), runtime_value);
                }
            }

            if let Some(slurp_file) = &self.input.slurp_file {
                for v in slurp_file.chunks(2) {
                    let path = PathBuf::from_str(&v[1]).into_diagnostic()?;

                    if !path.exists() {
                        return Err(miette!("File not found: {}", path.display()));
                    }

                    let content = fs::read_to_string(&path).into_diagnostic()?;
                    let json_values: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&content)
                        .into_iter::<serde_json::Value>()
                        .collect::<Result<_, _>>()
                        .into_diagnostic()?;
                    let runtime_value = mq_lang::RuntimeValue::Array(mq_lang::Shared::new(
                        json_values.into_iter().map(Into::into).collect(),
                    ));
                    engine.define_value(&v[0], runtime_value.clone()).into_diagnostic()?;
                    named.insert(mq_lang::Ident::new(&v[0]), runtime_value);
                }
            }

            let positional: Vec<mq_lang::RuntimeValue> = self
                .args
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|s| mq_lang::RuntimeValue::String(Shared::new(s.clone())))
                .collect();
            let args_map: DictMap = [
                (
                    mq_lang::Ident::new("positional"),
                    mq_lang::RuntimeValue::Array(Shared::new(positional)),
                ),
                (
                    mq_lang::Ident::new("named"),
                    mq_lang::RuntimeValue::Dict(Shared::new(named)),
                ),
            ]
            .into_iter()
            .collect();
            engine
                .define_value("ARGS", mq_lang::RuntimeValue::Dict(Shared::new(args_map)))
                .into_diagnostic()?;
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

        #[cfg(feature = "http-import")]
        {
            engine.set_http_import_enabled(self.input.program.http_import_enabled());
            let domains = self.input.program.http_import_domains();
            if !domains.is_empty() {
                engine.set_http_allowed_domains(domains);
            }
            if self.input.program.no_lockfile {
                engine.set_lockfile_enabled(false);
            }
            if self.input.program.frozen {
                engine.set_lockfile_frozen(true);
            }
            if let Some(path) = &self.input.program.lockfile_path {
                engine.set_lockfile_path(path.clone());
            }
            if self.input.program.clear_cache {
                engine.clear_http_cache_all().map_err(|e| miette!(e.to_string()))?;
            } else if self.input.program.refresh_modules {
                engine.clear_http_cache().map_err(|e| miette!(e.to_string()))?;
            }
        }

        if let Some(timeout) = self.timeout {
            engine.set_timeout(timeout);
        }
        engine.set_capture_stack_trace(self.stack_trace);

        #[cfg(feature = "debugger")]
        {
            use crate::debugger::DebuggerHandler;
            #[cfg(feature = "debug-trace")]
            let handler = {
                let mut handler = DebuggerHandler::new(engine.clone(), self.stop_on_error);
                handler.set_dump_stack(self.dump_stack);
                handler.set_color_output(self.output.color_output && !Self::is_no_color());
                handler
            };
            #[cfg(not(feature = "debug-trace"))]
            let handler = DebuggerHandler::new(engine.clone(), self.stop_on_error);
            engine.set_debugger_handler(Box::new(handler));
            engine.debugger().write().unwrap().activate();
        }

        Ok(engine)
    }

    fn get_query(&self) -> miette::Result<String> {
        if self.bytecode.get().is_some() {
            return Ok(String::new());
        }
        let query = match self.query.as_ref() {
            Some(q) if self.input.from_file => {
                let path = PathBuf::from_str(q).into_diagnostic()?;
                fs::read_to_string(&path)
                    .into_diagnostic()
                    .wrap_err_with(|| ReadFailure(path.clone()))?
            }
            Some(q) => q.clone(),
            None => return Err(miette!("Query is required")),
        };

        let aggregate = self.input.program.aggregate.then_some("nodes");
        Ok(aggregate.map(|agg| format!("{} | {}", agg, query)).unwrap_or(query))
    }

    fn validate_frontmatter_option(&self) -> miette::Result<()> {
        if self.input.program.no_frontmatter
            && matches!(self.explicit_input_format(), Some(fmt) if !matches!(fmt, InputFormat::Markdown | InputFormat::Mdx))
        {
            return Err(miette!("--no-frontmatter only applies to -I markdown or -I mdx"));
        }
        Ok(())
    }

    /// How Markdown and MDX input is read.
    fn parse_options(&self) -> mq_markdown::ParseOptions {
        mq_markdown::ParseOptions::default().with_frontmatter(!self.input.program.no_frontmatter)
    }

    fn validate_csv_options(&self) -> miette::Result<()> {
        if (self.input.program.csv_delimiter.is_some() || self.input.program.no_header)
            && matches!(self.explicit_input_format(), Some(fmt) if !matches!(fmt, InputFormat::Csv | InputFormat::Tsv | InputFormat::Psv))
        {
            return Err(miette!(
                "--csv-delimiter/--no-header only apply to -I csv, -I tsv, or -I psv"
            ));
        }
        Ok(())
    }

    /// Whether the input of `file` is read as MDX, so that what is written for it is escaped as MDX.
    fn is_mdx_input(&self, file: &Option<PathBuf>) -> bool {
        match self.explicit_input_format() {
            Some(format) => matches!(format, InputFormat::Mdx),
            None => file
                .as_ref()
                .is_some_and(|file| matches!(InputFormat::from_path(file), InputFormat::Mdx)),
        }
    }

    fn explicit_input_format(&self) -> Option<InputFormat> {
        self.input
            .program
            .input_format
            .clone()
            .or_else(|| self.format.clone().map(InputFormat::from))
    }

    fn resolved_output_format(&self) -> OutputFormat {
        self.output
            .output_format
            .clone()
            .or_else(|| self.format.clone().map(OutputFormat::from))
            .unwrap_or_else(|| {
                self.output
                    .output_file
                    .as_deref()
                    .map(OutputFormat::from_path)
                    .unwrap_or_default()
            })
    }

    /// Under `--sandbox`, rejects `-o`/`--output` (and so `--append`/`--no-clobber`) unless
    /// `--allow-write` permits the target. Without `--sandbox`, output is unrestricted.
    fn check_output_permitted(&self) -> miette::Result<()> {
        let (Some(_), Some(path)) = (&self.input.program.sandbox, &self.output.output_file) else {
            return Ok(());
        };
        self.input
            .program
            .build_sandboxed_io()
            .check_write(path)
            .map_err(|e| miette!("cannot write output file: {e} (use --allow-write to permit it)"))
    }

    fn clobber_mode(&self) -> ClobberMode {
        if self.output.append {
            ClobberMode::Append
        } else if self.output.no_clobber {
            ClobberMode::NoClobber
        } else {
            ClobberMode::Overwrite
        }
    }

    fn auto_query_prefix(&self, file: &Option<PathBuf>) -> Option<String> {
        let fmt = match self.explicit_input_format() {
            Some(fmt) => fmt,
            None => InputFormat::from_path(file.as_ref()?),
        };
        self.tabular_query_prefix(&fmt)
            .or_else(|| fmt.module_query_prefix().map(str::to_string))
    }

    /// `--csv-delimiter`/`--no-header`-aware prefix for csv/tsv/psv; `None` otherwise.
    fn tabular_query_prefix(&self, fmt: &InputFormat) -> Option<String> {
        let has_header = !self.input.program.no_header;
        match fmt {
            InputFormat::Csv => Some(match self.input.program.csv_delimiter {
                Some(delimiter) => format!(
                    r#"import "csv" | csv::csv_parse_with_delimiter({:?}, {has_header})"#,
                    delimiter.to_string()
                ),
                None => format!(r#"import "csv" | csv::csv_parse({has_header})"#),
            }),
            InputFormat::Tsv => Some(format!(r#"import "csv" | csv::tsv_parse({has_header})"#)),
            InputFormat::Psv => Some(format!(r#"import "csv" | csv::psv_parse({has_header})"#)),
            _ => None,
        }
    }

    /// Sets the `__FILE__` globals; `None` (stdin) resets them only after a file set them,
    /// so a preceding file's values don't leak and user-defined globals survive.
    fn set_file_vars(&self, engine: &mut mq_lang::DefaultEngine, cache: &mut ProgramCache, file: Option<&Path>) {
        if file.is_none() && !cache.file_vars_set {
            return;
        }
        cache.file_vars_set = file.is_some();
        Self::define_file_vars(engine, file);
    }

    fn define_file_vars(engine: &mq_lang::DefaultEngine, file: Option<&Path>) {
        let file = file.unwrap_or_else(|| Path::new(""));
        let path = file.to_string_lossy();
        let name = file.file_name().unwrap_or_default().to_string_lossy();
        let stem = file.file_stem().unwrap_or_default().to_string_lossy();
        engine.define_string_values(&[("__FILE__", &path), ("__FILE_NAME__", &name), ("__FILE_STEM__", &stem)]);
    }

    fn resolve_input(
        &self,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<Vec<mq_lang::RuntimeValue>> {
        let text = content.as_str().unwrap_or("");
        Ok(
            match self.explicit_input_format().unwrap_or_else(|| {
                if let Some(file) = file {
                    InputFormat::from_path(file)
                } else if text.is_empty() && io::stdin().is_terminal() {
                    InputFormat::Null
                } else {
                    InputFormat::Markdown
                }
            }) {
                // Native formats
                InputFormat::Markdown => mq_lang::parse_markdown_input_with(text, self.parse_options())?,
                InputFormat::Mdx => mq_lang::parse_mdx_input_with(text, self.parse_options())?,
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
                | InputFormat::Gron
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
        let runtime_values = if self.output.skip.is_some() || self.output.limit.is_some() {
            let compact = runtime_values.compact();
            let had_empties = compact.len() < runtime_values.len();
            let paginated: Vec<mq_lang::RuntimeValue> = self
                .output
                .paginate(compact)
                .into_iter()
                .map(|v| {
                    if had_empties {
                        let mut v = v;
                        v.set_position(None);
                        v
                    } else {
                        v
                    }
                })
                .collect();
            paginated.into()
        } else {
            runtime_values
        };

        if self.quiet {
            return self.print(runtime_values, self.is_mdx_input(file));
        }

        if let Some(input) = grep_input {
            self.record_exit_status(runtime_values.values());
            let (before, after) = self.output.context_counts();
            let handle = OutputSink::open(
                &self.output.output_file,
                self.output.atomic_output,
                self.output.unbuffered,
                self.clobber_mode(),
                &self.output.output_claimed,
            )?;
            grep::print_grep(runtime_values, &input, file, handle, before, after)
        } else {
            self.print(runtime_values, self.is_mdx_input(file))
        }
    }

    fn execute(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        cache: &mut ProgramCache,
        query: &str,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        self.set_file_vars(engine, cache, file.as_deref());
        let index = self.program_index(engine, cache, query, file)?;
        if cache.separator.is_none()
            && let Some(separator) = &self.output.separator
        {
            cache.separator = Some(engine.compile(separator).map_err(|error| *error)?);
        }
        self.run_program(
            engine,
            &cache.programs[index].1,
            cache.separator.as_ref(),
            file,
            content,
        )
    }

    /// Returns the effective query string combining any auto-prefix with the base query.
    fn effective_query(&self, query: &str, file: &Option<PathBuf>) -> String {
        match self.auto_query_prefix(file) {
            Some(prefix) => format!("{} | {}", prefix, query),
            None => query.to_string(),
        }
    }

    /// Compiles the query for `file`, or loads the `.mqc` program.
    fn prepare_program(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        query: &str,
        file: &Option<PathBuf>,
    ) -> miette::Result<mq_lang::CompiledProgram> {
        match self.bytecode.get() {
            Some(mqc) => self.load_mqc_program(engine, mqc, file),
            None => engine
                .compile(&self.effective_query(query, file))
                .map_err(|error| miette::Report::new(*error)),
        }
    }

    /// Returns the index in `cache` of `file`'s program, preparing it once per [`ProgramKey`].
    fn program_index(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        cache: &mut ProgramCache,
        query: &str,
        file: &Option<PathBuf>,
    ) -> miette::Result<usize> {
        let key = self.program_key(file);
        if let Some(index) = cache.programs.iter().position(|(cached, _)| *cached == key) {
            return Ok(index);
        }
        let program = self.prepare_program(engine, query, file)?;
        self.dump_compiled_bytecode(engine, &program)?;
        cache.programs.push((key, program));
        Ok(cache.programs.len() - 1)
    }

    #[cfg(feature = "debug-trace")]
    fn dump_compiled_bytecode(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        program: &mq_lang::CompiledProgram,
    ) -> miette::Result<()> {
        if let Some(format) = self.dump_bytecode {
            let dump = engine.dump_bytecode(program).map_err(|error| *error)?;
            let color = self.output.color_output && !Self::is_no_color();
            io::stdout()
                .write_all(format.render(&dump, color).as_bytes())
                .into_diagnostic()?;
        }
        Ok(())
    }

    /// Dumps the bytecode of each distinct program without reading input or running it.
    #[cfg(feature = "debug-trace")]
    fn dump_bytecode_only(&self) -> miette::Result<()> {
        let query = self.get_query()?;
        let files = match self.resolved_files()? {
            Some(files) if !files.is_empty() => files.into_iter().map(Some).collect(),
            _ => vec![None],
        };
        let mut engine = self.create_engine()?;
        let mut cache = ProgramCache::default();
        for file in &files {
            self.set_file_vars(&mut engine, &mut cache, file.as_deref());
            self.program_index(&mut engine, &mut cache, &query, file)?;
        }
        Ok(())
    }

    #[cfg(not(feature = "debug-trace"))]
    fn dump_compiled_bytecode(
        &self,
        _engine: &mut mq_lang::DefaultEngine,
        _program: &mq_lang::CompiledProgram,
    ) -> miette::Result<()> {
        Ok(())
    }

    /// What the prepared program for `file` depends on.
    fn program_key(&self, file: &Option<PathBuf>) -> ProgramKey {
        (self.auto_query_prefix(file), self.mqc_input_format(file))
    }

    /// Returns true if one prepared program is valid for every file.
    fn all_files_share_program(&self, files: &[(Option<PathBuf>, ContentData)]) -> bool {
        if files.is_empty() {
            return true;
        }
        let first = self.program_key(&files[0].0);
        files[1..].iter().all(|(f, _)| self.program_key(f) == first)
    }

    fn execute_once(&self) -> miette::Result<()> {
        #[cfg(feature = "debug-trace")]
        if self.dump_bytecode.is_some() {
            return self.dump_bytecode_only();
        }
        if self.input.stream {
            self.process_streaming()
        } else {
            self.process_batch()
        }
    }

    #[cfg(feature = "watch")]
    fn watch_targets(&self) -> miette::Result<Vec<PathBuf>> {
        let mut targets = self.resolved_files()?.unwrap_or_default();

        if targets.is_empty() || targets.iter().any(|t| Self::is_stdin_path(t)) {
            return Err(miette!(
                "--watch requires at least one input file; stdin cannot be watched"
            ));
        }

        if self.input.from_file
            && let Some(query_path) = &self.query
        {
            targets.push(PathBuf::from(query_path));
        }

        Ok(targets)
    }

    #[cfg(feature = "watch")]
    fn canonical_parent_dir(path: &Path) -> PathBuf {
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf())
    }

    /// Errors go to stderr instead of ending the loop, so one bad query doesn't stop watching.
    #[cfg(feature = "watch")]
    fn run_once_watch(&self) {
        if let Err(err) = self.execute_once() {
            if let Some(code) = Self::halt_exit_code(&err) {
                std::process::exit(code);
            }
            self.report_error(&err);
        }
    }

    /// A `[filename]` tag for status lines, so they're never mistaken for query output
    /// interleaved on the same terminal. Colorized when possible, plain text otherwise.
    #[cfg(feature = "watch")]
    fn watch_badge(watch_paths: &[PathBuf]) -> String {
        let names: Vec<String> = watch_paths
            .iter()
            .map(|p| {
                p.file_name()
                    .map_or_else(|| p.display().to_string(), |n| n.to_string_lossy().to_string())
            })
            .collect();
        let label = if names.len() <= 3 {
            names.join(",")
        } else {
            format!("{} files", names.len())
        };
        let tag = format!("[{label}]");

        if io::stderr().is_terminal() && !Self::is_no_color() {
            tag.cyan().bold().to_string()
        } else {
            tag
        }
    }

    #[cfg(feature = "watch")]
    fn watch_divider() -> String {
        let line = "─".repeat(40);
        if io::stderr().is_terminal() && !Self::is_no_color() {
            line.dimmed().to_string()
        } else {
            line
        }
    }

    /// Watches each target's parent dir (not the file itself) and filters by filename,
    /// since editors that save via delete-and-rename would break a watch on the file's inode.
    #[cfg(feature = "watch")]
    fn run_watch(&self) -> miette::Result<()> {
        use notify::Watcher as _;

        self.ensure_watch_supported()?;

        let watch_paths = self.watch_targets()?;
        let targets: rustc_hash::FxHashSet<(PathBuf, OsString)> = watch_paths
            .iter()
            .filter_map(|p| {
                p.file_name()
                    .map(|name| (Self::canonical_parent_dir(p), name.to_os_string()))
            })
            .collect();

        eprintln!(
            "{} Watching for changes. Press Ctrl-C to stop.",
            Self::watch_badge(&watch_paths)
        );
        self.run_once_watch();

        let (tx, rx) = mpsc::channel::<PathBuf>();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                for path in event.paths {
                    let _ = tx.send(path);
                }
            }
        })
        .map_err(|e| miette!("Failed to start file watcher: {e}"))?;

        let mut watched_dirs = BTreeSet::new();
        for path in &watch_paths {
            let dir = Self::canonical_parent_dir(path);
            if watched_dirs.insert(dir.clone()) {
                watcher
                    .watch(&dir, notify::RecursiveMode::NonRecursive)
                    .map_err(|e| miette!("Failed to watch {}: {e}", dir.display()))?;
            }
        }

        loop {
            let Ok(first) = rx.recv() else {
                return Ok(());
            };

            // Debounce: a single save often fires several fs events in quick
            // succession (write + rename, etc.); coalesce them into one re-run.
            let mut changed = vec![first];
            while let Ok(path) = rx.recv_timeout(Duration::from_millis(150)) {
                changed.push(path);
            }

            let relevant = changed.iter().any(|p| {
                p.file_name()
                    .is_some_and(|name| targets.contains(&(Self::canonical_parent_dir(p), name.to_os_string())))
            });

            if relevant {
                eprintln!(
                    "\n{}\n{} Changed, re-running...",
                    Self::watch_divider(),
                    Self::watch_badge(&watch_paths)
                );
                self.run_once_watch();
            }
        }
    }

    fn process_batch(&self) -> Result<(), miette::Error> {
        let query = self.get_query()?;
        let files = self.read_contents()?;

        if self.output.count {
            return self.process_batch_count(&query, &files);
        }

        if self.input.eval_all {
            return self.execute_eval_all(&query, &files);
        }

        // Keep --append sequential: parallel files racing the same read-then-rename
        // append could clobber each other.
        if files.len() > self.parallel_threshold && !self.output.append {
            // `CompiledProgram` uses `Rc`, so each worker keeps its own engine and programs.
            // `init` runs once per rayon split, not per worker; coarse splits keep it to a few per worker.
            let min_len = files.len().div_ceil(rayon::current_num_threads() * 4);
            files.par_iter().with_min_len(min_len).try_for_each_init(
                || Ok::<_, miette::Error>((self.create_engine()?, ProgramCache::default())),
                |worker, (file, content)| {
                    let (engine, cache) = worker
                        .as_mut()
                        .map_err(|error| miette!("Failed to prepare parallel query worker: {error}"))?;
                    self.execute(engine, cache, &query, file, content)
                },
            )?;
        } else {
            let mut engine = self.create_engine()?;
            let mut cache = ProgramCache::default();
            for (file, content) in &files {
                self.execute(&mut engine, &mut cache, &query, file, content)?;
            }
        }

        Ok(())
    }

    /// `__FILE__`-family vars aren't set here: no single file is "current" once combined.
    fn execute_eval_all(&self, query: &str, files: &[(Option<PathBuf>, ContentData)]) -> miette::Result<()> {
        if !self.all_files_share_program(files) {
            return Err(miette!(
                "--eval-all requires all input files to use the same input format"
            ));
        }

        let first_file = files.first().map(|(f, _)| f).unwrap_or(&None);
        let mut engine = self.create_engine()?;

        let mut combined_input = Vec::new();
        for (file, content) in files {
            combined_input.extend(self.resolve_input(file, content)?);
        }

        let is_grep = matches!(self.resolved_output_format(), OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| combined_input.clone());

        #[cfg(feature = "vm-profile")]
        let vm_profile = self.vm_profile.then(mq_lang::vm_profile::VmProfileScope::start);
        let mut cache = ProgramCache::default();
        let index = self.program_index(&mut engine, &mut cache, query, first_file)?;
        let runtime_values = engine
            .eval_compiled(&cache.programs[index].1, combined_input.into_iter())
            .map_err(|error| *error)?;

        #[cfg(feature = "vm-profile")]
        self.emit_vm_profile(vm_profile, &None);

        self.emit_results(runtime_values, grep_input, &None)
    }

    /// Runs `program` on one input, with any file-scoped globals already installed.
    fn run_program(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        program: &mq_lang::CompiledProgram,
        separator: Option<&mq_lang::CompiledProgram>,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        let input = self.resolve_input(file, content)?;
        let is_grep = matches!(self.resolved_output_format(), OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| input.clone());

        #[cfg(feature = "vm-profile")]
        let vm_profile = self.vm_profile.then(mq_lang::vm_profile::VmProfileScope::start);

        let runtime_values = if self.output.update {
            let results = engine
                .eval_compiled(program, input.clone().into_iter())
                .map_err(|e| *e)?;
            self.apply_update(input, results)?
        } else {
            engine.eval_compiled(program, input.into_iter()).map_err(|e| *e)?
        };

        #[cfg(feature = "vm-profile")]
        self.emit_vm_profile(vm_profile, file);

        if self.output.update && self.output.diff {
            return self.emit_diff(&runtime_values, file, content);
        }

        if let Some(separator) = separator {
            let separator = engine
                .eval_compiled(
                    separator,
                    vec![mq_lang::RuntimeValue::String(Shared::new("".to_string()))].into_iter(),
                )
                .map_err(|e| *e)?;
            self.print(separator, false)?;
        }

        self.emit_results(runtime_values, grep_input, file)
    }

    fn count_file(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        cache: &mut ProgramCache,
        query: &str,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<usize> {
        self.set_file_vars(engine, cache, file.as_deref());
        let index = self.program_index(engine, cache, query, file)?;
        #[cfg(feature = "vm-profile")]
        let vm_profile = self.vm_profile.then(mq_lang::vm_profile::VmProfileScope::start);
        let input = self.resolve_input(file, content)?;
        let runtime_values = engine
            .eval_compiled(&cache.programs[index].1, input.into_iter())
            .map_err(|error| *error)?;
        #[cfg(feature = "vm-profile")]
        self.emit_vm_profile(vm_profile, file);
        let matches = self.output.paginate(runtime_values.compact());
        self.record_exit_status(&matches);
        Ok(matches.len())
    }

    #[cfg(feature = "vm-profile")]
    fn emit_vm_profile(&self, scope: Option<mq_lang::vm_profile::VmProfileScope>, file: &Option<PathBuf>) {
        let Some(scope) = scope else {
            return;
        };
        let target = file
            .as_ref()
            .map_or_else(|| "stdin".to_string(), |path| path.display().to_string());
        eprintln!("Tarn VM profile ({target})\n{}", scope.finish());
    }

    fn process_batch_count(&self, query: &str, files: &[(Option<PathBuf>, ContentData)]) -> miette::Result<()> {
        let multiple_files = files.len() > 1;
        let mut total = 0usize;
        let mut engine = self.create_engine()?;
        let mut cache = ProgramCache::default();

        if self.quiet {
            for (file, content) in files {
                self.count_file(&mut engine, &mut cache, query, file, content)?;
            }
            return Ok(());
        }

        let mut handle = OutputSink::open(
            &self.output.output_file,
            self.output.atomic_output,
            self.output.unbuffered,
            self.clobber_mode(),
            &self.output.output_claimed,
        )?;

        for (file, content) in files {
            let count = self.count_file(&mut engine, &mut cache, query, file, content)?;
            total += count;
            if multiple_files {
                let name = file
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(stdin)".to_string());
                Self::write_ignore_pipe(&mut handle, format!("{}: {}\n", name, count).as_bytes())?;
            }
        }

        if multiple_files {
            Self::write_ignore_pipe(&mut handle, format!("total: {}\n", total).as_bytes())?;
        } else {
            Self::write_ignore_pipe(&mut handle, format!("{}\n", total).as_bytes())?;
        }

        handle.finish()?;

        Ok(())
    }

    fn process_streaming(&self) -> miette::Result<()> {
        if self.is_binary_format() {
            return Err(miette!(
                "Streaming mode is not supported for binary input formats (bytes, cbor)"
            ));
        }
        let query = self.get_query()?;
        let mut engine = self.create_engine()?;
        // A stream contains many independent inputs but normally only one effective query and
        // one set of file globals per file. Keep both pieces of state outside the per-line hot
        // path so short queries do not spend their time allocating query strings or recreating
        // `__FILE__` values.
        let mut cache = ProgramCache::default();
        let mut program_index = 0;
        let mut active_file: Option<Option<PathBuf>> = None;

        self.process_lines(|file, line| {
            let file_changed = active_file
                .as_ref()
                .is_none_or(|current_file| current_file.as_ref() != file);
            if file_changed {
                active_file = Some(file.cloned());
                let current_file = active_file
                    .as_ref()
                    .ok_or_else(|| miette!("streaming input did not retain its active file"))?;
                self.set_file_vars(&mut engine, &mut cache, current_file.as_deref());

                program_index = self.program_index(&mut engine, &mut cache, &query, current_file)?;
            }
            let current_file = active_file
                .as_ref()
                .ok_or_else(|| miette!("streaming input did not select an active file"))?;
            self.run_program(
                &mut engine,
                &cache.programs[program_index].1,
                None,
                current_file,
                &line.into(),
            )
        })
    }

    fn process_lines<F>(&self, mut process: F) -> miette::Result<()>
    where
        F: FnMut(Option<&PathBuf>, String) -> miette::Result<()>,
    {
        // If files are specified, process each file line by line
        if let Some(files) = self.resolved_files()? {
            for file in &files {
                if Self::is_stdin_path(file) {
                    for line_result in io::BufReader::new(io::stdin().lock()).lines() {
                        process(None, line_result.into_diagnostic()?)?;
                    }
                    continue;
                }
                let file_handle = fs::File::open(file)
                    .into_diagnostic()
                    .wrap_err_with(|| ReadFailure(file.clone()))?;
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
            self.explicit_input_format(),
            Some(InputFormat::Bytes) | Some(InputFormat::Cbor)
        )
    }

    fn needs_binary_read_for_file(&self, file: &Path) -> bool {
        self.explicit_input_format()
            .map(|fmt| fmt.needs_binary_read())
            .unwrap_or_else(|| {
                let ext = file.extension().unwrap_or_default().to_string_lossy().to_lowercase();
                InputFormat::from_extension(&ext).needs_binary_read()
            })
    }

    /// Expands glob patterns (`*`, `?`, `[`) that don't exist as literal paths, so `mq`
    /// behaves the same on shells that don't expand globs themselves (e.g. Windows `cmd.exe`).
    fn expand_glob_patterns(files: &[PathBuf]) -> miette::Result<Vec<PathBuf>> {
        let mut expanded = Vec::with_capacity(files.len());

        for file in files {
            let pattern = file.to_string_lossy();

            if file.exists() || !pattern.contains(['*', '?', '[']) {
                expanded.push(file.clone());
                continue;
            }

            let matches = glob::glob(&pattern)
                .map_err(|e| miette!("invalid glob pattern `{}`: {}", pattern, e))?
                .filter_map(Result::ok)
                .collect::<Vec<_>>();

            if matches.is_empty() {
                expanded.push(file.clone());
            } else {
                expanded.extend(matches);
            }
        }

        Ok(expanded)
    }

    fn resolved_files(&self) -> miette::Result<Option<Vec<PathBuf>>> {
        let files = self.files.as_deref().map(Self::expand_glob_patterns).transpose()?;
        if files
            .as_ref()
            .is_some_and(|f| f.iter().filter(|p| Self::is_stdin_path(p)).count() > 1)
        {
            return Err(miette!("`-` (stdin) can be specified only once"));
        }
        Ok(files)
    }

    /// Returns `true` if `path` is `-`, which refers to stdin.
    fn is_stdin_path(path: &Path) -> bool {
        path == Path::new("-")
    }

    fn read_stdin(&self) -> miette::Result<ContentData> {
        if self.is_binary_format() {
            let mut buf = Vec::new();
            io::stdin().read_to_end(&mut buf).into_diagnostic()?;
            Ok(buf.into())
        } else {
            let mut input = String::new();
            io::stdin().read_to_string(&mut input).into_diagnostic()?;
            Ok(input.into())
        }
    }

    fn read_files_content(&self, files: &[PathBuf]) -> miette::Result<Vec<(Option<PathBuf>, ContentData)>> {
        files
            .iter()
            .map(|file| {
                if Self::is_stdin_path(file) {
                    return Ok((None, self.read_stdin()?));
                }
                let content = if InputFormat::is_gzip_path(file) {
                    self.read_gzip_file(file)?
                } else {
                    let read = if self.needs_binary_read_for_file(file) {
                        fs::read(file).map(ContentData::from)
                    } else {
                        fs::read_to_string(file).map(ContentData::from)
                    };
                    read.into_diagnostic().wrap_err_with(|| ReadFailure(file.clone()))?
                };
                Ok((Some(file.clone()), content))
            })
            .collect()
    }

    fn read_gzip_file(&self, file: &Path) -> miette::Result<ContentData> {
        let raw = fs::read(file)
            .into_diagnostic()
            .wrap_err_with(|| ReadFailure(file.to_path_buf()))?;
        let mut decompressed = Vec::new();
        flate2::read::GzDecoder::new(&raw[..])
            .read_to_end(&mut decompressed)
            .into_diagnostic()?;

        let fmt = self
            .explicit_input_format()
            .unwrap_or_else(|| InputFormat::from_path(file));
        if fmt.needs_binary_read() {
            Ok(ContentData::Bytes(decompressed))
        } else {
            String::from_utf8(decompressed).map(ContentData::Text).map_err(|e| {
                miette!(
                    "{} does not contain valid UTF-8 after decompression: {}",
                    file.display(),
                    e
                )
            })
        }
    }

    fn read_contents(&self) -> miette::Result<Vec<(Option<PathBuf>, ContentData)>> {
        if matches!(self.explicit_input_format(), Some(InputFormat::Null)) {
            return Ok(vec![(None, ContentData::empty())]);
        }

        self.resolved_files()?
            .map(|files| self.read_files_content(&files))
            .unwrap_or_else(|| {
                if io::stdin().is_terminal() {
                    return Ok(vec![(None, ContentData::empty())]);
                }

                Ok(vec![(None, self.read_stdin()?)])
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
    fn collect_markdown_nodes(value: &mq_lang::RuntimeValue, nodes: &mut Vec<mq_markdown::Node>) {
        match value {
            mq_lang::RuntimeValue::Markdown(node, _) => nodes.push((**node).clone()),
            mq_lang::RuntimeValue::Array(items) => {
                for item in items.iter() {
                    Self::collect_markdown_nodes(item, nodes);
                }
            }
            _ => {}
        }
    }

    /// Returns `true` if the dict is a known expandable typed dict (has `type: :symbol`).
    fn is_typed_dict(map: &DictMap) -> bool {
        let type_key = mq_lang::Ident::new("type");
        matches!(
            map.get(&type_key),
            Some(mq_lang::RuntimeValue::Symbol(s)) if s.resolve_with(|s| matches!(s, "section" | "table"))
        )
    }

    /// Expands a typed dict (one with `type: :symbol`) into Markdown nodes.
    ///
    /// Returns `None` if the dict is not a known expandable type.
    /// To add support for a new type, add a match arm for the type name.
    fn expand_typed_dict(map: &DictMap) -> Option<Vec<mq_markdown::Node>> {
        let type_key = mq_lang::Ident::new("type");
        match map.get(&type_key) {
            Some(mq_lang::RuntimeValue::Symbol(s)) => match s.to_string().as_str() {
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

    /// Clears position information from a Markdown value's node (recursively);
    /// other value kinds are passed through unchanged. Used by `--no-position`.
    fn strip_markdown_position(mut value: mq_lang::RuntimeValue) -> mq_lang::RuntimeValue {
        value.strip_positions();
        value
    }

    fn build_markdown(&self, runtime_values: &[mq_lang::RuntimeValue], mdx: bool) -> mq_markdown::Markdown {
        let mut markdown =
            mq_markdown::Markdown::new(runtime_values.iter().flat_map(Self::runtime_value_to_nodes).collect());
        markdown.set_options(mq_markdown::RenderOptions {
            list_style: self.output.list_style.clone().map(|style| match style {
                ListStyle::Dash => mq_markdown::ListStyle::Dash,
                ListStyle::Plus => mq_markdown::ListStyle::Plus,
                ListStyle::Star => mq_markdown::ListStyle::Star,
            }),
            link_title_style: match self.output.link_title_style.clone() {
                LinkTitleStyle::Double => mq_markdown::TitleSurroundStyle::Double,
                LinkTitleStyle::Single => mq_markdown::TitleSurroundStyle::Single,
                LinkTitleStyle::Paren => mq_markdown::TitleSurroundStyle::Paren,
            },
            link_url_style: match self.output.link_url_style.clone() {
                LinkUrlStyle::None => mq_markdown::UrlSurroundStyle::None,
                LinkUrlStyle::Angle => mq_markdown::UrlSurroundStyle::Angle,
            },
            mdx,
        });
        markdown
    }

    /// Renders `runtime_values` to a byte buffer without writing anywhere.
    /// `emit_diff` always passes `colorize: false`; it needs plain text to diff
    /// against the uncolored original, and colors the diff lines itself.
    fn render(&self, runtime_values: &[mq_lang::RuntimeValue], colorize: bool, mdx: bool) -> miette::Result<Vec<u8>> {
        let mut buf = Vec::new();

        match self.resolved_output_format() {
            OutputFormat::Raw => {
                for value in runtime_values {
                    match value {
                        mq_lang::RuntimeValue::Bytes(b) => buf.extend_from_slice(b),
                        _ => buf.extend_from_slice(value.to_string().as_bytes()),
                    }
                }
            }
            OutputFormat::Json => {
                let theme = colorize.then(mq_markdown::ColorTheme::from_env);
                let json_str = crate::output::json::runtime_values_to_json(
                    runtime_values,
                    theme.as_ref(),
                    self.output.compact,
                    &self.output.json_indent_unit(),
                )?;
                buf.extend_from_slice(json_str.as_bytes());
                if !json_str.ends_with('\n') {
                    buf.push(b'\n');
                }
            }
            OutputFormat::Html => {
                let markdown = self.build_markdown(runtime_values, mdx);
                buf.extend_from_slice(markdown.to_html().as_bytes());
            }
            OutputFormat::Text => {
                let markdown = self.build_markdown(runtime_values, mdx);
                buf.extend_from_slice(markdown.to_text().as_bytes());
            }
            OutputFormat::Markdown if colorize => {
                let markdown = self.build_markdown(runtime_values, mdx);
                let theme = mq_markdown::ColorTheme::from_env();
                buf.extend_from_slice(markdown.to_colored_string_with_theme(&theme).as_bytes());
            }
            OutputFormat::Markdown => {
                let markdown = self.build_markdown(runtime_values, mdx);
                buf.extend_from_slice(markdown.to_string().as_bytes());
            }
            OutputFormat::Table => {
                let theme = colorize.then(mq_markdown::ColorTheme::from_env);
                let table = crate::output::table::runtime_values_to_table(runtime_values, theme.as_ref());
                buf.extend_from_slice(format!("{}\n", table).as_bytes());
            }
            OutputFormat::Grep => {
                let markdown = self.build_markdown(runtime_values, mdx);
                buf.extend_from_slice(markdown.to_string().as_bytes());
            }
            OutputFormat::Gron => {
                let gron_str = crate::output::gron::runtime_values_to_gron(runtime_values);
                buf.extend_from_slice(gron_str.as_bytes());
            }
            OutputFormat::Csv => {
                let theme = colorize.then(mq_markdown::ColorTheme::from_env);
                let csv_str = crate::output::csv::runtime_values_to_csv(runtime_values, theme.as_ref())?;
                buf.extend_from_slice(csv_str.as_bytes());
            }
            OutputFormat::Toml => {
                let toml_str = crate::output::toml::runtime_values_to_toml(runtime_values)?;
                buf.extend_from_slice(toml_str.as_bytes());
            }
            OutputFormat::Toon => {
                let toon_str = crate::output::toon::runtime_values_to_toon(runtime_values)?;
                buf.extend_from_slice(toon_str.as_bytes());
            }
            OutputFormat::Xml => {
                let (indent_char, indent_size) = self.output.xml_indent();
                let xml_str = crate::output::xml::runtime_values_to_xml(runtime_values, indent_char, indent_size)?;
                buf.extend_from_slice(xml_str.as_bytes());
            }
            OutputFormat::Yaml => {
                let yaml_str = crate::output::yaml::runtime_values_to_yaml(runtime_values)?;
                buf.extend_from_slice(yaml_str.as_bytes());
            }
            OutputFormat::Shell => {
                let shell_str = crate::output::shell::runtime_values_to_shell(runtime_values);
                buf.extend_from_slice(shell_str.as_bytes());
            }
            OutputFormat::None => {}
        }

        Ok(buf)
    }

    /// Records the best outcome in `values` for --exit-status.
    fn record_exit_status(&self, values: &[mq_lang::RuntimeValue]) {
        if self.output.exit_status {
            let outcome = values.iter().map(Outcome::of).max().unwrap_or(Outcome::NoResult);
            BEST_OUTCOME.record(outcome);
        }
    }

    fn print(&self, runtime_values: mq_lang::RuntimeValues, mdx: bool) -> miette::Result<()> {
        let stripped_values: Option<Vec<mq_lang::RuntimeValue>> = self.output.no_position.then(|| {
            runtime_values
                .values()
                .iter()
                .cloned()
                .map(Self::strip_markdown_position)
                .collect()
        });
        let runtime_values: &[mq_lang::RuntimeValue] = stripped_values.as_deref().unwrap_or(runtime_values.values());

        self.record_exit_status(runtime_values);

        if self.quiet {
            return Ok(());
        }

        let mut handle = OutputSink::open(
            &self.output.output_file,
            self.output.atomic_output,
            self.output.unbuffered,
            self.clobber_mode(),
            &self.output.output_claimed,
        )?;

        let colorize = self.output.color_output && !Self::is_no_color();
        let buf = self.render(runtime_values, colorize, mdx)?;
        Self::write_ignore_pipe(&mut handle, &buf)?;
        handle.finish()?;

        Ok(())
    }

    /// Renders what `--update` would print and diffs it against the original input.
    fn emit_diff(
        &self,
        runtime_values: &mq_lang::RuntimeValues,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        self.record_exit_status(runtime_values.values());
        let original = content.as_str().unwrap_or("");
        let rendered = self.render(runtime_values.values(), false, self.is_mdx_input(file))?;
        let rendered = String::from_utf8_lossy(&rendered);

        if original != rendered {
            exit_status::mark_diff();
            self.print_unified_diff(original, &rendered, file)?;
        }

        Ok(())
    }

    /// Prints a unified diff of `original` vs `rendered` to stdout (or `-o`), headed
    /// by the file path (or `<stdin>` when there is none). Colorizes `+`/`-`/`@@`
    /// lines when `-C`/`--color-output` is set and `NO_COLOR` isn't.
    fn print_unified_diff(&self, original: &str, rendered: &str, file: &Option<PathBuf>) -> miette::Result<()> {
        if self.quiet {
            return Ok(());
        }

        let mut handle = OutputSink::open(
            &self.output.output_file,
            self.output.atomic_output,
            self.output.unbuffered,
            self.clobber_mode(),
            &self.output.output_claimed,
        )?;

        let label = file
            .as_ref()
            .map(|f| f.display().to_string())
            .unwrap_or_else(|| "<stdin>".to_string());

        let diff_text = similar::TextDiff::from_lines(original, rendered)
            .unified_diff()
            .header(&label, &label)
            .to_string();

        // Raw ANSI, not `colored::Colorize`; it auto-disables on non-tty stdout, but -C should force color.
        let colorize = self.output.color_output && !Self::is_no_color();
        let mut out = String::with_capacity(diff_text.len());
        for line in diff_text.lines() {
            if colorize && line.starts_with('+') && !line.starts_with("+++") {
                out.push_str("\x1b[32m");
                out.push_str(line);
                out.push_str("\x1b[0m");
            } else if colorize && line.starts_with('-') && !line.starts_with("---") {
                out.push_str("\x1b[31m");
                out.push_str(line);
                out.push_str("\x1b[0m");
            } else if colorize && line.starts_with("@@") {
                out.push_str("\x1b[36m");
                out.push_str(line);
                out.push_str("\x1b[0m");
            } else {
                out.push_str(line);
            }
            out.push('\n');
        }

        Self::write_ignore_pipe(&mut handle, out.as_bytes())?;
        handle.finish()?;

        Ok(())
    }
}

#[cfg(test)]
mod tests;
