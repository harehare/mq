use std::collections::BTreeMap;

use mq_lang::{Engine, MqResult, Value};
use rstest::{fixture, rstest};

#[fixture]
fn engine() -> Engine {
    let mut engine = mq_lang::Engine::default();
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
#[case::if_("let x = 1
      | let y = if (eq(x, 1)): 2 else: 3
      | y
      ",
        vec![Value::Number(0.into())],
              Ok(vec![Value::Number(2.into())].into()))]
#[case::if_("let x = 2
      | let y = if (eq(x, 1)): 1
      | y
      ",
        vec![Value::Number(0.into())], Ok(vec![Value::NONE].into()))]
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
#[case::contains("contains(\"test\")",
      vec![Value::String("testString".to_string())],
      Ok(vec![Value::TRUE].into()))]
#[case::contains("contains(\"test\")",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::FALSE].into()))]
#[case::is_array("is_array()",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::TRUE].into()))]
#[case::is_array("is_array(array(\"test\"))",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::TRUE].into()))]
#[case::is_array("is_string(array(\"test\"))",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::FALSE].into()))]
#[case::ltrimstr("ltrimstr(\"test\")",
      vec![Value::String("testString".to_string())],
      Ok(vec![Value::String("String".to_string())].into()))]
#[case::rtrimstr("rtrimstr(\"test\")",
      vec![Value::String("Stringtest".to_string())],
      Ok(vec![Value::String("String".to_string())].into()))]
#[case::is_empty("is_empty(\"\")",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::TRUE].into()))]
#[case::is_empty("is_empty(\"test\")",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::FALSE].into()))]
#[case::is_empty("is_empty(array(\"test\"))",
      vec![Value::String("String".to_string())],
      Ok(vec![Value::FALSE].into()))]
#[case::test("test(\"^hello.*\")",
      vec![Value::String("helloWorld".to_string())],
      Ok(vec![Value::TRUE].into()))]
#[case::test("test(\"^world.*\")",
      vec![Value::String("helloWorld".to_string())],
      Ok(vec![Value::FALSE].into()))]
#[case::test("select(contains(\"hello\"))",
      vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello world".to_string(), position: None}))],
      Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text{value: "hello world".to_string(), position: None}))].into()))]
#[case::first("first(array(1, 2, 3))",
      vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
      Ok(vec![Value::Number(1.into())].into()))]
#[case::first("first(array())",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::None].into()))]
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
#[case::map("def test(x): add(x, 1); | map(array(1, 2, 3), test)",
            vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
            Ok(vec![Value::Array(vec![Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())])].into()))]
#[case::optional_operator("
            def test_optional(x):
              None
            | test_optional(10)? | test_optional(10)?
            ",
              vec![Value::None],
              Ok(vec![Value::None].into()))]
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
#[case::csv2table_row("csv2table_row()",
            vec![Value::String("a,b,c".to_string()), Value::String("1,2,3".to_string())],
            Ok(vec![
              Value::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{values: vec![
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 0,
                        values: vec!["a".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 1,
                        values: vec!["b".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 2,
                        values: vec!["c".to_string().into()],
                        last_cell_in_row: true,
                        last_cell_of_in_table: false,
                        position: None
                    }),
              ], position: None})),
              Value::Markdown(mq_markdown::Node::TableRow(mq_markdown::TableRow{values: vec![
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 0,
                        values: vec!["1".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 1,
                        values: vec!["2".to_string().into()],
                        last_cell_in_row: false,
                        last_cell_of_in_table: false,
                        position: None
                    }),
                    mq_markdown::Node::TableCell(mq_markdown::TableCell{
                        row: 0,
                        column: 2,
                        values: vec!["3".to_string().into()],
                        last_cell_in_row: true,
                        last_cell_of_in_table: false,
                        position: None
                    }),
              ], position: None})),
            ].into()))]
#[case::func("let func1 = def _(): 1;
      | let func2 = def _(): 2;
      | add(func1(), func2())",
        vec![Value::Number(0.into())],
              Ok(vec![Value::Number(3.into())].into()))]
