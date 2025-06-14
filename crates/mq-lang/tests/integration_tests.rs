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
#[case::is_dict_true("is_dict()",
      vec![Value::new_dict()],
      Ok(vec![Value::TRUE].into()))]
#[case::is_dict_false("is_dict()",
      vec![Value::Array(Vec::new())],
      Ok(vec![Value::FALSE].into()))]
#[case::is_none_true("is_none(None)",
      vec!["text".into()],
      Ok(vec![Value::TRUE].into()))]
#[case::is_none_false("is_none()",
      vec![Value::Number(1.into())],
      Ok(vec![Value::FALSE].into()))]
#[case::is_bool_true("is_bool(true)",
        vec![Value::Bool(true)],
        Ok(vec![Value::TRUE].into()))]
#[case::is_bool_false("is_bool(false)",
        vec![Value::Bool(false)],
        Ok(vec![Value::TRUE].into()))]
#[case::is_bool_non_bool("is_bool(1)",
        vec![Value::Number(1.into())],
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
#[case::group_by_numbers("
            def get_remainder(x):
              mod(x, 3);
            | group_by(array(1, 2, 3, 4, 5, 6, 7, 8, 9), get_remainder)
            ",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(6.into()), Value::Number(7.into()), Value::Number(8.into()), Value::Number(9.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert("0".to_string(), Value::Array(vec![Value::Number(3.into()), Value::Number(6.into()), Value::Number(9.into())]));
                dict.insert("1".to_string(), Value::Array(vec![Value::Number(1.into()), Value::Number(4.into()), Value::Number(7.into())]));
                dict.insert("2".to_string(), Value::Array(vec![Value::Number(2.into()), Value::Number(5.into()), Value::Number(8.into())]));
                dict.into()
              }].into()))]
#[case::group_by_strings(r#"
            def get_length(s):
              len(s);
            | group_by(array("cat", "dog", "bird", "fish", "elephant"), get_length)
            "#,
              vec![Value::Array(vec![Value::String("cat".to_string()), Value::String("dog".to_string()), Value::String("bird".to_string()), Value::String("fish".to_string()), Value::String("elephant".to_string())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert("3".to_string(), Value::Array(vec![Value::String("cat".to_string()), Value::String("dog".to_string())]));
                dict.insert("4".to_string(), Value::Array(vec![Value::String("bird".to_string()), Value::String("fish".to_string())]));
                dict.insert("8".to_string(), Value::Array(vec![Value::String("elephant".to_string())]));
                dict.into()
              }].into()))]
#[case::group_by_empty_array("
            def identity(x):
              x;
            | group_by(array(), identity)
            ",
              vec![Value::Array(vec![])],
              Ok(vec![Value::new_dict()].into()))]
#[case::group_by_single_element("
            def identity(x):
              x;
            | group_by(array(42), identity)
            ",
              vec![Value::Array(vec![Value::Number(42.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert("42".to_string(), Value::Array(vec![Value::Number(42.into())]));
                dict.into()
              }].into()))]
#[case::group_by_all_same_key(r#"
            def always_same(x):
              "same";
            | group_by(array(1, 2, 3, 4), always_same)
            "#,
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert("same".to_string(), Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())]));
                dict.into()
              }].into()))]
#[case::group_by_boolean_result("
            def is_even(x):
              eq(mod(x, 2), 0);
            | group_by(array(1, 2, 3, 4, 5, 6), is_even)
            ",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(6.into())])],
              Ok(vec![{
                let mut dict = BTreeMap::new();
                dict.insert("false".to_string(), Value::Array(vec![Value::Number(1.into()), Value::Number(3.into()), Value::Number(5.into())]));
                dict.insert("true".to_string(), Value::Array(vec![Value::Number(2.into()), Value::Number(4.into()), Value::Number(6.into())]));
                dict.into()
              }].into()))]
#[case::is_h_true("is_h()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h_false("is_h()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h1_true("is_h1()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h1_false("is_h1()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 2,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h1_false("is_h1()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h2_true("is_h2()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 2,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h2_false("is_h2()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 3,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h2_false("is_h2()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h3_true("is_h3()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 3,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h3_false("is_h3()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 4,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h3_false("is_h3()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h4_true("is_h4()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 4,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h4_false("is_h4()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 5,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h4_false("is_h4()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h5_true("is_h5()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 5,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h5_false("is_h5()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 4,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h5_false("is_h5()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h6_true("is_h6()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 6,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
        }))].into()))]
