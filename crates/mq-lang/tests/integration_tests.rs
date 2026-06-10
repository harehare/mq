use std::collections::BTreeMap;

use mq_lang::{DefaultEngine, Engine, Ident, MqResult, RuntimeValue};
use rstest::{fixture, rstest};

#[fixture]
fn engine() -> DefaultEngine {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine
}

#[rstest]
#[case::def_("
    # comments
    def test_fn(s):
       let test = \"WORLD\" | ltrimstr(s, \"hello\") | upcase() | ltrimstr(test);
    | test_fn(\"helloWorld2025\")
    ",
      vec![RuntimeValue::String("helloWorld".to_string())],
      Ok(vec![RuntimeValue::String("2025".to_string())].into()))]
#[case::while_("
    var x = 5 |
    while (x > 0):
      # test
      x -= 1 | x;
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::foreach("
    foreach(x, array(1, 2, 3)):
      add(x, 1);
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])].into()))]
#[case::while_break("
    var x = 0 |
    while(x < 10):
      x += 1
      | if(x == 3):
        break
      else:
        x;
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::while_break_with_value("
    var x = 0 |
    while(x < 10):
      x += 1
      | if(x == 5):
        break: \"found\"
      else:
        x;
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::String("found".to_string())].into()))]
#[case::while_continue("
    var x = 0 |
    while(x < 4):
      x += 1
      | if(x == 3):
        continue
      else:
        x;
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(4.into())].into()))]
#[case::foreach_break("
    foreach(x, array(1, 2, 3, 4, 5)):
      if(x == 3):
        break
      else:
        x + 10;
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(11.into()), RuntimeValue::Number(12.into())])].into()))]
#[case::foreach_break_with_value("
    foreach(x, array(1, 2, 3, 4, 5)):
      if(x == 3):
        break: \"stopped at 3\"
      else:
        x + 10;
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("stopped at 3".to_string())].into()))]
#[case::foreach_continue("
    foreach(x, array(1, 2, 3, 4, 5)):
      if(x == 3):
        continue
      else:
        x + 10;
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(11.into()), RuntimeValue::Number(12.into()), RuntimeValue::Number(14.into()), RuntimeValue::Number(15.into())])].into()))]
#[case::while_do_end("
    var x = 5 |
    while (x > 0) do
      x -= 1 | x
    end
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::foreach_do_end("
    foreach(x, array(1, 2, 3)) do
      add(x, 1)
    end
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])].into()))]
#[case::while_do_end_break("
    var x = 0 |
    while(x < 10) do
      x += 1
      | if(x == 3):
        break
      else:
        x
      end
    end
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::foreach_do_end_continue("
    foreach(x, array(1, 2, 3, 4, 5)) do
      if(x == 3):
        continue
      else:
        x + 10
      end
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(11.into()), RuntimeValue::Number(12.into()), RuntimeValue::Number(14.into()), RuntimeValue::Number(15.into())])].into()))]
#[case::loop_break_with_value("
    loop:
      break: 42;
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Number(42.into())].into()))]
#[case::loop_break_with_value_complex("
    var x = 0 |
    loop:
      x += 1
      | if(x >= 5):
          break: x * 10
        else:
          x;
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Number(50.into())].into()))]
#[case::nested_do_end("
    let arr = array(array(1, 2), array(3, 4)) |
    foreach(row, arr) do
      foreach(x, row) do
        x * 2
      end
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(4.into())]), RuntimeValue::Array(vec![RuntimeValue::Number(6.into()), RuntimeValue::Number(8.into())])])].into()))]
#[case::match_do_end("
    match (2) do
      | 1: \"one\"
      | 2: \"two\"
      | _: \"other\"
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("two".to_string())].into()))]
#[case::match_do_end_type_pattern("
    match (array(1, 2, 3)) do
      | :array: \"is_array\"
      | :number: \"is_number\"
      | _: \"other\"
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("is_array".to_string())].into()))]
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
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(55.into())].into()))]
#[case::if_("let x = 1
      | let y = if (eq(x, 1)): 2 else: 3
      | y
      ",
        vec![RuntimeValue::Number(0.into())],
              Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::if_("let x = 2
      | let y = if (eq(x, 1)): 1
      | y
      ",
        vec![RuntimeValue::Number(0.into())], Ok(vec![RuntimeValue::NONE].into()))]
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
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Number(0.into())].into()))]
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
        vec![RuntimeValue::Number(1.into())],
        Ok(vec![RuntimeValue::Number(1.into())].into()))]
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
        vec![RuntimeValue::Number(2.into())],
        Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::contains("contains(\"test\")",
      vec![RuntimeValue::String("testString".to_string())],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::contains("contains(\"test\")",
      vec![RuntimeValue::String("String".to_string())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::is_array("is_array()",
      vec![RuntimeValue::Array(Vec::new())],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_array("is_array(array(\"test\"))",
      vec![RuntimeValue::Array(Vec::new())],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_array("is_string(array(\"test\"))",
      vec![RuntimeValue::Array(Vec::new())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::is_dict_true("is_dict()",
      vec![RuntimeValue::new_dict()],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_dict_false("is_dict()",
      vec![RuntimeValue::Array(Vec::new())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::is_none_true("is_none(None)",
      vec!["text".into()],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_none_false("is_none()",
      vec![RuntimeValue::Number(1.into())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::is_bool_true("is_bool(true)",
        vec![RuntimeValue::Boolean(true)],
        Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_bool_false("is_bool(false)",
        vec![RuntimeValue::Boolean(false)],
        Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_bool_non_bool("is_bool(1)",
        vec![RuntimeValue::Number(1.into())],
        Ok(vec![RuntimeValue::FALSE].into()))]
#[case::ltrimstr("ltrimstr(\"test\")",
      vec![RuntimeValue::String("testString".to_string())],
      Ok(vec![RuntimeValue::String("String".to_string())].into()))]
#[case::rtrimstr("rtrimstr(\"test\")",
      vec![RuntimeValue::String("Stringtest".to_string())],
      Ok(vec![RuntimeValue::String("String".to_string())].into()))]
#[case::ltrim("ltrim()",
      vec![RuntimeValue::String(" test ".to_string())],
      Ok(vec![RuntimeValue::String("test ".to_string())].into()))]
#[case::rtrim("rtrim()",
      vec![RuntimeValue::String(" test ".to_string())],
      Ok(vec![RuntimeValue::String(" test".to_string())].into()))]
#[case::is_empty("is_empty(\"\")",
      vec![RuntimeValue::String("String".to_string())],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::is_empty("is_empty(\"test\")",
      vec![RuntimeValue::String("String".to_string())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::is_empty("is_empty(array(\"test\"))",
      vec![RuntimeValue::String("String".to_string())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::test1("test(\"^hello.*\")",
      vec![RuntimeValue::String("helloWorld".to_string())],
      Ok(vec![RuntimeValue::TRUE].into()))]
#[case::test2("test(\"^world.*\")",
      vec![RuntimeValue::String("helloWorld".to_string())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::test3("select(contains(\"hello\"))",
      vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello world".to_string(), position: None}))],
      Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello world".to_string(), position: None}))].into()))]
#[case::first("first(array(1, 2, 3))",
      vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
      Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::first("first(array())",
      vec![RuntimeValue::Array(Vec::new())],
      Ok(vec![RuntimeValue::None].into()))]
#[case::last("last(array(1, 2, 3))",
      vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
      Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::last("last(array())",
      vec![RuntimeValue::Array(Vec::new())],
      Ok(vec![RuntimeValue::None].into()))]
#[case::select("select(contains(\"hello\"))",
      vec![RuntimeValue::String("hello world".to_string())],
      Ok(vec![RuntimeValue::String("hello world".to_string())].into()))]
#[case::closure("
      def make_adder(x):
        fn(y): add(x, y);
      end
      let add_five = make_adder(5)
      | add_five(10)
      ",
        vec![RuntimeValue::Number(10.into())],
        Ok(vec![RuntimeValue::Number(15.into())].into()))]
#[case::closure("
      def make_adder(x):
        fn(y): add(x, y);
      end
      let add_five = fn(x): add(x, 5);
      | add_five(10)
      ",
        vec![RuntimeValue::Number(10.into())],
        Ok(vec![RuntimeValue::Number(15.into())].into()))]
#[case::map("def test(x): add(x, 1); | map(array(1, 2, 3), test)",
            vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
            Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])].into()))]
#[case::filter("
            def is_even(x):
              eq(mod(x, 2), 0);
            | filter(array(1, 2, 3, 4, 5, 6), is_even)
            ",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into())])],
                    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(6.into())])].into()))]
#[case::filter("
            def is_odd(x):
              eq(mod(x, 2), 1);
            | filter(array(1, 2, 3, 4, 5, 6), is_odd)
            ",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into())])],
              Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(5.into())])].into()))]
#[case::func("let func1 = fn(): 1;
      | let func2 = fn(): 2;
      | add(func1(), func2())",
        vec![RuntimeValue::Number(0.into())],
              Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::interpolated_string("let val1 = \"Hello\"
      | s\"${val1} World!\"",
        vec![RuntimeValue::Number(0.into())],
             Ok(vec!["Hello World!".to_string().into()].into()))]
#[case::interpolated_string("s\"${self} World!\"",
        vec![RuntimeValue::String("Hello".into())],
             Ok(vec!["Hello World!".to_string().into()].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![RuntimeValue::new_markdown(mq_markdown::Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://github.com".to_string()), title: None, ident: "ident".to_string(), label: None }))],
      Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ position: None, url: mq_markdown::Url::new("https://github.com".to_string()), title: None, values: Vec::new()}))],
      Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![RuntimeValue::new_markdown(mq_markdown::Node::Image(mq_markdown::Image{ alt: "".to_string(), position: None, url: "https://github.com".to_string(), title: None }))],
      Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::matches_url("matches_url(\"https://gitlab.com\")",
      vec![RuntimeValue::String("https://gitlab.com".to_string())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::nest(".link | update(\"test\")",
      vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![
           mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("url".to_string()), title: None, values: Vec::new(), position: None }),
           mq_markdown::Node::Image(mq_markdown::Image{ alt: "".to_string(), url: "url".to_string(), title: None, position: None })
      ], position: None, depth: 1 }))],
      Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("test".to_string()), title: None, values: Vec::new(), position: None }))].into()))]
#[case::selector("nodes | .h",
      vec![
        RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }),], position: None, depth: 1 })),
        RuntimeValue::String("test".to_string()),
      ],
      Ok(vec![
        RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }),], position: None, depth: 1 })),
        RuntimeValue::NONE
      ].into()))]
#[case::selector("nodes | .h",
      vec![
        RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })),
        RuntimeValue::String("test".to_string()),
      ],
      Ok(vec![RuntimeValue::NONE, RuntimeValue::NONE].into()))]
#[case::sort_by("sort_by(get_title)",
      vec![RuntimeValue::Array(vec![
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])],
      Ok(vec![RuntimeValue::Array(vec![
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])].into()))]
#[case::sort_by("sort_by(get_url)",
      vec![RuntimeValue::Array(vec![
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])],
      Ok(vec![RuntimeValue::Array(vec![
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])].into()))]
#[case::sort_by(r#"def sort_test(v): if (eq(v, "3")): "1" elif (eq(v, "1")): "3" else: v; sort_by(sort_test)"#,
      vec![RuntimeValue::Array(vec![
         "2".to_string().into(),
         "1".to_string().into(),
         "3".to_string().into(),
      ])],
      Ok(vec![RuntimeValue::Array(vec![
         "3".to_string().into(),
         "2".to_string().into(),
         "1".to_string().into(),
      ])].into()))]
#[case::find_index("
      def is_even(x):
        eq(mod(x, 2), 0);
      | find_index(array(1, 3, 5, 6, 7), is_even)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(7.into())])],
        Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::find_index("
      def is_greater_than_five(x):
        gt(x, 5);
      | find_index(array(1, 3, 5, 6, 7), is_greater_than_five)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(7.into())])],
        Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::find_index_no_match("
      def is_negative(x):
        lt(x, 0);
      | find_index(array(1, 3, 5, 6, 7), is_negative)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(7.into())])],
        Ok(vec![RuntimeValue::Number((-1).into())].into()))]
#[case::find_index_empty_array("
      def is_even(x):
        eq(mod(x, 2), 0);
      | find_index(array(), is_even)
      ",
        vec![RuntimeValue::Array(vec![])],
        Ok(vec![RuntimeValue::Number((-1).into())].into()))]
#[case::skip("
          skip([1, 2, 3, 4, 5], 2)
          ",
          vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(1.into()),
              RuntimeValue::Number(2.into()),
              RuntimeValue::Number(3.into()),
              RuntimeValue::Number(4.into()),
              RuntimeValue::Number(5.into()),
          ])],
          Ok(vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(3.into()),
              RuntimeValue::Number(4.into()),
              RuntimeValue::Number(5.into()),
          ])].into()))]
#[case::skip_zero("
          skip([1, 2, 3], 0)
          ",
          vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(1.into()),
              RuntimeValue::Number(2.into()),
              RuntimeValue::Number(3.into()),
          ])],
          Ok(vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(1.into()),
              RuntimeValue::Number(2.into()),
              RuntimeValue::Number(3.into()),
          ])].into()))]
#[case::skip_all("
          skip([1, 2, 3], 3)
          ",
          vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(1.into()),
              RuntimeValue::Number(2.into()),
              RuntimeValue::Number(3.into()),
          ])],
          Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::skip_more_than_length("
          skip([1, 2, 3], 5)
          ",
          vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(1.into()),
              RuntimeValue::Number(2.into()),
              RuntimeValue::Number(3.into()),
          ])],
          Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::skip_empty("
          skip([], 2)
          ",
          vec![RuntimeValue::Array(vec![])],
          Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::skip_while("
      def is_less_than_four(x):
        lt(x, 4);
      | skip_while(array(1, 2, 3, 4, 5, 1, 2), is_less_than_four)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::skip_while_all_match("
      def is_positive(x):
        gt(x, 0);
      | skip_while(array(1, 2, 3), is_positive)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
        Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::skip_while_empty_array("
      def is_positive(x):
        gt(x, 0);
      | skip_while(array(), is_positive)
      ",
        vec![RuntimeValue::Array(vec![])],
        Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::take("
        take([1, 2, 3, 4, 5], 3)
        ",
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
            RuntimeValue::Number(4.into()),
            RuntimeValue::Number(5.into()),
        ])],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])].into()))]
#[case::take_zero("
        take([1, 2, 3], 0)
        ",
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])],
        Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::take_all("
        take([1, 2, 3], 3)
        ",
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])].into()))]
#[case::take_more_than_length("
        take([1, 2, 3], 5)
        ",
        vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])].into()))]
#[case::take_empty("
        take([], 2)
        ",
        vec![RuntimeValue::Array(vec![])],
        Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::take_while("
      def is_less_than_four(x):
        lt(x, 4);
      | take_while(array(1, 2, 3, 4, 5, 1, 2), is_less_than_four)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::take_while_none_match("
      def is_negative(x):
        lt(x, 0);
      | take_while(array(1, 2, 3), is_negative)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
        Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::take_while_all_match("
      def is_positive(x):
        gt(x, 0);
      | take_while(array(1, 2, 3), is_positive)
      ",
        vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
        Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::take_while_empty_array("
      def is_positive(x):
        gt(x, 0);
      | take_while(array(), is_positive)
      ",
        vec![RuntimeValue::Array(vec![])],
        Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::anonymous_fn("
        let f = fn(x): add(x, 1);
        | f(10)
        ",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(11.into())].into()))]
#[case::anonymous_fn_passed("
          def apply_func(f, x):
            f(x);
          | apply_func(fn(x): mul(x, 2);, 5)
          ",
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::anonymous_fn_return("
          def make_multiplier(factor):
            fn(x): mul(x, factor);;
          | let double = make_multiplier(2)
          | double(5)
          ",
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::array_empty("[]",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::array_with_elements("[1, 2, 3]",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::array_nested("[[1, 2], [3, 4]]",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())]),
            RuntimeValue::Array(vec![RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])
          ])].into()))]
#[case::array_mixed_types("[1, \"test\", []]",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::String("test".to_string()),
            RuntimeValue::Array(vec![])
          ])].into()))]
