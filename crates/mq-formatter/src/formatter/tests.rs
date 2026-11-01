use super::*;
use rstest::rstest;

#[rstest]
#[case::empty("", "")]
#[case::pipe_in_call_arg("f(1, 42 | one())", "f(1, 42 | one())")]
#[case::if_(
    "if(test):
        test
        else:
        test2",
    "if (test):
  test
else:
  test2
"
)]
#[case::def_(
    "def name(test):
        test | test2;",
    "def name(test):
  test | test2;
"
)]
#[case::foreach_(
    "foreach(x,array(1, 2, 3)):
        add(x, 1);",
    "foreach (x, array(1, 2, 3)):
  add(x, 1);
"
)]
#[case::def_(
    "def test():
        test1
        |test2
        |test3;",
    "def test():
  test1
  | test2
  | test3;
"
)]
#[case("test()|test2()", "test() | test2()")]
#[case(
    "if(test):
        test
        elif(test2):
        test2
        else:
        test3",
    "if (test):
  test
elif (test2):
  test2
else:
  test3
"
)]
#[case::if_(
    "if(test):
        test
        elif(test2):
        test2
        else:
        test3",
    "if (test):
  test
elif (test2):
  test2
else:
  test3
"
)]
#[case::if_(
    "if(test):
        test
        else:
        test2",
    "if (test):
  test
else:
  test2
"
)]
#[case::if_else(
    "if(test):
        test
        else: do
        test2
        end",
    "if (test):
  test
else: do
    test2
  end
"
)]
#[case::one_line("if(test): test else: test2", "if (test): test else: test2")]
#[case::one_line(
    "if(test): test elif(test2): test2 else: test3",
    "if (test): test elif (test2): test2 else: test3"
)]
#[case::foreach_one_line("foreach(x,array(1,2,3)):add(x,1);", "foreach (x, array(1, 2, 3)): add(x, 1);")]
#[case::foreach_one_line(
    "foreach(x,array(1,2,3)):add(x,1);|add(1,2);",
    "foreach (x, array(1, 2, 3)): add(x, 1); | add(1, 2);"
)]
#[case::foreach_one_line(".[]|upcase()", ".[] | upcase()")]
#[case::while_multiline(
    "while(condition()):
        process();",
    "while (condition()):
  process();
"
)]
#[case::while_oneline("while(condition()): process();", "while (condition()): process();")]
#[case::while_with_pipe(
    "while(check_condition()):
        data
        | process()
        | output();",
    "while (check_condition()):
  data
  | process()
  | output();
"
)]
#[case::loop_multiline(
    "loop:
        process();",
    "loop:
  process();
"
)]
#[case::loop_oneline("loop: process();", "loop: process();")]
#[case::loop_with_pipe(
    "loop:
        data
        | process()
        | output();",
    "loop:
  data
  | process()
  | output();
"
)]
#[case::loop_with_break(
    "loop:
        add(self,1)
        |if(gt(self,5)):break;;",
    "loop:
  add(self, 1)
  | if (gt(self, 5)): break;;
"
)]
#[case::until_multiline(
    "until(condition()):
        process();",
    "until (condition()):
  process();
"
)]
#[case::until_oneline("until(condition()): process();", "until (condition()): process();")]
#[case::until_with_pipe(
    "until(check_condition()):
        data
        | process()
        | output();",
    "until (check_condition()):
  data
  | process()
  | output();
"
)]
#[case::until_with_break(
    "until(false):
        add(self,1)
        |if(gt(self,5)):break;;",
    "until (false):
  add(self, 1)
  | if (gt(self, 5)): break;;
"
)]
#[case::unless_multiline(
    "unless(test):
        test",
    "unless (test):
  test
"
)]
#[case::unless_oneline("unless(test): test", "unless (test): test")]
#[case::def(
    r##".h
| let link = to_link(add("#", to_text(self)), to_text(self), "")
| if (eq(to_md_name(), "h1")):
to_md_list(link, 1)
elif (eq(to_md_name(),"h2")):
to_md_list(link, 2)
elif (eq(to_md_name(), "h3")):
to_md_list(link, 3)
elif (eq(to_md_name(), "h4")):
to_md_list(link, 4)
elif (eq(to_md_name(), "h5")):
to_md_list(link, 5)
else:
None"##,
    r##".h
| let link = to_link(add("#", to_text(self)), to_text(self), "")
| if (eq(to_md_name(), "h1")):
    to_md_list(link, 1)
  elif (eq(to_md_name(), "h2")):
    to_md_list(link, 2)
  elif (eq(to_md_name(), "h3")):
    to_md_list(link, 3)
  elif (eq(to_md_name(), "h4")):
    to_md_list(link, 4)
  elif (eq(to_md_name(), "h5")):
    to_md_list(link, 5)
  else:
    None
"##
)]
#[case::def(
    r#"def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
  let first_char = upcase(first(word))
  | let rest_str = downcase(slice(word, 1, len(word)))
  | s"${first_char}${rest_str}";
  | join("");
| snake_to_camel()"#,
    r#"def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("");
| snake_to_camel()
"#
)]
#[case::def(
        r#"def snake_to_camel(x): let words = split(x, "_") | foreach (word, words): let first_char = upcase(first(word)) | let rest_str = downcase(slice(word, 1, len(word))) | add(first_char, rest_str); | join("");| snake_to_camel()"#,
        r#"def snake_to_camel(x): let words = split(x, "_") | foreach (word, words): let first_char = upcase(first(word)) | let rest_str = downcase(slice(word, 1, len(word))) | add(first_char, rest_str); | join(""); | snake_to_camel()"#
    )]
#[case::let_(r#"let test = "test""#, r#"let test = "test""#)]
#[case::call(
    r#"test(
"test")"#,
    r#"test(
  "test")
"#
)]
#[case::call(
    r#"test(
"test"
  )"#,
    r#"test(
  "test"
)
"#
)]
#[case::call(
    r#"test(
"test"
,"test"
,true
  )"#,
    r#"test(
  "test"
  ,"test"
  ,true
)
"#
)]
#[case::interpolated_string(
    r#"test(
s"test${val1}"
  )"#,
    r#"test(
  s"test${val1}"
)
"#
)]
#[case::include("include  \"test.mq\"", "include \"test.mq\"")]
#[case::import("import  \"test.mq\"", "import \"test.mq\"")]
#[case::import_as("import  \"test.mq\"   as   m", "import \"test.mq\" as m")]
#[case::nodes("nodes|nodes", "nodes | nodes")]
#[case::fn_("fn(): program;", "fn(): program;")]
#[case::fn_multiline(
    "fn(arg1,arg2):
        program;",
    "fn(arg1, arg2):
  program;
"
)]
#[case::fn_args("map( fn():program;)", "map(fn(): program;)")]
#[case::array_trailing_comma("[1, 2,]", "[1, 2,]")]
#[case::dict_trailing_comma("{\"a\": 1,}", "{\"a\": 1,}")]
#[case::dict_trailing_comma_multiline("{\n  \"a\": 1,\n  \"b\": 2,\n}", "{\n  \"a\": 1,\n  \"b\": 2,\n}\n")]
#[case::array_comment_before_close("[\n  1,\n  # c\n]", "[\n  1,\n  # c\n]\n")]
#[case::dict_comment_before_close("{\n  \"a\": 1\n  # c\n}", "{\n  \"a\": 1\n  # c\n}\n")]
#[case::array_same_line_comment_before_close("[1, 2, # c\n]", "[1, 2, # c\n]\n")]
#[case::array_comments_before_close("[\n  1,\n\n  # a\n  # b\n]", "[\n  1,\n  # a\n  # b\n]\n")]
#[case::number_small_fraction("1e-9", "0.000000001")]
#[case::number_long_fraction("3.14159265358979", "3.14159265358979")]
#[case::number_integral_float("2.0", "2")]
#[case::array_empty("[]", "[]")]
#[case::array_single_element("[1]", "[1]")]
#[case::array_multiple_elements("[1,2,3]", "[1, 2, 3]")]
#[case::array_mixed_types("[1,\"test\",true]", "[1, \"test\", true]")]
#[case::array_nested("[[1,2],[3,4]]", "[[1, 2], [3, 4]]")]
#[case::array_with_spaces("[ 1 , 2 , 3 ]", "[1, 2, 3]")]
#[case::array_multiline(
    r#"[
    1,
    2,
    3
    ]"#,
    "[\n  1,\n  2,\n  3\n]\n"
)]
#[case::let_with_array(r#"let arr = [1, 2, 3]"#, r#"let arr = [1, 2, 3]"#)]
#[case::let_with_array_multiline(
    r#"let arr = [
