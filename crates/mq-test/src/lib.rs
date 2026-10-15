//! Runner for `.mq` test files with coverage and reporting.

mod coverage;
mod highlight;
mod html;
mod runner;
mod snapshot;

pub use coverage::{CoverageFormat, FileCoverage};
pub use runner::TestRunner;