#[case::array_length("len([])",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::array_length("len([1, 2, 3, 4])",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(4.into())].into()))]
#[case::dict_new_empty("dict()",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::new_dict()].into()))]
#[case::dict_set_get_string("let m = dict() | let m = set(m, \"name\", \"Jules\") | get(m, \"name\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::String("Jules".to_string())].into()))]
#[case::dict_set_get_number("let m = set(dict(), \"age\", 30) | get(m, \"age\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(30.into())].into()))]
#[case::dict_set_get_array("let m = set(dict(), \"data\", [1, 2, 3]) | get(m, \"data\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::dict_set_get_bool("let m = set(dict(), \"active\", true) | get(m, \"active\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::dict_set_get_none("let m = set(dict(), \"nothing\", None) | get(m, \"nothing\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::None].into()))]
#[case::dict_get_non_existent("let m = dict() | get(m, \"missing\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::None].into()))]
#[case::dict_set_overwrite("let m = set(dict(), \"name\", \"Jules\") | let m = set(m, \"name\", \"Vincent\") | get(m, \"name\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::String("Vincent".to_string())].into()))]
#[case::dict_nested_set_get("let m1 = dict() | let m2 = set(dict(), \"level\", 2) | let m = set(m1, \"nested\", m2) | get(get(m, \"nested\"), \"level\")",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::dict_keys_empty("keys(dict())",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::dict_keys_non_empty("let m = set(set(dict(), \"a\", 1), \"b\", 2) | keys(m)",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])].into()))]
#[case::dict_values_empty("values(dict())",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::dict_values_non_empty("let m = set(set(dict(), \"a\", 1), \"b\", \"hello\") | values(m)",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::String("hello".to_string())])].into()))]
#[case::dict_len_empty("len(dict())",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::dict_len_non_empty("len(set(set(dict(), \"a\", 1), \"b\", 2))",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::dict_type_is_dict("type(dict())",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
#[case::dict_contains_existing_key(r#"let m = set(dict(), "name", "Jules") | contains(m, "name")"#,
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::dict_contains_non_existing_key(r#"let m = set(dict(), "name", "Jules") | contains(m, "age")"#,
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::dict_contains_empty(r#"contains(dict(), "any_key")"#,
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::dict_contains_multiple_keys(r#"let m = set(set(set(dict(), "a", 1), "b", 2), "c", 3) | contains(m, "b")"#,
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::dict_map_identity(r#"let m = dict(["a", 1], ["b", 2]) | map(m, fn(kv): kv;)"#,
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![{
          let mut dict = BTreeMap::new();
          dict.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
          dict.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
          dict.into()
        }].into()))]
#[case::dict_map_transform_values("
        def double_value(kv):
          array(first(kv), mul(last(kv), 2));
        | let m = set(set(dict(), \"x\", 5), \"y\", 10)
        | map(m, double_value)
        ",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![{
            let mut dict = BTreeMap::new();
            dict.insert(Ident::new("x"), RuntimeValue::Number(10.into()));
            dict.insert(Ident::new("y"), RuntimeValue::Number(20.into()));
            dict.into()
          }].into()))]
#[case::dict_map_transform_keys(r#"
          def prefix_key(kv):
            array(add("prefix_", first(kv)), last(kv));
          | let m = set(set(dict(), "a", 1), "b", 2)
          | map(m, prefix_key)
          "#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![{
              let mut dict = BTreeMap::new();
              dict.insert(Ident::new("prefix_a"), RuntimeValue::Number(1.into()));
              dict.insert(Ident::new("prefix_b"), RuntimeValue::Number(2.into()));
              dict.into()
            }].into()))]
#[case::dict_map_empty("map(dict(), fn(kv): kv;)",
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::new_dict()].into()))]
#[case::dict_map_complex_transform(r#"
          def transform_entry(kv):
            let key = first(kv)
            | let value = last(kv)
            | array(add(key, "_transformed"), add(value, 100));
          | let m = set(set(dict(), "num1", 1), "num2", 2)
          | map(m, transform_entry)
          "#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![{
              let mut dict = BTreeMap::new();
              dict.insert(Ident::new("num1_transformed"), RuntimeValue::Number(101.into()));
              dict.insert(Ident::new("num2_transformed"), RuntimeValue::Number(102.into()));
              dict.into()
            }].into()))]
#[case::dict_filter_even_values(r#"
            def is_even_value(kv):
              last(kv) | mod(2) | eq(0);
            | let m = dict(["a", 1], ["b", 2], ["c", 4])
            | filter(m, is_even_value)
            "#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![{
              let mut dict = BTreeMap::new();
              dict.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
              dict.insert(Ident::new("c"), RuntimeValue::Number(4.into()));
              dict.into()
            }].into()))]
#[case::dict_filter_empty("filter(dict(), fn(kv): true;)",
           vec![RuntimeValue::Number(0.into())],
           Ok(vec![RuntimeValue::new_dict()].into()))]
#[case::group_by_numbers("
            def get_remainder(x):
              mod(x, 3);
            | group_by(array(1, 2, 3, 4, 5, 6, 7, 8, 9), get_remainder)
            ",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(7.into()), RuntimeValue::Number(8.into()), RuntimeValue::Number(9.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("0"), RuntimeValue::Array(vec![RuntimeValue::Number(3.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(9.into())]));
                dict.insert(Ident::new("1"), RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(7.into())]));
                dict.insert(Ident::new("2"), RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(8.into())]));
                dict.into()
              }].into()))]
#[case::group_by_strings(r#"
            def get_length(s):
              len(s);
            | group_by(array("cat", "dog", "bird", "fish", "elephant"), get_length)
            "#,
              vec![RuntimeValue::Array(vec![RuntimeValue::String("cat".to_string()), RuntimeValue::String("dog".to_string()), RuntimeValue::String("bird".to_string()), RuntimeValue::String("fish".to_string()), RuntimeValue::String("elephant".to_string())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("3"), RuntimeValue::Array(vec![RuntimeValue::String("cat".to_string()), RuntimeValue::String("dog".to_string())]));
                dict.insert(Ident::new("4"), RuntimeValue::Array(vec![RuntimeValue::String("bird".to_string()), RuntimeValue::String("fish".to_string())]));
                dict.insert(Ident::new("8"), RuntimeValue::Array(vec![RuntimeValue::String("elephant".to_string())]));
                dict.into()
              }].into()))]
#[case::group_by_empty_array("
            def identity(x):
              x;
            | group_by(array(), identity)
            ",
              vec![RuntimeValue::Array(vec![])],
              Ok(vec![RuntimeValue::new_dict()].into()))]
#[case::group_by_single_element("
            def identity(x):
              x;
            | group_by(array(42), identity)
            ",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("42"), RuntimeValue::Array(vec![RuntimeValue::Number(42.into())]));
                dict.into()
              }].into()))]
#[case::group_by_all_same_key(r#"
            def always_same(x):
              "same";
            | group_by(array(1, 2, 3, 4), always_same)
            "#,
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("same"), RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())]));
                dict.into()
              }].into()))]
#[case::group_by_boolean_result("
            def is_even(x):
              eq(mod(x, 2), 0);
            | group_by(array(1, 2, 3, 4, 5, 6), is_even)
            ",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("false"), RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(5.into())]));
                dict.insert(Ident::new("true"), RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(6.into())]));
                dict.into()
              }].into()))]
#[case::is_h_true("is_h()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h_false("is_h()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h1_true("is_h1()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h1_false("is_h1()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 2,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h1_false("is_h1()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h2_true("is_h2()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 2,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h2_false("is_h2()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 3,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h2_false("is_h2()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h3_true("is_h3()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 3,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h3_false("is_h3()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 4,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h3_false("is_h3()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h4_true("is_h4()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 4,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h4_false("is_h4()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 5,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h4_false("is_h4()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h5_true("is_h5()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 5,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h5_false("is_h5()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 4,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h5_false("is_h5()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h6_true("is_h6()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 6,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h6_false("is_h6()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 5,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h6_false("is_h6()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_em_true("is_em()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
          values: vec![],
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "true".to_string(),
          position: None,
        }))].into()))]
#[case::is_em_false("is_em()",
        vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_html_true("is_html()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Html(mq_markdown::Html {
              value: "<b>bold</b>".to_string(),
              position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
          }))].into()))]
#[case::is_html_false("is_html()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not html".to_string(),
              position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
          }))].into()))]
#[case::is_yaml_true("is_yaml()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Yaml(mq_markdown::Yaml {
            value: "---\nkey: value\n".to_string(),
            position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_yaml_false("is_yaml()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not yaml".to_string(),
            position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_toml_true("is_toml()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Toml(mq_markdown::Toml {
            value: "[section]\nkey = \"value\"\n".to_string(),
            position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_toml_false("is_toml()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not toml".to_string(),
            position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_code_true("is_code()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            position: None,
            fence: true,
            meta: None,
            lang: Some("rust".to_string()),
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_code_false("is_code()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not code".to_string(),
            position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_text_true("is_text()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "sample".to_string(),
            position: None,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_text_false("is_text()",
          vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
          }))],
          Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_list_true("is_list()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 1,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_list_false("is_list()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not a list".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_flow_expression_true("is_mdx_flow_expression()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::MdxFlowExpression(mq_markdown::MdxFlowExpression {
              value: "1 + 2".into(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_flow_expression_false("is_mdx_flow_expression()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_flow_element_true("is_mdx_jsx_flow_element()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement {
              name: Some("Component".to_string()),
              attributes: vec![],
              children: vec![],
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_flow_element_false("is_mdx_jsx_flow_element()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not jsx".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_text_element_true("is_mdx_jsx_text_element()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement {
              name: Some("InlineComponent".into()),
              attributes: vec![],
              children: vec![],
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_text_element_false("is_mdx_jsx_text_element()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not jsx text".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_text_expression_true("is_mdx_text_expression()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::MdxTextExpression(mq_markdown::MdxTextExpression {
              value: "foo + bar".into(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_text_expression_false("is_mdx_text_expression()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx text expr".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_js_esm_true("is_mdx_js_esm()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::MdxJsEsm(mq_markdown::MdxJsEsm {
              value: "export const foo = 1;".into(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_js_esm_false("is_mdx_js_esm()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not esm".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_true("is_mdx()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::MdxFlowExpression(mq_markdown::MdxFlowExpression {
              value: "1 + 2".into(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_false("is_mdx()",
            vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx".to_string(),
              position: None,
            }))],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::any_true("
              any([1, 2, 3], fn(x): x == 2;)",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
              Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::any_false("
              any([1, 2, 3], fn(x): x == 4;)",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])],
              Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::any_empty_array("
              any([], fn(x): x == 1;)",
              vec![RuntimeValue::Array(vec![])],
              Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::any_dict_true(r#"any(dict(["a", 1], ["b", 2]), fn(kv): last(kv) == 2;)"#,
              vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
                dict.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
                dict.into()
              }],
              Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::any_dict_false(r#"any(dict(["a", 1], ["b", 2]), fn(kv): last(kv) == 3;)"#,
              vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
                dict.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
                dict.into()
              }],
              Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::all_true("
              all([2, 4, 6], fn(x): mod(x, 2) == 0;)",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(6.into())])],
              Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::all_false("
              all([2, 3, 6], fn(x): mod(x, 2) == 0;)",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(6.into())])],
              Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::all_empty_array("
              all([], fn(x): x == 1;)",
              vec![RuntimeValue::Array(vec![])],
              Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::all_dict_true(r#"all(dict(["a", 2], ["b", 4]), fn(kv): mod(last(kv), 2) == 0;)"#,
              vec![{
              let mut dict = BTreeMap::new();
              dict.insert(Ident::new("a"), RuntimeValue::Number(2.into()));
              dict.insert(Ident::new("b"), RuntimeValue::Number(4.into()));
              dict.into()
              }],
              Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::all_dict_false(r#"all(dict(["a", 2], ["b", 3]), fn(kv): mod(last(kv), 2) == 0;)"#,
              vec![{
              let mut dict = BTreeMap::new();
              dict.insert(Ident::new("a"), RuntimeValue::Number(2.into()));
              dict.insert(Ident::new("b"), RuntimeValue::Number(3.into()));
              dict.into()
              }],
              Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::in_array_true("in([1, 2, 3], 2)",
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::in_array_false("in([1, 2, 3], 4)",
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::in_string_true(r#"in("hello", "ell")"#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::in_string_false(r#"in("hello", "xyz")"#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::in_array_true(r#"in(["a", "b", "c"], ["a", "b"])"#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::in_array_false(r#"in(["a", "c"], ["a", "b"])"#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::fold_sum("
            def sum(acc, x):
              add(acc, x);
            | fold([1, 2, 3, 4], 0, sum)
            ",
            vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])],
            Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::fold_concat(r#"
            def concat(acc, x):
              add(acc, x);
            | fold(["a", "b", "c"], "", concat)
            "#,
            vec![RuntimeValue::Array(vec![RuntimeValue::String("a".into()), RuntimeValue::String("b".into()), RuntimeValue::String("c".into())])],
            Ok(vec![RuntimeValue::String("abc".into())].into()))]
#[case::fold_empty("
            def sum(acc, x):
              add(acc, x);
            | fold([], 0, sum)
            ",
            vec![RuntimeValue::Array(vec![])],
            Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::unique_by_numbers("
            def get_remainder(x):
              mod(x, 3);
            | unique_by([1, 2, 3, 4, 5, 6, 7, 8, 9], get_remainder)
            ",
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(7.into()), RuntimeValue::Number(8.into()), RuntimeValue::Number(9.into())])],
              Ok(vec![RuntimeValue::Array(vec![
              RuntimeValue::Number(1.into()),
              RuntimeValue::Number(2.into()),
              RuntimeValue::Number(3.into()),
              ])].into()))]
#[case::unique_by_strings(r#"
            def get_length(s):
              len(s);
            | unique_by(["cat", "dog", "bird", "fish", "elephant"], get_length)
            "#,
              vec![RuntimeValue::Array(vec![RuntimeValue::String("cat".to_string()), RuntimeValue::String("dog".to_string()), RuntimeValue::String("bird".to_string()), RuntimeValue::String("fish".to_string()), RuntimeValue::String("elephant".to_string())])],
              Ok(vec![RuntimeValue::Array(vec![
              RuntimeValue::String("cat".to_string()),
              RuntimeValue::String("bird".to_string()),
              RuntimeValue::String("elephant".to_string()),
              ])].into()))]
#[case::unique_by_empty_array("
            def identity(x):
              x;
            | unique_by([], identity)
            ",
              vec![RuntimeValue::Array(vec![])],
              Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::unique_by_all_same_key(r#"
            def always_same(x):
              "same";
            | unique_by([1, 2, 3, 4], always_same)
            "#,
              vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])],
              Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into())])].into()))]
#[case::dict_literal_empty("let d = {} | d",
            vec![RuntimeValue::Number(0.into())], // Dummy input
            Ok(vec![RuntimeValue::new_dict()].into()))]
#[case::dict_literal_simple(r#"let d = {"a": 1, "b": "two"} | d"#, // Mixing string and ident keys
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
                dict.insert(Ident::new("b"), RuntimeValue::String("two".to_string()));
                dict.into()
            }].into()))]
#[case::dict_literal_access_after_creation(r#"let d = {"name": "Jules", "occupation": "Philosopher"} | get(d, "name")"#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::String("Jules".to_string())].into()))]
#[case::dict_literal_trailing_comma(r#"let d = {"a":1, "b":2,} | len(d)"#,
            vec![RuntimeValue::Number(0.into())],
            Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::selector_attr(".code.lang",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Code(mq_markdown::Code{ lang: Some("rust".to_string()), meta: None, fence: true, value: "value".to_string(),  position: None })),
            ],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "rust".to_string(),
              position: None,
            }))].into()))]
#[case::standalone_attr_selector_lang(".lang",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Code(mq_markdown::Code{ lang: Some("rust".to_string()), meta: None, fence: true, value: "value".to_string(),  position: None })),
            ],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "rust".to_string(),
              position: None,
            }))].into()))]
#[case::standalone_attr_selector_set(".lang |= \"python\" | .lang",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Code(mq_markdown::Code{ lang: Some("rust".to_string()), meta: None, fence: true, value: "".to_string(),  position: None })),
            ],
            Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "python".to_string(),
              position: None,
            }))].into()))]
#[case::recursive_selector_with_children("nodes | ..",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{
                  values: vec![
                      mq_markdown::Node::Text(mq_markdown::Text { value: "hello".to_string(), position: None }),
                      mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("url".to_string()), title: None, values: Vec::new(), position: None }),
                  ],
                  position: None,
                  depth: 1,
              })),
            ],
            Ok(vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "hello".to_string(), position: None })),
              RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("url".to_string()), title: None, values: Vec::new(), position: None })),
            ].into()))]
#[case::recursive_selector_leaf_node("nodes | ..",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "leaf".to_string(), position: None })),
            ],
            Ok(vec![].into()))]