1,
2,
3
]"#,
    "let arr = [\n  1,\n  2,\n  3\n]\n"
)]
#[case::dict_empty("{}", "{}")]
#[case::def_with_let_and_array(
    r#"def foo():
  let arr = [
  1,
  2,
  3];
"#,
    r#"def foo():
  let arr = [
    1,
    2,
    3
  ];
"#
)]
#[case::dict_single_pair("{\"key\": \"value\"}", "{\"key\": \"value\"}")]
#[case::dict_multiple_pairs(
    "{\"key1\": \"value1\", \"key2\": \"value2\"}",
    "{\"key1\": \"value1\", \"key2\": \"value2\"}"
)]
#[case::dict_with_spaces("{ \"key\" : \"value\" }", "{\"key\": \"value\"}")]
#[case::dict_mixed_types(
    "{\"str\": \"value\", \"num\": 42, \"bool\": true}",
    "{\"str\": \"value\", \"num\": 42, \"bool\": true}"
)]
#[case::dict_nested("{\"outer\": {\"inner\": \"value\"}}", "{\"outer\": {\"inner\": \"value\"}}")]
#[case::dict_multiline(
    r#"{
"key1": "value1",
"key2": "value2"
}"#,
    "{\n  \"key1\": \"value1\",\n  \"key2\": \"value2\"\n}\n"
)]
#[case::dict_multiline_mixed(
    r#"{
"str": "value",
"num": 42,
"bool": true
}"#,
    "{\n  \"str\": \"value\",\n  \"num\": 42,\n  \"bool\": true\n}\n"
)]
#[case::equal_operator("let x = 1 == 2", "let x = 1 == 2")]
#[case::not_equal_operator("let y = 3 != 4", "let y = 3 != 4")]
#[case::string_with_newline(r#""line1\nline2""#, r#""line1\nline2""#)]
#[case::plus_operator("let x = 1 + 2", "let x = 1 + 2")]
#[case::let_newline_after_equal(
    r#"let x =
"test""#,
    r#"let x =
  "test"
"#
)]
#[case::let_with_if_multiline(
    r#"let x = if(test):
test
else:
test2"#,
    r#"let x = if (test):
  test
else:
  test2
"#
)]
#[case::let_with_while_multiline(
    r#"let x = while(condition()):
process();"#,
    r#"let x = while (condition()):
  process();
"#
)]
#[case::less_than_operator("let x = 1 < 2", "let x = 1 < 2")]
#[case::less_than_equal_operator("let x = 1 <= 2", "let x = 1 <= 2")]
#[case::greater_than_operator("let x = 2 > 1", "let x = 2 > 1")]
#[case::greater_than_equal_operator("let x = 2 >= 1", "let x = 2 >= 1")]
#[case::range_operator("1..1", "1..1")]
#[case::range_operator_with_spaces("1 .. 1", "1..1")]
#[case::range_operator_with_variables("x..y", "x..y")]
#[case::range_operator_with_string(r#""1" .. "2""#, r#""1".."2""#)]
#[case::selector_attr(".code.lang", ".code.lang")]
#[case::standalone_attr_selector(".lang", ".lang")]
#[case::standalone_attr_selector_value(".value", ".value")]
#[case::env("let ENV = $env", "let ENV = $env")]
#[case::mul("1 * 1", "1 * 1")]
#[case::mul("1 / 1", "1 / 1")]
#[case::and("true && false", "true && false")]
#[case::or("true || false", "true || false")]
#[case::binary_op_multiline_or(
    r#"1
|| 2
|| 3"#,
    "1
 || 2
 || 3
"
)]
#[case::binary_op_multiline(
    r#"let v = 1
|| 2
|| 3"#,
    "let v = 1
   || 2
   || 3
"
)]
#[case::def_contains(
    r#"def contains(haystack, needle):
if (is_dict(haystack)):
  not(is_none(get(haystack, needle)))
else:
  index(haystack, needle) != -1;
"#,
    r#"def contains(haystack, needle):
  if (is_dict(haystack)):
    not(is_none(get(haystack, needle)))
  else:
    index(haystack, needle) != -1;
"#
)]
#[case::escape_sequence_clear_screen(r#""\x1b[2J\x1b[H""#, r#""\x1b[2J\x1b[H""#)]
#[case::control_character_bell(r#""\x07""#, r#""\x07""#)]
#[case::control_character_backspace(r#""\x08""#, r#""\x08""#)]
#[case::control_character_vertical_tab(r#""\x0b""#, r#""\x0b""#)]
#[case::control_character_form_feed(r#""\x0c""#, r#""\x0c""#)]
#[case::control_character_escape(r#""\x1b""#, r#""\x1b""#)]
#[case::control_character_delete(r#""\x7f""#, r#""\x7f""#)]
#[case::not_operator("!true", "!true")]
#[case::let_with_if_multiline_in_while(
    r#"while(condition()):
  let x = 1
  | let y = if(test):
test
else:
test2
end
"#,
    r#"while (condition()):
  let x = 1
  | let y = if (test):
      test
    else:
      test2
end
"#
)]
#[case::let_with_while_multiline(
    r#"let x = while(condition()):
process();"#,
    r#"let x = while (condition()):
  process();
"#
)]
#[case::let_with_while_multiline2(
    r#""test"
| let x = while(condition()):
process();"#,
    r#""test"
| let x = while (condition()):
    process();
