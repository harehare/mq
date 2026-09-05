use clap::{CommandFactory, Parser, Subcommand};
use colored::Colorize;
use miette::IntoDiagnostic;
use miette::miette;
use mq_lang::DefaultEngine;
use mq_lang::Shared;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fmt::Write as _;
use std::io::BufRead;
use std::io::IsTerminal;
use std::io::{self, BufWriter, Read, Write};
use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;
use std::{fs, path::PathBuf};
use which::which;

// Tracks whether any non-falsy value has been printed during a run.
// Used by --exit-status / -e to decide the process exit code. A plain
// AtomicBool (rather than a thread_local) is required because batch
// processing can fan out across rayon worker threads.
static HAD_TRUTHY_OUTPUT: AtomicBool = AtomicBool::new(false);

// Tracks whether --diff found any changed input, for the exit-code-1 CI check below.
static HAD_DIFF: AtomicBool = AtomicBool::new(false);

use crate::grep;
use mq_help as help;

#[derive(Parser, Debug, Default)]
#[command(name = "mq")]
#[command(author = env!("CARGO_PKG_AUTHORS"))]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(after_help = "# Examples\n\n\
    mq 'query' file.md\n\n\
    Run `mq help examples` for more usage examples, or `mq help <name>` for\n\
    function/selector/module docs.\n")]
#[command(
    about = "mq is a markdown processor that can filter markdown nodes by using jq-like syntax.",
    long_about = None,
    disable_help_subcommand = true
)]
pub struct Cli {
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

    /// Optimization level for AST transformations (none = no changes, basic = constant folding and dead-branch elimination, full = all passes).
    #[arg(short='O', long = "optimize-level", value_enum, default_value_t = OptimizeLevel::None)]
    optimize_level: OptimizeLevel,

    /// Maximum time in seconds allowed for query evaluation before aborting (e.g. 0.5, 5).
    /// No timeout by default.
    #[arg(long, value_name = "SECONDS")]
    timeout: Option<f64>,

    /// Enter the interactive debugger when an uncaught error occurs (mq-dbg only).
    #[cfg(feature = "debugger")]
    #[arg(long = "stop-on-error", default_value_t = false)]
    stop_on_error: bool,

    /// Print the Tarn VM operand stack whenever the debugger stops (mq-dbg `debug-trace` build only).
    #[cfg(feature = "debug-trace")]
    #[arg(long = "dump-stack", default_value_t = false)]
    dump_stack: bool,

    /// Print the Tarn VM bytecode to stderr before execution (mq-dbg `debug-trace` build only).
    #[cfg(feature = "debug-trace")]
    #[arg(long = "dump-bytecode", default_value_t = false)]
    dump_bytecode: bool,
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

#[derive(Clone, Debug, Default, clap::ValueEnum)]
enum OptimizeLevel {
    #[default]
    None,
    Basic,
    Full,
}

impl From<OptimizeLevel> for mq_lang::OptimizationLevel {
    fn from(level: OptimizeLevel) -> Self {
        match level {
            OptimizeLevel::None => mq_lang::OptimizationLevel::None,
            OptimizeLevel::Basic => mq_lang::OptimizationLevel::Basic,
            OptimizeLevel::Full => mq_lang::OptimizationLevel::Full,
        }
    }
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

    /// Allow `import`/`include` to fetch modules over HTTP(S). Disabled by default
    #[cfg(feature = "http-import")]
    #[arg(long = "allow-http-import", default_value_t = false)]
    allow_http_import: bool,

    /// Allow HTTP imports from additional domain(s) beyond the default. Has no effect
    /// unless `--allow-http-import` (or `--allow-all`) is also passed.
    /// Use `github.com/{user}/{repo}` to allow a specific repository (expanded automatically),
    /// or a plain domain like `example.com` to allow any path under that host.
    /// Repeat to allow multiple extra domains.
    #[cfg(feature = "http-import")]
    #[arg(long = "allowed-domain")]
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
    /// `--frozen`; use in CI so a new module's content is only ever trusted during a reviewable local run whose mq.lock diff gets committed, not silently during CI.
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

    /// Allow the `read_file`/`read_file_bytes`/`collection`/`file_exists`/`embed_images`
    /// functions to read from the filesystem. Disabled by default. Pass with no value to
    /// allow reading anywhere, or `--allow-read=PATH` (files or directories; repeat the
    /// flag, or comma-separate, to add more) to restrict reads to just those paths and
    /// their descendants. The `=` is required so a bare path after the flag isn't
    /// swallowed as a query/file positional instead.
    #[arg(short = 'R', long = "allow-read", num_args = 0.., require_equals = true, value_delimiter = ',', value_name = "PATH")]
    allow_read: Option<Vec<PathBuf>>,

    /// Allow the `write_file`/`extract_images` functions to write to the filesystem.
    /// Disabled by default. Pass with no value to allow writing anywhere, or
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
}

#[derive(Clone, Debug, clap::Args, Default)]
struct OutputArgs {
    /// Set output format. When omitted, inferred from the `-o`/`--output` file
    /// extension if given (e.g. `.json` -> json, `.csv` -> csv), else defaults to
    /// markdown.
    #[arg(short = 'F', long, value_enum)]
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

    /// Exit with code 1 if the last output value is false, null, or the output
    /// is empty. Mirrors jq's --exit-status / -e flag.
    #[arg(short = 'e', long = "exit-status", default_value_t = false)]
    exit_status: bool,

    /// Output only the count of matching (non-None) results. Mirrors grep -c.
    /// With multiple files, prints "filename: N" per file and "total: N" at the end.
    #[arg(short = 'c', long = "count", default_value_t = false, conflicts_with_all = ["update", "stream"])]
    count: bool,

    /// Skip the first N matching results before outputting.
    #[arg(long, value_name = "N", conflicts_with = "update")]
    skip: Option<usize>,

    /// Limit output to at most N results.
    #[arg(long, value_name = "N", conflicts_with = "update")]
    limit: Option<usize>,

    /// Omit Markdown node position information from structured output
    /// (json, table, gron, csv, toml, toon, xml, yaml). Reduces output size
    /// when source line/column spans aren't needed.
    #[arg(long, default_value_t = false)]
    no_position: bool,

