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
/// comments. Returns `true` when every example passes.
pub fn run_doc_examples(files: &[std::path::PathBuf]) -> bool {
    doctest::run(files)
}