#[case::interpolated_string("let val1 = \"Hello\"
      | s\"${val1} World!\"",
        vec![Value::Number(0.into())],
             Ok(vec!["Hello World!".to_string().into()].into()))]
#[case::interpolated_string("s\"${self} World!\"",
        vec![Value::String("Hello".into())],
             Ok(vec!["Hello World!".to_string().into()].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![Value::Markdown(mq_markdown::Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://github.com".to_string()), title: None, ident: "ident".to_string(), label: None }))],
      Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ position: None, url: mq_markdown::Url::new("https://github.com".to_string()), title: None, values: Vec::new()}))],
      Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::matches_url("matches_url(\"https://github.com\")",
      vec![Value::Markdown(mq_markdown::Node::Image(mq_markdown::Image{ alt: "".to_string(), position: None, url: "https://github.com".to_string(), title: None }))],
      Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text { position: None, value: "true".to_string() }))].into()))]
#[case::matches_url("matches_url(\"https://gitlab.com\")",
      vec![Value::String("https://gitlab.com".to_string())],
      Ok(vec![Value::FALSE].into()))]
#[case::nest(".link | update(\"test\")",
      vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![
           mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("url".to_string()), title: None, values: Vec::new(), position: None }),
           mq_markdown::Node::Image(mq_markdown::Image{ alt: "".to_string(), url: "url".to_string(), title: None, position: None })
      ], position: None, depth: 1 }))],
      Ok(vec![Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("test".to_string()), title: None, values: Vec::new(), position: None }))].into()))]
#[case::selector("nodes | .h",
      vec![
        Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }),], position: None, depth: 1 })),
        Value::String("test".to_string()),
      ],
      Ok(vec![
        Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading{ values: vec![mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None }),], position: None, depth: 1 })),
        Value::NONE
      ].into()))]
#[case::selector("nodes | .h",
      vec![
        Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })),
        Value::String("test".to_string()),
      ],
      Ok(vec![Value::NONE, Value::NONE].into()))]
#[case::sort_by("sort_by(get_title)",
      vec![Value::Array(vec![
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])],
      Ok(vec![Value::Array(vec![
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])].into()))]
#[case::sort_by("sort_by(get_url)",
      vec![Value::Array(vec![
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])],
      Ok(vec![Value::Array(vec![
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang1".to_string()), title: Some(mq_markdown::Title::new("2".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
          Value::Markdown(mq_markdown::Node::Link(mq_markdown::Link{ url: mq_markdown::Url::new("http://mqlang2".to_string()), title: Some(mq_markdown::Title::new("1".to_string())), values: vec![
            mq_markdown::Node::Text(mq_markdown::Text { value: "text".to_string(), position: None })
          ], position: None })),
      ])].into()))]
#[case::sort_by(r#"def sort_test(v): if (eq(v, "3")): "1" elif (eq(v, "1")): "3" else: v; sort_by(sort_test)"#,
      vec![Value::Array(vec![
         "2".to_string().into(),
         "1".to_string().into(),
         "3".to_string().into(),
      ])],
      Ok(vec![Value::Array(vec![
         "3".to_string().into(),
         "2".to_string().into(),
         "1".to_string().into(),
      ])].into()))]
#[case::find_index("
      def is_even(x):
        eq(mod(x, 2), 0);
      | find_index(array(1, 3, 5, 6, 7), is_even)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(3.into()), Value::Number(5.into()), Value::Number(6.into()), Value::Number(7.into())])],
        Ok(vec![Value::Number(3.into())].into()))]
#[case::find_index("
      def is_greater_than_five(x):
        gt(x, 5);
      | find_index(array(1, 3, 5, 6, 7), is_greater_than_five)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(3.into()), Value::Number(5.into()), Value::Number(6.into()), Value::Number(7.into())])],
        Ok(vec![Value::Number(3.into())].into()))]