#[case::recursive_selector_nested("nodes | ..",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Blockquote(mq_markdown::Blockquote{
                  values: vec![
                      mq_markdown::Node::Heading(mq_markdown::Heading{
                          values: vec![
                              mq_markdown::Node::Text(mq_markdown::Text { value: "nested".to_string(), position: None }),
                          ],
                          position: None,
                          depth: 2,
                      }),
                  ],
                  position: None,
              })),
            ],
            Ok(vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "nested".to_string(), position: None })),
              RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{
                  values: vec![
                      mq_markdown::Node::Text(mq_markdown::Text { value: "nested".to_string(), position: None }),
                  ],
                  position: None,
                  depth: 2,
              })),
            ].into()))]
#[case::recursive_selector_pipe_filter("nodes | .. | filter(fn(x): select(x, .text);)",
            vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{
                  values: vec![
                      mq_markdown::Node::Text(mq_markdown::Text { value: "hello".to_string(), position: None }),
                      mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("url".to_string()), title: None, values: Vec::new(), position: None }),
                  ],
                  position: None,
                  depth: 1,
              })),
            ],
            Ok(vec![
              RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "hello".to_string(), position: None })),
            ].into()))]
#[case::do_block_simple("
    do
      \"hello world\"
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("hello world".to_string())].into()))]
#[case::do_block_multiple_statements("
    do
      let x = 5
      | let y = 10
      | add(x, y)
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Number(15.into())].into()))]
#[case::do_block_with_input("
    do
      add(\"processed: \", self)
    end
    ",
      vec![RuntimeValue::String("input".to_string())],
      Ok(vec![RuntimeValue::String("processed: input".to_string())].into()))]
#[case::do_block_nested("
    do
      let x = 5
      | do
          let y = 10
          | add(x, y)
        end
      | add(x, 100)
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Number(105.into())].into()))]
#[case::do_block_with_function("
    def double(x):
      mul(x, 2);
    | do
        let value = 21
        | double(value)
      end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::Number(42.into())].into()))]
#[case::if_no_end("
    def test_fn(x):
      if (eq(x, 1)):
        \"one\"
      elif (eq(x, 2)):
        \"two\"
      else:
        \"other\";
    | test_fn(1)
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("one".to_string())].into()))]
#[case::if_elif_no_end("
    def test_fn(x):
      if (eq(x, 1)):
        \"one\"
      elif (eq(x, 2)):
        \"two\"
      else:
        \"other\";
    | test_fn(2)
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("two".to_string())].into()))]
#[case::if_else_no_end("
    def test_fn(x):
      if (eq(x, 1)):
        \"one\"
      elif (eq(x, 2)):
        \"two\"
      else:
        \"other\";
    | test_fn(3)
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::nested_if_no_end("
    def test_nested(x, y):
      if (gt(x, 0)):
        if (gt(y, 0)):
          \"both positive\"
        else:
          \"x positive, y non-positive\"
      else:
        \"x non-positive\";
    | test_nested(1, 1)
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("both positive".to_string())].into()))]
#[case::if_in_do_block("
    do
      let x = 5
      | if (gt(x, 3)):
          \"greater than 3\"
        else:
          \"not greater than 3\"
    end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("greater than 3".to_string())].into()))]
#[case::if_do_block_simple("
    let v = 1
    | if (v > 0):
      do
        \"hello world\" | upcase()
      end
    ",
      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("HELLO WORLD".to_string())].into()))]
#[case::let_do_block_simple("
    let v = do \"hello world\" | upcase() end | v
    ",      vec![RuntimeValue::Number(0.into())],
      Ok(vec![RuntimeValue::String("HELLO WORLD".to_string())].into()))]
#[case::array_with_comment("[1 # comment\n, 2, 3]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
          RuntimeValue::Number(1.into()),
          RuntimeValue::Number(2.into()),
          RuntimeValue::Number(3.into())])].into()))]
#[case::dict_literal_with_comment(
  r#"let d = {
    "a": 1, # comment for a
    "b": 2, # comment for b
    # comment line
    "c": 3
  } | d"#,
  vec![RuntimeValue::Number(0.into())],
  Ok(vec![{
    let mut dict = BTreeMap::new();
    dict.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
    dict.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
    dict.insert(Ident::new("c"), RuntimeValue::Number(3.into()));
    dict.into()
  }].into())
)]
#[case::empty_array_iterator_expand("[]*.[]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::array_iter_numbers("[1, 2, 3] | .[]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])].into()))]
#[case::array_iter_strings(r#"["a", "b", "c"] | .[]"#,
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::String("a".to_string()),
            RuntimeValue::String("b".to_string()),
            RuntimeValue::String("c".to_string()),
        ])].into()))]
#[case::dict_iter_values(r#"{"a": 1, "b": 2, "c": 3} | .[]"#,
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])].into()))]
#[case::array_of_dicts_iter(r#"[{"a": 1}, {"b": 2}] | .[]"#,
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            {
                let mut d = BTreeMap::new();
                d.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
                RuntimeValue::Dict(d)
            },
            {
                let mut d = BTreeMap::new();
                d.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
                RuntimeValue::Dict(d)
            },
        ])].into()))]
#[case::array_index_first("[1, 2, 3] | .[0]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::array_index_middle("[1, 2, 3] | .[1]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::array_index_last("[1, 2, 3] | .[2]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::array_index_out_of_bounds("[1, 2, 3] | .[5]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::NONE].into()))]
#[case::array_mul_decimal("[2,1]*0.2",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(0.4.into()),
            RuntimeValue::Number(0.2.into())
        ])].into()))]
#[case::array_mul_large_number("[0.4,0.2]*5E9",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(2000000000.0.into()),
            RuntimeValue::Number(1000000000.0.into())
        ])].into()))]
#[case::array_mul_small_integer("[1,2]*5",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into())
        ])].into()))]
#[case::array_mul_at_boundary("[1,2]*1000",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![{
            let mut arr = Vec::with_capacity(2000);
            for _ in 0..1000 {
                arr.push(RuntimeValue::Number(1.into()));
                arr.push(RuntimeValue::Number(2.into()));
            }
            RuntimeValue::Array(arr)
        }].into()))]
#[case::array_mul_over_boundary("[1,2]*1001",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1001.into()),
            RuntimeValue::Number(2002.into())
        ])].into()))]
#[case::range_mul_decimal_large("2..1*.2*5E9",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(2000000000.0.into()),
            RuntimeValue::Number(1000000000.0.into())
        ])].into()))]
#[case::array_concat_normal("[1,2]+[3,4]",
        vec![RuntimeValue::Number(0.into())],
        Ok(vec![RuntimeValue::Array(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
            RuntimeValue::Number(4.into())
        ])].into()))]
#[case::get_variable_simple("
          let x = 42
          | get_variable(\"x\")
          ",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(42.into())].into()))]
#[case::set_variable_simple("
          set_variable(\"x\", 99)
          | get_variable(\"x\")
          ",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(99.into())].into()))]
#[case::set_variable_overwrite("
          set_variable(\"x\", 1)
          | set_variable(\"x\", 2)
          | get_variable(\"x\")
          ",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::set_and_get_multiple_variables("
          set_variable(\"a\", \"foo\")
          | set_variable(\"b\", \"bar\")
          | get_variable(\"a\") + get_variable(\"b\")
          ",
          vec![RuntimeValue::Number(0.into())],
          Ok(vec![RuntimeValue::String("foobar".to_string())].into()))]
#[case::to_mdx_single_text(
            r#""<Component />" | to_mdx() | first() | to_string()"#,
            vec![RuntimeValue::None],
            Ok(vec![RuntimeValue::String("<Component />".to_string())].into())
          )]
#[case::var_basic("
    var x = 10 | x
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::var_and_assign("
    var x = 10 | x = 20 | x
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(20.into())].into()))]
#[case::var_multiple_assigns("
    var count = 0 | count = count + 1 | count = count + 2 | count
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::var_with_string("
    var name = \"Alice\" | name = \"Bob\" | name
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("Bob".to_string())].into()))]
#[case::var_in_loop("
    var sum = 0 |
    foreach (i, array(1, 2, 3, 4, 5)):
        sum = sum + i
        | sum;
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(6.into()), RuntimeValue::Number(10.into()), RuntimeValue::Number(15.into())])].into()))]
#[case::macro_basic("
    macro double(x) do
      x + x
    end
    | double(5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::macro_with_string("
    macro greet(name) do
      s\"Hello, ${name}!\"
    end
    | greet(\"World\")
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::String("Hello, World!".to_string())].into()))]
#[case::macro_multiple_params("
    macro add_three(a, b, c) do
      a + b + c
    end
    | add_three(1, 2, 3)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(6.into())].into()))]
#[case::macro_with_function_call("
    macro apply_twice(f, x) do
      f(f(x))
    end
    | def inc(n): n + 1;
    | apply_twice(inc, 5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(7.into())].into()))]
#[case::macro_nested_calls("
    macro double(x): x + x
    | macro quadruple(x): double(double(x))
    | quadruple(3)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(12.into())].into()))]
#[case::macro_with_let("
    macro let_double(x) do
      let y = x | y + y
    end
    | let_double(7)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(14.into())].into()))]
#[case::macro_with_if("
    macro max(a, b) do
        if(a > b): a else: b
    end
    | max(10, 5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::macro_with_array("
    macro first_two(arr) do
      arr[0:2]
    end
    | first_two([1, 2, 3, 4, 5])
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::macro_parameter_shadowing("
    let x = 100 |
    macro use_param(x) do
      x * 2
    end
    | use_param(5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::macro_quote_basic("
    macro make_expr(x) do
      quote: unquote(x) + 1
    end
    | make_expr(5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(6.into())].into()))]
#[case::macro_quote_multiple_expressions("
    macro wrap_expr(x) do
      quote do
        let result = unquote(x) | result * 2
      end
    end
    | wrap_expr(5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(10.into())].into()))]
#[case::macro_quote_with_function("
    macro define_double() do
        quote: def double(x): x * 2 end
    end
    | define_double() | double(7)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(14.into())].into()))]
#[case::macro_quote_nested("
    macro compute(a, b) do
        quote: unquote(a) + unquote(b) * 2
    end
    | compute(10, 5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(20.into())].into()))]
#[case::macro_quote_with_if("
    macro conditional_expr(x) do
        quote: if(unquote(x) > 10): \"large\" else: \"small\"
    end
    | conditional_expr(15)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::String("large".to_string())].into()))]
#[case::macro_quote_preserve_structure("
    macro make_array(a, b, c) do
        quote: [unquote(a), unquote(b), unquote(c)]
    end
    | make_array(1, 2, 3)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::macro_quote_with_let_outside("
    macro test(x) do
        let y = x + 1 |
        quote: unquote(y) * 2
    end
    | test(5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(12.into())].into()))]
#[case::macro_quote_mixed_code("
    macro compute(x) do
        let a = x * 2 |
        let b = x + 10 |
        quote: unquote(a) + unquote(b)
    end
    | compute(5)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(25.into())].into()))]
#[case::macro_quote_variable_reference("
    macro make_computation(x) do
        let base = x |
        | quote: unquote(base) * 3 + unquote(x)
    end
    | make_computation(4)
    ",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(16.into())].into()))]
#[case::default_params_with_all_args1(r#"
    def greet(name, greeting="Hello"): greeting + " " + name; | greet("Alice")"#,
    vec!["test".into()],
    Ok(vec!["Hello Alice".into()].into()))]
#[case::default_params_with_all_args2(r#"
    def greet(name, greeting="Hello"): greeting + " " + name; | greet("Alice", "Hi")"#,
    vec!["test".into()],
    Ok(vec!["Hi Alice".into()].into()))]
#[case::default_params_with_self(r#"
    def greet(name, greeting="Hello"): greeting + " " + name; | greet()"#,
    vec!["Alice".into()],
    Ok(vec!["Hello Alice".into()].into()))]
#[case::default_params_with_expr(r#"
    def greet(name, greeting="Hello" + " Hi"): greeting + " " + name; | greet()"#,
    vec!["Alice".into()],
    Ok(vec!["Hello Hi Alice".into()].into()))]
#[case::quote_ast_get_args("let a = 10 | let b = 20 | _ast_get_args(quote: a + b) | len()",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::quote_ast_to_code("let a = 10 | _ast_to_code(quote: a)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::String("a".into())].into()))]
#[case::double_not_true("!!true",
    vec![RuntimeValue::Boolean(false)],
    Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::double_not_false("!!false",
    vec![RuntimeValue::Boolean(false)],
    Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::plus_equal(r#"
    var i = 1 | i += 10 | i"#,
    vec!["".into()],
    Ok(vec![RuntimeValue::Number(11.into())].into()))]
#[case::minus_equal(r#"
    var i = 10 | i -= 1 | i"#,
    vec!["".into()],
    Ok(vec![RuntimeValue::Number(9.into())].into()))]
#[case::slash_equal(r#"
    var i = 4 | i /= 2 | i"#,
    vec!["".into()],
    Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::star_equal(r#"
    var i = 4 | i *= 2 | i"#,
    vec!["".into()],
    Ok(vec![RuntimeValue::Number(8.into())].into()))]
#[case::percent_equal(r#"
    var i = 3 | i %= 2 | i"#,
    vec!["".into()],
    Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::double_slash_equal(r#"
    var i = 5 | i //= 2 | i"#,
    vec!["".into()],
    Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::set_attr(r#"
    to_markdown("```\ntest\n```") | .code | first() | .code.value |= "updated" | .code.value | to_text()"#,
    vec!["".into()],
    Ok(vec!["updated".into()].into()))]
#[case::variadic_all_args("def f(*args): args; | f(1, 2, 3)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::variadic_with_regular_param("def f(a, *rest): rest; | f(1, 2, 3)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::variadic_empty_rest("def f(a, *rest): rest; | f(1)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::variadic_no_args("def f(*args): args; | f()",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::variadic_with_pipe("def f(a, *rest): [a, rest]; | 10 | f(20, 30)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(20.into()), RuntimeValue::Array(vec![RuntimeValue::Number(30.into())])])].into()))]
#[case::variadic_with_two_regular_params("def f(a, b, *rest): [a, b, rest]; | f(1, 2, 3, 4)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Array(vec![RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])])].into()))]
#[case::variadic_fn_syntax("let g = fn(*args): args; | g(1, 2)",
    vec![RuntimeValue::Number(0.into())],
    Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::is_regex_match_match(r#"is_regex_match("test1", "[a-z0-9]+")"#,
    vec![RuntimeValue::None],
    Ok(vec![true.into()].into()))]
#[case::is_regex_match_no_match(r#"is_regex_match("abc", "[0-9]+")"#,
    vec![RuntimeValue::None],
    Ok(vec![false.into()].into()))]
#[case::is_regex_match_none_input(r#"is_regex_match(., "[a-z]+")"#,
    vec![RuntimeValue::None],
    Ok(vec![false.into()].into()))]
#[case::is_regex_match_markdown(r#"is_regex_match(., "hello")"#,
    vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "hello world".to_string(), position: None }))],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::regex_op(r#""test1" =~ "[a-z0-9]+""#,
    vec![RuntimeValue::None],
    Ok(vec![true.into()].into()))]
#[case::regex_non_match(r#""abc" =~ "[0-9]+""#,
    vec![RuntimeValue::None],
    Ok(vec![false.into()].into()))]
#[case::regex_complex_pattern(r#""abc123XYZ" =~ "[a-z]+[0-9]+[A-Z]+""#,
    vec![RuntimeValue::None],
    Ok(vec![true.into()].into()))]
#[case::regex_none_input(r#". =~ "foo""#,
    vec![RuntimeValue::None],
    Ok(vec![false.into()].into()))]
#[case::regex_markdown_match(r#". =~ "hello""#,
    vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "hello world".to_string(), position: None }))],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::regex_markdown_non_match(r#". =~ "foo""#,
    vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "hello world".to_string(), position: None }))],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "false".to_string() }))].into()))]
#[case::regex_in_if(r#""test" | if (. =~ "test"): true else: false"#,
    vec![RuntimeValue::None],
    Ok(vec![true.into()].into()))]
#[case::shift_left_number("shift_left(1, 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()),)]
#[case::shift_left_number_operator("1 << 2", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()),)]
#[case::shift_right_number("shift_right(4, 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()),)]
#[case::shift_right_number_operator("4 >> 2", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()),)]
#[case::shift_left_array_operator("[1] << 2", vec![RuntimeValue::None], Ok(vec![vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())].into()].into()),)]
#[case::shift_right_array_operator("2 >> [1]", vec![RuntimeValue::None], Ok(vec![vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(1.into())].into()].into()),)]
#[case::shift_right_header_level_h1("to_markdown(\"# Heading 1\") | first() | shift_right(1)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 2,
        values: vec!["Heading 1".to_string().into()],
        position: None
    }))].into()))]