"#
)]
#[case::pipe_let_block_in_nested_do(
    "def f(t):\n  if (t): 1\n  else: do\n      let a = 1\n      | let b = foreach (r, t):\n      r\n      end\n      | let c = if (a):\n      b\n      else:\n      a\n      | c\n    end\nend",
    "def f(t):\n  if (t): 1\n  else: do\n      let a = 1\n      | let b = foreach (r, t):\n          r\n        end\n      | let c = if (a):\n          b\n        else:\n          a\n      | c\n    end\nend\n"
)]
#[case::fn_arg_after_pipe(
    "def f(xs):\n  let t = 1\n  | fold(xs, 0, fn(acc, x):\n  let y = x + 1\n  | acc + y;\n  )\nend",
    "def f(xs):\n  let t = 1\n  | fold(xs, 0, fn(acc, x):\n      let y = x + 1\n      | acc + y;\n    )\nend\n"
)]
#[case::fn_arg_after_pipe_in_do(
    "let t = 1\n| do\n    let u = 2\n    | filter(xs, fn(x):\n    x > u;\n    )\n  end",
    "let t = 1\n| do\n    let u = 2\n    | filter(xs, fn(x):\n        x > u;\n      )\n  end\n"
)]
#[case::array_index_access("let arr = [1, 2, 3]\n|arr[1]", "let arr = [1, 2, 3]\n| arr[1]\n")]
#[case::array_index_access_inline("arr[0]", "arr[0]")]
#[case::index_on_dict_literal_after_def("def f(x): x;\n{\"a\":1}[\"a\"]", "def f(x): x;\n{\"a\": 1}[\"a\"]\n")]
#[case::slice_on_string_literal_after_def("def f(x): x;\n\"abc\"[1:2]", "def f(x): x;\n\"abc\"[1:2]\n")]
#[case::index_on_array_literal_in_body("def f(x):\n  1\n  |[1][0]\nend", "def f(x):\n  1\n  | [1][0]\nend\n")]
#[case::dict_index_access(
    "let d = {\"key\": \"value\"}\n|d[\"key\"]",
    "let d = {\"key\": \"value\"}\n| d[\"key\"]\n"
)]
#[case::dict_index_access_inline("d[\"key\"]", "d[\"key\"]")]
#[case::comment_first_line("# comment\nlet x = 1", "# comment\nlet x = 1\n")]
#[case::string_keeps_unicode_escape(r#"let x = "\u{D7FF}\u{E000}""#, r#"let x = "\u{D7FF}\u{E000}""#)]
#[case::string_keeps_hex_escape(r#"let x = "\x41""#, r#"let x = "\x41""#)]
#[case::string_keeps_regex_escapes(r#"let x = "\d+\s\/""#, r#"let x = "\d+\s\/""#)]
#[case::string_escapes_raw_tab("let x = \"a\tb\"", r#"let x = "a\tb""#)]
#[case::string_escapes_raw_newline("let x = \"a\nb\"", "let x = \"a\\nb\"")]
#[case::dict_key_keeps_unicode_escape(r#"{"\u{E000}": 1}"#, r#"{"\u{E000}": 1}"#)]
#[case::interpolated_string_keeps_unicode_escape(r#"s"\u{E000}${x}""#, r#"s"\u{E000}${x}""#)]
#[case::string_after_multibyte_text(r#"let s = "日本語" | let x = "\x41""#, r#"let s = "日本語" | let x = "\x41""#)]
#[case::string_after_leading_tab("\tlet x = \"\\u{41}\"", r#"let x = "\u{41}""#)]
#[case::string_with_crlf("let x = 1\r\n| let y = \"\\u{41}\"\r\n", "let x = 1\n| let y = \"\\u{41}\"\n")]
#[case::string_keeps_quote_and_backslash(r#"let x = "a\"b\\c""#, r#"let x = "a\"b\\c""#)]
#[case::string_empty(r#"let x = """#, r#"let x = """#)]
#[case::string_multiline_keeps_escape_on_second_line("let x = \"a\n  \\u{41}\"", r#"let x = "a\n  \u{41}""#)]
#[case::array_keeps_escapes(r#"["\u{41}", "\x42"]"#, r#"["\u{41}", "\x42"]"#)]
#[case::match_pattern_keeps_escape(r#"match (x): | "\u{41}": 1 | _: 2 end"#, r#"match (x): | "\u{41}": 1 | _: 2 end"#)]
#[case::symbol_keeps_escape(r#":"\u{41}""#, r#":"\u{41}""#)]
#[case::interpolated_string_keeps_quote_and_backslash(r#"s"\"${a}\\""#, r#"s"\"${a}\\""#)]
#[case::interpolated_string_keeps_raw_newline("s\"a\n${x}\"", "s\"a\n${x}\"\n")]
#[case::array_arg_after_pipe(
    "def f():\n  let r = 1\n  | g(r, [\n  1,\n  2,\n  ])\nend",
    "def f():\n  let r = 1\n  | g(r, [\n      1,\n      2,\n    ])\nend\n"
)]
#[case::dict_arg_after_pipe(
    "def f():\n  let r = 1\n  | g(r, {\n  \"a\": 1,\n  })\nend",
    "def f():\n  let r = 1\n  | g(r, {\n      \"a\": 1,\n    })\nend\n"
)]
#[case::dict_after_pipe(
    "def f():\n  let r = 1\n  | {\n  a: r\n  }\nend",
    "def f():\n  let r = 1\n  | {\n      a: r\n    }\nend\n"
)]
#[case::array_arg_without_pipe("def f():\n  g(r, [\n  1,\n  ])\nend", "def f():\n  g(r, [\n    1,\n  ])\nend\n")]
#[case::comment_keeps_leading_spaces("#   [1, 2],\nlet x = 1", "#   [1, 2],\nlet x = 1\n")]
#[case::comment_keeps_leading_tab("#\tcomment\nlet x = 1", "#\tcomment\nlet x = 1\n")]
#[case::comment_adds_space_when_missing("#comment\nlet x = 1", "# comment\nlet x = 1\n")]
#[case::comment_trims_trailing_spaces("# comment   \nlet x = 1", "# comment\nlet x = 1\n")]
#[case::comment_keeps_leading_spaces_before_close("[\n  1,\n  #   c\n]", "[\n  1,\n  #   c\n]\n")]
#[case::comment_inline("let x = 1 # inline comment", "let x = 1 # inline comment")]
#[case::comment_multiline(
    "let x = 1\n# multiline comment\n| let y = 2",
    "let x = 1\n# multiline comment\n| let y = 2\n"
)]
#[case::comment_after_expr_multiline(
    "if(test):\n  test # comment\nelse:\n  test2 # comment2",
    "if (test):\n  test # comment\nelse:\n  test2 # comment2\n"
)]
#[case::comment_after_expr_inline(
    "if(test): test # comment else: test2 # comment2",
    "if (test): test # comment else: test2 # comment2"
)]
#[case::fn_as_call_arg_single_line("map(fn(): program;)", "map(fn(): program;)")]
#[case::fn_as_call_arg_multi_line("map(\n  fn(arg):\n    process(arg);\n)", "map(\n  fn(arg):\n    process(arg);\n)\n")]
#[case::fn_as_call_arg_with_other_args("map(fn(): program;, 1, \"test\")", "map(fn(): program;, 1, \"test\")")]
#[case::nested_fn_as_call_arg("outer(map(fn(): inner();))", "outer(map(fn(): inner();))")]
#[case::fn_as_call_arg_with_multiline_args(
    "map(\n  fn(x):\n    process(x);\n  ,\n  fn(y):\n    process(y);\n)",
    "map(\n  fn(x):\n    process(x);\n  ,\n  fn(y):\n    process(y);\n)\n"
)]
#[case::fn_arg_without_terminator("map(xs, fn(x): x + 1)", "map(xs, fn(x): x + 1)")]
#[case::fn_arg_without_terminator_before_comma("map(fn(x): x + 1, 1, \"test\")", "map(fn(x): x + 1, 1, \"test\")")]
#[case::fn_arg_without_terminator_with_pipe(
    "filter(xs, fn(x): x | select(.h), 1)",
    "filter(xs, fn(x): x | select(.h), 1)"
)]
#[case::nested_fn_arg_without_terminator("outer(map(xs, fn(x): inner(x)))", "outer(map(xs, fn(x): inner(x)))")]
#[case::fn_arg_without_terminator_multi_line(
    "map(\n  fn(arg):\n    process(arg)\n)",
    "map(\n  fn(arg):\n    process(arg)\n)\n"
)]
#[case::fn_arg_without_terminator_after_pipe(
    "let t = 1\n| fold(xs, 0, fn(acc, x):\n  let y = x + 1\n  | acc + y\n  )",
    "let t = 1\n| fold(xs, 0, fn(acc, x):\n    let y = x + 1\n    | acc + y\n  )\n"
)]
#[case::fn_without_params("map(xs, fn: self + 1)", "map(xs, fn: self + 1)")]
#[case::fn_without_params_with_pipe(
    "filter(xs, fn: select(.h || .code) | !is_none, 1)",
    "filter(xs, fn: select(.h || .code) | !is_none, 1)"
)]
#[case::fn_without_params_nested("map(xs, fn: map(fn: to_text))", "map(xs, fn: map(fn: to_text))")]
#[case::fn_without_params_terminated("let f = fn: self + 1;", "let f = fn: self + 1;")]
#[case::fn_without_params_spacing("map(xs, fn : self)", "map(xs, fn: self)")]
#[case::fn_without_params_multi_line("map(\n  fn:\n    process(self)\n)", "map(\n  fn:\n    process(self)\n)\n")]
#[case::group_simple("(1)", "(1)")]
#[case::group_with_expr("(1 + 2)", "(1 + 2)")]
#[case::group_with_nested_group("((1 + 2) * 3)", "((1 + 2) * 3)")]
#[case::group_with_multiple_ops("(1 + 2 * 3)", "(1 + 2 * 3)")]
#[case::group_with_array("(array(1, 2))", "(array(1, 2))")]
#[case::group_with_dict("({\"key\": \"value\"})", "({\"key\": \"value\"})")]
#[case::group_with_comment("(1 + 2) # group comment", "(1 + 2) # group comment")]
#[case::group_with_call("(test(1, 2))", "(test(1, 2))")]
#[case::group_with_if("(if(test): test else: test2)", "(if (test): test else: test2)")]
#[case::fn_end("fn(): test end", "fn(): test end")]
#[case::negate_operator("-v", "-v")]
#[case::try_catch_multiline(
    r#"try:
  process()
