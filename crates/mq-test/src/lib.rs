//! Runner for `.mq` test files with coverage and reporting.

mod coverage;
mod doctest;
mod highlight;
mod html;
mod resolver;
mod runner;
mod snapshot;

pub use coverage::{CoverageFormat, FileCoverage};
pub use runner::{OutputFormat, TestRunner};

/// Runs the `# => expected` examples in Markdown ` ```mq ` blocks and in fenced blocks of `.mq`
/// comments. Returns `true` when at least one example ran and every example passed.
///
/// Files are read through an in-memory I/O, so only standard modules resolve. With
/// `module_directories`, local modules are read from the file's directory and these directories.
pub fn run_doc_examples(files: &[std::path::PathBuf], module_directories: &[std::path::PathBuf]) -> bool {
    doctest::run(files, module_directories)
}
