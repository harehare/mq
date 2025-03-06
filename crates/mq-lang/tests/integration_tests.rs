use mq_lang::{Engine, MqResult, Value};
use rstest::{fixture, rstest};

#[fixture]
fn engine() -> Engine {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module().unwrap();
    engine
}

#[rstest]
#[case::def_("
    # comments
    def test_fn(s):
       let test = \"WORLD\" | ltrimstr(s, \"hello\") | upcase() | ltrimstr(test);
    | test_fn(\"helloWorld2025\")
    ",
      vec![Value::String("helloWorld".to_string())],
      Ok(vec![Value::String("2025".to_string())].into()))]
#[case::while_("
    let x = 5 |
    while(gt(x, 0)):
      # test
      let x = sub(x, 1) | x;
    ",
      vec![Value::Number(10.into())],
      Ok(vec![Value::Array(vec![Value::Number(4.into()), Value::Number(3.into()), Value::Number(2.into()), Value::Number(1.into()), Value::Number(0.into())])].into()))]
#[case::until("
    until(gt(1)):
      sub(1); | add(2) | pow(2) | div(3)
    ",
      vec![Value::Number(10.into())],
      Ok(vec![Value::Number(3.into())].into()))]
#[case::until("
    until(gt(1)):
      sub(1); | add(2) | pow(2) | div(3)
    ",
      vec![Value::Number(10.into())],
      Ok(vec![Value::Number(3.into())].into()))]
#[case::until("
      let x = 5 |
      until(gt(x, 0)):
        let x = sub(x, 1) | x
      ",
        vec![Value::Number(5.into())],
        Ok(vec![Value::Number(0.into())].into()))]
#[case::foreach("
    foreach(x, array(1, 2, 3)):
      add(x, 1);
    ",
      vec![Value::Number(10.into())],
      Ok(vec![Value::Array(vec![Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())])].into()))]
#[case::if_("
    def fibonacci(x):
      if(eq(x, 0)):
        0
      elif(eq(x, 1)):
        1
      else:
        add(fibonacci(sub(x, 1)), fibonacci(sub(x, 2)))
      ; | fibonacci(10)
    ",
      vec![Value::Number(10.into())],
      Ok(vec![Value::Number(55.into())].into()))]
#[case::elif_("
      def test_fn(x):
        if (eq(x, 0)):
            0
        elif (eq(x, 1)):
            1
        else:
            2;
      | test_fn(0)
      ",
        vec![Value::Number(0.into())],
        Ok(vec![Value::Number(0.into())].into()))]
#[case::elif_("
      def test_fn(x):
        if (eq(x, 0)):
            0
        elif (eq(x, 1)):
            1
        else:
            2;
      | test_fn(1)
      ",
        vec![Value::Number(1.into())],
        Ok(vec![Value::Number(1.into())].into()))]
#[case::elif_("
      def test_fn(x):
        if (eq(x, 0)):
            0
        elif (eq(x, 1)):
            1
        else:
            2;
      | test_fn(2)
      ",
        vec![Value::Number(2.into())],
        Ok(vec![Value::Number(2.into())].into()))]
// contains
#[case::contains("contains(\"test\")",
      vec![Value::String("testString".to_string())],
      Ok(vec![Value::TRUE].into()))]
#[case::contains("contains(\"test\")",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::FALSE].into()))]
// is_array
#[case::is_array("is_array()",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::TRUE].into()))]
#[case::is_array("is_array(array(\"test\"))",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::TRUE].into()))]
#[case::is_array("is_string(array(\"test\"))",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::FALSE].into()))]
// ltrimstr
#[case::ltrimstr("ltrimstr(\"test\")",
      vec![Value::String("testString".to_string())],
      Ok(vec![Value::String("String".to_string())].into()))]
// rtrimstr
#[case::rtrimstr("rtrimstr(\"test\")",
      vec![Value::String("Stringtest".to_string())],
      Ok(vec![Value::String("String".to_string())].into()))]
// is_empty
#[case::is_empty("is_empty(\"\")",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::TRUE].into()))]
#[case::is_empty("is_empty(\"test\")",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::FALSE].into()))]
#[case::is_empty("is_empty(array(\"test\"))",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::FALSE].into()))]
// test
#[case::test("test(\"^hello.*\")",
      vec![Value::String("helloWorld".to_string())],
      Ok(vec![Value::TRUE].into()))]
#[case::test("test(\"^world.*\")",
      vec![Value::String("helloWorld".to_string())],
      Ok(vec![Value::FALSE].into()))]
// select
#[case::test("select(contains(\"hello\"))",
      vec![Value::Markdown(mq_md::Node::Text(mq_md::Text{value: "hello world".to_string(), position: None}))],
      Ok(vec![Value::Markdown(mq_md::Node::Text(mq_md::Text{value: "hello world".to_string(), position: None}))].into()))]
// first
#[case::first("first(array(1, 2, 3))",
      vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
      Ok(vec![Value::Number(1.into())].into()))]
#[case::first("first(array())",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::None].into()))]
// last
#[case::last("last(array(1, 2, 3))",
      vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
      Ok(vec![Value::Number(3.into())].into()))]
#[case::last("last(array())",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::None].into()))]
#[case::test("select(contains(\"hello\"))",
      vec![Value::String("hello world".to_string())],
      Ok(vec![Value::String("hello world".to_string())].into()))]
#[case::closure("
      def make_adder(x):
        def adder(y):
            add(x, y);
      ;
      let add_five = make_adder(5)
      | add_five(10)
      ",
        vec![Value::Number(10.into())],
        Ok(vec![Value::Number(15.into())].into()))]
#[case::closure("
      def make_adder(x):
        def adder(y):
            add(x, y);
      ;
      let add_five = def adder(x): add(x, 5);
      | add_five(10)
      ",
        vec![Value::Number(10.into())],
        Ok(vec![Value::Number(15.into())].into()))]
#[case::map("def test(x): add(x, 1); map(array(1, 2, 3), test)",
            vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
            Ok(vec![Value::Array(vec![Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())])].into()))]
#[case::optional_operator("
            def test_optional(x):
              None
            | test_optional(10)? | test_optional(10)?
            ",
              vec![Value::None],
              Ok(vec![Value::None].into()))]
// filter
#[case::filter("
            def is_even(x):
              eq(mod(x, 2), 0);
            | filter(array(1, 2, 3, 4, 5, 6), is_even)
            ",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(6.into())])],
                    Ok(vec![Value::Array(vec![Value::Number(2.into()), Value::Number(4.into()), Value::Number(6.into())])].into()))]
#[case::filter("
            def is_odd(x):
              eq(mod(x, 2), 1);
            | filter(array(1, 2, 3, 4, 5, 6), is_odd)
            ",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(6.into())])],
              Ok(vec![Value::Array(vec![Value::Number(1.into()), Value::Number(3.into()), Value::Number(5.into())])].into()))]
fn test(
    mut engine: Engine,
    #[case] program: &str,
    #[case] input: Vec<Value>,
    #[case] expected: MqResult,
) {
    assert_eq!(engine.eval(program, input.into_iter()), expected);
}