catch:
  handle_error()"#,
    "try:
  process()
catch:
  handle_error()
"
)]
#[case::try_catch_oneline("try: process() catch: handle_error()", "try: process() catch: handle_error()")]
#[case::try_catch_with_binder(
    "try: process() catch(e): handle_error(e)",
    "try: process() catch(e): handle_error(e)"
)]
#[case::try_catch_with_binder_extra_spaces(
    "try: process() catch( e ): handle_error(e)",
    "try: process() catch(e): handle_error(e)"
)]
#[case::try_catch_with_binder_multiline(
    r#"try:
  process()
catch(e):
  handle_error(e)"#,
    "try:
  process()
catch(e):
  handle_error(e)
"
)]
#[case::try_catch_with_finally(
    r#"try:
  process()
catch:
  handle_error()
"#,
    "try:
  process()
catch:
  handle_error()
"
)]
#[case::coalesce_operator("let x = a?? b", "let x = a ?? b")]
#[case::coalesce_operator_with_call("let result = get_value() ?? default()", "let result = get_value() ?? default()")]
#[case::coalesce_operator_with_literal("let x = value ?? 42", "let x = value ?? 42")]
#[case::coalesce_operator_with_string("let s = str ?? \"default\"", "let s = str ?? \"default\"")]
#[case::coalesce_operator_chain("let x = a ?? b ?? c", "let x = a ?? b ?? c")]
#[case::coalesce_operator_in_if(
    "if(a ?? b): do_something() else: do_other()",
    "if (a ?? b): do_something() else: do_other()"
)]
#[case::coalesce_operator_in_array("[a ?? b, c]", "[a ?? b, c]")]
#[case::coalesce_operator_in_dict("{\"key\": a ?? b}", "{\"key\": a ?? b}")]
#[case::coalesce_operator_with_comment("let x = a ?? b # fallback", "let x = a ?? b # fallback")]
#[case::call_dynamic("v[0](1,2,3)", "v[0](1, 2, 3)")]
#[case::dict_with_fn_value_multiline(
    r#"{
"key1": fn():
process();
,"key2": "value2"
}"#,
    r#"{
  "key1": fn():
    process();
  ,"key2": "value2"
}
"#
)]
#[case::do_block_multiline(
    r#"do
  process1()
  | process2()
end
"#,
    "do
  process1()
  | process2()
end
"
)]
#[case::do_block_oneline("do process1() | process2();", "do process1() | process2();")]
#[case::let_with_do_block_multiline(
    r#"let result = do
  step1()
  | step2();
"#,
    "let result = do
  step1()
  | step2();
"
)]
#[case::let_with_do_block_oneline("let result = do step1() | step2();", "let result = do step1() | step2();")]
#[case::symbol_with_ident(":foo", ":foo")]
#[case::symbol_with_string(r#":"bar""#, r#":"bar""#)]
#[case::symbol_with_spaces(":  foo", ":foo")]
#[case::symbol_in_array("[:foo, :bar]", "[:foo, :bar]")]
#[case::symbol_in_dict(r#"{key: :value}"#, r#"{key: :value}"#)]
#[case::symbol_comparison(":foo == :bar", ":foo == :bar")]
#[case::match_simple(
    "match(x): | 1: \"one\" | _: \"other\" end",
    "match (x): | 1: \"one\" | _: \"other\" end"
)]
#[case::match_multiline(
    r#"match(x):
| 1: "one"
| 2: "two"
| _: "other"
end"#,
    r#"match (x):
  | 1: "one"
  | 2: "two"
  | _: "other"
end
"#
)]
#[case::match_with_guard(
    "match(x): | n if(n > 0): \"positive\" | _: \"non-positive\" end",
    "match (x): | n if (n > 0): \"positive\" | _: \"non-positive\" end"
)]
#[case::match_with_array_pattern(
    "match(arr): | [a, b]: add(a, b) | _: 0 end",
    "match (arr): | [a, b]: add(a, b) | _: 0 end"
)]
#[case::match_with_array_pattern_with_literal(
    "match(arr): | [1, 2]: add(1, 2) | _: 0 end",
    "match (arr): | [1, 2]: add(1, 2) | _: 0 end"
)]
#[case::match_with_array_pattern_with_symbol(
    "match(arr): | [:string, :string]: add(1, 2) | _: 0 end",
    "match (arr): | [:string, :string]: add(1, 2) | _: 0 end"
)]
#[case::match_with_dict_pattern(
    "match(obj): | {name: n}: n | _: \"unknown\" end",
    "match (obj): | {name: n}: n | _: \"unknown\" end"
)]
#[case::match_nested_in_let(
    r#"let result = match(x):
| 1: "one"
| 2: "two"
| _: "other"
end"#,
    r#"let result = match (x):
  | 1: "one"
  | 2: "two"
  | _: "other"
end
"#
)]
#[case::match_with_type_pattern(
    "match(val): | :string: \"is string\" | :number: \"is number\" | _: \"other\" end",
    "match (val): | :string: \"is string\" | :number: \"is number\" | _: \"other\" end"
)]
#[case::match_multiline_in_pipe(
    r#""test"
| match(x):
  | 1: "one"
  | 2: "two"
  end"#,
    r#""test"
| match (x):
    | 1: "one"
    | 2: "two"
  end
"#
)]
#[case::dict_nested_multiline_level3(
    r#"{
"level1": {
"level2": {
"level3": "value"
}
}
}"#,
    r#"{
  "level1": {
    "level2": {
      "level3": "value"
    }
  }
}
"#
)]
#[case::array_nested_multiline_level3(
    r#"[
[
[
"value"
]
]
]"#,
    r#"[
  [
    [
      "value"
    ]
  ]
]
"#
)]
#[case::comment_with_newline("# comment\nlet x = 1", "# comment\nlet x = 1\n")]
#[case::comment_with_indent(
    "if(test):\n  test # comment\nelse:\n  test2 # comment2",
    "if (test):\n  test # comment\nelse:\n  test2 # comment2\n"
)]
#[case::comment_inline_with_indent("let x = 1 # inline comment", "let x = 1 # inline comment")]
#[case::comment_multiline_with_indent(
    "let x = 1\n  # multiline comment\n| let y = 2",
    "let x = 1\n# multiline comment\n| let y = 2\n"
)]
#[case::comment_after_expr_multiline_with_indent(
    "if(test):\n  test # comment\nelse:\n    test2 # comment2",
    "if (test):\n  test # comment\nelse:\n  test2 # comment2\n"
)]
#[case::comment_after_expr_inline_with_indent(
    "if(test): test # comment else:    test2 # comment2",
    "if (test): test # comment else:    test2 # comment2"
)]
#[case::interpolated_string_with_escaped_brackets(r#"s"\\[${phrase}\\]\\(""#, r#"s"\\[${phrase}\\]\\(""#)]
#[case::interpolated_string_with_backslash(r#"s"\\test""#, r#"s"\\test""#)]
#[case::module_with_body(
    r#"module test:
import "foo.mq"
| def main(): test();
end"#,
    r#"module test:
  import "foo.mq"
  | def main(): test();
end
"#
)]
#[case::module_with_do(
    r#"module test  do
import "foo.mq"
| def main(): test();
end"#,
    r#"module test do
  import "foo.mq"
  | def main(): test();
end
"#
)]
#[case::module_with_do_and_new_line(
    r#"module test
do
import "foo.mq"
| def main(): test();
end"#,
    r#"module test
  do
  import "foo.mq"
  | def main(): test();
