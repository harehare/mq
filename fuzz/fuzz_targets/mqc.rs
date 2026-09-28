#![no_main]

use libfuzzer_sys::fuzz_target;
use mq_fuzz::MqcInput;

fuzz_target!(|input: MqcInput| {
    mq_fuzz::load_and_run_mqc(&input);
});
