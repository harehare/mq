use mq_macros::mq_eval;

#[test]
fn test_mq_eval_valid_code() {
    let output = mq_eval! {".h | nodes | filter(fn(v): not(is_none(v));)", "# input\ntest"}.unwrap();
    assert_eq!(output.len(), 2);
}