#[case::find_index_no_match("
      def is_negative(x):
        lt(x, 0);
      | find_index(array(1, 3, 5, 6, 7), is_negative)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(3.into()), Value::Number(5.into()), Value::Number(6.into()), Value::Number(7.into())])],
        Ok(vec![Value::Number((-1).into())].into()))]
#[case::find_index_empty_array("
      def is_even(x):
        eq(mod(x, 2), 0);
      | find_index(array(), is_even)
      ",
        vec![Value::Array(vec![])],
        Ok(vec![Value::Number((-1).into())].into()))]
#[case::skip_while("
      def is_less_than_four(x):
        lt(x, 4);
      | skip_while(array(1, 2, 3, 4, 5, 1, 2), is_less_than_four)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(1.into()), Value::Number(2.into())])],
        Ok(vec![Value::Array(vec![Value::Number(4.into()), Value::Number(5.into()), Value::Number(1.into()), Value::Number(2.into())])].into()))]
#[case::skip_while_all_match("
      def is_positive(x):
        gt(x, 0);
      | skip_while(array(1, 2, 3), is_positive)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
        Ok(vec![Value::Array(vec![])].into()))]
#[case::skip_while_empty_array("
      def is_positive(x):
        gt(x, 0);
      | skip_while(array(), is_positive)
      ",
        vec![Value::Array(vec![])],
        Ok(vec![Value::Array(vec![])].into()))]
#[case::take_while("
      def is_less_than_four(x):
        lt(x, 4);
      | take_while(array(1, 2, 3, 4, 5, 1, 2), is_less_than_four)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(1.into()), Value::Number(2.into())])],
        Ok(vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])].into()))]
#[case::take_while_none_match("
      def is_negative(x):
        lt(x, 0);
      | take_while(array(1, 2, 3), is_negative)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
        Ok(vec![Value::Array(vec![])].into()))]
#[case::take_while_all_match("
      def is_positive(x):
        gt(x, 0);
      | take_while(array(1, 2, 3), is_positive)
      ",
        vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
        Ok(vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])].into()))]
#[case::take_while_empty_array("
      def is_positive(x):
        gt(x, 0);
      | take_while(array(), is_positive)
      ",
        vec![Value::Array(vec![])],
        Ok(vec![Value::Array(vec![])].into()))]
#[case::anonymous_fn("
        let f = fn(x): add(x, 1);
        | f(10)
        ",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(11.into())].into()))]
#[case::anonymous_fn_passed("
          def apply_func(f, x):
            f(x);
          | apply_func(fn(x): mul(x, 2);, 5)
          ",
            vec![Value::Number(0.into())],
            Ok(vec![Value::Number(10.into())].into()))]
#[case::anonymous_fn_return("
          def make_multiplier(factor):
            fn(x): mul(x, factor);;
          | let double = make_multiplier(2)
          | double(5)
          ",
            vec![Value::Number(0.into())],
            Ok(vec![Value::Number(10.into())].into()))]
#[case::array_empty("[]",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![])].into()))]
#[case::array_with_elements("[1, 2, 3]",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])].into()))]
#[case::array_nested("[[1, 2], [3, 4]]",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![
            Value::Array(vec![Value::Number(1.into()), Value::Number(2.into())]),
            Value::Array(vec![Value::Number(3.into()), Value::Number(4.into())])
          ])].into()))]
#[case::array_mixed_types("[1, \"test\", []]",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![
            Value::Number(1.into()),
            Value::String("test".to_string()),
            Value::Array(vec![])
          ])].into()))]
#[case::array_length("len([])",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(0.into())].into()))]
#[case::array_length("len([1, 2, 3, 4])",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(4.into())].into()))]
#[case::dict_new_empty("dict()",
          vec![Value::Number(0.into())],
          Ok(vec![Value::new_dict()].into()))]
