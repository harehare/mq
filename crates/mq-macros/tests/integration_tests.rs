use mq_macros::mq_eval;

#[test]
fn test_mq_eval_valid_code() {
    let output = mq_eval!(".h", "input");
}