    /// Print JSON on a single line, without pretty-printing. Only valid with -F json.
    #[arg(long, default_value_t = false)]
    compact: bool,
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
        /// Print Markdown instead of formatted text — e.g. queryable with mq itself:
        /// `mq help section --markdown | mq 'select(.code.lang == "mq")'`
        #[arg(long)]
        markdown: bool,
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
            format!(
                "  {} - Generate a shell completion script and print it to stdout",
                "completion".green()
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

    /// Generate a shell completion script for the given shell and print it to stdout.
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
    /// line — hundreds of lines otherwise means hundreds of flushed syscalls.
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
            // alone for output stability — pass the leading dot (`.table`) for the
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

    /// Sorted, deduped selector and top-level function names for the `mq help` index.
    /// Cheap: uses `top_level_entries`, which never parses standard-module sources.
    fn help_index_names() -> (Vec<String>, Vec<String>) {
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

        (selectors, functions)
    }

    /// Builds the grouped `mq help` index text: selectors, top-level functions, and modules
    /// (each with a one-line summary), for a name-less human-readable lookup.
    fn help_index_text() -> String {
        let (selectors, functions) = Self::help_index_names();
        let mut out = String::new();

        let _ = writeln!(out, "{}", "Selectors:".bold().cyan());
        for s in selectors {
            let _ = writeln!(out, "  {s}");
        }

        let _ = writeln!(out, "\n{}", "Functions:".bold().cyan());
        for f in functions {
            let _ = writeln!(out, "  {f}");
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

    /// Same content as [`Self::help_index_text`], as Markdown — queryable with mq itself.
    fn help_index_markdown() -> String {
        let (selectors, functions) = Self::help_index_names();
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

    /// Text for the `mq help examples` topic — kept out of clap's own `after_help` so
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

        let _ = writeln!(out, "\n{}", "Updating markdown in place (-U):".bold().cyan());
        let _ = writeln!(
            out,
            "  mq -U '.h1 | update(\"New title\")' file.md  # rewrite the h1, leave everything else untouched"
        );

        let _ = writeln!(out, "\n{}", "Filtering with context (-F grep):".bold().cyan());
        let _ = writeln!(
            out,
            "  mq -F grep --context 1 '.h' file.md  # matches with 1 node of context on each side"
        );
        let _ = writeln!(out, "  (also: -B/--before-context, --after-context)");

        let _ = writeln!(out, "\n{}", "Passing arguments to queries (ARGS):".bold().cyan());
        let _ = writeln!(
            out,
            "  When --args or --argv is given, ARGS = {{\"positional\": [...], \"named\": {{...}}}}\n"
        );
        let _ = writeln!(out, "  mq -I null 'name' --args name Alice");
        let _ = writeln!(out, "  mq -I null 'ARGS | .\"named\"' --args name Alice");
        let _ = writeln!(out, "  # => {{\"name\": \"Alice\"}}\n");
        let _ = writeln!(
            out,
            "  mq -I null 'ARGS | .\"positional\"' --argv x y z  # must come after query and files"
        );
        let _ = writeln!(out, "  # => [\"x\", \"y\", \"z\"]\n");
        let _ = writeln!(out, "  mq -I null 'ARGS' file.md --args name Alice --argv x y z");
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

    /// Same content as [`Self::examples_topic_text`], as Markdown — queryable with mq itself
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

        let _ = writeln!(out, "\n## Updating markdown in place (-U)\n");
        let _ = writeln!(
            out,
            "```sh\nmq -U '.h1 | update(\"New title\")' file.md  # rewrite the h1, leave everything else untouched\n```"
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
            "When `--args` or `--argv` is given, `ARGS = {{\"positional\": [...], \"named\": {{...}}}}`\n"
        );
        let _ = writeln!(
            out,
            "```sh\nmq -I null 'name' --args name Alice\nmq -I null 'ARGS | .\"named\"' --args name Alice\n\
            # => {{\"name\": \"Alice\"}}\n\n\
            mq -I null 'ARGS | .\"positional\"' --argv x y z  # must come after query and files\n\
            # => [\"x\", \"y\", \"z\"]\n\n\
            mq -I null 'ARGS' file.md --args name Alice --argv x y z\n\
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
            mq --allow-net=api.example.com 'http(\"get\", \"https://api.example.com/data\")' file.md\n```"
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

    pub fn run(&self) -> miette::Result<()> {
        if self.list {
            return self.list_commands();
        }

        if (self.output.before_context.is_some()
            || self.output.after_context.is_some()
            || self.output.context.is_some())
            && !matches!(self.resolved_output_format(), OutputFormat::Grep)
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

        if !matches!(self.explicit_input_format(), Some(InputFormat::Markdown) | None) && self.output.update {
            return Err(miette!("The output format is not supported for the update option"));
        }

        if self.output.diff && matches!(self.resolved_output_format(), OutputFormat::Grep) {
            return Err(miette!("--diff is not supported with -F grep"));
        }

        if self.output.compact && !matches!(self.resolved_output_format(), OutputFormat::Json) {
            return Err(miette!("--compact is only valid with -F json"));
        }

        if (self.input.csv_delimiter.is_some() || self.input.no_header)
            && matches!(self.explicit_input_format(), Some(fmt) if !matches!(fmt, InputFormat::Csv | InputFormat::Tsv | InputFormat::Psv))
        {
            return Err(miette!(
                "--csv-delimiter/--no-header only apply to -I csv, -I tsv, or -I psv"
            ));
        }

        match &self.commands {
            Some(Commands::Repl { files }) => {
                let engine = self.create_engine()?;
                let input = match files {
                    Some(files) if !files.is_empty() => {
                        let files = Self::expand_glob_patterns(files)?;
                        let contents = self.read_files_content(&files)?;
                        let mut combined = Vec::new();
                        for (file, content) in &contents {
                            combined.extend(self.resolve_input(file, content)?);
                        }
                        combined
                    }
                    _ => vec![mq_lang::RuntimeValue::String(Shared::new("".to_string()))],
                };
                mq_repl::Repl::with_engine(engine, input).run()
            }
            None if self.query.is_none() => {
                let engine = self.create_engine()?;
                mq_repl::Repl::with_engine(engine, vec![mq_lang::RuntimeValue::String(Shared::new("".to_string()))])
                    .run()
            }
            #[cfg(feature = "debugger")]
            Some(Commands::Dap) => mq_dap::start().map_err(|e| miette!(e.to_string())),
            Some(Commands::Completion { shell }) => Self::generate_completion(shell),
            Some(Commands::Help { name, json, markdown }) => Self::run_help(name.as_deref(), *json, *markdown),
            #[cfg(feature = "watch")]
            None if self.input.watch => self.run_watch(),
            None => {
                let result = self.execute_once();

                // --exit-status / -e: exit with code 1 if no truthy value was
                // produced. Mirrors jq's behaviour: false and null are falsy;
                // everything else (including empty string, 0, [], {}) is truthy.
                if self.output.exit_status {
                    let had_truthy = HAD_TRUTHY_OUTPUT.load(Ordering::Relaxed);
                    if !had_truthy {
                        result?;
                        std::process::exit(1);
                    }
                }

                // --diff: exit 1 if anything would change, for CI gating.
                if self.output.diff && HAD_DIFF.load(Ordering::Relaxed) {
                    result?;
                    std::process::exit(1);
                }

                result
            }
        }
    }

    fn create_engine(&self) -> miette::Result<DefaultEngine> {
        let sandboxed_io = mq_lang::SandboxedIo::new(mq_lang::NativeIo::default());
        let sandboxed_io = if self.input.allow_all {
            sandboxed_io.allow_all()
        } else {
            sandboxed_io
                .allow_read(self.input.allow_read.clone())
                .allow_write(self.input.allow_write.clone())
                .allow_net(self.input.allow_net.clone())
                .allow_run(self.input.allow_run.clone())
                .allow_env(self.input.allow_env.clone())
        };
        let mut engine = mq_lang::DefaultEngine::default();
        engine.set_io(Shared::new(sandboxed_io));
        engine.load_builtin_module();
        engine.set_optimization_level(self.optimize_level.clone().into());

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

        if self.input.args.is_some()
            || self.argv.is_some()
            || self.input.argjson.is_some()
            || self.input.slurp_file.is_some()
        {
            let mut named: BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue> = BTreeMap::new();
            if let Some(args) = &self.input.args {
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
                    engine.define_value(&v[0], runtime_value.clone());
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
                    engine.define_value(&v[0], runtime_value.clone());
                    named.insert(mq_lang::Ident::new(&v[0]), runtime_value);
                }
            }

            let positional: Vec<mq_lang::RuntimeValue> = self
                .argv
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|s| mq_lang::RuntimeValue::String(Shared::new(s.clone())))
                .collect();
            let args_map: BTreeMap<mq_lang::Ident, mq_lang::RuntimeValue> = [
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
            engine.define_value("ARGS", mq_lang::RuntimeValue::Dict(Shared::new(args_map)));
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
            engine.set_http_import_enabled(self.input.allow_http_import || self.input.allow_all);
            if let Some(domains) = &self.input.allowed_domains {
                engine.set_http_allowed_domains(domains.clone());
            }
            if self.input.no_lockfile {
                engine.set_lockfile_enabled(false);
            }
            if self.input.frozen {
                engine.set_lockfile_frozen(true);
            }
            if let Some(path) = &self.input.lockfile_path {
                engine.set_lockfile_path(path.clone());
            }
            if self.input.clear_cache {
                engine.clear_http_cache_all().map_err(|e| miette!(e.to_string()))?;
            } else if self.input.refresh_modules {
                engine.clear_http_cache().map_err(|e| miette!(e.to_string()))?;
            }
        }

        if let Some(secs) = self.timeout {
            if secs <= 0.0 {
                return Err(miette!("--timeout must be greater than 0"));
            }
            engine.set_timeout(std::time::Duration::from_secs_f64(secs));
        }

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

    fn explicit_input_format(&self) -> Option<InputFormat> {
        self.input
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
        let has_header = !self.input.no_header;
        match fmt {
            InputFormat::Csv => Some(match self.input.csv_delimiter {
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
            match self.explicit_input_format().unwrap_or_else(|| {
                if let Some(file) = file {
                    InputFormat::from_path(file)
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

        #[cfg(feature = "debug-trace")]
        let program = engine.compile(query).map_err(|error| *error)?;
        #[cfg(feature = "debug-trace")]
        self.dump_compiled_bytecode(engine, &program)?;

        let input = self.resolve_input(file, content)?;
        let is_grep = matches!(self.resolved_output_format(), OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| input.clone());

        let runtime_values = if self.output.update {
            #[cfg(feature = "debug-trace")]
            let results = engine
                .eval_compiled(&program, input.clone().into_iter())
                .map_err(|error| *error)?;
            #[cfg(not(feature = "debug-trace"))]
            let results = engine.eval(query, input.clone().into_iter()).map_err(|error| *error)?;
            self.apply_update(input, results)?
        } else {
            #[cfg(feature = "debug-trace")]
            {
                engine
                    .eval_compiled(&program, input.into_iter())
                    .map_err(|error| *error)?
            }
            #[cfg(not(feature = "debug-trace"))]
            {
                engine.eval(query, input.into_iter()).map_err(|error| *error)?
            }
        };

        if self.output.update && self.output.diff {
            return self.emit_diff(&runtime_values, file, content);
        }

        if let Some(separator) = &self.output.separator {
            let separator = engine
                .eval(
                    separator,
                    vec![mq_lang::RuntimeValue::String(Shared::new("".to_string()))].into_iter(),
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

    #[cfg(feature = "debug-trace")]
    fn dump_compiled_bytecode(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        program: &mq_lang::CompiledProgram,
    ) -> miette::Result<()> {
        if self.dump_bytecode {
            let bytecode = engine.dump_bytecode(program).map_err(|error| *error)?;
            if self.output.color_output && !Self::is_no_color() {
                eprint!("{}", Self::colorize_bytecode(&bytecode));
            } else {
                eprint!("{bytecode}");
            }
        }
        Ok(())
    }

    #[cfg(feature = "debug-trace")]
    fn colorize_bytecode(bytecode: &str) -> String {
        let mut rendered = bytecode
            .lines()
            .map(|line| {
                if line == "Tarn VM bytecode" {
                    return line.bright_cyan().bold().to_string();
                }
                if line.starts_with("Chunk ") {
                    return line.bright_yellow().bold().to_string();
                }
                if matches!(line, "  frame" | "  instructions" | "  constants") {
                    return line.bright_green().bold().to_string();
                }
                if let Some((label, value)) = line.trim_start().split_once(": ") {
                    let indent = &line[..line.len() - line.trim_start().len()];
                    return format!("{indent}{}: {}", label.cyan(), value.dimmed());
                }
                if let Some(instruction) = line.strip_prefix("    ")
                    && instruction
                        .get(..4)
                        .is_some_and(|pc| pc.bytes().all(|byte| byte.is_ascii_digit()))
                {
                    let (pc, rest) = instruction.split_at(4);
                    let (opcode, location) = rest.trim_start().split_once(" @ ").unwrap_or((rest.trim_start(), ""));
                    let location = (!location.is_empty()).then(|| format!(" @ {}", location.dimmed()));
                    return format!(
                        "    {}  {}{}",
                        pc.dimmed(),
                        opcode.bright_blue(),
                        location.unwrap_or_default()
                    );
                }
                line.to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        if bytecode.ends_with('\n') {
            rendered.push('\n');
        }
        rendered
    }

    #[cfg(not(feature = "debug-trace"))]
    fn dump_compiled_bytecode(
        &self,
        _engine: &mut mq_lang::DefaultEngine,
        _program: &mq_lang::CompiledProgram,
    ) -> miette::Result<()> {
        Ok(())
    }

    /// Returns true if all files would produce the same effective query prefix.
    fn all_files_same_prefix(&self, files: &[(Option<PathBuf>, ContentData)]) -> bool {
        if files.is_empty() {
            return true;
        }
        let first = self.auto_query_prefix(&files[0].0);
        files[1..].iter().all(|(f, _)| self.auto_query_prefix(f) == first)
    }

    fn execute_once(&self) -> miette::Result<()> {
        if self.input.stream {
            self.process_streaming()
        } else {
            self.process_batch()
        }
    }

    #[cfg(feature = "watch")]
    fn watch_targets(&self) -> miette::Result<Vec<PathBuf>> {
        let mut targets = self.resolved_files()?.unwrap_or_default();

        if targets.is_empty() {
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
            eprintln!("{:?}", err);
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
                self.dump_compiled_bytecode(&mut engine, &program)?;
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

    /// `__FILE__`-family vars aren't set here: no single file is "current" once combined.
    fn execute_eval_all(&self, query: &str, files: &[(Option<PathBuf>, ContentData)]) -> miette::Result<()> {
        if !self.all_files_same_prefix(files) {
            return Err(miette!(
                "--eval-all requires all input files to use the same input format"
            ));
        }

        let effective_query = self.effective_query(query, files.first().map(|(f, _)| f).unwrap_or(&None));
        let mut engine = self.create_engine()?;

        let mut combined_input = Vec::new();
        for (file, content) in files {
            combined_input.extend(self.resolve_input(file, content)?);
        }

        let is_grep = matches!(self.resolved_output_format(), OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| combined_input.clone());

        #[cfg(feature = "debug-trace")]
        let program = engine.compile(&effective_query).map_err(|error| *error)?;
        #[cfg(feature = "debug-trace")]
        self.dump_compiled_bytecode(&mut engine, &program)?;
        #[cfg(feature = "debug-trace")]
        let runtime_values = engine
            .eval_compiled(&program, combined_input.into_iter())
            .map_err(|error| *error)?;
        #[cfg(not(feature = "debug-trace"))]
        let runtime_values = engine
            .eval(&effective_query, combined_input.into_iter())
            .map_err(|error| *error)?;

        self.emit_results(runtime_values, grep_input, &None)
    }

    fn execute_compiled(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        program: &mq_lang::CompiledProgram,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        if let Some(f) = file {
            self.set_file_vars(engine, f);
        }

        let input = self.resolve_input(file, content)?;
        let is_grep = matches!(self.resolved_output_format(), OutputFormat::Grep);
        let grep_input: Option<Vec<mq_lang::RuntimeValue>> = is_grep.then(|| input.clone());

        let runtime_values = if self.output.update {
            let results = engine
                .eval_compiled(program, input.clone().into_iter())
                .map_err(|e| *e)?;
            self.apply_update(input, results)?
        } else {
            engine.eval_compiled(program, input.into_iter()).map_err(|e| *e)?
        };

        if self.output.update && self.output.diff {
            return self.emit_diff(&runtime_values, file, content);
        }

        self.emit_results(runtime_values, grep_input, file)
    }

    fn count_file(
        &self,
        engine: &mut mq_lang::DefaultEngine,
        query: &str,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<usize> {
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
        #[cfg(feature = "debug-trace")]
        let program = engine.compile(query).map_err(|error| *error)?;
        #[cfg(feature = "debug-trace")]
        self.dump_compiled_bytecode(engine, &program)?;
        let input = self.resolve_input(file, content)?;
        #[cfg(feature = "debug-trace")]
        let runtime_values = engine
            .eval_compiled(&program, input.into_iter())
            .map_err(|error| *error)?;
        #[cfg(not(feature = "debug-trace"))]
        let runtime_values = engine.eval(query, input.into_iter()).map_err(|error| *error)?;
        Ok(self.output.paginate(runtime_values.compact()).len())
    }

    fn process_batch_count(&self, query: &str, files: &[(Option<PathBuf>, ContentData)]) -> miette::Result<()> {
        let multiple_files = files.len() > 1;
        let mut total = 0usize;
        let mut engine = self.create_engine()?;

        let stdout = io::stdout();
        let mut handle: Box<dyn Write> = if let Some(output_file) = &self.output.output_file {
            let file = fs::File::create(output_file).into_diagnostic()?;
            Box::new(BufWriter::new(file))
        } else if self.output.unbuffered {
            Box::new(stdout.lock())
        } else {
            Box::new(BufWriter::new(stdout.lock()))
        };

        for (file, content) in files {
            let count = self.count_file(&mut engine, query, file, content)?;
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

        if !self.output.unbuffered
            && let Err(e) = handle.flush()
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(miette!(e));
        }

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

        self.process_lines(|file, line| self.execute(&mut engine, &query, &file.cloned(), &line.into()))
    }

    fn process_lines<F>(&self, mut process: F) -> miette::Result<()>
    where
        F: FnMut(Option<&PathBuf>, String) -> miette::Result<()>,
    {
        // If files are specified, process each file line by line
        if let Some(files) = self.resolved_files()? {
            for file in &files {
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
        self.files
            .as_ref()
            .map(|files| Self::expand_glob_patterns(files))
            .transpose()
    }

    fn read_files_content(&self, files: &[PathBuf]) -> miette::Result<Vec<(Option<PathBuf>, ContentData)>> {
        files
            .iter()
            .map(|file| {
                let content = if InputFormat::is_gzip_path(file) {
                    self.read_gzip_file(file)?
                } else if self.needs_binary_read_for_file(file) {
                    fs::read(file).map(Into::into).into_diagnostic()?
                } else {
                    fs::read_to_string(file).map(Into::into).into_diagnostic()?
                };
                Ok((Some(file.clone()), content))
            })
            .collect()
    }

    fn read_gzip_file(&self, file: &Path) -> miette::Result<ContentData> {
        let raw = fs::read(file).into_diagnostic()?;
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

    /// Returns true if a RuntimeValue is considered "falsy" for --exit-status.
    /// Mirrors jq's definition (false and null are falsy) but also treats
    /// empty values as falsy: `select()` on markdown input doesn't drop
    /// non-matching nodes, it replaces them with an empty `Markdown` value,
    /// so a plain `None`/`Boolean(false)` check alone would never observe
    /// the common "no match" case.
    fn is_falsy(value: &mq_lang::RuntimeValue) -> bool {
        matches!(value, mq_lang::RuntimeValue::Boolean(false)) || value.is_empty()
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

    /// Renders `runtime_values` to a byte buffer without writing anywhere.
    /// `emit_diff` always passes `colorize: false` — it needs plain text to diff
    /// against the uncolored original, and colors the diff lines itself.
    fn render(&self, runtime_values: &[mq_lang::RuntimeValue], colorize: bool) -> miette::Result<Vec<u8>> {
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
                let json_str =
                    crate::output::json::runtime_values_to_json(runtime_values, theme.as_ref(), self.output.compact)?;
                buf.extend_from_slice(json_str.as_bytes());
            }
            OutputFormat::Html => {
                let markdown = self.build_markdown(runtime_values);
                buf.extend_from_slice(markdown.to_html().as_bytes());
            }
            OutputFormat::Text => {
                let markdown = self.build_markdown(runtime_values);
                buf.extend_from_slice(markdown.to_text().as_bytes());
            }
            OutputFormat::Markdown if colorize => {
                let markdown = self.build_markdown(runtime_values);
                let theme = mq_markdown::ColorTheme::from_env();
                buf.extend_from_slice(markdown.to_colored_string_with_theme(&theme).as_bytes());
            }
            OutputFormat::Markdown => {
                let markdown = self.build_markdown(runtime_values);
                buf.extend_from_slice(markdown.to_string().as_bytes());
            }
            OutputFormat::Table => {
                let theme = colorize.then(mq_markdown::ColorTheme::from_env);
                let table = crate::output::table::runtime_values_to_table(runtime_values, theme.as_ref());
                buf.extend_from_slice(format!("{}\n", table).as_bytes());
            }
            OutputFormat::Grep => {
                let markdown = self.build_markdown(runtime_values);
                buf.extend_from_slice(markdown.to_string().as_bytes());
            }
            OutputFormat::Gron => {
                let gron_str = crate::output::gron::runtime_values_to_gron(runtime_values);
                buf.extend_from_slice(gron_str.as_bytes());
            }
            OutputFormat::Csv => {
                let csv_str = crate::output::csv::runtime_values_to_csv(runtime_values)?;
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
                let xml_str = crate::output::xml::runtime_values_to_xml(runtime_values)?;
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
        let stripped_values: Option<Vec<mq_lang::RuntimeValue>> = self.output.no_position.then(|| {
            runtime_values
                .values()
                .iter()
                .cloned()
                .map(Self::strip_markdown_position)
                .collect()
        });
        let runtime_values: &[mq_lang::RuntimeValue] = stripped_values.as_deref().unwrap_or(runtime_values.values());

        // Track truthy output for --exit-status.
        if self.output.exit_status && runtime_values.iter().any(|v| !Self::is_falsy(v)) {
            HAD_TRUTHY_OUTPUT.store(true, Ordering::Relaxed);
        }

        let colorize = self.output.color_output && !Self::is_no_color();
        let buf = self.render(runtime_values, colorize)?;
        Self::write_ignore_pipe(&mut handle, &buf)?;

        if !self.output.unbuffered
            && let Err(e) = handle.flush()
            && e.kind() != std::io::ErrorKind::BrokenPipe
        {
            return Err(miette!(e));
        }

        Ok(())
    }

    /// Renders what `--update` would print and diffs it against the original input.
    fn emit_diff(
        &self,
        runtime_values: &mq_lang::RuntimeValues,
        file: &Option<PathBuf>,
        content: &ContentData,
    ) -> miette::Result<()> {
        let original = content.as_str().unwrap_or("");
        let rendered = self.render(runtime_values.values(), false)?;
        let rendered = String::from_utf8_lossy(&rendered);

        if original != rendered {
            HAD_DIFF.store(true, Ordering::Relaxed);
            self.print_unified_diff(original, &rendered, file)?;
        }

        Ok(())
    }

    /// Prints a unified diff of `original` vs `rendered` to stdout (or `-o`), headed
    /// by the file path (or `<stdin>` when there is none). Colorizes `+`/`-`/`@@`
    /// lines when `-C`/`--color-output` is set and `NO_COLOR` isn't.
    fn print_unified_diff(&self, original: &str, rendered: &str, file: &Option<PathBuf>) -> miette::Result<()> {
        let stdout = io::stdout();
        let mut handle: Box<dyn Write> = if let Some(output_file) = &self.output.output_file {
            let file = fs::File::create(output_file).into_diagnostic()?;
            Box::new(BufWriter::new(file))
        } else if self.output.unbuffered {
            Box::new(stdout.lock())
        } else {
            Box::new(BufWriter::new(stdout.lock()))
        };

        let label = file
            .as_ref()
            .map(|f| f.display().to_string())
            .unwrap_or_else(|| "<stdin>".to_string());

        let diff_text = similar::TextDiff::from_lines(original, rendered)
            .unified_diff()
            .header(&label, &label)
            .to_string();

        // Raw ANSI, not `colored::Colorize` — it auto-disables on non-tty stdout, but -C should force color.
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
    fn test_allow_read_flag_gates_read_file() {
        let (_, temp_file_path) = create_file("test_allow_read.md", "hello");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let query = format!("read_file(\"{}\")", temp_file_path.to_string_lossy());

        let blocked_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.clone()),
            files: None,
            ..Cli::default()
        };
        assert!(
            blocked_cli.run().is_err(),
            "read_file should be blocked without --allow-read"
        );

        let allowed_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allow_read: Some(vec![]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query),
            files: None,
            ..Cli::default()
        };
        assert!(allowed_cli.run().is_ok(), "read_file should succeed with --allow-read");
    }

    #[test]
    fn test_allow_run_flag_gates_system() {
        let query = "system(\"echo\", [\"hello\"])";

        let blocked_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };
        assert!(
            blocked_cli.run().is_err(),
            "system should be blocked without --allow-run"
        );

        let allowed_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allow_run: Some(vec![]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };
        assert!(allowed_cli.run().is_ok(), "system should succeed with --allow-run");

        let restricted_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allow_run: Some(vec!["ls".to_string()]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };
        assert!(
            restricted_cli.run().is_err(),
            "system should be blocked when --allow-run only names other commands"
        );
    }

    #[test]
    fn test_allow_env_flag_gates_var_interpolation() {
        // SAFETY: no other threads read/write this env var concurrently in this test.
        unsafe { std::env::set_var("MQ_ALLOW_ENV_TEST_VAR", "test_value") };
        defer! {
            unsafe { std::env::remove_var("MQ_ALLOW_ENV_TEST_VAR") };
        }

        let query = "$MQ_ALLOW_ENV_TEST_VAR";

        let blocked_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };
        assert!(blocked_cli.run().is_err(), "$VAR should be blocked without --allow-env");

        let allowed_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allow_env: Some(vec![]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };
        assert!(allowed_cli.run().is_ok(), "$VAR should succeed with --allow-env");

        let restricted_cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allow_env: Some(vec!["MQ_OTHER_VAR".to_string()]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };
        assert!(
            restricted_cli.run().is_err(),
            "$VAR should be blocked when --allow-env only names other variables"
        );
    }

    #[test]
    fn test_allow_all_flag_grants_every_capability() {
        // SAFETY: no other threads read/write this env var concurrently in this test.
        unsafe { std::env::set_var("MQ_ALLOW_ALL_TEST_VAR", "test_value") };
        defer! {
            unsafe { std::env::remove_var("MQ_ALLOW_ALL_TEST_VAR") };
        }

        let base_cli = |query: &str, allow_all: bool| Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allow_all,
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(query.to_string()),
            files: None,
            ..Cli::default()
        };

        assert!(
            base_cli("$MQ_ALLOW_ALL_TEST_VAR", false).run().is_err(),
            "$VAR should be blocked without --allow-all"
        );
        assert!(
            base_cli("$MQ_ALLOW_ALL_TEST_VAR", true).run().is_ok(),
            "$VAR should succeed with --allow-all"
        );

        let system_query = "system(\"echo\", [\"hi\"])";
        assert!(
            base_cli(system_query, false).run().is_err(),
            "system should be blocked without --allow-all"
        );
        assert!(
            base_cli(system_query, true).run().is_ok(),
            "system should succeed with --allow-all"
        );
    }

    #[rstest]
    #[case(&["mq", "--allow-all", "--allow-read=/tmp", "self"])]
    #[case(&["mq", "--allow-all", "--allow-write=/tmp", "self"])]
    #[case(&["mq", "--allow-all", "--allow-net=example.com", "self"])]
    #[case(&["mq", "--allow-all", "--allow-run", "self"])]
    #[case(&["mq", "--allow-all", "--allow-env", "self"])]
    #[case(&["mq", "--allow-all", "--allow-http-import", "self"])]
    fn test_allow_all_conflicts_with_individual_allow_flags(#[case] args: &[&str]) {
        assert!(
            Cli::try_parse_from(args).is_err(),
            "--allow-all should conflict with individual --allow-* flags"
        );
    }

    #[cfg(feature = "http-import")]
    #[test]
    fn test_http_import_disabled_by_default() {
        // No network call is made: resolution fails on the enabled-check before any
        // request, so this stays fast and deterministic regardless of connectivity.
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(r#"import "github.com/harehare/lisp""#.to_string()),
            files: None,
            ..Cli::default()
        };

        let err = cli.run().unwrap_err();
        assert!(
            err.to_string().contains("HTTP module imports are disabled"),
            "error was: {err}"
        );
    }

    #[cfg(feature = "http-import")]
    #[test]
    fn test_allowed_domain_alone_does_not_enable_http_import() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                allowed_domains: Some(vec!["example.com".to_string()]),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some(r#"import "https://example.com/mod.mq""#.to_string()),
            files: None,
            ..Cli::default()
        };

        let err = cli.run().unwrap_err();
        assert!(
            err.to_string().contains("HTTP module imports are disabled"),
            "error was: {err}"
        );
    }

    #[cfg(feature = "http-import")]
    #[test]
    fn test_no_lockfile_conflicts_with_frozen() {
        assert!(
            Cli::try_parse_from(["mq", "--no-lockfile", "--frozen", "self"]).is_err(),
            "--no-lockfile should conflict with --frozen"
        );
    }

    #[rstest]
    #[case(0.0)]
    #[case(-1.0)]
    fn test_timeout_rejects_non_positive(#[case] secs: f64) {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("1".to_string()),
            files: None,
            timeout: Some(secs),
            ..Cli::default()
        };

        assert!(cli.run().is_err());
    }

    #[test]
    fn test_timeout_aborts_infinite_loop() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("while(true): 1;".to_string()),
            files: None,
            timeout: Some(0.001),
            ..Cli::default()
        };

        assert!(cli.run().is_err());
    }

    #[test]
    fn test_timeout_allows_normal_query() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("1 + 1".to_string()),
            files: None,
            timeout: Some(5.0),
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
            OutputFormat::Gron,
            OutputFormat::Csv,
            OutputFormat::Xml,
            OutputFormat::Yaml,
            OutputFormat::Toon,
            OutputFormat::Shell,
        ] {
            let cli = Cli {
                input: InputArgs::default(),
                output: OutputArgs {
                    output_format: Some(format.clone()),
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
                output_format: Some(OutputFormat::Json),
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
        assert!(
            nodes[0].get("position").is_some(),
            "position should be present by default"
        );
    }

    #[test]
    fn test_output_format_json_markdown_input_no_position() {
        let (_, temp_file_path) = create_file("test_json_md_input_no_position.md", "# Test");
        let (_, output_file) = create_file("test_json_md_output_no_position.json", "");
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
                output_format: Some(OutputFormat::Json),
                output_file: Some(output_file.clone()),
                no_position: true,
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
        let nodes = parsed.as_array().unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0]["type"], "Heading");
        assert!(
            nodes[0].get("position").is_none(),
            "--no-position should omit position from the top-level node"
        );
        let children = nodes[0]["values"].as_array().expect("heading should have values");
        assert!(
            children.iter().all(|c| c.get("position").is_none()),
            "--no-position should omit position from child nodes too"
        );
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
                output_format: Some(OutputFormat::Json),
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
                output_format: Some(OutputFormat::Json),
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
    fn test_output_format_gron() {
        let (_, temp_file_path) = create_file("test_gron_output.md", "# Title\n");
        let (_, output_file) = create_file("test_gron_output.gron", "");
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
                output_format: Some(OutputFormat::Gron),
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
        assert!(output_content.contains("json[0].type = \"Heading\";\n"));
        assert!(output_content.contains("json[0].depth = 1;\n"));
    }

    #[test]
    fn test_input_format_gron_reconstructs_data() {
        let (_, temp_file_path) = create_file(
            "test_gron_input.gron",
            "json = {};\njson.a = {};\njson.a.b = \"deep\";\njson.arr = [];\njson.arr[0] = 1;\njson.arr[1] = 2;\n",
        );
        let (_, output_file) = create_file("test_gron_input_output.json", "");
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
                output_format: Some(OutputFormat::Json),
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
        assert_eq!(parsed["a"]["b"], "deep");
        assert_eq!(parsed["arr"][0], 1.0);
        assert_eq!(parsed["arr"][1], 2.0);
    }

    #[test]
    fn test_output_format_gron_then_input_format_gron_round_trip() {
        let (_, md_file) = create_file("test_gron_roundtrip.md", "# Title\n\nSome *text* here.\n");
        let (_, gron_file) = create_file("test_gron_roundtrip.gron", "");
        let (_, json_file) = create_file("test_gron_roundtrip.json", "");
        let md_file_clone = md_file.clone();
        let gron_file_clone = gron_file.clone();
        let json_file_clone = json_file.clone();

        defer! {
            if md_file_clone.exists() {
                std::fs::remove_file(&md_file_clone).ok();
            }
            if gron_file_clone.exists() {
                std::fs::remove_file(&gron_file_clone).ok();
            }
            if json_file_clone.exists() {
                std::fs::remove_file(&json_file_clone).ok();
            }
        }

        let to_gron = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: Some(OutputFormat::Gron),
                output_file: Some(gron_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![md_file.clone()]),
            ..Cli::default()
        };
        assert!(to_gron.run().is_ok());

        let from_gron = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: Some(OutputFormat::Json),
                output_file: Some(json_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![gron_file]),
            ..Cli::default()
        };
        assert!(from_gron.run().is_ok());

        let direct_json = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: Some(OutputFormat::Json),
                output_file: Some(json_file.with_extension("direct.json")),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![md_file]),
            ..Cli::default()
        };
        assert!(direct_json.run().is_ok());

        let roundtrip_content = fs::read_to_string(&json_file).expect("Failed to read roundtrip output");
        let direct_content =
            fs::read_to_string(json_file.with_extension("direct.json")).expect("Failed to read direct output");
        fs::remove_file(json_file.with_extension("direct.json")).ok();

        let roundtrip_value: serde_json::Value =
            serde_json::from_str(&roundtrip_content).expect("Roundtrip output should be valid JSON");
        let direct_value: serde_json::Value =
            serde_json::from_str(&direct_content).expect("Direct output should be valid JSON");
        assert_eq!(roundtrip_value, direct_value);
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::None),
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
                output_format: Some(OutputFormat::Table),
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
                output_format: Some(OutputFormat::Table),
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
                output_format: Some(OutputFormat::Table),
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
                output_format: Some(OutputFormat::Table),
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
                output_format: Some(OutputFormat::Table),
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
                output_format: Some(OutputFormat::Text),
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
    fn test_eval_all_combines_nodes_across_files() {
        let (_, temp_file1) = create_file("test_eval_all1.md", "# File One\n\n## Sub A");
        let (_, temp_file2) = create_file("test_eval_all2.md", "# File Two\n\n## Sub B");
        let (_, output_file) = create_file("test_eval_all_output.md", "");
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

        // -A alone aggregates per file (2 separate counts); --eval-all -A must combine
        // both files into a single collection before "nodes" runs, giving one total.
        let cli = Cli {
            input: InputArgs {
                aggregate: true,
                eval_all: true,
                ..Default::default()
            },
            output: OutputArgs {
                output_file: Some(output_file.clone()),
                output_format: Some(OutputFormat::Text),
                ..Default::default()
            },
            commands: None,
            query: Some(".h | len".to_string()),
            files: Some(vec![temp_file1, temp_file2]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(
            output_content.trim(),
            "4",
            "should count headings across both files as one"
        );
    }

    #[test]
    fn test_eval_all_rejects_mixed_input_formats() {
        let (_, temp_md) = create_file("test_eval_all_mixed.md", "# Test");
        let (_, temp_json) = create_file("test_eval_all_mixed.json", r#"{"a":1}"#);
        let temp_md_clone = temp_md.clone();
        let temp_json_clone = temp_json.clone();

        defer! {
            if temp_md_clone.exists() {
                std::fs::remove_file(&temp_md_clone).ok();
            }
            if temp_json_clone.exists() {
                std::fs::remove_file(&temp_json_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                eval_all: true,
                ..Default::default()
            },
            output: OutputArgs::default(),
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_md, temp_json]),
            ..Cli::default()
        };

        let err = cli.run().unwrap_err();
        assert!(err.to_string().contains("--eval-all requires"));
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
    #[case("gron", InputFormat::Gron)]
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
    #[case("unknown", InputFormat::Markdown)] // default fallback
    fn test_from_extension(#[case] ext: &str, #[case] expected: InputFormat) {
        assert_eq!(InputFormat::from_extension(ext), expected);
    }

    #[rstest]
    #[case("file.csv", InputFormat::Csv)]
    #[case("file.csv.gz", InputFormat::Csv)]
    #[case("file.json.gz", InputFormat::Json)]
    #[case("file.tar.gz", InputFormat::Markdown)] // "tar" isn't a recognized inner extension
    #[case("file.gz", InputFormat::Markdown)] // no inner extension at all
    #[case("file.md", InputFormat::Markdown)]
    fn test_input_format_from_path(#[case] filename: &str, #[case] expected: InputFormat) {
        assert_eq!(InputFormat::from_path(&PathBuf::from(filename)), expected);
    }

    #[rstest]
    #[case("html", OutputFormat::Html)]
    #[case("htm", OutputFormat::Html)]
    #[case("txt", OutputFormat::Raw)]
    #[case("log", OutputFormat::Raw)]
    #[case("json", OutputFormat::Json)]
    #[case("csv", OutputFormat::Csv)]
    #[case("toml", OutputFormat::Toml)]
    #[case("toon", OutputFormat::Toon)]
    #[case("xml", OutputFormat::Xml)]
    #[case("yaml", OutputFormat::Yaml)]
    #[case("yml", OutputFormat::Yaml)]
    #[case("gron", OutputFormat::Gron)]
    #[case("unknown", OutputFormat::Markdown)] // default fallback
    #[case("md", OutputFormat::Markdown)]
    fn test_output_format_from_extension(#[case] ext: &str, #[case] expected: OutputFormat) {
        assert_eq!(OutputFormat::from_extension(ext), expected);
    }

    #[rstest]
    #[case("file.json", OutputFormat::Json)]
    #[case("file.csv", OutputFormat::Csv)]
    #[case("file.yaml", OutputFormat::Yaml)]
    #[case("file.md", OutputFormat::Markdown)]
    #[case("file", OutputFormat::Markdown)] // no extension at all
    fn test_output_format_from_path(#[case] filename: &str, #[case] expected: OutputFormat) {
        assert_eq!(OutputFormat::from_path(&PathBuf::from(filename)), expected);
    }

    #[rstest]
    #[case("file.json", Some(r#"import "json" | json::json_parse()"#))]
    #[case("file.gron", Some(r#"import "gron" | gron::gron_parse()"#))]
    #[case("file.yaml", Some(r#"import "yaml" | yaml::yaml_parse()"#))]
    #[case("file.yml", Some(r#"import "yaml" | yaml::yaml_parse()"#))]
    #[case("file.toml", Some(r#"import "toml" | toml::toml_parse()"#))]
    #[case("file.xml", Some(r#"import "xml" | xml::xml_parse()"#))]
    #[case("file.toon", Some(r#"import "toon" | toon::toon_parse()"#))]
    #[case("file.csv", Some(r#"import "csv" | csv::csv_parse(true)"#))]
    #[case("file.tsv", Some(r#"import "csv" | csv::tsv_parse(true)"#))]
    #[case("file.psv", Some(r#"import "csv" | csv::psv_parse(true)"#))]
    #[case("file.cbor", Some(r#"import "cbor" | cbor::cbor_parse()"#))]
    #[case("file.csv.gz", Some(r#"import "csv" | csv::csv_parse(true)"#))]
    #[case("file.json.gz", Some(r#"import "json" | json::json_parse()"#))]
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

    #[rstest]
    #[case(InputFormat::Csv, None, false, r#"import "csv" | csv::csv_parse(true)"#)]
    #[case(InputFormat::Csv, None, true, r#"import "csv" | csv::csv_parse(false)"#)]
    #[case(
        InputFormat::Csv,
        Some(';'),
        false,
        r#"import "csv" | csv::csv_parse_with_delimiter(";", true)"#
    )]
    #[case(InputFormat::Tsv, None, true, r#"import "csv" | csv::tsv_parse(false)"#)]
    #[case(InputFormat::Psv, None, true, r#"import "csv" | csv::psv_parse(false)"#)]
    fn test_tabular_query_prefix(
        #[case] fmt: InputFormat,
        #[case] csv_delimiter: Option<char>,
        #[case] no_header: bool,
        #[case] expected: &str,
    ) {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(fmt.clone()),
                csv_delimiter,
                no_header,
                ..Default::default()
            },
            ..Cli::default()
        };
        assert_eq!(cli.tabular_query_prefix(&fmt).as_deref(), Some(expected));
    }

    #[test]
    fn test_tabular_query_prefix_none_for_non_tabular_format() {
        let cli = Cli {
            input: InputArgs::default(),
            ..Cli::default()
        };
        assert_eq!(cli.tabular_query_prefix(&InputFormat::Json), None);
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::Raw),
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

    #[test]
    fn test_output_format_auto_detected_from_output_file_extension() {
        let (_, temp_file_path) = create_file("out_fmt_auto_input.md", "# Test");
        let (_, output_file) = create_file("out_fmt_auto_output.json", "");
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

        // No -F given; the `.json` extension on -o should drive the output format.
        let cli = Cli {
            input: InputArgs::default(),
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
        let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
        assert!(parsed.is_array(), "extension-inferred JSON output should be an array");
        assert_eq!(parsed[0]["type"], "Heading");
    }

    #[test]
    fn test_output_format_explicit_flag_overrides_extension() {
        let (_, temp_file_path) = create_file("out_fmt_override_input.md", "# Test");
        let (_, output_file) = create_file("out_fmt_override_output.json", "");
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

        // -F yaml should win over the `.json` extension on -o.
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: Some(OutputFormat::Yaml),
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
            serde_json::from_str::<serde_json::Value>(&output_content).is_err(),
            "output should be YAML, not JSON"
        );
        assert!(output_content.contains("type: Heading") || output_content.contains("type: heading"));
    }

    #[test]
    fn test_format_flag_sets_both_input_and_output() {
        // ".dat" isn't a recognized extension on either side, so without -T this
        // would parse as markdown and render as markdown.
        let (_, temp_file_path) = create_file("format_flag_input.dat", r#"{"key": "value"}"#);
        let (_, output_file) = create_file("format_flag_output.dat", "");
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
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            format: Some(IoFormat::Json),
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
        assert_eq!(parsed["key"], "value");
    }

    #[test]
    fn test_format_flag_overridden_by_explicit_input_and_output_format() {
        let (_, temp_file_path) = create_file("format_flag_override_input.dat", "# Test");
        let (_, output_file) = create_file("format_flag_override_output.dat", "");
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
                input_format: Some(InputFormat::Markdown),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Table),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            format: Some(IoFormat::Json),
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        assert!(
            serde_json::from_str::<serde_json::Value>(&output_content).is_err(),
            "explicit -F table should win over -T json"
        );
    }

    #[test]
    fn test_csv_delimiter_and_no_header_flags() {
        let (_, temp_file_path) = create_file("custom_delim_test.csv", "Alice;30\nBob;25\n");
        let temp_file_path_clone = temp_file_path.clone();
        let (_, output_file) = create_file("custom_delim_output.json", "");
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
                input_format: Some(InputFormat::Csv),
                csv_delimiter: Some(';'),
                no_header: true,
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Json),
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
        let json: serde_json::Value = serde_json::from_str(&content).expect("output should be valid JSON");
        // Headerless: every row is an array of values, not a dict keyed by a first-row header.
        assert_eq!(json, serde_json::json!([["Alice", "30"], ["Bob", "25"]]));
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
    #[case(InputFormat::Gron, Some(r#"import "gron" | gron::gron_parse()"#))]
    #[case(InputFormat::Json, Some(r#"import "json" | json::json_parse()"#))]
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::Raw),
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

    #[rstest]
    #[case::string_value(
        "string_value",
        vec!["name".to_string(), r#""Alice""#.to_string()],
        "name",
        "Alice"
    )]
    #[case::number_value(
        "number_value",
        vec!["count".to_string(), "42".to_string()],
        "count",
        "42"
    )]
    #[case::bool_value(
        "bool_value",
        vec!["flag".to_string(), "true".to_string()],
        "flag",
        "true"
    )]
    #[case::null_value(
        "null_value",
        vec!["n".to_string(), "null".to_string()],
        "n",
        ""
    )]
    #[case::array_value(
        "array_value",
        vec!["list".to_string(), "[1,2,3]".to_string()],
        "list",
        r#"[1, 2, 3]"#
    )]
    #[case::object_value(
        "object_value",
        vec!["obj".to_string(), r#"{"a":1}"#.to_string()],
        "obj",
        r#"{"a": 1}"#
    )]
    #[case::args_named_access(
        "args_named_access",
        vec!["count".to_string(), "42".to_string()],
        r#"ARGS | ."named""#,
        r#"{"count": 42}"#
    )]
    fn test_argjson(#[case] suffix: &str, #[case] argjson: Vec<String>, #[case] query: &str, #[case] expected: &str) {
        let (_, output_file) = create_file(&format!("test_argjson_{suffix}.md"), "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                argjson: Some(argjson),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Raw),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(query.to_string()),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(result.trim(), expected, "query: {}", query);
    }

    #[test]
    fn test_argjson_invalid_json_errors() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                argjson: Some(vec!["name".to_string(), "not valid json".to_string()]),
                ..Default::default()
            },
            query: Some("name".to_string()),
            ..Cli::default()
        };

        assert!(
            cli.run().is_err(),
            "--argjson with malformed JSON should return an error"
        );
    }

    #[test]
    fn test_argjson_combined_with_args_and_argv() {
        let (_, output_file) = create_file("test_argjson_combined.md", "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                args: Some(vec!["name".to_string(), "Alice".to_string()]),
                argjson: Some(vec!["count".to_string(), "42".to_string()]),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Raw),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(r#"ARGS | ."named""#.to_string()),
            argv: Some(vec!["x".to_string()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        // `named` is backed by a `BTreeMap<Ident, _>`, whose key order depends on the
        // global string interner's symbol assignment order rather than the key text,
        // so compare parsed JSON values instead of the raw serialized string.
        let actual: serde_json::Value = serde_json::from_str(result.trim()).expect("output should be valid JSON");
        let expected: serde_json::Value = serde_json::from_str(r#"{"count": 42, "name": "Alice"}"#).unwrap();
        assert_eq!(actual, expected);
    }

    #[rstest]
    #[case::single_object("single_object", r#"{"a": 1}"#, "data", r#"[{"a": 1}]"#)]
    #[case::single_array("single_array", "[1, 2, 3]", "data", r#"[[1, 2, 3]]"#)]
    #[case::multiple_concatenated_values("multiple_values", "1 2 3", "data", "[1, 2, 3]")]
    fn test_slurpfile(#[case] suffix: &str, #[case] file_content: &str, #[case] query: &str, #[case] expected: &str) {
        let (_, data_file) = create_file(&format!("test_slurpfile_{suffix}.json"), file_content);
        let (_, output_file) = create_file(&format!("test_slurpfile_{suffix}_out.md"), "");
        let data_file_clone = data_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            std::fs::remove_file(&data_file_clone).ok();
            std::fs::remove_file(&output_file_clone).ok();
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                slurp_file: Some(vec!["data".to_string(), data_file.to_string_lossy().to_string()]),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Raw),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(query.to_string()),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(result.trim(), expected, "query: {}", query);
    }

    #[test]
    fn test_slurpfile_invalid_json_errors() {
        let (_, data_file) = create_file("test_slurpfile_invalid.json", "not valid json");
        let data_file_clone = data_file.clone();

        defer! {
            std::fs::remove_file(&data_file_clone).ok();
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                slurp_file: Some(vec!["data".to_string(), data_file.to_string_lossy().to_string()]),
                ..Default::default()
            },
            query: Some("data".to_string()),
            ..Cli::default()
        };

        assert!(
            cli.run().is_err(),
            "--slurpfile with malformed JSON should return an error"
        );
    }

    #[test]
    fn test_slurpfile_missing_file_errors() {
        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                slurp_file: Some(vec!["data".to_string(), "/nonexistent/path/to/file.json".to_string()]),
                ..Default::default()
            },
            query: Some("data".to_string()),
            ..Cli::default()
        };

        assert!(
            cli.run().is_err(),
            "--slurpfile with a missing file should return an error"
        );
    }

    #[test]
    fn test_slurpfile_combined_with_args_and_argjson() {
        let (_, data_file) = create_file("test_slurpfile_combined.json", r#"{"c": 3}"#);
        let (_, output_file) = create_file("test_slurpfile_combined_out.md", "");
        let data_file_clone = data_file.clone();
        let output_file_clone = output_file.clone();

        defer! {
            std::fs::remove_file(&data_file_clone).ok();
            std::fs::remove_file(&output_file_clone).ok();
        }

        let cli = Cli {
            input: InputArgs {
                input_format: Some(InputFormat::Null),
                args: Some(vec!["name".to_string(), "Alice".to_string()]),
                argjson: Some(vec!["count".to_string(), "42".to_string()]),
                slurp_file: Some(vec!["data".to_string(), data_file.to_string_lossy().to_string()]),
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Raw),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some("data".to_string()),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(result.trim(), r#"[{"c": 3}]"#, "query: data");
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
                output_format: Some(OutputFormat::Text),
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
                    output_format: Some(OutputFormat::Text),
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
    fn test_files_glob_pattern_expansion() {
        let (temp_dir, file1) = create_file("mq_glob_expand_test_a.md", "# File One");
        let (_, file2) = create_file("mq_glob_expand_test_b.md", "# File Two");
        let output_file = temp_dir.join("mq_glob_expand_result.md");
        let file1_clone = file1.clone();
        let file2_clone = file2.clone();
        let output_clone = output_file.clone();

        defer! {
            if file1_clone.exists() { std::fs::remove_file(&file1_clone).ok(); }
            if file2_clone.exists() { std::fs::remove_file(&file2_clone).ok(); }
            if output_clone.exists() { std::fs::remove_file(&output_clone).ok(); }
        }

        let pattern = temp_dir.join("mq_glob_expand_test_*.md");
        let cli = Cli {
            input: InputArgs {
                aggregate: true,
                eval_all: true,
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Text),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(".h | len".to_string()),
            files: Some(vec![pattern]),
            argv: None,
            ..Cli::default()
        };

        assert!(cli.run().is_ok(), "glob pattern should expand and run successfully");
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(result.trim(), "2", "both files matched by the glob should be counted");
    }

    #[test]
    fn test_files_multiple_glob_patterns_expansion() {
        let (temp_dir, file1) = create_file("mq_glob_multi_test_a.md", "# File One");
        let (_, file2) = create_file("mq_glob_multi_other_b.md", "# File Two");
        let output_file = temp_dir.join("mq_glob_multi_result.md");
        let file1_clone = file1.clone();
        let file2_clone = file2.clone();
        let output_clone = output_file.clone();

        defer! {
            if file1_clone.exists() { std::fs::remove_file(&file1_clone).ok(); }
            if file2_clone.exists() { std::fs::remove_file(&file2_clone).ok(); }
            if output_clone.exists() { std::fs::remove_file(&output_clone).ok(); }
        }

        let pattern1 = temp_dir.join("mq_glob_multi_test_*.md");
        let pattern2 = temp_dir.join("mq_glob_multi_other_*.md");
        let cli = Cli {
            input: InputArgs {
                aggregate: true,
                eval_all: true,
                ..Default::default()
            },
            output: OutputArgs {
                output_format: Some(OutputFormat::Text),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            query: Some(".h | len".to_string()),
            files: Some(vec![pattern1, pattern2]),
            argv: None,
            ..Cli::default()
        };

        assert!(
            cli.run().is_ok(),
            "multiple glob patterns should expand and run successfully"
        );
        let result = fs::read_to_string(&output_file).expect("Failed to read output");
        assert_eq!(
            result.trim(),
            "2",
            "files matched by both glob patterns should be counted"
        );
    }

    #[test]
    fn test_files_glob_pattern_no_matches_falls_through_as_literal() {
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs::default(),
            query: Some("self".to_string()),
            files: Some(vec![PathBuf::from("/nonexistent/mq_glob_no_match_*.md")]),
            argv: None,
            ..Cli::default()
        };

        assert!(
            cli.run().is_err(),
            "non-matching glob pattern should surface a file error"
        );
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
                output_format: Some(OutputFormat::Raw),
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
                output_format: Some(OutputFormat::Text),
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

    #[cfg(feature = "watch")]
    #[test]
    fn test_watch_targets_requires_a_file() {
        let cli = Cli {
            files: None,
            query: Some("self".to_string()),
            ..Cli::default()
        };
        assert!(
            cli.watch_targets().is_err(),
            "--watch on stdin (no files) should be rejected"
        );
    }

    #[cfg(feature = "watch")]
    #[test]
    fn test_watch_targets_includes_input_files() {
        let file = PathBuf::from("input.md");
        let cli = Cli {
            files: Some(vec![file.clone()]),
            query: Some("self".to_string()),
            ..Cli::default()
        };
        assert_eq!(cli.watch_targets().unwrap(), vec![file]);
    }

    #[cfg(feature = "watch")]
    #[test]
    fn test_watch_targets_includes_query_file_when_from_file() {
        let input_file = PathBuf::from("input.md");
        let query_file = "query.mq";
        let cli = Cli {
            input: InputArgs {
                from_file: true,
                ..Default::default()
            },
            files: Some(vec![input_file.clone()]),
            query: Some(query_file.to_string()),
            ..Cli::default()
        };
        assert_eq!(
            cli.watch_targets().unwrap(),
            vec![input_file, PathBuf::from(query_file)]
        );
    }
}