#[case::dict_set_get_string("let m = dict() | let m = set(m, \"name\", \"Jules\") | get(m, \"name\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::String("Jules".to_string())].into()))]
#[case::dict_set_get_number("let m = set(dict(), \"age\", 30) | get(m, \"age\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(30.into())].into()))]
#[case::dict_set_get_array("let m = set(dict(), \"data\", [1, 2, 3]) | get(m, \"data\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])].into()))]
#[case::dict_set_get_bool("let m = set(dict(), \"active\", true) | get(m, \"active\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Bool(true)].into()))]
#[case::dict_set_get_none("let m = set(dict(), \"nothing\", None) | get(m, \"nothing\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::None].into()))]
#[case::dict_get_non_existent("let m = dict() | get(m, \"missing\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::None].into()))]
#[case::dict_set_overwrite("let m = set(dict(), \"name\", \"Jules\") | let m = set(m, \"name\", \"Vincent\") | get(m, \"name\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::String("Vincent".to_string())].into()))]
#[case::dict_nested_set_get("let m1 = dict() | let m2 = set(dict(), \"level\", 2) | let m = set(m1, \"nested\", m2) | get(get(m, \"nested\"), \"level\")",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(2.into())].into()))]
#[case::dict_keys_empty("keys(dict())",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![])].into()))]
#[case::dict_keys_non_empty("let m = set(set(dict(), \"a\", 1), \"b\", 2) | keys(m)",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![Value::String("a".to_string()), Value::String("b".to_string())])].into()))]
#[case::dict_values_empty("values(dict())",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![])].into()))]
#[case::dict_values_non_empty("let m = set(set(dict(), \"a\", 1), \"b\", \"hello\") | values(m)",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Array(vec![Value::Number(1.into()), Value::String("hello".to_string())])].into()))]
#[case::dict_len_empty("len(dict())",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(0.into())].into()))]
#[case::dict_len_non_empty("len(set(set(dict(), \"a\", 1), \"b\", 2))",
          vec![Value::Number(0.into())],
          Ok(vec![Value::Number(2.into())].into()))]
#[case::dict_type_is_dict("type(dict())",
          vec![Value::Number(0.into())],
          Ok(vec![Value::String("dict".to_string())].into()))]
#[case::dict_contains_existing_key(r#"let m = set(dict(), "name", "Jules") | contains(m, "name")"#,
          vec![Value::Number(0.into())],
          Ok(vec![Value::Bool(true)].into()))]
#[case::dict_contains_non_existing_key(r#"let m = set(dict(), "name", "Jules") | contains(m, "age")"#,
          vec![Value::Number(0.into())],
          Ok(vec![Value::Bool(false)].into()))]
#[case::dict_contains_empty(r#"contains(dict(), "any_key")"#,
          vec![Value::Number(0.into())],
          Ok(vec![Value::Bool(false)].into()))]
#[case::dict_contains_multiple_keys(r#"let m = set(set(set(dict(), "a", 1), "b", 2), "c", 3) | contains(m, "b")"#,
          vec![Value::Number(0.into())],
          Ok(vec![Value::Bool(true)].into()))]
#[case::dict_map_identity(r#"let m = dict(["a", 1], ["b", 2]) | map(m, fn(kv): kv;)"#,
        vec![Value::Number(0.into())],
        Ok(vec![{
          let mut dict = BTreeMap::new();
          dict.insert("a".to_string(), Value::Number(1.into()));
          dict.insert("b".to_string(), Value::Number(2.into()));
          dict.into()
        }].into()))]
#[case::dict_map_transform_values("
        def double_value(kv):
          array(first(kv), mul(last(kv), 2));
        | let m = set(set(dict(), \"x\", 5), \"y\", 10)
        | map(m, double_value)
        ",
          vec![Value::Number(0.into())],
          Ok(vec![{
            let mut dict = BTreeMap::new();
            dict.insert("x".to_string(), Value::Number(10.into()));
            dict.insert("y".to_string(), Value::Number(20.into()));
            dict.into()
          }].into()))]