#[case::shift_right_header_level_h1_operator("let md = do to_markdown(\"# Heading 1\") | first(); | md >> 1",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 2,
        values: vec!["Heading 1".to_string().into()],
        position: None
    }))].into()))]
#[case::shift_right_header_level_h6("to_markdown(\"###### Heading 6\") | first() | shift_right(1)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 6,
        values: vec!["Heading 6".to_string().into()],
        position: None
    }))].into()))]
#[case::shift_left_header_level_h2("to_markdown(\"## Heading 2\") | first() | shift_left(1)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Heading 2".to_string().into()],
        position: None
    }))].into()))]
#[case::shift_left_header_level_h2_via_binding("let md = do to_markdown(\"## Heading 2\") | first(); | md << 1",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Heading 2".to_string().into()],
        position: None
    }))].into()))]
#[case::shift_left_header_level_h1("to_markdown(\"# Heading 1\") | first() | shift_left(1)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Heading 1".to_string().into()],
        position: None
    }))].into()))]
#[case::shift_left_string_basic("\"abcdef\" | shift_left(2)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("cdef".to_string())].into()))]
#[case::shift_right_string_basic("\"abcdef\" | shift_right(2)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("abcd".to_string())].into()))]
#[case::shift_left_string_amount_greater_than_length("\"abc\" | shift_left(10)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("".to_string())].into()))]
#[case::shift_right_string_amount_equal_to_length("\"abc\" | shift_right(3)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("".to_string())].into()))]
#[case::convert_string_to_h1_function("convert(\"Hello\", :h1)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_h1_operator("\"Hello\" @ :h1",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_h2_operator("\"Hello\" @ :h2",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 2,
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_h1_via_string_operator("\"Hello\" @ \"#\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_h2_via_string_operator("\"Hello\" @ \"##\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 2,
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_base64_operator("\"text\" @ :base64",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("dGV4dA==".to_string())].into()))]
#[case::convert_string_to_uri_operator("\"hello world\" @ :uri",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("hello%20world".to_string())].into()))]
#[case::convert_string_to_urid_operator("\"hello%20world\" @ :urid",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("hello world".to_string())].into()))]
#[case::convert_string_to_sh_operator("\"hello world\" @ :sh",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("'hello world'".to_string())].into()))]
#[case::convert_string_to_blockquote_operator("\"Hello\" @ \">\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Blockquote(mq_markdown::Blockquote {
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_list_item_operator("\"Hello\" @ \"-\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::List(mq_markdown::List {
        values: vec!["Hello".to_string().into()],
        index: 0,
        ordered: false,
        level: 1,
        checked: None,
        position: None,
    }))].into()))]
#[case::convert_string_to_strikethrough_operator("\"Hello\" @ \"~~\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Delete(mq_markdown::Delete {
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_strong_operator("\"Hello\" @ \"**\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Strong(mq_markdown::Strong {
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::convert_string_to_horizontal_rule_operator("\"Hello\" @ \"--\"",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule {
        position: None,
    }))].into()))]
#[case::skip_while_basic("skip_while([1,2,3,4,5], fn(x): x < 3;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(3.into()),
        RuntimeValue::Number(4.into()),
        RuntimeValue::Number(5.into()),
    ])].into()))]
#[case::skip_while_all_match("skip_while([1,2,3], fn(x): x < 10;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::skip_while_none_match("skip_while([1,2,3], fn(x): x > 10;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
        RuntimeValue::Number(2.into()),
        RuntimeValue::Number(3.into()),
    ])].into()))]
#[case::skip_while_empty_array("skip_while([], fn(x): x < 3;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::skip_while_stops_at_first_non_match("skip_while([1,3,2,4], fn(x): x < 3;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(3.into()),
        RuntimeValue::Number(2.into()),
        RuntimeValue::Number(4.into()),
    ])].into()))]
#[case::take_while_basic("take_while([1,2,3,4,5], fn(x): x < 3;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
        RuntimeValue::Number(2.into()),
    ])].into()))]
#[case::take_while_all_match("take_while([1,2,3], fn(x): x < 10;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
        RuntimeValue::Number(2.into()),
        RuntimeValue::Number(3.into()),
    ])].into()))]
#[case::take_while_none_match("take_while([1,2,3], fn(x): x > 10;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::take_while_empty_array("take_while([], fn(x): x < 3;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::take_while_stops_at_first_non_match("take_while([1,3,2,4], fn(x): x < 3;)",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
    ])].into()))]
#[case::slice_end_only("let x = [1, 2, 3] | x[:2]",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
        RuntimeValue::Number(2.into()),
    ])].into()))]
#[case::slice_end_only_single("let x = [1, 2, 3] | x[:1]",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
    ])].into()))]
#[case::slice_end_only_empty("let x = [1, 2, 3] | x[:0]",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::slice_full_colon("let x = [1, 2, 3] | x[:]",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![
        RuntimeValue::Number(1.into()),
        RuntimeValue::Number(2.into()),
        RuntimeValue::Number(3.into()),
    ])].into()))]
#[case::dict_symbol_access_unchanged("let d = {type: :section} | d[:type]",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Symbol(Ident::new("section"))].into()))]
// --- or-pattern integration tests ---
#[case::match_or_number_first_alt(
    r#"match(1) do | 1 || 2 || 3: "small" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("small".to_string())].into()))]
#[case::match_or_number_second_alt(
    r#"match(2) do | 1 || 2 || 3: "small" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("small".to_string())].into()))]
#[case::match_or_number_third_alt(
    r#"match(3) do | 1 || 2 || 3: "small" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("small".to_string())].into()))]
#[case::match_or_number_no_match_wildcard(
    r#"match(5) do | 1 || 2 || 3: "small" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::match_or_number_no_arm_matches(
    r#"match(9) do | 1 || 2: "one or two" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::None].into()))]
#[case::match_or_string_first_alt(
    r#"match("a") do | "a" || "b": "letter" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("letter".to_string())].into()))]
#[case::match_or_string_second_alt(
    r#"match("b") do | "a" || "b": "letter" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("letter".to_string())].into()))]
#[case::match_or_string_no_match(
    r#"match("c") do | "a" || "b": "letter" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::match_or_bool_true(
    r#"match(true) do | true || false: "bool" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("bool".to_string())].into()))]
#[case::match_or_bool_false(
    r#"match(false) do | true || false: "bool" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("bool".to_string())].into()))]
#[case::match_or_none_literal(
    r#"match(.) do | none || 1: "matched" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("matched".to_string())].into()))]
#[case::match_or_type_string_matches(
    r#"match("hello") do | :string || :bool: "str or bool" | :number: "number" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("str or bool".to_string())].into()))]
#[case::match_or_type_number_matches(
    r#"match(42) do | :string || :bool: "str or bool" | :number: "number" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("number".to_string())].into()))]
#[case::match_or_type_no_match(
    r#"match(42) do | :string || :bool: "str or bool" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::match_or_type_array_or_dict(
    r#"match(array(1,2)) do | :array || :dict: "collection" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("collection".to_string())].into()))]
#[case::match_or_piped_input_first_alt(
    r#"match(.) do | 1 || 2: "one or two" | _: "other" end"#,
    vec![RuntimeValue::Number(1.into())],
    Ok(vec![RuntimeValue::String("one or two".to_string())].into()))]
#[case::match_or_piped_input_second_alt(
    r#"match(.) do | 1 || 2: "one or two" | _: "other" end"#,
    vec![RuntimeValue::Number(2.into())],
    Ok(vec![RuntimeValue::String("one or two".to_string())].into()))]
#[case::match_or_piped_input_no_match(
    r#"match(.) do | 1 || 2: "one or two" | _: "other" end"#,
    vec![RuntimeValue::Number(3.into())],
    Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::match_or_with_guard_passes(
    r#"let x = 5 | match(x) do | 4 || 5 || 6 if (x > 4): "big" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("big".to_string())].into()))]
#[case::match_or_with_guard_fails(
    r#"let x = 4 | match(x) do | 4 || 5 || 6 if (x > 4): "big" | _: "other" end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::match_or_two_alts_with_ident_binding(
    r#"match(3) do | 1 || 2: "small" | x: to_string(x) end"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("3".to_string())].into()))]
#[case::from_html_heading(
    r#""<h1>Hello</h1>" | from_html() | first()"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        depth: 1,
        values: vec!["Hello".to_string().into()],
        position: None,
    }))].into()))]
#[case::from_html_paragraph(
    r#""<p>Hello world</p>" | from_html() | first() | to_text()"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("Hello world".to_string())].into()))]
#[case::from_html_empty(
    r#""" | from_html()"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::pow_integer("pow(2, 3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(8.into())].into()),)]
