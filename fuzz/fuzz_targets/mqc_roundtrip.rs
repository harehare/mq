#![no_main]

use libfuzzer_sys::fuzz_target;
use mq_fuzz::Context;

fuzz_target!(|context: Context| {
    mq_fuzz::roundtrip_mqc(&context);
});