#[case::is_h6_false("is_h6()",
        vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
          values: vec![],
          position: None,
          depth: 5,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_h6_false("is_h6()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_em_true("is_em()",
        vec![Value::Markdown(mq_markdown::Node::Emphasis(mq_markdown::Emphasis {
          values: vec![],
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "true".to_string(),
          position: None,
        }))].into()))]
#[case::is_em_false("is_em()",
        vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))],
        Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
          value: "false".to_string(),
          position: None,
        }))].into()))]
#[case::is_html_true("is_html()",
          vec![Value::Markdown(mq_markdown::Node::Html(mq_markdown::Html {
              value: "<b>bold</b>".to_string(),
              position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
          }))].into()))]
#[case::is_html_false("is_html()",
          vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not html".to_string(),
              position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
          }))].into()))]
#[case::is_yaml_true("is_yaml()",
          vec![Value::Markdown(mq_markdown::Node::Yaml(mq_markdown::Yaml {
            value: "---\nkey: value\n".to_string(),
            position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_yaml_false("is_yaml()",
          vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not yaml".to_string(),
            position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_toml_true("is_toml()",
          vec![Value::Markdown(mq_markdown::Node::Toml(mq_markdown::Toml {
            value: "[section]\nkey = \"value\"\n".to_string(),
            position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_toml_false("is_toml()",
          vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not toml".to_string(),
            position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_code_true("is_code()",
          vec![Value::Markdown(mq_markdown::Node::Code(mq_markdown::Code {
            value: "let x = 1;".to_string(),
            position: None,
            fence: true,
            meta: None,
            lang: Some("rust".to_string()),
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_code_false("is_code()",
          vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "not code".to_string(),
            position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_text_true("is_text()",
          vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "sample".to_string(),
            position: None,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "true".to_string(),
            position: None,
          }))].into()))]
#[case::is_text_false("is_text()",
          vec![Value::Markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
            values: vec![],
            position: None,
            depth: 1,
          }))],
          Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
            value: "false".to_string(),
            position: None,
          }))].into()))]
#[case::is_list_true("is_list()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 1,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_list_false("is_list()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not a list".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_flow_expression_true("is_mdx_flow_expression()",
            vec![Value::Markdown(mq_markdown::Node::MdxFlowExpression(mq_markdown::MdxFlowExpression {
              value: "1 + 2".into(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_flow_expression_false("is_mdx_flow_expression()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_flow_element_true("is_mdx_jsx_flow_element()",
            vec![Value::Markdown(mq_markdown::Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement {
              name: Some("Component".to_string()),
              attributes: vec![],
              children: vec![],
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_flow_element_false("is_mdx_jsx_flow_element()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not jsx".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_text_element_true("is_mdx_jsx_text_element()",
            vec![Value::Markdown(mq_markdown::Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement {
              name: Some("InlineComponent".into()),
              attributes: vec![],
              children: vec![],
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_jsx_text_element_false("is_mdx_jsx_text_element()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not jsx text".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_text_expression_true("is_mdx_text_expression()",
            vec![Value::Markdown(mq_markdown::Node::MdxTextExpression(mq_markdown::MdxTextExpression {
              value: "foo + bar".into(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_text_expression_false("is_mdx_text_expression()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx text expr".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_js_esm_true("is_mdx_js_esm()",
            vec![Value::Markdown(mq_markdown::Node::MdxJsEsm(mq_markdown::MdxJsEsm {
              value: "export const foo = 1;".into(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_js_esm_false("is_mdx_js_esm()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not esm".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_true("is_mdx()",
            vec![Value::Markdown(mq_markdown::Node::MdxFlowExpression(mq_markdown::MdxFlowExpression {
              value: "1 + 2".into(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_mdx_false("is_mdx()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not mdx".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_list1_true("is_list1()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 0,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_list1_false("is_list1()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 2,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_list1_false("is_list1()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not a list".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_list2_true("is_list2()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 1,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_list2_false("is_list2()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 0,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_list2_false("is_list2()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not a list".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_list3_true("is_list3()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 2,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "true".to_string(),
              position: None,
            }))].into()))]
#[case::is_list3_false("is_list3()",
            vec![Value::Markdown(mq_markdown::Node::List(mq_markdown::List {
              values: vec![],
              position: None,
              ordered: false,
              level: 1,
              index: 1,
              checked: Some(false),
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::is_list3_false("is_list3()",
            vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "not a list".to_string(),
              position: None,
            }))],
            Ok(vec![Value::Markdown(mq_markdown::Node::Text(mq_markdown::Text {
              value: "false".to_string(),
              position: None,
            }))].into()))]
#[case::range_basic("range(1, 5, 1)",
              vec![Value::Number(0.into())],
              Ok(vec![Value::Array(vec![
                Value::Number(1.into()),
                Value::Number(2.into()),
                Value::Number(3.into()),
                Value::Number(4.into()),
                Value::Number(5.into()),
              ])].into()))]
#[case::range_negative("range(-2, 2, 1)",
              vec![Value::Number(0.into())],
              Ok(vec![Value::Array(vec![
                Value::Number((-2).into()),
                Value::Number((-1).into()),
                Value::Number(0.into()),
                Value::Number(1.into()),
                Value::Number(2.into()),
              ])].into()))]
#[case::range_single("range(3, 3, 1)",
              vec![Value::Number(0.into())],
              Ok(vec![Value::Array(vec![
                Value::Number(3.into()),
              ])].into()))]
#[case::any_true("
              any([1, 2, 3], fn(x): x == 2;)",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
              Ok(vec![Value::Bool(true)].into()))]
#[case::any_false("
              any([1, 2, 3], fn(x): x == 4;)",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into())])],
              Ok(vec![Value::Bool(false)].into()))]
#[case::any_empty_array("
              any([], fn(x): x == 1;)",
              vec![Value::Array(vec![])],
              Ok(vec![Value::Bool(false)].into()))]
#[case::any_dict_true(r#"any(dict(["a", 1], ["b", 2]), fn(kv): last(kv) == 2;)"#,
              vec![{
                let mut dict = BTreeMap::new();
                dict.insert("a".to_string(), Value::Number(1.into()));
                dict.insert("b".to_string(), Value::Number(2.into()));
                dict.into()
              }],
              Ok(vec![Value::Bool(true)].into()))]
#[case::any_dict_false(r#"any(dict(["a", 1], ["b", 2]), fn(kv): last(kv) == 3;)"#,
              vec![{
                let mut dict = BTreeMap::new();
                dict.insert("a".to_string(), Value::Number(1.into()));
                dict.insert("b".to_string(), Value::Number(2.into()));
                dict.into()
              }],
              Ok(vec![Value::Bool(false)].into()))]
#[case::all_true("
              all([2, 4, 6], fn(x): mod(x, 2) == 0;)",
              vec![Value::Array(vec![Value::Number(2.into()), Value::Number(4.into()), Value::Number(6.into())])],
              Ok(vec![Value::Bool(true)].into()))]
#[case::all_false("
              all([2, 3, 6], fn(x): mod(x, 2) == 0;)",
              vec![Value::Array(vec![Value::Number(2.into()), Value::Number(3.into()), Value::Number(6.into())])],
              Ok(vec![Value::Bool(false)].into()))]
#[case::all_empty_array("
              all([], fn(x): x == 1;)",
              vec![Value::Array(vec![])],
              Ok(vec![Value::Bool(true)].into()))]
#[case::all_dict_true(r#"all(dict(["a", 2], ["b", 4]), fn(kv): mod(last(kv), 2) == 0;)"#,
              vec![{
              let mut dict = BTreeMap::new();
              dict.insert("a".to_string(), Value::Number(2.into()));
              dict.insert("b".to_string(), Value::Number(4.into()));
              dict.into()
              }],
              Ok(vec![Value::Bool(true)].into()))]
#[case::all_dict_false(r#"all(dict(["a", 2], ["b", 3]), fn(kv): mod(last(kv), 2) == 0;)"#,
              vec![{
              let mut dict = BTreeMap::new();
              dict.insert("a".to_string(), Value::Number(2.into()));
              dict.insert("b".to_string(), Value::Number(3.into()));
              dict.into()
              }],
              Ok(vec![Value::Bool(false)].into()))]
#[case::in_array_true("in([1, 2, 3], 2)",
            vec![Value::Number(0.into())],
            Ok(vec![Value::Bool(true)].into()))]
#[case::in_array_false("in([1, 2, 3], 4)",
            vec![Value::Number(0.into())],
            Ok(vec![Value::Bool(false)].into()))]
#[case::in_string_true(r#"in("hello", "ell")"#,
            vec![Value::Number(0.into())],
            Ok(vec![Value::Bool(true)].into()))]
#[case::in_string_false(r#"in("hello", "xyz")"#,
            vec![Value::Number(0.into())],
            Ok(vec![Value::Bool(false)].into()))]
