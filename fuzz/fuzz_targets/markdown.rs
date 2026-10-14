#![no_main]

use libfuzzer_sys::fuzz_target;
use mq_fuzz::MarkdownInput;

fuzz_target!(|input: MarkdownInput| {
    mq_fuzz::read_markdown(&input);
});