end
"#
)]
#[case::comment_preserves_indent_after_newline(
    "let x = 1\n    # indented comment after newline\n| let y = 2",
    "let x = 1\n# indented comment after newline\n| let y = 2\n"
)]
#[case::comment_preserves_indent_after_newline_deep(
    "if(test):\n  test\n    # deeper indented comment\nelse:\n  test2",
    "if (test):\n  test\n# deeper indented comment\nelse:\n  test2\n"
)]
#[case::comment_preserves_indent_after_newline_array(
    "[1,\n    # comment for 2\n  2]",
    "[1,\n  # comment for 2\n  2]\n"
)]
#[case::comment_preserves_indent_after_newline_dict(
    "{\n  \"a\": 1,\n    # comment for b\n  \"b\": 2\n}",
    "{\n  \"a\": 1,\n  # comment for b\n  \"b\": 2\n}\n"
)]
#[case::match_preserves_indent_after_newline(
    "let v = \nmatch (x):\n|    1: \"one\"\n|    2: \"two\"\n  end",
    "let v =\n  match (x):\n    | 1: \"one\"\n    | 2: \"two\"\n  end\n"
)]
#[case::match_with_do_block(
    "let result = match (x):\n| 1: do\n    foo() |\n    bar()\n  end\n| 2: \"two\"\nend",
    "let result = match (x):\n  | 1: do\n      foo() |\n      bar()\n    end\n  | 2: \"two\"\nend\n"
)]
#[case::assign("var i=0 | i=i + 1", "var i = 0 | i = i + 1")]
#[case::assign_right_multiline(
    r#"let x =
"test""#,
    r#"let x =
  "test"
"#
)]
#[case::assign_right_multiline_pipe(
    r#"let x =
"test"
| upcase()"#,
    r#"let x =
  "test"
| upcase()
"#
)]
#[case::assign_right_multiline_if(
    r#"let x =
if(test):
test
else:
test2"#,
    r#"let x =
  if (test):
    test
  else:
    test2
"#
)]
#[case::if_without_colon_oneline("if(test) 1 else 2", "if (test) 1 else 2")]
#[case::if_without_colon_multiline(
    r#"if(test)
test
else
test2"#,
    r#"if (test)
  test
else
  test2
"#
)]
#[case::if_elif_else_without_colon_oneline("if(test) 1 elif(test2) 2 else 3", "if (test) 1 elif (test2) 2 else 3")]
#[case::if_elif_else_without_colon_multiline(
    r#"if(test)
test
elif(test2)
test2
else
test3"#,
    r#"if (test)
  test
elif (test2)
  test2
else
  test3
"#
)]
#[case::if_without_colon_with_do_block(
    r#"if(test) do
test
end
else do
test2
end"#,
    r#"if (test) do
    test
  end
else do
    test2
  end
"#
)]
#[case::while_do_end_oneline("while(x > 0) do x - 1 end", "while (x > 0) do x - 1 end")]
#[case::while_do_end_multiline(
    r#"while(x > 0) do
let x = x - 1 | x
end"#,
    r#"while (x > 0) do
  let x = x - 1 | x
end
"#
)]
#[case::while_do_end_with_break(
    r#"while(x < 10) do
let x = x + 1
| if(x == 3):
break
else:
x
end"#,
    r#"while (x < 10) do
  let x = x + 1
  | if (x == 3):
      break
    else:
      x
end
"#
)]
#[case::foreach_do_end_oneline(
    "foreach(item, arr) do process(item) end",
    "foreach (item, arr) do process(item) end"
)]
#[case::foreach_do_end_multiline(
    r#"foreach(x, array(1, 2, 3)) do
add(x, 1)
end"#,
    r#"foreach (x, array(1, 2, 3)) do
  add(x, 1)
end
"#
)]
#[case::foreach_do_end_with_continue(
    r#"foreach(x, array(1, 2, 3, 4, 5)) do
if(x == 3):
continue
else:
x + 10
end"#,
    r#"foreach (x, array(1, 2, 3, 4, 5)) do
  if (x == 3):
    continue
  else:
    x + 10
end
"#
)]
#[case::foreach_do_end_nested(
    r#"foreach(row, arr) do
foreach(x, row) do
x * 2
end
end"#,
    r#"foreach (row, arr) do
  foreach (x, row) do
    x * 2
  end
end
"#
)]
#[case::match_do_end_oneline(
    "match(x) do | 1: \"one\" | 2: \"two\" | _: \"other\" end",
    "match (x) do | 1: \"one\" | 2: \"two\" | _: \"other\" end"
)]
#[case::match_do_end_multiline(
    r#"match(2) do
| 1: "one"
| 2: "two"
| _: "other"
end"#,
    r#"match (2) do
  | 1: "one"
  | 2: "two"
  | _: "other"
end
"#
)]
#[case::match_do_end_type_pattern(
    r#"match(array(1, 2, 3)) do
| :array: "is_array"
| :number: "is_number"
| _: "other"
end"#,
    r#"match (array(1, 2, 3)) do
  | :array: "is_array"
  | :number: "is_number"
  | _: "other"
end
"#
)]
#[case::match_do_end_with_guard(
    r#"match(x) do
| n if(n > 0): "positive"
| _: "non-positive"
end"#,
    r#"match (x) do
  | n if (n > 0): "positive"
  | _: "non-positive"
end
"#
)]
#[case::def_with_default_single(
    "def greet(name = \"World\"):
        name;",
    "def greet(name = \"World\"):
  name;
"
)]
#[case::def_with_multiple_defaults(
    "def foo(a, b = 2, c = 3):
        a + b + c;",
    "def foo(a, b = 2, c = 3):
  a + b + c;
"
)]
#[case::def_with_mixed_params(
    "def bar(x,y=10):
        x+y;",
    "def bar(x, y = 10):
  x + y;
"
)]
#[case::def_with_array_default(
    "def baz(x,y=[1,2,3]):
        x+len(y);",
    "def baz(x, y = [1, 2, 3]):
  x + len(y);
"
)]
#[case::def_with_ident_default(
    "def f(x = y):
        x;",
    "def f(x = y):
  x;
"
)]
#[case::def_with_string_default(
    "def test(a,b=\"test\"):
        a+b;",
    "def test(a, b = \"test\"):
  a + b;
"
)]
#[case::def_with_number_default(
    "def calc(x=42):
        x * 2;",
    "def calc(x = 42):
  x * 2;
"
)]
#[case::def_with_boolean_default(
    "def result(x=true):
        x;",
    "def result(x = true):
  x;
"
)]
#[case::def_all_params_with_defaults(
    "def calc(a=1,b=2,c=3):
        a+b+c;",
    "def calc(a = 1, b = 2, c = 3):
  a + b + c;
"
)]
#[case::def_expr_with_default(
    "def calc(a=1,b=2,c=3 + 1):
        a+b+c;",
    "def calc(a = 1, b = 2, c = 3 + 1):
  a + b + c;
"
)]
#[case::def_expr_with_variadic(
    "def calc(v,*args):
        v + args;",
    "def calc(v, *args):
  v + args;
"
)]
#[case::assign_with_selector_attr("value.url|= value.alt", "value.url |= value.alt")]
#[case::index_assign("arr[0]=10", "arr[0] = 10")]
#[case::self_attribute("self.level", "self.level")]
#[case::dot_attribute(". .level", ". .level")]
#[case::dot_attribute_comment(". # note\n .level", ". # note\n  .level\n")]
#[case::self_attribute_comment("self # note\n .level", "self # note\n  .level\n")]
#[case::ident_attribute_comment("x # note\n .level", "x # note\n  .level\n")]
#[case::dot_descendant(". .li", ". .li")]
#[case::recursive_name("..h", "..h")]
#[case::recursive_key("..\"key\"", "..\"key\"")]
#[case::recursive_name_in_args("f(..h, 1)", "f(..h, 1)")]
#[case::index_assign_string_key("dict[\"key\"]=\"value\"", "dict[\"key\"] = \"value\"")]
#[case::index_compound_assign("arr[0]+=1", "arr[0] += 1")]
#[case::selector_call_heading_single_arg(".h(1)", ".h(1)")]
#[case::selector_call_heading_multi_arg(".h(1,2)", ".h(1, 2)")]
#[case::selector_call_code_lang(".code(\"rust\")", ".code(\"rust\")")]
#[case::selector_call_pipe(".h(1)|.text()", ".h(1) | .text()")]
#[case::descendant_chain(".blockquote .code", ".blockquote .code")]
#[case::descendant_chain_no_space(".blockquote.code", ".blockquote .code")]
#[case::descendant_chain_extra_space(".blockquote  .code", ".blockquote .code")]
#[case::descendant_chain_three_levels(".blockquote .list .code", ".blockquote .list .code")]
#[case::descendant_chain_with_selector_call(".blockquote .code(\"rust\")", ".blockquote .code(\"rust\")")]
#[case::descendant_chain_trailing_attr(".blockquote .code.lang", ".blockquote .code.lang")]
#[case::arrow_simple("->(): program;", "->(): program;")]
#[case::arrow_multiline(
    "->(arg1,arg2):
        program;",
    "->(arg1, arg2):
  program;
