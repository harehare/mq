#![no_main]

use libfuzzer_sys::fuzz_target;
use mq_fuzz::StdlibInput;

fuzz_target!(|input: StdlibInput| {
    mq_fuzz::run_stdlib(&input);
});