#[case::in_array_true(r#"in(["a", "b", "c"], ["a", "b"])"#,
            vec![Value::Number(0.into())],
            Ok(vec![Value::Bool(true)].into()))]
#[case::in_array_false(r#"in(["a", "c"], ["a", "b"])"#,
            vec![Value::Number(0.into())],
            Ok(vec![Value::Bool(false)].into()))]
#[case::to_csv_single_row(
            "to_csv()",
            vec![Value::Array(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])],
            Ok(vec![Value::String("a,b,c".to_string())].into())
          )]
#[case::to_tsv_single_row(
            "to_tsv()",
            vec![Value::Array(vec![
              Value::String("a".to_string()),
              Value::String("b".to_string()),
              Value::String("c".to_string()),
            ])],
            Ok(vec![Value::String("a\tb\tc".to_string())].into())
          )]
#[case::fold_sum("
            def sum(acc, x):
              add(acc, x);
            | fold([1, 2, 3, 4], 0, sum)
            ",
            vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())])],
            Ok(vec![Value::Number(10.into())].into()))]
#[case::fold_concat(r#"
            def concat(acc, x):
              add(acc, x);
            | fold(["a", "b", "c"], "", concat)
            "#,
            vec![Value::Array(vec![Value::String("a".into()), Value::String("b".into()), Value::String("c".into())])],
            Ok(vec![Value::String("abc".into())].into()))]