"
)]
#[case::arrow_as_call_arg("map( ->():program;)", "map(->(): program;)")]
#[case::arrow_as_call_arg_single_line("map(->(): program;)", "map(->(): program;)")]
#[case::arrow_as_call_arg_with_other_args("map(->(): program;, 1, \"test\")", "map(->(): program;, 1, \"test\")")]
#[case::nested_arrow_as_call_arg("outer(map(->(): inner();))", "outer(map(->(): inner();))")]
#[case::arrow_end("->(): test end", "->(): test end")]
#[case::bytes_literal_basic(r#"b"abc""#, r#"b"abc""#)]
#[case::bytes_literal_hex(r#"b"\xf0\x9f\x99\x82""#, r#"b"\xf0\x9f\x99\x82""#)]
#[case::bytes_literal_with_pipe(r#"b"abc"  |  len"#, r#"b"abc" | len"#)]
#[case::bytes_literal_in_call(r#"len(b"abc")"#, r#"len(b"abc")"#)]
#[case::as_binding_basic("42 as x | x", "42 as x | x")]
#[case::as_binding_spaces("42  as  x  |  x", "42 as x | x")]
#[case::as_binding_selector(".text as title | title", ".text as title | title")]
#[case::def_with_do_block_body(
    r#"def test(x):
  do
    step1(x)
    | step2(x)
  end;"#,
    r#"def test(x):
  do
    step1(x)
    | step2(x)
  end;
"#
)]
#[case::foreach_colon_with_do_block_body(
    r#"foreach(x, items):
  do
    process(x)
  end;"#,
    r#"foreach (x, items):
  do
    process(x)
  end;
"#
)]
#[case::foreach_do_with_nested_do_block(
    r#"foreach(row, arr) do
  do
    process(row)
  end
end"#,
    r#"foreach (row, arr) do
  do
    process(row)
  end
end
"#
)]
#[case::fn_multiline_params_as_first_array_element(
    "let fns = [fn(
  a,
  b
): a + b;, fn(c): c;]",
    "let fns = [fn(
  a,
  b
): a + b;, fn(c): c;]
"
)]
#[case::fn_multiline_variadic_params_as_first_array_element(
    "let fns = [fn(
  a,
  *rest
): rest;, fn(c): c;]",
    "let fns = [fn(
  a,
  *rest
): rest;, fn(c): c;]
"
)]
#[case::fn_multiline_params_as_non_first_array_element(
    "let fns = [first, fn(
  a,
  b
): a + b;]",
    "let fns = [first, fn(
  a,
  b
): a + b;]
"
)]
#[case::fn_multiline_params_in_nested_array(
    "let xs = [[fn(
  a,
  b
): a + b;]]",
    "let xs = [[fn(
  a,
  b
): a + b;]]
"
)]
#[case::fn_multiline_params_as_call_arg(
    "map(arr, fn(
  a,
  b
): a + b;)",
    "map(arr, fn(
  a,
  b
): a + b;)
"
)]
#[case::qualified_call_multiline_args_nested_in_call(
    "let result = to_string(md::doc(
    md::h(\"Items\", 2),
    map([\"a\", \"b\"], fn(x): md::list(x);),
  ))",
    "let result = to_string(md::doc(
  md::h(\"Items\", 2),
  map([\"a\", \"b\"], fn(x): md::list(x);),
))
"
)]
#[case::yield_oneline("def g(): yield: 1;", "def g(): yield: 1;")]
#[case::yield_bare("def g(): yield;", "def g(): yield;")]
#[case::break_with_value("while (true): break: 1;", "while (true): break: 1;")]
#[case::yield_multiline(
    "def g():
        yield: 1
        | yield: 2;",
    "def g():
  yield: 1
  | yield: 2;
"
)]
#[case::def_with_comment_before_yield(
    "def g():
        # a comment
        yield: 1;",
    "def g():
  # a comment
  yield: 1;
"
)]
fn test_format(#[case] code: &str, #[case] expected: &str) {
    let result = Formatter::new(None).format(code);
    assert_eq!(result.unwrap(), expected);
}

/// Builds a `def` whose block expression sits `depth` `do`s deep after `prefix`
/// (`"let b = "`, `"| let b = "` or `"| "`), indented as expected: the block body goes
/// one level below where the expression's line content starts (after any `| `).
fn block_program(block: &[(usize, &str)], prefix: &str, depth: usize, indent_width: usize) -> String {
    let pad = |level: usize| " ".repeat(level * indent_width);
    let line_indent = depth + 1;
    let piped = prefix.starts_with('|');
    let base = line_indent + usize::from(piped);
    let mut lines = vec!["def f(t):".to_string()];
    for d in 0..depth {
        lines.push(format!("{}do", pad(d + 1)));
    }
    if piped {
        lines.push(format!("{}let a = 1", pad(line_indent)));
    }
    lines.push(format!("{}{prefix}{}", pad(line_indent), block[0].1));
    for (level, text) in &block[1..] {
        lines.push(format!("{}{text}", pad(base + level)));
    }
    if prefix.contains("let") {
        lines.push(format!("{}| b", pad(line_indent)));
    }
    for d in (0..depth).rev() {
        lines.push(format!("{}end", pad(d + 1)));
    }
    lines.push("end".to_string());
    lines.join("\n") + "\n"
}

#[rstest]
#[case::foreach(&[(0, "foreach (r, t):"), (1, "r"), (0, "end")])]
#[case::while_(&[(0, "while (t):"), (1, "r"), (0, "end")])]
#[case::until(&[(0, "until (t):"), (1, "r"), (0, "end")])]
#[case::loop_(&[(0, "loop:"), (1, "break"), (0, "end")])]
#[case::do_(&[(0, "do"), (1, "r"), (0, "end")])]
#[case::fn_(&[(0, "fn(x):"), (1, "x"), (0, "end")])]
#[case::if_else(&[(0, "if (t):"), (1, "1"), (0, "else:"), (1, "2")])]
#[case::if_elif(&[(0, "if (t):"), (1, "1"), (0, "elif (t):"), (1, "2"), (0, "else:"), (1, "3")])]
#[case::match_(&[(0, "match (t):"), (1, "| 1: \"one\""), (1, "| _: \"other\""), (0, "end")])]
#[case::call_with_fn(&[(0, "map(t, fn(x):"), (1, "let y = x"), (1, "| y;"), (0, ")")])]
#[case::array(&[(0, "["), (1, "1,"), (1, "2"), (0, "]")])]
#[case::dict(&[(0, "{"), (1, "\"a\": 1,"), (1, "\"b\": 2"), (0, "}")])]
#[case::call_with_array(&[(0, "g(t, ["), (1, "1,"), (1, "2"), (0, "])")])]
#[case::call_with_dict(&[(0, "g(t, {"), (1, "\"a\": 1"), (0, "})")])]
#[case::array_of_dicts(&[(0, "g(t, ["), (1, "{\"a\": 1},"), (1, "{\"b\": 2},"), (0, "])")])]
#[case::nested_array_in_dict(&[(0, "{"), (1, "\"a\": ["), (2, "1"), (1, "]"), (0, "}")])]
#[case::array_comment_before_close(&[(0, "g(t, ["), (1, "1,"), (1, "# c"), (0, "])")])]
fn test_format_block_indent(
    #[case] block: &[(usize, &str)],
    #[values("let b = ", "| let b = ", "| ")] prefix: &str,
    #[values(0, 1, 2)] depth: usize,
    #[values(2, 4)] indent_width: usize,
) {
    let expected = block_program(block, prefix, depth, indent_width);
    let flattened = expected.lines().map(str::trim_start).collect::<Vec<_>>().join("\n");
    let format = |code: &str| {
        Formatter::new(Some(FormatterConfig {
            indent_width,
            ..FormatterConfig::default()
        }))
        .format(code)
        .unwrap()
    };

    assert_eq!(format(&expected), expected, "not idempotent");
    assert_eq!(format(&flattened), expected, "from flattened input");
}

