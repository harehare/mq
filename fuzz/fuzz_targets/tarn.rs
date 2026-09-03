#![no_main]

// `mq-lang/tarn` routes `Engine::eval` through the bytecode VM instead of the tree-walker
// (see `mq-lang/src/tarn.rs`); without it this target would silently fuzz the tree-walker
// under the wrong name. Run with `cargo +nightly fuzz run tarn --features tarn`.
#[cfg(not(feature = "tarn"))]
compile_error!(
    "the `tarn` fuzz target requires `--features tarn`, e.g. `cargo +nightly fuzz run tarn --features tarn`"
);

use libfuzzer_sys::fuzz_target;
use mq_fuzz::Context;

fuzz_target!(|context: Context| {
    mq_fuzz::eval_and_check(&context);
});