#[case::fold_empty("
            def sum(acc, x):
              add(acc, x);
            | fold([], 0, sum)
            ",
            vec![Value::Array(vec![])],
            Ok(vec![Value::Number(0.into())].into()))]
#[case::unique_by_numbers("
            def get_remainder(x):
              mod(x, 3);
            | unique_by([1, 2, 3, 4, 5, 6, 7, 8, 9], get_remainder)
            ",
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into()), Value::Number(5.into()), Value::Number(6.into()), Value::Number(7.into()), Value::Number(8.into()), Value::Number(9.into())])],
              Ok(vec![Value::Array(vec![
              Value::Number(1.into()),
              Value::Number(2.into()),
              Value::Number(3.into()),
              ])].into()))]
#[case::unique_by_strings(r#"
            def get_length(s):
              len(s);
            | unique_by(["cat", "dog", "bird", "fish", "elephant"], get_length)
            "#,
              vec![Value::Array(vec![Value::String("cat".to_string()), Value::String("dog".to_string()), Value::String("bird".to_string()), Value::String("fish".to_string()), Value::String("elephant".to_string())])],
              Ok(vec![Value::Array(vec![
              Value::String("cat".to_string()),
              Value::String("bird".to_string()),
              Value::String("elephant".to_string()),
              ])].into()))]
#[case::unique_by_empty_array("
            def identity(x):
              x;
            | unique_by([], identity)
            ",
              vec![Value::Array(vec![])],
              Ok(vec![Value::Array(vec![])].into()))]
#[case::unique_by_all_same_key(r#"
            def always_same(x):
              "same";
            | unique_by([1, 2, 3, 4], always_same)
            "#,
              vec![Value::Array(vec![Value::Number(1.into()), Value::Number(2.into()), Value::Number(3.into()), Value::Number(4.into())])],
              Ok(vec![Value::Array(vec![Value::Number(1.into())])].into()))]
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