#[rstest]
#[case::sort_imports(
    r#"import "c.mq"
| import "a.mq"
| import "b.mq""#,
    r#"import "a.mq"
| import "b.mq"
| import "c.mq"
"#
)]
#[case::sort_imports_with_alias(
    r#"import "c.mq"
| import "a.mq" as a
| import "b.mq""#,
    r#"import "a.mq" as a
| import "b.mq"
| import "c.mq"
"#
)]
#[case::sort_functions(
    r#"def z(): test;
def a(): test;
def m(): test;
"#,
    "def a(): test;\ndef m(): test;\ndef z(): test;\n"
)]
#[case::sort_fields(
    r#"let z = 1
| let a = 2
| let m = 3"#,
    r#"let a = 2
| let m = 3
| let z = 1
"#
)]
#[case::sort_mixed(
    r#"let z = 1
| let a = 2
| import "b.mq"
| import "a.mq"
def y(): test;
def b(): test;"#,
    "let a = 2\n| let z = 1\n| import \"a.mq\"\n| import \"b.mq\"\ndef b(): test;\ndef y(): test;\n"
)]
#[case::definitions_stay_before_pipeline(
    "def z(): 1;\n# doc for a\ndef a(): z();\nlet v = a()\n| v",
    "# doc for a\ndef a(): z();\ndef z(): 1;\nlet v = a()\n| v\n"
)]
#[case::runs_split_by_other_statement(
    "def z(): 1;\ndef a(): 2;\n| x\n| def y(): 3;\ndef b(): 4;",
    "def a(): 2;\ndef z(): 1;\n| x\n| def b(): 4;\ndef y(): 3;\n"
)]
#[case::lets_stay_around_other_statement(
    "let z = 1\n| let a = 2\n| foo()\n| let y = 3\n| let b = 4",
    "let a = 2\n| let z = 1\n| foo()\n| let b = 4\n| let y = 3\n"
)]
#[case::blank_line_stays_with_slot("def z(): 1;\n\ndef a(): 2;", "def a(): 2;\n\ndef z(): 1;\n")]
#[case::comment_before_pipe_follows_binding(
    "let z = 1\n# a is the second binding\n| let a = 2",
    "# a is the second binding\nlet a = 2\n| let z = 1\n"
)]
#[case::comment_before_pipe_follows_def(
    "def z(): 1;\n# about a\n| def a(): 2;\n| def m(): 3;",
    "# about a\ndef a(): 2;\n| def m(): 3;\n| def z(): 1;\n"
)]
#[case::comment_moves_onto_inline_pipe(
    "let z = 1 | let b = 2\n# about m\n| let m = 3",
    "let b = 2\n# about m\n| let m = 3\n| let z = 1\n"
)]
#[case::pipeline_untouched("add() | sub()\n| mul()", "add() | sub()\n| mul()\n")]
fn test_format_with_sort(#[case] code: &str, #[case] expected: &str) {
    let config = FormatterConfig {
        indent_width: 2,
        sort_imports: true,
        sort_functions: true,
        sort_fields: true,
        max_width: None,
    };
    let result = Formatter::new(Some(config)).format(code).unwrap();
    assert_eq!(result, expected);
    let (_, errors) = mq_lang::parse_recovery(&result);
    assert!(!errors.has_errors(), "{result:?} does not parse");
}

#[rstest]
#[case::wraps_top_level_pipeline(
    "select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\",\"b\") | join(\",\")",
    40,
    "select(\"h1\") | upcase() | add_class(\"title\")\n| trim() | to_text() | replace(\"a\", \"b\")\n| join(\",\")\n"
)]
#[case::wraps_pipeline_inside_def(
    "def foo(x):\n  select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\",\"b\") | join(\",\");",
    40,
    "def foo(x):\n  select(\"h1\") | upcase() | add_class(\"title\")\n  | trim() | to_text() | replace(\"a\", \"b\")\n  | join(\",\");\n"
)]
#[case::keeps_short_pipeline_inline("select(\"h1\") | upcase()", 80, "select(\"h1\") | upcase()")]
fn test_format_with_max_width(#[case] code: &str, #[case] max_width: usize, #[case] expected: &str) {
    let config = FormatterConfig {
        max_width: Some(max_width),
        ..Default::default()
    };
    let result = Formatter::new(Some(config)).format(code);
    assert_eq!(result.unwrap(), expected);
}

