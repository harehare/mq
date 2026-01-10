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
    let x = 5 |
    while (x > 0):
      # test
      let x = x - 1 | x;
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
    let x = 0 |
    while(x < 10):
      let x = x + 1
      | if(x == 3):
        break
      else:
        x;
    ",
      vec![RuntimeValue::Number(10.into())],
      Ok(vec![RuntimeValue::Number(2.into())].into()))]
#[case::while_continue("
    let x = 0 |
    while(x < 4):
      let x = x + 1
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
    let x = 5 |
    while (x > 0) do
      let x = x - 1 | x
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
    let x = 0 |
    while(x < 10) do
      let x = x + 1
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
      vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello world".to_string(), position: None}), None)],
      Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello world".to_string(), position: None}), None )].into()))]
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
      vec![RuntimeValue::Markdown(mq_markdown::Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://github.com".to_string()), title: None, ident: "ident".to_string(), label: None }), None)],
      Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }), None)].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ position: None, url: mq_markdown::Url::new("https://github.com".to_string()), title: None, values: Vec::new()}), None)],
      Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }), None)].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![RuntimeValue::Markdown(mq_markdown::Node::Image(mq_markdown::Image{ alt: "".to_string(), position: None, url: "https://github.com".to_string(), title: None }), None)],
      Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }), None)].into()))]
#[case::matches_url("matches_url(\"https://gitlab.com\")",
      vec![RuntimeValue::String("https://gitlab.com".to_string())],
      Ok(vec![RuntimeValue::FALSE].into()))]
#[case::nest(".link | update(\"test\")",
      vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![
           mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("url".to_string()), title: None, values: Vec::new(), position: None }),
           mq_markdown::Node::Image(mq_markdown::Image{ alt: "".to_string(), url: "url".to_string(), title: None, position: None })
      ], position: None, depth: 1 }), None)],
      Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("test".to_string()), title: None, values: Vec::new(), position: None }), None)].into()))]
#[case::selector("nodes | .h",
      vec![
        RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }),], position: None, depth: 1 }), None),
        RuntimeValue::String("test".to_string()),
      ],
      Ok(vec![
        RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }),], position: None, depth: 1 }), None),
        RuntimeValue::NONE
      ].into()))]
#[case::selector("nodes | .h",
      vec![
        RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }), None),
        RuntimeValue::String("test".to_string()),
      ],
      Ok(vec![RuntimeValue::NONE, RuntimeValue::NONE].into()))]
#[case::sort_by("sort_by(get_title)",
      vec![RuntimeValue::Array(vec![
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
      ])],
      Ok(vec![RuntimeValue::Array(vec![
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
      ])].into()))]
#[case::sort_by("sort_by(get_url)",
      vec![RuntimeValue::Array(vec![
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
      ])],
      Ok(vec![RuntimeValue::Array(vec![
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
          RuntimeValue::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None }), None),
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
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h_false("is_h()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h1_true("is_h1()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h1_false("is_h1()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 2,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h1_false("is_h1()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h2_true("is_h2()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 2,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h2_false("is_h2()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 3,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h2_false("is_h2()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h3_true("is_h3()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 3,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h3_false("is_h3()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 4,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h3_false("is_h3()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h4_true("is_h4()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 4,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h4_false("is_h4()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 5,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h4_false("is_h4()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h5_true("is_h5()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 5,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h5_false("is_h5()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 4,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h5_false("is_h5()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h6_true("is_h6()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 6,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }), None)].into()))]
#[case::is_h6_false("is_h6()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 5,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_h6_false("is_h6()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_em_true("is_em()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
          values: vec![],
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "true".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_em_false("is_em()",
        vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)],
        Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }), None)].into()))]
#[case::is_html_true("is_html()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Html(mq_markdown::Html {
              value: "<b>bold</b>".to_string(),
              position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
          }), None)].into()))]
#[case::is_html_false("is_html()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not html".to_string(),
              position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
          }), None)].into()))]
#[case::is_yaml_true("is_yaml()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Yaml(mq_markdown::Yaml {
            value: "---\nkey: value\n".to_string(),
            position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_yaml_false("is_yaml()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not yaml".to_string(),
            position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_toml_true("is_toml()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Toml(mq_markdown::Toml {
            value: "[section]\nkey = \"value\"\n".to_string(),
            position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_toml_false("is_toml()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not toml".to_string(),
            position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_code_true("is_code()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            position: None,
            fence: true,
            meta: None,
            lang: Some("rust".to_string()),
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_code_false("is_code()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not code".to_string(),
            position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_text_true("is_text()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "sample".to_string(),
            position: None,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_text_false("is_text()",
          vec![RuntimeValue::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
          }), None)],
          Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }), None)].into()))]
#[case::is_list_true("is_list()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 1,
              index: 1,
              checked: Some(false),
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_list_false("is_list()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not a list".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_flow_expression_true("is_mdx_flow_expression()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::MdxFlowExpression(mq_markdown::MdxFlowExpression {
              value: "1 + 2".into(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_flow_expression_false("is_mdx_flow_expression()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_jsx_flow_element_true("is_mdx_jsx_flow_element()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement {
              name: Some("Component".to_string()),
              attributes: vec![],
              children: vec![],
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_jsx_flow_element_false("is_mdx_jsx_flow_element()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not jsx".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_jsx_text_element_true("is_mdx_jsx_text_element()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement {
              name: Some("InlineComponent".into()),
              attributes: vec![],
              children: vec![],
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_jsx_text_element_false("is_mdx_jsx_text_element()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not jsx text".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_text_expression_true("is_mdx_text_expression()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::MdxTextExpression(mq_markdown::MdxTextExpression {
              value: "foo + bar".into(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_text_expression_false("is_mdx_text_expression()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx text expr".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_js_esm_true("is_mdx_js_esm()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::MdxJsEsm(mq_markdown::MdxJsEsm {
              value: "export const foo = 1;".into(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_js_esm_false("is_mdx_js_esm()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not esm".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_true("is_mdx()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::MdxFlowExpression(mq_markdown::MdxFlowExpression {
              value: "1 + 2".into(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }), None)].into()))]
#[case::is_mdx_false("is_mdx()",
            vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx".to_string(),
              position: None,
            }), None)],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }), None)].into()))]
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
#[case::to_csv_single_row(
            "to_csv()",
            vec![RuntimeValue::Array(vec![
                RuntimeValue::String("a".to_string()),
                RuntimeValue::String("b".to_string()),
                RuntimeValue::String("c".to_string()),
            ])],
            Ok(vec![RuntimeValue::String("a,b,c".to_string())].into())
          )]
#[case::to_tsv_single_row(
            "to_tsv()",
            vec![RuntimeValue::Array(vec![
              RuntimeValue::String("a".to_string()),
              RuntimeValue::String("b".to_string()),
              RuntimeValue::String("c".to_string()),
            ])],
            Ok(vec![RuntimeValue::String("a\tb\tc".to_string())].into())
          )]
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
              RuntimeValue::Markdown(mq_markdown::Node::Code(mq_markdown::Code{ lang: Some("rust".to_string()), meta: None, fence: true, value: "value".to_string(),  position: None }), None),
            ],
            Ok(vec![RuntimeValue::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "rust".to_string(),
              position: None,
            }), None)].into()))]
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
fn test_eval_error(mut engine: Engine, #[case] program: &str, #[case] input: Vec<RuntimeValue>) {
    assert!(engine.eval(program, input.into_iter()).is_err());
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
