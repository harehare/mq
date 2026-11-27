#![no_main]

use libfuzzer_sys::fuzz_target;
use mq_fuzz::Document;

fuzz_target!(|document: Document| {
    mq_fuzz::check_round_trip(&document);
});
