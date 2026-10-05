//! Prints the median wall-clock time of each pipeline phase per benchmark input.
//!
//! Run with `just bench-check-phases`. Phases inside the checker come from
//! `TypeChecker::check_profiled`.

#[path = "../benches/inputs.rs"]
mod inputs;

use std::time::{Duration, Instant};

use inputs::{INPUTS, input, is_builtin_source};
use mq_check::TypeChecker;
use mq_hir::Hir;
use url::Url;

const RUNS: usize = 11;

fn median(mut samples: Vec<Duration>) -> Duration {
    samples.sort();
    samples[samples.len() / 2]
}

fn ms(d: Duration) -> String {
    format!("{:8.2}", d.as_secs_f64() * 1000.0)
}

fn main() {
    let url = Url::parse("file:///bench.mq").unwrap();
    for name in INPUTS {
        let code = input(name);
        let runs = if name.ends_with("4000") { 3 } else { RUNS };
        let mut parse = Vec::new();
        let mut builtin_hir = Vec::new();
        let mut user_hir = Vec::new();
        let mut phases: Vec<Vec<(&'static str, Duration)>> = Vec::new();
        let mut total_check = Vec::new();

        for _ in 0..runs {
            let t = Instant::now();
            let _ = mq_lang::parse_recovery(&code);
            parse.push(t.elapsed());

            let mut hir = Hir::default();
            hir.builtin.disabled = is_builtin_source(name);
            let t = Instant::now();
            hir.add_code(None, "");
            builtin_hir.push(t.elapsed());

            let t = Instant::now();
            hir.add_code(Some(url.clone()), &code);
            user_hir.push(t.elapsed());

            let t = Instant::now();
            let (_, timings) = TypeChecker::new().check_profiled(&hir);
            total_check.push(t.elapsed());
            phases.push(timings.summed());
        }

        println!("== {name} ({} bytes, median of {runs}) ==", code.len());
        println!("  parse                          {} ms", ms(median(parse)));
        println!("  hir: builtin.mq                {} ms", ms(median(builtin_hir)));
        println!("  hir: user code                 {} ms", ms(median(user_hir)));
        for (i, (phase, _)) in phases[0].iter().enumerate() {
            let samples = phases.iter().map(|p| p[i].1).collect();
            println!("  check: {phase:<23} {} ms", ms(median(samples)));
        }
        println!("  check: total                   {} ms", ms(median(total_check)));
    }
}
