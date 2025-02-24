#![no_main]

use std::panic::catch_unwind;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Clone, Arbitrary)]
struct Context<'a> {
    script: &'a str,
}

fuzz_target!(|context: Context| {
    let result = catch_unwind(|| {
        let mut engine = mq_lang::Engine::default();
        let _ = engine.eval(
            context.script,
            vec![mq_lang::Value::String("".to_string())].into_iter(),
        );
    });

    match result {
        Ok(_) => (),
        Err(panic) => {
            let is_stack_overflow = panic
                .downcast_ref::<String>()
                .map(|s| s.contains("stack-overflow"))
                .unwrap_or(false);

            if !is_stack_overflow {
                std::panic::resume_unwind(panic);
            }
        }
    }
});