#[case::pow_zero_exp("pow(5, 0)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()),)]
#[case::pow_negative_exp("pow(2, -1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(0.5f64.into())].into()),)]
#[case::pow_float_exp("pow(4, 0.5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.0f64.into())].into()),)]
#[case::sqrt_perfect_square("sqrt(4)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.0f64.into())].into()),)]
#[case::sqrt_nine("sqrt(9)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.0f64.into())].into()),)]
#[case::sqrt_one("sqrt(1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.0f64.into())].into()),)]
#[case::ln_one("ln(1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(0.0f64.into())].into()),)]
#[case::ln_e("ln(2.718281828459045)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.0f64.into())].into()),)]
#[case::log10_one("log10(1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(0.0f64.into())].into()),)]
#[case::log10_hundred("log10(100)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.0f64.into())].into()),)]
#[case::exp_zero("exp(0)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.0f64.into())].into()),)]
#[case::negate_simple("negate(1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1).into())].into()))]
#[case::div_float("div(5, 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.5.into())].into()))]
#[case::gte_simple("gte(2, 1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::lte_simple("lte(1, 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::ne_simple("ne(1, 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::csv_parse_simple(r##"_csv_parse("a,b\n1,2", ",", true)"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![
    RuntimeValue::Dict(BTreeMap::from([
        (Ident::new("a"), RuntimeValue::String("1".to_string())),
        (Ident::new("b"), RuntimeValue::String("2".to_string())),
    ]))
])].into()))]
#[case::json_parse_simple(r##"_json_parse("{\"a\": 1}")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict(BTreeMap::from([
    (Ident::new("a"), RuntimeValue::Number(1.into())),
]))].into()))]
#[case::yaml_parse_simple(r##"_yaml_parse("a: 1")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict(BTreeMap::from([
    (Ident::new("a"), RuntimeValue::Number(1.into())),
]))].into()))]
#[case::toml_parse_simple(r##"_toml_parse("a = 1")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict(BTreeMap::from([
    (Ident::new("a"), RuntimeValue::Number(1.into())),
]))].into()))]
#[case::xml_parse_simple(r##"_xml_parse("<root>text</root>")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict(BTreeMap::from([
    (Ident::new("tag"), RuntimeValue::String("root".to_string())),
    (Ident::new("attributes"), RuntimeValue::new_dict()),
    (Ident::new("children"), RuntimeValue::Array(vec![])),
    (Ident::new("text"), RuntimeValue::String("text".to_string())),
]))].into()))]
#[case::and_builtin("and(true, false)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::or_builtin("or(false, true)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::coalesce_simple("coalesce(None, 1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::compact_array("compact([1, None, 2])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::uniq_array("uniq([1, 2, 1])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::sort_array("sort([3, 1, 2])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::utf8bytelen_simple(r##"utf8bytelen("あ")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::rindex_simple(r##"rindex("banana", "a")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::explode_simple(r##"explode("abc")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(97.into()), RuntimeValue::Number(98.into()), RuntimeValue::Number(99.into())])].into()))]
#[case::implode_simple(r##"implode([97, 98, 99])"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("abc".to_string())].into()))]
#[case::intern_simple(r##"intern("foo")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("foo".to_string())].into()))]
#[case::nan_builtin("nan() | is_nan()", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::infinite_builtin("infinite() > 0", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::to_md_text_simple(r##"to_md_text("hello")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello".to_string(), position: None}))].into()))]
#[case::to_h_simple(r##"to_h("title", 1)"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading{depth: 1, values: vec!["title".to_string().into()], position: None}))].into()))]
#[case::to_hr_simple("to_hr()", vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::HorizontalRule(mq_markdown::HorizontalRule{position: None}))].into()))]
#[case::to_strong_simple(r##"to_strong("bold")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Strong(mq_markdown::Strong{values: vec!["bold".to_string().into()], position: None}))].into()))]
#[case::to_em_simple(r##"to_em("italic")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis{values: vec!["italic".to_string().into()], position: None}))].into()))]
#[case::to_code_inline_simple(r##"to_code_inline("code")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::CodeInline(mq_markdown::CodeInline{value: "code".to_string().into(), position: None}))].into()))]
#[case::get_title_simple(r##"to_markdown("[link](url 'title')") | first() | get_title()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("title".to_string())].into()))]
#[case::get_url_simple(r##"to_markdown("[link](url)") | first() | get_url()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("url".to_string())].into()))]
#[case::set_code_block_lang_simple(r##"to_markdown("```\ncode\n```") | first() | set_code_block_lang("rust") | .code.lang"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("rust".to_string())].into()))]
#[case::set_list_ordered_simple(r##"to_markdown("- item") | first() | set_list_ordered(true) | .list.ordered"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::diff_simple(r##"_diff("abc", "abd") | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::get_markdown_position_simple(r##"to_markdown("# title") | first() | _get_markdown_position() | get("start_line")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::toon_parse_simple(r##"_toon_parse("a: 1")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict(BTreeMap::from([
    (Ident::new("a"), RuntimeValue::Number(1.into())),
]))].into()))]
#[case::capture_simple(r##"capture("abc123def", "(?P<num>\\d+)")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict(BTreeMap::from([
    (Ident::new("num"), RuntimeValue::String("123".to_string())),
]))].into()))]
#[case::is_debug_mode_simple("is_debug_mode()", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(cfg!(feature = "debugger"))].into()))]
#[case::set_check_simple(r##"to_markdown("- [ ] task") | first() | set_check(true) | .list.checked"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::stderr_simple(r##"stderr("test stderr")"##, vec![RuntimeValue::String("val".to_string())], Ok(vec![RuntimeValue::String("val".to_string())].into()))]
#[case::to_image_simple(r##"to_image("url", "alt", "title")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Image(mq_markdown::Image{url: "url".to_string(), alt: "alt".to_string(), title: Some("title".to_string()), position: None}))].into()))]
#[case::to_link_simple(r##"to_link("url", "text", "title")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Link(mq_markdown::Link{url: mq_markdown::Url::new("url".to_string()), title: Some(mq_markdown::Title::new("title".to_string())), values: vec!["text".to_string().into()], position: None}))].into()))]
#[case::to_math_simple(r##"to_math("E=mc^2")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::Math(mq_markdown::Math{value: "E=mc^2".to_string(), position: None}))].into()))]
#[case::to_math_inline_simple(r##"to_math_inline("E=mc^2")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::MathInline(mq_markdown::MathInline{value: "E=mc^2".to_string().into(), position: None}))].into()))]
#[case::to_md_list_simple(r##"to_md_list("item", 1)"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::List(mq_markdown::List{values: vec!["item".to_string().into()], index: 0, ordered: false, level: 1, checked: None, position: None}))].into()))]
#[case::to_md_table_row_simple(r##"to_md_table_row("a", "b")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{values: vec![
    mq_markdown::Node::TableCell(mq_markdown::TableCell{row: 0, column: 0, values: vec!["a".to_string().into()], position: None}),
    mq_markdown::Node::TableCell(mq_markdown::TableCell{row: 0, column: 1, values: vec!["b".to_string().into()], position: None}),
], position: None}))].into()))]
#[case::to_md_table_cell_simple(r##"to_md_table_cell("val", 1, 2)"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::TableCell(mq_markdown::TableCell{row: 1, column: 2, values: vec!["val".to_string().into()], position: None}))].into()))]
#[case::to_md_name_simple(r##"to_markdown("# title") | first() | to_md_name()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("h1".to_string())].into()))]
#[case::entries_simple(r##"entries({"a": 1})"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::Number(1.into())])])].into()))]
#[case::del_array_simple("del([1, 2, 3], 1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::index_string_simple(r##"index("hello", "e")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::set_ref_simple(r##"to_markdown("[link][id]\n\n[id]: url") | first() | set_ref("newlabel") | .link_ref.label"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("newlabel".to_string())].into()))]
#[case::downcase_simple(r##"downcase("ABC")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("abc".to_string())].into()))]
#[case::gsub_simple(r##"gsub("a1b2", "\\d", "x")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("axbx".to_string())].into()))]
#[case::regex_match_simple(r##"regex_match("a1b2", "\\d")"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("1".to_string()), RuntimeValue::String("2".to_string())])].into()))]
#[case::slice_simple(r##"slice("abcdef", 1, 4)"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("bcd".to_string())].into()))]
#[case::sort_by_impl_simple(r##"_sort_by_impl([[2, "b"], [1, "a"]])"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![
    RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::String("a".to_string())]),
    RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::String("b".to_string())]),
])].into()))]
#[case::selector_task(r##"to_markdown("- [ ] todo\n- [x] done") | .task | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::selector_todo(r##"to_markdown("- [ ] todo\n- [x] done") | .todo | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::selector_done(r##"to_markdown("- [ ] todo\n- [x] done") | .done | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::selector_mdx_jsx_flow(r##"to_mdx("<Component />") | .mdx_jsx_flow_element | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::selector_mdx_flow_expr(r##"to_mdx("{1 + 1}") | .mdx_flow_expression | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::selector_footnote(r##"to_markdown("[^1]: note\n\n# title") | .footnote | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::selector_definition(r##"to_markdown("[id]: url") | .definition | compact() | len()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::selector_dict_empty_returns_none("{} | .h | is_none()", vec![RuntimeValue::None], Ok(vec![RuntimeValue::TRUE].into()))]
#[case::selector_dict_with_markdown_values(r##"{"docs": to_markdown("# Title\n\ntext")} | .h | is_dict()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::TRUE].into()))]
#[case::selector_dict_preserves_type_key(r##"{"type": "mytype"} | .h | entries() | first() | last()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("mytype".to_string())].into()))]
#[case::selector_array_of_dicts_preserves_dict(r##"array({"docs": to_markdown("# Title")}) | .h | first() | is_dict()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::TRUE].into()))]
#[case::selector_call_h(r##"to_markdown("# h1\n\n## h2\n\ntest") | .h(2).depth | compact() | first()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::selector_call_code_lang(r##"to_markdown("```rust\ncode\n```") | .code("rust").lang | compact() | first()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("rust".to_string())].into()))]
#[case::selector_bracket_variable_list(r##"let v = 1 | to_markdown("- a\n- b\n- c") | .[v] | compact() | first() | to_string()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("- b".to_string())].into()))]
#[case::selector_bracket_variable_table_row(r##"let v = 1 | to_markdown("| a | b |\n|---|---|\n| c | d |") | .[v][] | compact() | first() | to_string()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("c".to_string())].into()))]
#[case::selector_bracket_variable_table_col(r##"let v = 1 | to_markdown("| a | b |\n|---|---|\n| c | d |") | .[][v] | compact() | first() | to_string()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("b".to_string())].into()))]
#[case::selector_bracket_expr_list(r##"let v = 0 | to_markdown("- a\n- b\n- c") | .[v + 1] | compact() | first() | to_string()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("- b".to_string())].into()))]
#[case::selector_bracket_expr_table_col(r##"let v = 0 | to_markdown("| a | b |\n|---|---|\n| c | d |") | .[][v + 1] | compact() | first() | to_string()"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("b".to_string())].into()))]
#[case::module_inline_function("
    module math:
        def mysum(a, b): a + b;
    end
    | math::mysum(1, 2)
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::module_inline_let("
    module constants:
        let pi = 314
    end
    | constants::pi
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(314.into())].into()))]
#[case::module_hoisting("
    def call_math(): math::mysum(10, 5);
    | call_math()
    | module math:
        def mysum(a, b): a + b;
    end
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(15.into())].into()))]
#[case::module_extension("
    module math:
        def mysum(a, b): a + b;
    end
    module math:
        def mymul(a, b): a * b;
    end
    | math::mysum(2, 3) + math::mymul(2, 3)
    ",
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::Number(11.into())].into()))]
// partial: explicitly create a partial function with the first arg pre-filled
#[case::partial_basic("def f(a, b, c): c; | let p = partial(f, 10) | p(42)", vec![RuntimeValue::Number(5.into())], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// partial: the pre-filled arg is accessible in the body of the partial function
#[case::partial_captured_arg("def f(a, b, c): a; | let p = partial(f, 42) | p(99, 1)", vec![RuntimeValue::Number(5.into())], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// partial: pre-fill multiple args and verify they combine correctly in the final call
#[case::partial_multiple_args("def f(a, b, c): a + b + c; | let p = partial(f, 10, 20) | p(30)", vec![RuntimeValue::Number(5.into())], Ok(vec![RuntimeValue::Number(60.into())].into()))]
// partial: 2-param function can be partially applied — the scenario that triggered the redesign
#[case::partial_two_param("def plus(a, b): a + b; | let plus10 = partial(plus, 10) | plus10(5)", vec![RuntimeValue::Number(0.into())], Ok(vec![RuntimeValue::Number(15.into())].into()))]
// property selector: quoted form (."key") is the only way to access dict keys
#[case::property_selector_quoted_h1(r#"."h1""#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("h1"), RuntimeValue::String("title".to_string())); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::String("title".to_string())].into()))]
#[case::property_selector_quoted_url(r#"."url""#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("url"), RuntimeValue::String("https://example.com".to_string())); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::String("https://example.com".to_string())].into()))]
#[case::property_selector_quoted_text(r#"."text""#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("text"), RuntimeValue::String("hello".to_string())); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
// property selector: quoted form with spaces in key
#[case::property_selector_quoted_space(r#"."my key""#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("my key"), RuntimeValue::String("val".to_string())); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::String("val".to_string())].into()))]
// property selector: missing key returns None
#[case::property_selector_quoted_missing(r#"."h1""#, vec![{let d = std::collections::BTreeMap::new(); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::None].into()))]
// nested property selector: ."a"."b" accesses {"a": {"b": 1}}
#[case::property_selector_nested(r#"."a"."b""#, vec![{let mut outer = std::collections::BTreeMap::new(); let mut inner = std::collections::BTreeMap::new(); inner.insert(Ident::new("b"), RuntimeValue::Number(1.into())); outer.insert(Ident::new("a"), RuntimeValue::Dict(inner)); RuntimeValue::Dict(outer)}], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// nested property selector: ."a"."b"."c" accesses three levels deep
#[case::property_selector_nested_three(r#"."a"."b"."c""#, vec![{let mut outer = std::collections::BTreeMap::new(); let mut mid = std::collections::BTreeMap::new(); let mut inner = std::collections::BTreeMap::new(); inner.insert(Ident::new("c"), RuntimeValue::Number(42.into())); mid.insert(Ident::new("b"), RuntimeValue::Dict(inner)); outer.insert(Ident::new("a"), RuntimeValue::Dict(mid)); RuntimeValue::Dict(outer)}], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// nested property selector: missing intermediate key returns None
#[case::property_selector_nested_missing(r#"."a"."b""#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("a"), RuntimeValue::Number(1.into())); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::None].into()))]
// property iterator: ."items"[] iterates all elements of the array stored at the key
#[case::property_selector_iterator(r#"."items"[]"#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("items"), RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string()), RuntimeValue::String("c".to_string())])); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string()), RuntimeValue::String("c".to_string())])].into()))]
// property iterator with index: ."items"[0] accesses the first element of the array
#[case::property_selector_iterator_index(r#"."items"[0]"#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("items"), RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::String("a".to_string())].into()))]
// property iterator with index: ."items"[1] accesses the second element
#[case::property_selector_iterator_index_1(r#"."items"[1]"#, vec![{let mut d = std::collections::BTreeMap::new(); d.insert(Ident::new("items"), RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])); RuntimeValue::Dict(d)}], Ok(vec![RuntimeValue::String("b".to_string())].into()))]
// chained property iterator: ."a"."b"[] iterates all elements of a nested array
#[case::property_selector_nested_iterator(r#"."a"."b"[]"#, vec![{let mut outer = std::collections::BTreeMap::new(); let mut inner = std::collections::BTreeMap::new(); inner.insert(Ident::new("b"), RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])); outer.insert(Ident::new("a"), RuntimeValue::Dict(inner)); RuntimeValue::Dict(outer)}], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
// paren-free calls: 0-arg user-defined function called without parentheses
#[case::paren_free_zero_arg_user_fn("def greet(): \"Hello!\"; | greet", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("Hello!".to_string())].into()))]
// paren-free calls: 1-arg user-defined function called without parentheses uses current value
#[case::paren_free_one_arg_user_fn("def double(x): x * 2; | double", vec![RuntimeValue::Number(5.into())], Ok(vec![RuntimeValue::Number(10.into())].into()))]
// paren-free calls: 1-arg function with 1 default param — pipeline value bound to required param
#[case::paren_free_one_required_one_default("def inc(x, step = 1): x + step; | inc", vec![RuntimeValue::Number(10.into())], Ok(vec![RuntimeValue::Number(11.into())].into()))]
// paren-free calls: 1-arg builtin (len) called without parentheses uses the current pipeline value implicitly
#[case::paren_free_zero_arg_builtin("compact([1, None, 2]) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// paren-free calls: 1-arg builtin (to_string) called without parentheses uses current value
#[case::paren_free_one_arg_builtin_to_string("42 | to_string", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("42".to_string())].into()))]
// paren-free calls: upcase builtin
#[case::paren_free_builtin_upcase("\"hello\" | upcase", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("HELLO".to_string())].into()))]
// paren-free calls: downcase builtin
#[case::paren_free_builtin_downcase("\"HELLO\" | downcase", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
// paren-free calls: trim builtin
#[case::paren_free_builtin_trim("\"  hello  \" | trim", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
// paren-free calls: to_number builtin
#[case::paren_free_builtin_to_number("\"42\" | to_number", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// paren-free calls: chained builtin paren-free calls
#[case::paren_free_builtin_chained("\"  HELLO  \" | trim | downcase", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
// paren-free calls: chained pipeline with multiple paren-free user functions
#[case::paren_free_chained_user_fns("def double(x): x * 2; | def inc(x): x + 1; | 5 | double | inc", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(11.into())].into()))]
// paren-free calls: mix of paren-free user fn and builtin in one pipeline
#[case::paren_free_mixed_user_and_builtin("def double(x): x * 2; | 21 | double | to_string", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("42".to_string())].into()))]
// paren-free calls: variable access still works correctly (not auto-called)
#[case::paren_free_variable_not_called("let x = 42 | x", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// paren-free calls: passing function as value to map still works (no spurious auto-call)
#[case::paren_free_fn_as_value_preserved("map([\"a\", \"b\"], upcase)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("A".to_string()), RuntimeValue::String("B".to_string())])].into()))]
// paren-free calls: passing user-defined function as value to map (no spurious auto-call)
#[case::paren_free_user_fn_as_value_preserved("def double(x): x * 2; | map([1, 2, 3], double)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(6.into())])].into()))]
// shadowing builtin: user-defined function with same name as builtin calls the native builtin inside its body
#[case::shadow_builtin_upcase("def upcase: upcase() | ltrimstr(\"HELLO\"); | \"hello\" | upcase", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("".to_string())].into()))]
// shadowing builtin: user function wraps builtin and adds extra transformation
#[case::shadow_builtin_with_extra("def upcase(x): upcase(x) | ltrimstr(\"HELLO\"); | upcase(\"hello\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("".to_string())].into()))]
// shadowing builtin: outer scope sees user-defined function
#[case::shadow_builtin_outer_scope("def upcase: upcase(); | \"world\" | upcase", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("WORLD".to_string())].into()))]
// gmtime: Unix epoch → UTC broken-down array [year, mon(0-11), mday, hour, min, sec, wday(0=Sun), yday(0-365)]
#[case::gmtime_epoch("gmtime(0)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![
    RuntimeValue::Number(1970.into()), // year
    RuntimeValue::Number(0.into()),    // mon (Jan=0)
    RuntimeValue::Number(1.into()),    // mday
    RuntimeValue::Number(0.into()),    // hour
    RuntimeValue::Number(0.into()),    // min
    RuntimeValue::Number(0.into()),    // sec
    RuntimeValue::Number(4.into()),    // wday (Thu=4)
    RuntimeValue::Number(0.into()),    // yday
])].into()))]
#[case::gmtime_known("gmtime(1704067200)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![
    RuntimeValue::Number(2024.into()), // year
    RuntimeValue::Number(0.into()),    // mon (Jan=0)
    RuntimeValue::Number(1.into()),    // mday
    RuntimeValue::Number(0.into()),    // hour
    RuntimeValue::Number(0.into()),    // min
    RuntimeValue::Number(0.into()),    // sec
    RuntimeValue::Number(1.into()),    // wday (Mon=1)
    RuntimeValue::Number(0.into()),    // yday
])].into()))]
// mktime: broken-down UTC array → Unix timestamp (seconds)
#[case::mktime_epoch("mktime(array(1970, 0, 1, 0, 0, 0, 4, 0))", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(0.into())].into()))]
#[case::mktime_known("mktime(array(2024, 0, 1, 0, 0, 0, 1, 0))", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1704067200_i64.into())].into()))]
// gmtime | mktime roundtrip
#[case::gmtime_mktime_roundtrip("gmtime(1704067200) | mktime", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1704067200_i64.into())].into()))]
// localtime: returns array of 8 elements
#[case::localtime_len("len(localtime(0))", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(8.into())].into()))]
// localtime: year element is 1970 (UTC+0 to UTC+14 all land on 1970-01-01 for ts=0; UTC-12 still 1969 so we check >=1969)
#[case::localtime_year_type("type(localtime(0))", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("array".to_string())].into()))]
// strftime: format timestamp as date string (UTC)
#[case::strftime_date("strftime(1704067200, \"%Y-%m-%d\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("2024-01-01".to_string())].into()))]
#[case::strftime_datetime("strftime(0, \"%Y-%m-%dT%H:%M:%S\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("1970-01-01T00:00:00".to_string())].into()))]
#[case::strftime_year("strftime(1704067200, \"%Y\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("2024".to_string())].into()))]
// now | gmtime / strftime pipeline
#[case::now_gmtime_len("now | gmtime | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(8.into())].into()))]
#[case::now_strftime_len("now | strftime(\"%Y-%m-%d\") | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(10.into())].into()))]
// date_add: add days → result is array of length 8
#[case::date_add_days_len("gmtime(1704067200) | date_add(1, \"days\") | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(8.into())].into()))]
// date_add: add days then mktime = original + 86400
#[case::date_add_days_roundtrip("gmtime(1704067200) | date_add(1, \"days\") | mktime", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1704153600_i64.into())].into()))]
// date_add: add months calendar-aware (2024-01-31 + 1 month = 2024-02-29)
#[case::date_add_month_clamp("gmtime(1706659200) | date_add(1, \"months\") | mktime", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1709164800_i64.into())].into()))]
// date_add: add years calendar-aware (2024-02-29 + 1 year = 2025-02-28)
#[case::date_add_year_clamp("gmtime(1709164800) | date_add(1, \"years\") | mktime", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1740700800_i64.into())].into()))]
// date_add: pipeline now | gmtime | date_add returns array
#[case::date_add_now_pipeline("now | gmtime | date_add(7, \"days\") | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(8.into())].into()))]
// date_diff: 1 day apart
#[case::date_diff_days("date_diff(gmtime(1704067200), gmtime(1704153600), \"days\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// date_diff: 24 hours apart
#[case::date_diff_hours("date_diff(gmtime(1704067200), gmtime(1704153600), \"hours\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(24.into())].into()))]
// date_diff: negative (reversed order)
#[case::date_diff_negative("date_diff(gmtime(1704153600), gmtime(1704067200), \"days\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1_i64).into())].into()))]
// date_diff: same → 0
#[case::date_diff_zero("date_diff(gmtime(1704067200), gmtime(1704067200), \"seconds\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(0.into())].into()))]
// byte string literals
#[case::bytes_literal_basic(r#"b"abc""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![97, 98, 99])].into()))]
#[case::bytes_literal_hex_escape(r#"b"\xf0\x9f\x99\x82""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![0xf0, 0x9f, 0x99, 0x82])].into()))]
#[case::bytes_literal_escape_sequences(r#"b"\n\r\t\\""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![b'\n', b'\r', b'\t', b'\\'])].into()))]
#[case::bytes_literal_empty(r#"b"""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![])].into()))]
#[case::bytes_literal_len(r#"b"abc" | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::bytes_literal_len_hex(r#"b"\xf0\x9f\x99\x82" | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()))]
#[case::bytes_literal_equality(r#"b"abc" == b"abc""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::bytes_literal_equality_false(r#"b"abc" == b"def""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::bytes_literal_inequality(r#"b"abc" != b"def""#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::bytes_literal_type_name(r#"type(b"abc")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("bytes".to_string())].into()))]
#[case::bytes_literal_is_empty(r#"is_empty(b"")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::bytes_literal_not_empty(r#"is_empty(b"a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
// as binding: bind value to name and access it later
#[case::as_binding_basic("42 as x | x", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// as binding: bind returns original pipeline value (not bound value)
#[case::as_binding_passthrough("42 as x | \"hello\"", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
// as binding: bind literal to name, use in expression
#[case::as_binding_in_expression("1 as a | 2 as b | add(a, b)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// as binding: bind selector result to name
#[case::as_binding_selector("let v = \"hello\" | v as s | upcase(s)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("HELLO".to_string())].into()))]
// as binding: multiple bindings in same pipeline
#[case::as_binding_multiple("1 as a | 2 as b | add(a, b)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// as binding: bind in def body
#[case::as_binding_in_def("def double_add(x): x as a | add(a, a); | double_add(5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(10.into())].into()))]
// try/catch: try expression succeeds → returns try result
#[case::try_success("try: 42 catch: 0", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// try/catch: try expression fails → falls through to catch
#[case::try_catch_on_error("try: undefined_func() catch: 99", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(99.into())].into()))]
// try without catch: try expression succeeds → returns result
#[case::try_no_catch_success("try: 1 + 1", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// try without catch: try expression fails → returns None
#[case::try_no_catch_failure("try: undefined_func()", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// try/catch: nested try blocks
#[case::try_nested("try: (try: undefined_func() catch: \"inner\") catch: \"outer\"", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("inner".to_string())].into()))]
// foreach over string: iterates each character
#[case::foreach_string("foreach(c, \"abc\"): c;", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string()), RuntimeValue::String("c".to_string())])].into()))]
// foreach over string with break
#[case::foreach_string_break("foreach(c, \"abcde\"): if(c == \"c\"): break else: c;", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])].into()))]
// foreach over string with continue
#[case::foreach_string_continue("foreach(c, \"abc\"): if(c == \"b\"): continue else: c;", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("c".to_string())])].into()))]
// foreach over string: break with value
#[case::foreach_string_break_value("foreach(c, \"abc\"): if(c == \"b\"): break: \"found\" else: c;", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("found".to_string())].into()))]
// pattern destructuring in let: array pattern
#[case::let_array_destruct("let [a, b] = [1, 2] | add(a, b)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// pattern destructuring in let: array with wildcard
#[case::let_array_wildcard("let [_, b] = [1, 2] | b", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// pattern destructuring in let: array rest pattern
#[case::let_array_rest("let [first, ..rest] = [1, 2, 3] | len(rest)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// pattern destructuring in let: dict pattern
#[case::let_dict_destruct(r#"let {name: n} = {"name": "Alice"} | n"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("Alice".to_string())].into()))]
// pattern destructuring in var: array pattern
#[case::var_array_destruct("var [a, b] = [10, 20] | a + b", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(30.into())].into()))]
// match: array pattern
#[case::match_array_pattern("match([1, 2, 3]) do | [a, b, c]: a + b + c end", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(6.into())].into()))]
// match: array rest pattern in match arm
#[case::match_array_rest_pattern("match([1, 2, 3, 4]) do | [first, ..rest]: len(rest) end", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// match: dict pattern in match arm
#[case::match_dict_pattern(r#"match({"x": 10}) do | {x: v}: v end"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(10.into())].into()))]
// match: no arm matches → None
#[case::match_no_match("match(42) do | 0: \"zero\" end", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// match: wildcard in arm
#[case::match_wildcard_arm(r#"match("anything") do | _: "matched" end"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("matched".to_string())].into()))]
// repeat: string repeated N times
#[case::repeat_string("\"ab\" * 3", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ababab".to_string())].into()))]
// repeat: array repeated N times
#[case::repeat_array("[1, 2] * 3", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
// repeat: array * 0 returns empty
#[case::repeat_array_zero("[1, 2] * 0", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![])].into()))]
// intern: non-string arg (number)
#[case::intern_non_string("intern(42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("42".to_string())].into()))]
// is_nan: non-number returns false
#[case::is_nan_non_number("is_nan(\"not a number\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
// to_markdown: returns array of markdown nodes
#[case::to_markdown_call(r##"to_markdown("# title") | len"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// to_mdx: returns array
#[case::to_mdx_call(r##"to_mdx("<div />") | len"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// all_symbols: returns array of symbols
#[case::all_symbols_call("all_symbols() | type", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("array".to_string())].into()))]
// loop: breaks immediately
#[case::loop_immediate_break("loop: break: 42", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// loop: increment counter until break
#[case::loop_counter("var i = 0 | loop: i += 1 | if(i >= 3): break: i", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// quote: returns AST node type
#[case::quote_returns_ast("type(quote: 42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ast".to_string())].into()))]
// unquote inside quote resolves the binding
#[case::quote_unquote("let x = 5 | type(quote: unquote(x))", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ast".to_string())].into()))]
// base64 encode/decode roundtrip
#[case::base64_encode(r#"base64("hello")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("aGVsbG8=".to_string())].into()))]
#[case::base64_decode(r#"base64d("aGVsbG8=")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
#[case::base64_roundtrip(r#"base64d(base64("hello world"))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello world".to_string())].into()))]
#[case::base64url_encode(r#"base64url("hello") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
#[case::base64url_roundtrip(r#"base64urld(base64url("hello world"))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello world".to_string())].into()))]
// base64 with bytes
#[case::base64_bytes(r#"base64(b"\x48\x69") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// md5 hash
#[case::md5_len(r#"md5("hello") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(32.into())].into()))]
#[case::md5_type(r#"type(md5("hello"))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// sha256 hash
#[case::sha256_len(r#"sha256("hello") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(64.into())].into()))]
// sha512 hash
#[case::sha512_len(r#"sha512("hello") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(128.into())].into()))]
// hex encoding
#[case::to_hex_basic(r#"to_hex(b"\xde\xad")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dead".to_string())].into()))]
#[case::from_hex_len(r#"from_hex("deadbeef") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()))]
#[case::hex_roundtrip(r#"to_hex(from_hex("deadbeef"))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("deadbeef".to_string())].into()))]
// utf8: bytes to string
#[case::utf8_basic(r#"utf8(b"hello")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
// bitwise byte operations
#[case::xor_bytes(r#"xor(b"\xff", b"\xf0")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![0x0f])].into()))]
#[case::band_bytes(r#"band(b"\xff", b"\x0f")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![0x0f])].into()))]
#[case::bor_bytes(r#"bor(b"\xf0", b"\x0f")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![0xff])].into()))]
#[case::bnot_bytes(r#"bnot(b"\x00")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![0xff])].into()))]
// pack / unpack
#[case::pack_u8(r#"pack("u8", 255)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![0xff])].into()))]
#[case::pack_unpack_roundtrip(r#"unpack("u8", pack("u8", 42))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
#[case::pack_u16be(r#"pack("u16be", 256) | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// min / max
#[case::min_numbers("min(3, 5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::min_numbers_reverse("min(5, 3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::max_numbers("max(3, 5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::max_numbers_reverse("max(5, 3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::min_strings(r#"min("apple", "banana")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("apple".to_string())].into()))]
#[case::max_strings(r#"max("apple", "banana")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("banana".to_string())].into()))]
#[case::min_with_none("min(None, 5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
#[case::max_with_none("max(None, 5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// to_array conversions
#[case::to_array_string(r#"to_array("ab")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::String("a".to_string()), RuntimeValue::String("b".to_string())])].into()))]
#[case::to_array_number("to_array(42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(42.into())])].into()))]
#[case::to_array_bytes(r#"to_array(b"ab")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(97.into()), RuntimeValue::Number(98.into())])].into()))]
#[case::to_array_none("to_array(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::to_array_already_array("to_array([1, 2])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
// to_bytes conversions
#[case::to_bytes_string(r#"to_bytes("hi") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::to_bytes_bytes(r#"to_bytes(b"hi") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::to_bytes_array("to_bytes([72, 101, 108])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![72, 101, 108])].into()))]
// url_encode
#[case::url_encode_spaces(r#"url_encode("hello world")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello%20world".to_string())].into()))]
#[case::url_encode_plain(r#"url_encode("abc")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("abc".to_string())].into()))]
// to_number conversion
#[case::to_number_string(r#"to_number("42")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// to_html conversion
#[case::to_html_string(r#"to_html("hello") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// to_text conversion
#[case::to_text_string(r#"to_text("hello")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
#[case::to_text_number(r#"to_text(42)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("42".to_string())].into()))]
// to_markdown_string
#[case::to_markdown_string_call(r#"to_markdown_string("hello") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// from_date
#[case::from_date_rfc3339(r#"type(from_date("2024-01-01T00:00:00Z"))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("number".to_string())].into()))]
// while: continue on first iteration (first=true path)
#[case::while_continue_first("var i = 0 | while(i < 3): i += 1 | if(i == 1): continue else: i;", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// while: break with no previous value (first=true path)
#[case::while_break_immediately("while(true): break;", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// del: remove character from string
#[case::del_string(r#"del("hello", 1)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hllo".to_string())].into()))]
// del: None returns None
#[case::del_none("del(None, 0)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// del: remove key from dict by string
#[case::del_dict_string(r#"del({"a": 1, "b": 2}, "a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Dict({let mut m = std::collections::BTreeMap::new(); m.insert(mq_lang::Ident::new("b"), RuntimeValue::Number(2.into())); m})].into()))]
// index: bytes haystack
#[case::index_bytes(r#"index(b"hello", b"ll")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// index: bytes not found
#[case::index_bytes_not_found(r#"index(b"hello", b"xyz")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1).into())].into()))]
// index: array element
#[case::index_array("index([1, 2, 3], 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// index: array element not found
#[case::index_array_not_found("index([1, 2, 3], 99)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1).into())].into()))]
// index: None returns -1
#[case::index_none(r#"index(None, "a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1).into())].into()))]
// rindex: bytes
#[case::rindex_bytes(r#"rindex(b"abab", b"ab")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// rindex: array of strings
#[case::rindex_array(r#"rindex(["a", "b", "a"], "a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// rindex: None returns -1
#[case::rindex_none(r#"rindex(None, "a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1).into())].into()))]
// slice: array with positive indices
#[case::slice_array("slice([1, 2, 3, 4, 5], 1, 4)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])].into()))]
// slice: array with out-of-bounds → empty
#[case::slice_array_empty("slice([1, 2, 3], 5, 10)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![])].into()))]
// slice: string with negative indices
#[case::slice_string_negative(r#"slice("hello", -3, -1)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ll".to_string())].into()))]
// slice: bytes
#[case::slice_bytes(r#"slice(b"hello", 1, 4)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![b'e', b'l', b'l'])].into()))]
// slice: bytes empty (out of bounds)
#[case::slice_bytes_empty(r#"slice(b"hello", 10, 20)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Bytes(vec![])].into()))]
// slice: None returns None
#[case::slice_none("slice(None, 0, 1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// update: update markdown value
#[case::update_markdown_with_string("update(None, \"new_val\")", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// index: string not found → -1
#[case::index_string_not_found(r#"index("hello", "xyz")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1).into())].into()))]
// partial: create partial function application
#[case::partial_basic("def add(x, y): x + y; | let add5 = partial(add, 5) | add5(3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(8.into())].into()))]
// coalesce: first non-None value
#[case::coalesce_first_none("coalesce(None, 42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
#[case::coalesce_first_value("coalesce(10, 42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(10.into())].into()))]
// to_date: format unix timestamp as string
#[case::to_date_basic("to_date(0, \"%Y\") | type", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// math: ceil
#[case::ceil_basic("ceil(2.3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.0f64.into())].into()))]
#[case::ceil_negative("ceil(-2.7)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-2.0f64).into())].into()))]
// math: floor
#[case::floor_basic("floor(2.7)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.0f64.into())].into()))]
#[case::floor_negative("floor(-2.3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-3.0f64).into())].into()))]
// math: round
#[case::round_basic("round(2.5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.0f64.into())].into()))]
#[case::round_down("round(2.4)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.0f64.into())].into()))]
// math: trunc
#[case::trunc_basic("trunc(2.9)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.0f64.into())].into()))]
#[case::trunc_negative("trunc(-2.9)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-2.0f64).into())].into()))]
// math: abs
#[case::abs_positive("abs(5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::abs_negative("abs(-5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// split: array split
#[case::split_array("split([1, 2, 3, 2, 4], 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into())]), RuntimeValue::Array(vec![RuntimeValue::Number(3.into())]), RuntimeValue::Array(vec![RuntimeValue::Number(4.into())])  ])].into()))]
#[case::split_array_no_match("split([1, 2, 3], 99)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])])].into()))]
#[case::split_array_empty("split([], 1)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Array(vec![])])].into()))]
// negate: negate a number
#[case::negate_positive("negate(5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-5).into())].into()))]
#[case::negate_negative("negate(-5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// join: join array elements with separator
#[case::join_basic(r#"join(["a", "b", "c"], ", ")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("a, b, c".to_string())].into()))]
#[case::join_empty(r#"join([], "-")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("".to_string())].into()))]
#[case::join_single(r#"join(["only"], "-")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("only".to_string())].into()))]
#[case::join_empty_sep(r#"join(["a", "b"], "")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ab".to_string())].into()))]
// reverse: reverse array, string, bytes
#[case::reverse_array("reverse([1, 2, 3])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(3.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(1.into())])].into()))]
#[case::reverse_string(r#"reverse("abc")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("cba".to_string())].into()))]
#[case::reverse_string_empty(r#"reverse("")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("".to_string())].into()))]
#[case::reverse_bytes(r#"reverse(b"\x01\x02\x03") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::reverse_array_empty("reverse([])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![])].into()))]
// flatten: flatten nested arrays
#[case::flatten_basic("flatten([1, [2, 3], 4])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(4.into())])].into()))]
#[case::flatten_already_flat("flatten([1, 2, 3])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::flatten_empty("flatten([])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![])].into()))]
#[case::flatten_non_array("flatten(42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// insert: insert into array, string, dict
#[case::insert_array_middle("insert([1, 2, 3], 1, 99)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(99.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::insert_array_begin("insert([1, 2], 0, 0)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(0.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::insert_string(r#"insert("hllo", 1, "e")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
#[case::insert_dict(r#"insert({"a": 1}, "b", 2) | get("b")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// basename, dirname, extname, stem, path_join: path utilities
#[case::basename_basic(r#"basename("/path/to/file.txt")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("file.txt".to_string())].into()))]
#[case::basename_no_dir(r#"basename("file.txt")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("file.txt".to_string())].into()))]
#[case::dirname_basic(r#"dirname("/path/to/file.txt")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("/path/to".to_string())].into()))]
#[case::dirname_no_dir(r#"dirname("file.txt")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String(".".to_string())].into()))]
#[case::extname_basic(r#"extname("file.txt")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String(".txt".to_string())].into()))]
#[case::extname_no_ext(r#"extname("file")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("".to_string())].into()))]
#[case::stem_basic(r#"stem("file.txt")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("file".to_string())].into()))]
#[case::stem_no_ext(r#"stem("file")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("file".to_string())].into()))]
#[case::path_join_basic(r#"path_join("/path", "file.txt") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// add: various type combinations
#[case::add_string_number(r#"add("hello", 42)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello42".to_string())].into()))]
#[case::add_number_string(r#"add(42, "!")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("!42".to_string())].into()))]
#[case::add_array_value("add([1, 2], 3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])].into()))]
#[case::add_value_array("add(0, [1, 2])", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(0.into()), RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])].into()))]
#[case::add_none_number("add(None, 5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::add_number_none("add(5, None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::add_bytes(r#"add(b"\x01", b"\x02") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::add_dict_dict(r#"add({"a": 1}, {"b": 2}) | get("a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// get: string index access
#[case::get_string_index(r#"get("hello", 0)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("h".to_string())].into()))]
#[case::get_string_negative_index(r#"get("hello", -1)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("o".to_string())].into()))]
#[case::get_string_out_of_bounds(r#"get("hi", 99)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
#[case::get_array_negative(r#"get([1, 2, 3], -1)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
#[case::get_none_key(r#"get(None, "x")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// set: array out-of-bounds extends
#[case::set_array_extend("set([1, 2], 4, 99) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// repeat: via builtin
#[case::repeat_string_builtin(r#"repeat("ab", 3)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ababab".to_string())].into()))]
// update: non-None non-Markdown returns second value
#[case::update_non_markdown_returns_value("update(42, 99)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(99.into())].into()))]
// match expression
#[case::match_basic(r#"match(1) do | 1: "one" | 2: "two" | _: "other" end"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("one".to_string())].into()))]
#[case::match_wildcard(r#"match(99) do | 1: "one" | _: "other" end"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("other".to_string())].into()))]
#[case::match_string(r#"match("hello") do | "hello": 1 | _: 0 end"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
#[case::match_no_arm_returns_none(r#"match(99) do | 1: "one" | 2: "two" end"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// foreach: iteration produces array of results
#[case::foreach_sum("foreach(x, [1, 2, 3]): x * 2", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(2.into()), RuntimeValue::Number(4.into()), RuntimeValue::Number(6.into())])].into()))]
// qualified access to module members
#[case::module_access("module m: def double(x): x * 2; end | m::double(5)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(10.into())].into()))]
// function passed as first-class value
#[case::fn_as_value("def sq(x): x * x; | map([2, 3, 4], sq)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(4.into()), RuntimeValue::Number(9.into()), RuntimeValue::Number(16.into())])].into()))]
// try-catch: catches runtime errors
#[case::try_catch_on_error("try: error(\"e\") catch: \"caught\"", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("caught".to_string())].into()))]
// optimizer: while loop variant with reassignment
#[case::while_with_reassign("var n = 0 | while(n < 3): n += 1 | n", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// len: on bytes
#[case::len_bytes(r#"len(b"hello")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// to_string: None
#[case::to_string_none("to_string(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("".to_string())].into()))]
// range: 3-arg numeric (start, end, step) - end is inclusive
#[case::range_3_arg("range(1, 8, 2)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Array(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into()), RuntimeValue::Number(5.into()), RuntimeValue::Number(7.into())])].into()))]
#[case::range_3_arg_zero("range(0, 6, 3) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// range: single-char string range (end inclusive)
#[case::range_char("range(\"a\", \"e\") | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// range: single-char string range with step (end inclusive)
#[case::range_char_step("range(\"a\", \"g\", 2) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()))]
// range: multi-char string range (end inclusive)
#[case::range_multi_char("range(\"aa\", \"ac\") | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// compact: non-array returns the value unchanged
#[case::compact_non_array_string(r#"compact("hello")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("hello".to_string())].into()))]
#[case::compact_non_array_number("compact(42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// comparison ops: mixed types return false
#[case::gt_mixed_types(r#"gt("a", 42)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::gte_mixed_types(r#"gte("a", 42)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::lt_mixed_types(r#"lt("a", 42)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
#[case::lte_mixed_types(r#"lte("a", 42)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
// add: array + array concatenation
#[case::add_array_array("add(array(1, 2), array(3, 4)) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()))]
// add: array + non-array appends element
#[case::add_array_element("add(array(1, 2), 3) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// add: non-array + array prepends element
#[case::add_element_array("add(1, array(2, 3)) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// add: dict + dict merges
#[case::add_dict_dict("add(dict(), dict()) | type", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// md5/sha256/sha512: None input → None output
#[case::md5_none("md5(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
#[case::sha256_none("sha256(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
#[case::sha512_none("sha512(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// md5/sha256: non-string coerced to string and hashed
#[case::md5_number("md5(42) | type", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
#[case::sha256_number("sha256(42) | type", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
#[case::sha512_number("sha512(42) | type", vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// min/max: symbol comparisons
#[case::min_symbols(r#"min(:a, :b)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Symbol(mq_lang::Ident::new("a"))].into()))]
#[case::max_symbols(r#"max(:a, :b)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Symbol(mq_lang::Ident::new("b"))].into()))]
// gsub: None input → None
#[case::gsub_none(r#"gsub(None, "x", "y")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// replace: None input → None
#[case::replace_none(r#"replace(None, "x", "y")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// split: None input → empty array
#[case::split_none(r#"split(None, " ")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::EMPTY_ARRAY].into()))]
// to_link: empty title → link with no title
#[case::to_link_empty_title(r##"to_link("url", "text", "") | type"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// get_title: link with no title → None
#[case::get_title_link_no_title(r##"to_link("url", "text", "") | get_title"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// get_title: image with title
#[case::get_title_image(r##"to_image("url", "alt", "title") | get_title"##, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("title".to_string())].into()))]
// get_title: non-markdown → None
#[case::get_title_non_markdown("get_title(42)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// set_check: non-list → returns first arg
#[case::set_check_non_list(r#"set_check("not_a_list", true)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("not_a_list".to_string())].into()))]
// set_list_ordered: non-list → returns first arg
#[case::set_list_ordered_non_list(r#"set_list_ordered("not_a_list", true)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("not_a_list".to_string())].into()))]
// set_code_block_lang: non-code-block → returns first arg
#[case::set_code_block_lang_non_code(r#"set_code_block_lang("not_a_code", "rust")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("not_a_code".to_string())].into()))]
// set_attr: non-markdown → returns first arg
#[case::set_attr_non_markdown(r#"set_attr("not_markdown", "key", "value")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("not_markdown".to_string())].into()))]
// attr: non-markdown → returns first arg
#[case::attr_non_markdown(r#"attr("not_markdown", "key")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("not_markdown".to_string())].into()))]
// set_variable with symbol key
#[case::set_variable_symbol(r#"set_variable(:myvar, 42) | get_variable(:myvar)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(42.into())].into()))]
// _diff: two different strings → paired delete+insert = 2 dicts
#[case::diff_strings("_diff(\"abc\", \"axc\") | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// _diff: two arrays - equal(1), paired(delete 2/insert 4), equal(3) = 4 dicts
#[case::diff_arrays_len("_diff(array(1, 2, 3), array(1, 4, 3)) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(4.into())].into()))]
// sort: array of strings (triggers position-clearing path for non-markdown)
#[case::sort_strings(r#"sort(["b", "a", "c"]) | first"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("a".to_string())].into()))]
// from_date: RFC3339 string → number
#[case::from_date_rfc3339_type(r#"type(from_date("2024-06-01T00:00:00Z"))"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("number".to_string())].into()))]
// url_encode: number fallback (non-string/non-markdown uses to_string fallback)
#[case::url_encode_number(r#"url_encode(42) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// upcase: None input → None
#[case::upcase_none("upcase(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// downcase: None input → None
#[case::downcase_none("downcase(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// trim: None input → None
#[case::trim_none("trim(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// ltrim: None input → None
#[case::ltrim_none("ltrim(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// rtrim: None input → None
#[case::rtrim_none("rtrim(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// print: returns current value unchanged
#[case::print_returns_current(r#"print("side effect")"#, vec![RuntimeValue::String("input_val".to_string())], Ok(vec![RuntimeValue::String("input_val".to_string())].into()))]
// ends_with: bytes vs bytes
#[case::ends_with_bytes(r#"ends_with(b"hello", b"lo")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
#[case::ends_with_bytes_false(r#"ends_with(b"hello", b"he")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
// starts_with: bytes vs bytes
#[case::starts_with_bytes(r#"starts_with(b"hello", b"he")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(true)].into()))]
// ends_with: None → false
#[case::ends_with_none(r#"ends_with(None, "x")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
// starts_with: None → false
#[case::starts_with_none(r#"starts_with(None, "x")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Boolean(false)].into()))]
// index: bytes in bytes
#[case::index_bytes(r#"index(b"hello", b"ll")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// index: None → -1
#[case::index_none(r#"index(None, "x")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1_i64).into())].into()))]
// index: array contains value
#[case::index_array_value(r#"index([1, 2, 3], 2)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// rindex: bytes in bytes
#[case::rindex_bytes(r#"rindex(b"hello", b"l")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(3.into())].into()))]
// rindex: None → -1
#[case::rindex_none(r#"rindex(None, "l")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number((-1_i64).into())].into()))]
// rindex: array rindex
#[case::rindex_array(r#"rindex(["a", "b", "a"], "a")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// del: dict with symbol key
#[case::del_dict_symbol(r#"del({a: 1, b: 2}, :a) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// join: error path is tested in error tests; happy path
#[case::join_array(r#"join(["a", "b", "c"], "-")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("a-b-c".to_string())].into()))]
// set: extend array with gap
#[case::set_array_extend("set(array(1, 2), 4, 99) | len", vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
// set: dict with symbol key
#[case::set_dict_symbol(r#"set({a: 1}, :b, 2) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// dict: with array of [key, value] pairs
#[case::dict_from_array_pairs(r#"dict([[":a", 1], [":b", 2]]) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// base64/base64d: with Markdown heading input
#[case::base64_markdown(r#"to_h("test", 1) | base64 | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::base64d_markdown(r#"to_h("dGVzdA==", 1) | base64d | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// base64url: with Markdown input
#[case::base64url_markdown(r#"to_h("test", 1) | base64url | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// base64urld: with Markdown input (decode base64url-encoded heading text)
#[case::base64urld_markdown(r#"to_h("dGVzdA", 1) | base64urld | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// md5/sha256/sha512: with Bytes input (from_hex creates bytes)
#[case::md5_bytes(r#"md5(from_hex("68656c6c6f")) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
#[case::sha256_bytes(r#"sha256(from_hex("68656c6c6f")) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
#[case::sha512_bytes(r#"sha512(from_hex("68656c6c6f")) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("string".to_string())].into()))]
// md5/sha256/sha512: with Markdown input
#[case::md5_markdown(r#"to_h("test", 1) | md5 | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::sha256_markdown(r#"to_h("test", 1) | sha256 | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::sha512_markdown(r#"to_h("test", 1) | sha512 | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// to_hex: None input → None
#[case::to_hex_none("to_hex(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// utf8: None input → None
#[case::utf8_none("utf8(None)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// ltrim/rtrim/upcase: with Markdown input
#[case::ltrim_markdown(r#"to_h("  test  ", 1) | ltrim | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::rtrim_markdown(r#"to_h("  test  ", 1) | rtrim | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::upcase_markdown(r#"to_h("test", 1) | upcase | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// sub/div/mod: string arguments are converted to numbers
#[case::sub_strings(r#"sub("10", "3")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(7.into())].into()))]
#[case::div_strings(r#"div("10", "2")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(5.into())].into()))]
#[case::mod_strings(r#"mod("10", "3")"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(1.into())].into()))]
// mul: string * number repeats string
#[case::mul_string_repeat(r#"mul("ab", 3)"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("ababab".to_string())].into()))]
// mul: array * number repeats array
#[case::mul_array_number(r#"mul(array(1, 2), 3) | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(6.into())].into()))]
// mul: None * number → None
#[case::mul_none_number("mul(None, 3)", vec![RuntimeValue::None], Ok(vec![RuntimeValue::None].into()))]
// mul: bytes * number repeats bytes
#[case::mul_bytes_number(r#"mul(from_hex("ff"), 3) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("bytes".to_string())].into()))]
// get: Markdown node at given character index
#[case::get_markdown_index(r#"to_h("hello", 1) | get(0) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// insert: Dict + Symbol key
#[case::insert_dict_symbol(r#"insert({a: 1}, :b, 2) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// _csv_parse: basic 1-row parse
#[case::csv_parse_basic(r#"_csv_parse("a,b\n1,2") | len"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::Number(2.into())].into()))]
// _csv_parse: with custom delimiter
#[case::csv_parse_delimiter(r#"_csv_parse("a;b;c", ";") | first | first"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("a".to_string())].into()))]
// _csv_parse: with header row
#[case::csv_parse_header(r#"_csv_parse("name,age\nAlice,30", ",", true) | first | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// _xml_parse: basic element with text
#[case::xml_parse_basic(r#"_xml_parse("<root>hello</root>") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// _xml_parse: self-closing element
#[case::xml_parse_empty_element(r#"_xml_parse("<br/>") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// set_variable: with string key
#[case::set_variable_string_key(r#"set_variable("myvar", 42) | get_variable("myvar") | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("number".to_string())].into()))]
// _diff: array with string elements hits string inline-diff path
#[case::diff_arrays_strings(r#"_diff(["old", "same"], ["new", "same"]) | first | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("dict".to_string())].into()))]
// from_hex: with Markdown input (heading with valid hex text)
#[case::from_hex_markdown(r#"to_h("74657374", 1) | from_hex | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("bytes".to_string())].into()))]
// mul: Markdown * number repeats markdown value
#[case::mul_markdown_number(r#"to_h("ab", 1) | mul(2) | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// downcase: with Markdown input
#[case::downcase_markdown(r#"to_h("TEST", 1) | downcase | type"#, vec![RuntimeValue::None], Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
// wikilink selector
#[case::wikilink_select(
    r#"to_markdown("[[target]]") | first() | .wikilink"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::WikiLink(mq_markdown::WikiLink { target: "target".to_string(), text: None, position: None }))].into()))]
