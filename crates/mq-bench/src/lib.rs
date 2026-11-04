//! Runner for `bench_` functions in `.mq` files.

mod alloc;
mod runner;
mod stats;

pub use alloc::{CountingAlloc, MemStats};
pub use runner::{BenchRecord, BenchRunner, OutputFormat, Phase};