#[test]
fn test_format_with_max_width_is_idempotent() {
    let code =
        "select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\",\"b\") | join(\",\")";
    let config = || FormatterConfig {
        max_width: Some(40),
        ..Default::default()
    };
    let once = Formatter::new(Some(config())).format(code).unwrap();
    let twice = Formatter::new(Some(config())).format(&once).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn test_format_without_max_width_does_not_wrap() {
    let code =
        "select(\"h1\") | upcase() | add_class(\"title\") | trim() | to_text() | replace(\"a\", \"b\") | join(\",\")";
    let result = Formatter::default().format(code);
    assert_eq!(result.unwrap(), code);
}

#[test]
fn test_format_yield_is_idempotent() {
    for code in ["def g(): yield: 1;", "def g(): yield: 1 | yield: 2;"] {
        let once = Formatter::new(None).format(code).unwrap();
        let twice = Formatter::new(None).format(&once).unwrap();
        assert_eq!(once, twice, "not idempotent for {code:?}");
    }
}

const FORMAT_STATEMENTS: &[&str] = &[
    "def f(x):\n  x + 1;",
    "let a = [1,2]",
    "if(a):1 elif(b):2 else:3",
    "foreach(v,a):v;",
    "match(x): | 1: :a | _: :b end",
    "{\"k\": 1}",
    ".h1",
    "try: 1 catch(e): e",
    "fn(x): x;",
    "while(a): break;",
    "a[0](1)",
    "1 as n",
];

/// The previous allocating `is_let_line`, kept as an oracle.
fn reference_is_let_line(output: &str) -> bool {
    let start = output.rfind('\n').map_or(0, |pos| pos + 1);
    if start < output.len() {
        let last_line = &output[start..];
        (!last_line.starts_with("let ") && last_line.trim().starts_with("let "))
            || last_line.trim().replace(" ", "").starts_with("|let")
    } else {
        false
    }
}

fn is_let_line(output: &str) -> bool {
    let mut formatter = Formatter::new(None);
    formatter.output = output.to_string();
    formatter.is_let_line()
}

#[rstest]
#[case::top_level_let("let x = 1", false)]
#[case::indented_let("  let x = 1", true)]
#[case::piped_let("| let x", true)]
#[case::piped_let_no_space("|let x", true)]
#[case::piped_let_spaced("|   let x", true)]
#[case::last_line_only("let a = 1\n  | let b", true)]
#[case::previous_line_ignored("  let a\nfoo", false)]
#[case::trailing_newline("  let a\n", false)]
#[case::empty("", false)]
#[case::not_let("  letter", false)]
#[case::pipe_other("| foo", false)]
fn test_is_let_line(#[case] output: &str, #[case] expected: bool) {
    assert_eq!(is_let_line(output), expected);
    assert_eq!(reference_is_let_line(output), expected);
}

#[test]
fn test_format_with_cst_normalizes_escapes_without_source() {
    let (mut nodes, _) = mq_lang::parse_recovery(r#""\u{41}\t""#);
    let result = Formatter::new(None).format_with_cst(&mut nodes).unwrap();
    assert_eq!(result, r#""A\t""#);
}

/// Comments before a terminating `;` are kept, and formatting them again changes nothing.
/// Comments between an expression and its postfix `[`/`(` on the next line are kept.
#[rstest::rstest]
#[case::index("x # note\n[0]", "x # note\n[0]\n")]
#[case::call("x # note\n(1)", "x # note\n(1)\n")]
#[case::index_after_call("f(x) # note\n[0]", "f(x) # note\n[0]\n")]
#[case::qualified_call("mod::f # note\n(1)", "mod::f # note\n(1)\n")]
#[case::in_def("def g():\n  x # note\n  [0]\n;", "def g():\n  x # note\n  [0];\n")]
fn test_format_keeps_comment_before_postfix(#[case] code: &str, #[case] expected: &str) {
    let once = Formatter::new(None).format(code).unwrap();
    assert_eq!(once, expected);
    assert_eq!(Formatter::new(None).format(&once).unwrap(), once);
}

#[rstest::rstest]
#[case::own_line_in_def("def f():\n  1\n  # note\n;", "def f():\n  1\n  # note\n;\n")]
#[case::trailing_in_def("def f():\n  1 # trail\n;", "def f():\n  1 # trail\n;\n")]
#[case::after_let("let x = 1\n# c\n;", "let x = 1\n# c\n;\n")]
#[case::in_text("# a ; b\nlet x = 1", "# a ; b\nlet x = 1\n")]
fn test_format_keeps_comment_before_semicolon(#[case] code: &str, #[case] expected: &str) {
    let once = Formatter::new(None).format(code).unwrap();
    assert_eq!(once, expected);
    assert_eq!(Formatter::new(None).format(&once).unwrap(), once);
}

/// Unparsable source in an `Error` node is emitted as written, strings included.
#[rstest::rstest]
#[case::junk_string("foo(1 \"a b\", 2)", "foo(1 \"a b\", 2)")]
#[case::unknown_token("foo(1, §, 2)", "foo(1, §, 2)")]
#[case::missing_paren("foo(1, 2", "foo(1, 2")]
#[case::array_final_newline("[1\n", "[1\n")]
#[case::array_items_final_newline("[1, 2\n", "[1, 2\n")]
#[case::dict_final_newline("{\"a\": 1\n", "{\"a\": 1\n")]
#[case::call_final_newline("foo(1\n", "foo(1\n")]
fn test_format_with_cst_keeps_broken_source(#[case] code: &str, #[case] expected: &str) {
    let (mut nodes, errors) = mq_lang::parse_recovery(code);
    assert!(errors.has_errors());

    let result = Formatter::new(None).format_with_cst(&mut nodes).unwrap();

    assert_eq!(result, expected);
}

/// A comment before an absent closer at EOF is kept exactly once.
#[rstest::rstest]
#[case::call("foo(1 # note\n")]
#[case::array("[1 # note\n")]
#[case::dict("{\"a\": 1 # note\n")]
#[case::call_own_line("foo(1,\n  # note\n")]
#[case::array_own_line("[1,\n  # note\n")]
#[case::dict_own_line("{\"a\": 1,\n  # note\n")]
fn test_format_with_cst_keeps_comment_before_missing_closer(#[case] code: &str) {
    let (mut nodes, errors) = mq_lang::parse_recovery(code);
    assert!(errors.has_errors());

    let result = Formatter::new(None).format_with_cst(&mut nodes).unwrap();

    assert_eq!(result.matches("# note").count(), 1, "{code:?} -> {result:?}");
}

#[test]
fn test_format_sort_fields_keeps_escapes() {
    let config = FormatterConfig {
        sort_fields: true,
        ..FormatterConfig::default()
    };
    let result = Formatter::new(Some(config))
        .format("let b = \"\\u{41}\"\n| let a = {\"\\u{E000}\": \"\\x42\"}")
        .unwrap();
    assert_eq!(result, "let a = {\"\\u{E000}\": \"\\x42\"}\n| let b = \"\\u{41}\"\n");
}

/// Escape sequences and raw characters a string literal can contain.
const STRING_PIECES: &[&str] = &[
    "a",
    "Z",
    "0",
    " ",
    "日",
    "é",
    "😀",
    "\\n",
    "\\t",
    "\\r",
    "\\\"",
    "\\\\",
    "\\/",
    "\\d",
    "\\s",
    "\\(",
    "\\{",
    "\\x41",
    "\\x1b",
    "\\u{41}",
    "\\u{D7FF}",
    "\\u{E000}",
    "\\u{10FFFF}",
];

proptest::proptest! {
    /// A string literal is written back exactly as in the source.
    #[test]
    fn prop_string_literal_is_preserved(
        pieces in proptest::collection::vec(proptest::sample::select(STRING_PIECES), 0..12),
        prefix in proptest::sample::select(&["let x = ", "| ", "f(", "[", "{\"k\": ", "s"][..]),
    ) {
        let body = pieces.concat();
        let suffix = match prefix {
            "f(" => ")",
            "[" => "]",
            "{\"k\": " => "}",
            _ => "",
        };
        let code = format!("{prefix}\"{body}\"{suffix}");
        let code = code.strip_prefix("| ").unwrap_or(&code).to_string();
        // Interpolated strings accept fewer escapes than plain strings.
        proptest::prop_assume!(!mq_lang::parse_recovery(&code).1.has_errors());
        let once = Formatter::new(None).format(&code).unwrap();
        proptest::prop_assert!(once.contains(&format!("\"{body}\"")), "{code:?} -> {once:?}");
        let twice = Formatter::new(None).format(&once).unwrap();
        proptest::prop_assert_eq!(twice, once);
    }

    /// Whitespace after `#` is kept; only a missing space is added.
    #[test]
    fn prop_comment_keeps_leading_whitespace(text in "[ \t]{0,3}[a-z\\[\\],\" ]{0,12}") {
        let formatted = Formatter::new(None).format(&format!("#{text}\nlet x = 1")).unwrap();
        let text = text.trim_end();
        let expected = if text.is_empty() || text.starts_with([' ', '\t']) {
            format!("#{text}")
        } else {
            format!("# {text}")
        };
        proptest::prop_assert_eq!(formatted.lines().next().unwrap(), expected.trim_end());
    }

    #[test]
    fn prop_is_let_line_matches_reference(output in "[ |let\\na-z\\t]{0,24}") {
        proptest::prop_assert_eq!(is_let_line(&output), reference_is_let_line(&output));
    }

    /// Formatting through a pre-parsed CST matches formatting from source.
    #[test]
    fn prop_format_with_cst_matches_format(
        stmts in proptest::collection::vec(proptest::sample::select(FORMAT_STATEMENTS), 1..6)
    ) {
        let code = stmts.join(" | ");
        let expected = Formatter::new(None).format(&code).unwrap();
        let (mut nodes, _) = mq_lang::parse_recovery(&code);
        let actual = Formatter::new(None).format_with_cst(&mut nodes).unwrap();
        proptest::prop_assert_eq!(actual, expected);
    }

    #[test]
    fn prop_format_is_idempotent(
        stmts in proptest::collection::vec(proptest::sample::select(FORMAT_STATEMENTS), 1..6)
    ) {
        let once = Formatter::new(None).format(&stmts.join(" | ")).unwrap();
        let twice = Formatter::new(None).format(&once).unwrap();
        proptest::prop_assert_eq!(twice, once);
    }
}

#[rstest]
#[case::space_chain("# c\n.blockquote .code\n")]
#[case::space_chain_call("# c\n.blockquote .code(\"rust\")\n")]
#[case::two_comments("# c1\n# c2\n.blockquote .list\n")]
#[case::property_list("# c\n.\"a\"[]\n")]
#[case::property_chain("# c\n.\"a\".\"b\"\n")]
fn test_format_selector_chain_keeps_leading_comment_once(#[case] code: &str) {
    let once = Formatter::new(None).format(code).unwrap();
    let twice = Formatter::new(None).format(&once).unwrap();
    assert_eq!(once, code);
    assert_eq!(once, twice);
}