#[case::wikilink_with_text_select(
    r#"to_markdown("[[target|display]]") | first() | .wikilink"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::WikiLink(mq_markdown::WikiLink { target: "target".to_string(), text: Some("display".to_string()), position: None }))].into()))]
#[case::wikilink_url_attr(
    r#"to_markdown("[[My Notes]]") | first() | .wikilink | .url"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("My Notes".to_string())].into()))]
#[case::wikilink_value_attr(
    r#"to_markdown("[[target|display]]") | first() | .wikilink | .value"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("display".to_string())].into()))]
#[case::link_includes_wikilink(
    r#"to_markdown("[[target]]") | first() | .link"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::new_markdown(mq_markdown::Node::WikiLink(mq_markdown::WikiLink { target: "target".to_string(), text: None, position: None }))].into()))]
// callout selector
#[case::callout_select_type(
    r#"to_markdown("> [!NOTE]\n> body") | first() | .callout | type"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::callout_kind_attr(
    r#"to_markdown("> [!WARNING]\n> body") | first() | .callout | .kind"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("WARNING".to_string())].into()))]
#[case::callout_note_kind_attr(
    r#"to_markdown("> [!NOTE]\n> body") | first() | .callout | .kind"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("NOTE".to_string())].into()))]
#[case::callout_title_attr(
    r#"to_markdown("> [!NOTE] My Title\n> body") | first() | .callout | .title"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("My Title".to_string())].into()))]
