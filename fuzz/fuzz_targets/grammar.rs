#![no_main]

use libfuzzer_sys::fuzz_target;
use mq_fuzz::Program;

fuzz_target!(|program: Program| {
    mq_fuzz::run_program(&program);
});