#[case::dict_map_transform_keys(r#"
          def prefix_key(kv):
            array(add("prefix_", first(kv)), last(kv));
          | let m = set(set(dict(), "a", 1), "b", 2)
          | map(m, prefix_key)
          "#,
            vec![Value::Number(0.into())],
            Ok(vec![{
              let mut dict = BTreeMap::new();
              dict.insert("prefix_a".to_string(), Value::Number(1.into()));
              dict.insert("prefix_b".to_string(), Value::Number(2.into()));
              dict.into()
            }].into()))]
#[case::dict_map_empty("map(dict(), fn(kv): kv;)",
            vec![Value::Number(0.into())],
            Ok(vec![Value::new_dict()].into()))]
#[case::dict_map_complex_transform(r#"
          def transform_entry(kv):
            let key = first(kv)
            | let value = last(kv)
            | array(add(key, "_transformed"), add(value, 100));
          | let m = set(set(dict(), "num1", 1), "num2", 2)
          | map(m, transform_entry)
          "#,
            vec![Value::Number(0.into())],
            Ok(vec![{
              let mut dict = BTreeMap::new();
              dict.insert("num1_transformed".to_string(), Value::Number(101.into()));
              dict.insert("num2_transformed".to_string(), Value::Number(102.into()));
              dict.into()
            }].into()))]
#[case::dict_filter_even_values(r#"
            def is_even_value(kv):
              last(kv) | mod(2) | eq(0);
            | let m = dict(["a", 1], ["b", 2], ["c", 4])
            | filter(m, is_even_value)
            "#,
            vec![Value::Number(0.into())],
            Ok(vec![{
              let mut dict = BTreeMap::new();
              dict.insert("b".to_string(), Value::Number(2.into()));
              dict.insert("c".to_string(), Value::Number(4.into()));
              dict.into()
            }].into()))]
#[case::dict_filter_empty("filter(dict(), fn(kv): true;)",
           vec![Value::Number(0.into())],
           Ok(vec![Value::new_dict()].into()))]
fn test_eval(
    mut engine: Engine,
    #[case] program: &str,
    #[case] input: Vec<Value>,
    #[case] expected: MqResult,
) {
    assert_eq!(engine.eval(program, input.into_iter()), expected);
}

#[rstest]
#[case::empty("", vec![Value::Number(0.into())])]
#[case::error("f()def f(): 1", vec![Value::Number(0.into())])]
#[case::func("def func1(): 1 | func1(); | func1()", vec![Value::Number(0.into())])]
#[case::func("def func1(x): 1; | func1(1, 2)", vec![Value::Number(0.into())])]
#[case::invalid_definition("func1(1, 2)", vec![Value::Number(0.into())])]
#[case::interpolated_string("s\"${val1} World!\"", vec![Value::Number(0.into())])]
#[case::foreach("foreach(x, 1): add(x, 1);", vec![Value::Number(10.into())])]
#[case::dict_get_on_non_map("get(\"not_a_map\", \"key\")", vec![Value::Number(0.into())],)]
#[case::dict_set_on_non_map("set(123, \"key\", \"value\")", vec![Value::Number(0.into())],)]
#[case::dict_keys_on_non_map("keys([1,2,3])", vec![Value::Number(0.into())],)]
#[case::dict_values_on_non_map("values(true)", vec![Value::Number(0.into())],)]
#[case::dict_get_wrong_key_type("let m = new_dict() | get(m, 123)", vec![Value::Number(0.into())],)]
#[case::dict_set_wrong_key_type("let m = new_dict() | set(m, false, \"value\")", vec![Value::Number(0.into())],)]
#[case::dict_get_wrong_arg_count("let m = new_dict() | get(m)", vec![Value::Number(0.into())],)]
#[case::dict_set_wrong_arg_count("let m = new_dict() | set(m, \"key\")", vec![Value::Number(0.into())],)]
fn test_eval_error(mut engine: Engine, #[case] program: &str, #[case] input: Vec<Value>) {
    assert!(engine.eval(program, input.into_iter()).is_err());
}