#[case::callout_no_title_attr(
    r#"to_markdown("> [!NOTE]\n> body") | first() | .callout | .title"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::NONE].into()))]
#[case::callout_value_attr(
    r#"to_markdown("> [!NOTE]\n> body") | first() | .callout | .value"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("body".to_string())].into()))]
#[case::callout_no_match_on_blockquote(
    r#"to_markdown("> plain quote") | first() | .callout"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::NONE].into()))]
#[case::callout_kind_lowercase_normalized(
    r#"to_markdown("> [!tip]\n> body") | first() | .callout | .kind"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("TIP".to_string())].into()))]
// embed selector
#[case::embed_select_type(
    r#"to_markdown("![[image.png]]") | first() | .embed | type"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("markdown".to_string())].into()))]
#[case::embed_url_attr(
    r#"to_markdown("![[image.png]]") | first() | .embed | .url"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("image.png".to_string())].into()))]
#[case::embed_value_attr(
    r#"to_markdown("![[image.png|300]]") | first() | .embed | .value"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("300".to_string())].into()))]
#[case::embed_value_falls_back_to_target(
    r#"to_markdown("![[note.md]]") | first() | .embed | .value"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::String("note.md".to_string())].into()))]
#[case::embed_no_match_on_wikilink(
    r#"to_markdown("[[target]]") | first() | .embed"#,
    vec![RuntimeValue::None],
    Ok(vec![RuntimeValue::NONE].into()))]
fn test_eval(mut engine: Engine, #[case] program: &str, #[case] input: Vec<RuntimeValue>, #[case] expected: MqResult) {
    assert_eq!(engine.eval(program, input.into_iter()), expected);
}

#[rstest]
#[case::invalid_function_syntax("f()def f(): 1", vec![RuntimeValue::Number(0.into())])]
#[case::func("def func1(): 1 | func1(); | func1()", vec![RuntimeValue::Number(0.into())])]
#[case::func("def func1(x): 1; | func1(1, 2)", vec![RuntimeValue::Number(0.into())])]
#[case::func_invalid_definition("def f(x): 1; | f2(1, 2)", vec![RuntimeValue::Number(0.into())])]
#[case::invalid_definition("func1(1, 2)", vec![RuntimeValue::Number(0.into())])]
#[case::interpolated_string("s\"${val1} World!\"", vec![RuntimeValue::Number(0.into())])]
#[case::foreach("foreach(x, 1): add(x, 1);", vec![RuntimeValue::Number(10.into())])]
#[case::dict_get_on_non_map("get(\"not_a_map\", \"key\")", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_set_on_non_map("set(123, \"key\", \"value\")", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_keys_on_non_map("keys([1,2,3])", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_values_on_non_map("values(true)", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_get_wrong_key_type("let m = new_dict() | get(m, 123)", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_set_wrong_key_type("let m = new_dict() | set(m, false, \"value\")", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_get_wrong_arg_count("let m = new_dict() | get(m)", vec![RuntimeValue::Number(0.into())],)]
#[case::dict_set_wrong_arg_count("let m = new_dict() | set(m, \"key\")", vec![RuntimeValue::Number(0.into())],)]
#[case::assign_to_immutable("let x = 10 | x = 20", vec![RuntimeValue::Number(0.into())],)]
#[case::macro_undefined("undefined_macro(5)", vec![RuntimeValue::Number(0.into())],)]
#[case::macro_arity_mismatch_too_few("
    macro add_two(a, b):
        a + b;
    | add_two(1)
    ", vec![RuntimeValue::Number(0.into())],)]
#[case::macro_arity_mismatch_too_many("
    macro double(x):
        x + x;
    | double(1, 2, 3)
    ", vec![RuntimeValue::Number(0.into())],)]
#[case::unquote_outside_quote("unquote(5)", vec![RuntimeValue::Number(0.into())],)]
#[case::variadic_not_last_param("def f(*a, b): a", vec![RuntimeValue::Number(0.into())],)]
#[case::multiple_variadic_params("def f(*a, *b): a", vec![RuntimeValue::Number(0.into())],)]
#[case::macro_variadic_param("macro m(*args): args", vec![RuntimeValue::Number(0.into())],)]
#[case::regex_invalid_pattern(r#""abc" =~ "[invalid""#, vec![RuntimeValue::None],)]
#[case::is_regex_match_invalid_pattern(r#"is_regex_match("abc", "[invalid")"#, vec![RuntimeValue::None],)]
// recursion depth exceeded
#[case::recursion_limit("def f(x): f(x); | f(1)", vec![RuntimeValue::None],)]
// too many args to a user-defined function
#[case::too_many_args_user_fn("def f(x): x; | f(1, 2, 3)", vec![RuntimeValue::None],)]
// too few args to a variadic user-defined function (need 2+ required, given 0)
#[case::too_few_args_variadic_fn("def f(x, y, *rest): x; | f()", vec![RuntimeValue::None],)]
// calling a non-function value as a function (InvalidDefinition)
#[case::call_non_function("let x = 42 | x(1)", vec![RuntimeValue::None],)]
// xor with mismatched byte slice lengths
#[case::xor_mismatched_lengths(r#"xor(b"\xff\xf0", b"\x00")"#, vec![RuntimeValue::None],)]
// band with mismatched byte slice lengths
#[case::band_mismatched_lengths(r#"band(b"\xff\xf0", b"\x00")"#, vec![RuntimeValue::None],)]
// bor with mismatched byte slice lengths
#[case::bor_mismatched_lengths(r#"bor(b"\xff\xf0", b"\x00")"#, vec![RuntimeValue::None],)]
// from_hex invalid hex string
#[case::from_hex_invalid("from_hex(\"xyz\")", vec![RuntimeValue::None],)]
// to_hex with non-bytes
#[case::to_hex_non_bytes("to_hex(\"string\")", vec![RuntimeValue::None],)]
// base64d invalid input
#[case::base64d_invalid(r#"base64d("not-valid-base64!!!")"#, vec![RuntimeValue::None],)]
// to_bytes with out-of-range element
#[case::to_bytes_invalid_element("to_bytes([256])", vec![RuntimeValue::None],)]
// user-defined error
#[case::user_defined_error(r#"error("my custom error")"#, vec![RuntimeValue::None],)]
// partial: too many pre-filled args (provides more args than function has params)
#[case::partial_too_many_args("def f(a): a; | partial(f, 1, 2)", vec![RuntimeValue::None],)]
// partial: non-function as first arg
#[case::partial_non_function(r#"partial("not_a_function", 1)"#, vec![RuntimeValue::None],)]
// halt: non-number arg → type error
#[case::halt_non_number(r#"halt("string")"#, vec![RuntimeValue::None],)]
// error: non-string arg → type error (only strings allowed)
#[case::error_non_string("error(42)", vec![RuntimeValue::None],)]
// gmtime: non-number arg → type error
#[case::gmtime_non_number(r#"gmtime("string")"#, vec![RuntimeValue::None],)]
// localtime: non-number arg → type error
#[case::localtime_non_number(r#"localtime("string")"#, vec![RuntimeValue::None],)]
// mktime: wrong-length array → type error
#[case::mktime_wrong_length("mktime(array(1, 2))", vec![RuntimeValue::None],)]
// strftime: first arg not a number → type error
#[case::strftime_non_number(r#"strftime("string", "%Y")"#, vec![RuntimeValue::None],)]
// date_add: wrong types (number instead of array) → type error
#[case::date_add_wrong_types(r#"date_add(42, 1, "days")"#, vec![RuntimeValue::None],)]
// date_diff: wrong types → type error
#[case::date_diff_wrong_types(r#"date_diff("bad", "bad2", "days")"#, vec![RuntimeValue::None],)]
// ln: non-number → type error
#[case::ln_non_number(r#"ln("x")"#, vec![RuntimeValue::None],)]
// log10: non-number → type error
#[case::log10_non_number(r#"log10("x")"#, vec![RuntimeValue::None],)]
// sqrt: non-number → type error
#[case::sqrt_non_number(r#"sqrt("x")"#, vec![RuntimeValue::None],)]
// exp: non-number → type error
#[case::exp_non_number(r#"exp("x")"#, vec![RuntimeValue::None],)]
// pow: non-number base → type error
#[case::pow_non_number(r#"pow("x", 2)"#, vec![RuntimeValue::None],)]
// ceil: non-number → type error
#[case::ceil_non_number(r#"ceil("x")"#, vec![RuntimeValue::None],)]
// floor: non-number → type error
#[case::floor_non_number(r#"floor("x")"#, vec![RuntimeValue::None],)]
// round: non-number → type error
#[case::round_non_number(r#"round("x")"#, vec![RuntimeValue::None],)]
// trunc: non-number → type error
#[case::trunc_non_number(r#"trunc("x")"#, vec![RuntimeValue::None],)]
// abs: non-number → type error
#[case::abs_non_number(r#"abs("x")"#, vec![RuntimeValue::None],)]
// sort: non-array → type error
#[case::sort_non_array(r#"sort("x")"#, vec![RuntimeValue::None],)]
// uniq: non-array → type error
#[case::uniq_non_array(r#"uniq("x")"#, vec![RuntimeValue::None],)]
// join: non-array first arg → type error
#[case::join_non_array("join(42, \",\")", vec![RuntimeValue::None],)]
// reverse: non-array/string/bytes → type error
#[case::reverse_non_array("reverse(42)", vec![RuntimeValue::None],)]
// trim: non-string/non-markdown/non-none → type error
#[case::trim_non_string("trim(42)", vec![RuntimeValue::None],)]
// ltrim: non-string → type error
#[case::ltrim_non_string("ltrim(42)", vec![RuntimeValue::None],)]
// rtrim: non-string → type error
#[case::rtrim_non_string("rtrim(42)", vec![RuntimeValue::None],)]
// upcase: non-string/non-markdown/non-none → type error
#[case::upcase_non_string("upcase(42)", vec![RuntimeValue::None],)]
// range: multi-char string with step → error
#[case::range_multichar_with_step(r#"range("aa", "zz", 2)"#, vec![RuntimeValue::None],)]
// range: invalid type → error
#[case::range_invalid_type("range(true)", vec![RuntimeValue::None],)]
// to_md_table_cell: non-number row → type error
#[case::to_md_table_cell_non_number(r#"to_md_table_cell("val", "not_number", 0)"#, vec![RuntimeValue::None],)]
// basename: non-string → type error
#[case::basename_non_string("basename(42)", vec![RuntimeValue::None],)]
// dirname: non-string → type error
#[case::dirname_non_string("dirname(42)", vec![RuntimeValue::None],)]
// extname: non-string → type error
#[case::extname_non_string("extname(42)", vec![RuntimeValue::None],)]
// stem: non-string → type error
#[case::stem_non_string("stem(42)", vec![RuntimeValue::None],)]
// path_join: non-string → type error
#[case::path_join_non_string(r#"path_join(42, "component")"#, vec![RuntimeValue::None],)]
// min: mixed types → type error
#[case::min_mixed_types(r#"min("str", 42)"#, vec![RuntimeValue::None],)]
// max: mixed types → type error
#[case::max_mixed_types(r#"max("str", 42)"#, vec![RuntimeValue::None],)]
// xor: non-bytes args → type error
#[case::xor_non_bytes(r#"xor("a", "b")"#, vec![RuntimeValue::None],)]
// band: non-bytes args → type error
#[case::band_non_bytes(r#"band("a", "b")"#, vec![RuntimeValue::None],)]
// bor: non-bytes args → type error
#[case::bor_non_bytes(r#"bor("a", "b")"#, vec![RuntimeValue::None],)]
// bnot: non-bytes arg → type error
#[case::bnot_non_bytes(r#"bnot("a")"#, vec![RuntimeValue::None],)]
// pack: non-string format → type error
#[case::pack_non_string("pack(42, 255)", vec![RuntimeValue::None],)]
// unpack: non-string format → type error
#[case::unpack_non_string_format(r#"unpack(42, b"\xff")"#, vec![RuntimeValue::None],)]
// del: string with non-number index → type error
#[case::del_string_non_number(r#"del("hello", "key")"#, vec![RuntimeValue::None],)]
// _csv_parse: non-string arg → type error
#[case::csv_parse_non_string("_csv_parse(42)", vec![RuntimeValue::None],)]
// _csv_parse: non-string delimiter → type error
#[case::csv_parse_non_string_delim(r#"_csv_parse("a,b", 42)"#, vec![RuntimeValue::None],)]
// _xml_parse: non-string arg → type error
#[case::xml_parse_non_string("_xml_parse(42)", vec![RuntimeValue::None],)]
// get: Markdown + non-number key → type error
#[case::get_markdown_non_number(r#"get(to_h("hi", 1), "key")"#, vec![RuntimeValue::None],)]
// mul: negative float * string → type error
#[case::mul_string_negative(r#"mul("ab", -1.5)"#, vec![RuntimeValue::None],)]
// sub: non-convertible args → type error
#[case::sub_non_convertible(r#"sub("x", "y")"#, vec![RuntimeValue::None],)]
// mod: non-convertible args → type error
#[case::mod_non_convertible(r#"mod("x", "y")"#, vec![RuntimeValue::None],)]
// assign_to_immutable: let binding then reassign → error
#[case::assign_let_then_modify(r#"let x = 5 | x = 10"#, vec![RuntimeValue::None],)]
fn test_eval_error(mut engine: Engine, #[case] program: &str, #[case] input: Vec<RuntimeValue>) {
    assert!(engine.eval(program, input.into_iter()).is_err());
}

#[rstest]
#[case::unclosed_brace("{key: val", vec![RuntimeValue::None], "Expected a closing brace")]
#[case::unclosed_paren("(upcase", vec![RuntimeValue::None], "Expected a closing parenthesis")]
#[case::unclosed_bracket("[1, 2", vec![RuntimeValue::None], "Expected a closing bracket")]
#[case::eof_after_not("!", vec![RuntimeValue::None], "Expected an expression after `!`")]
#[case::eof_after_plus("1 +", vec![RuntimeValue::None], "Expected an expression after `+`")]
#[case::eof_after_minus("-", vec![RuntimeValue::None], "Expected an expression after `-`")]
fn test_eof_error_messages(
    mut engine: Engine,
    #[case] program: &str,
    #[case] input: Vec<RuntimeValue>,
    #[case] expected_msg: &str,
) {
    let err = engine.eval(program, input.into_iter()).unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains(expected_msg),
        "Expected error message to contain {:?}, got {:?}",
        expected_msg,
        msg
    );
}

#[cfg(feature = "ast-json")]
mod ast_json {
    use mq_lang::{ArenaId, AstExpr, AstLiteral, AstNode, Program, Shared};
    use rstest::rstest;
    use smallvec::smallvec;

    fn default_token_id() -> ArenaId<Shared<mq_lang::Token>> {
        ArenaId::new(0)
    }

    #[rstest]
    #[case(
    Shared::new(AstNode {
        token_id: default_token_id(),
        expr: Shared::new(AstExpr::Literal(AstLiteral::String("hello".to_string()))),
    }),
    Some(vec!["Literal", "String", "hello"]),
    true
)]
    #[case(
    Shared::new(AstNode {
        token_id: default_token_id(),
        expr: Shared::new(AstExpr::Literal(AstLiteral::Number(123.45.into()))),
    }),
    Some(vec!["Literal", "Number", "123.45"]),
    true
)]
    #[case(
    Shared::new(AstNode {
        token_id: default_token_id(),
        expr: Shared::new(AstExpr::Ident(mq_lang::IdentWithToken::new("my_var"))),
    }),
    Some(vec!["Ident", "my_var"]),
    true
)]
    #[case(
    Shared::new(AstNode {
        token_id: default_token_id(),
        expr: Shared::new(AstExpr::Call(
            mq_lang::IdentWithToken::new("my_func"),
            smallvec![Shared::new(AstNode {
                token_id: default_token_id(),
                expr: Shared::new(AstExpr::Literal(AstLiteral::Number(1.into()))),
            })],
        )),
    }),
    Some(vec!["Call", "my_func", "Literal", "Number", "1.0"]),
    true
)]
    #[case(
    Shared::new(AstNode {
        token_id: default_token_id(),
        expr: Shared::new(AstExpr::If(smallvec![
            (
                Some(Shared::new(AstNode {
                    token_id: default_token_id(),
                    expr: Shared::new(AstExpr::Literal(AstLiteral::Bool(true))),
                })),
                Shared::new(AstNode {
                    token_id: default_token_id(),
                    expr: Shared::new(AstExpr::Literal(AstLiteral::String("then_branch".to_string()))),
                })
            )
        ])),
    }),
    Some(vec!["If", "Bool", "true", "String", "then_branch"]),
    false
)]
    fn test_astnode_serialization_deserialization(
        #[case] original_node: Shared<AstNode>,
        #[case] expected_json_parts: Option<Vec<&str>>,
        #[case] check_token_id: bool,
    ) {
        let json_string = original_node.to_json().unwrap();
        if let Some(parts) = expected_json_parts {
            for part in parts {
                assert!(json_string.contains(part), "json does not contain: {}", part);
            }
        }
        let deserialized_node: AstNode = AstNode::from_json(&json_string).unwrap();
        assert_eq!(deserialized_node.expr, original_node.expr);
        if check_token_id {
            assert_eq!(deserialized_node.token_id, default_token_id());
        }
        if let AstExpr::Ident(ident) = &*deserialized_node.expr {
            assert_eq!(ident.token, None);
        }
    }

    #[test]
    fn test_program_serialization_deserialization() {
        let node1 = Shared::new(AstNode {
            token_id: default_token_id(),
            expr: Shared::new(AstExpr::Literal(AstLiteral::String("first".to_string()))),
        });
        let node2 = Shared::new(AstNode {
            token_id: default_token_id(),
            expr: Shared::new(AstExpr::Literal(AstLiteral::Number(10.into()))),
        });
        let original_program: Program = vec![node1, node2];

        let json_string = serde_json::to_string_pretty(&original_program)
            .unwrap()
            .replace(" ", "");

        assert!(json_string.starts_with('['));
        assert!(json_string.contains("\"String\":\"first\""));
        assert!(json_string.contains("\"Number\":10.0"));
        assert!(json_string.ends_with("\n]"));

        let deserialized_program: Program = serde_json::from_str(&json_string).unwrap();

        assert_eq!(deserialized_program.len(), original_program.len());
        for (orig, deser) in original_program.iter().zip(deserialized_program.iter()) {
            assert_eq!(deser.expr, orig.expr);
            assert_eq!(deser.token_id, default_token_id());
        }
    }

    #[rstest]
    #[case("{invalid_json}")]
    #[case(r#"{\"expr\": {\"UnknownVariant\": \"some_data\"}}"#)]
    fn test_invalid_or_malformed_json_deserialization(#[case] json_string: &str) {
        let result: Result<AstNode, _> = AstNode::from_json(json_string);
        assert!(result.is_err());
    }
}
