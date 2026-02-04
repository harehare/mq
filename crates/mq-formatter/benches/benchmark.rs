use mq_formatter::{Formatter, FormatterConfig};

fn main() {
    divan::main();
}

#[divan::bench]
fn format_simple_code() {
    let code = r#"if(test):test else:test2"#;
    let mut formatter = Formatter::new(None);
    formatter.format(code).unwrap();
}

#[divan::bench]
fn format_complex_def() {
    let code = r#"def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
  let first_char = upcase(first(word))
  | let rest_str = downcase(slice(word, 1, len(word)))
  | s"${first_char}${rest_str}";
  | join("");
| snake_to_camel()"#;
    let mut formatter = Formatter::new(None);
    formatter.format(code).unwrap();
}

#[divan::bench]
fn format_nested_structures() {
    let code = r#"{
 "level1": {
 "level2": {
 "level3": "value"
 }
 }
 }"#;
    let mut formatter = Formatter::new(None);
    formatter.format(code).unwrap();
}

#[divan::bench]
fn format_with_sort() {
    let code = r#"let z = 1
| import "b.mq"
def y(): test;
| let a = 2
| import "a.mq"
def b(): test;
macro m(): test;"#;
    let config = FormatterConfig {
        indent_width: 2,
        sort_imports: true,
        sort_functions: true,
        sort_fields: true,
    };
    let mut formatter = Formatter::new(Some(config));
    formatter.format(code).unwrap();
}

#[divan::bench]
fn format_match_expression() {
    let code = r#"match(x):
 | 1: "one"
 | 2: "two"
 | _: "other"
 end"#;
    let mut formatter = Formatter::new(None);
    formatter.format(code).unwrap();
}

#[divan::bench]
fn format_large_array() {
    let code = r#"[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20]"#;
    let mut formatter = Formatter::new(None);
    formatter.format(code).unwrap();
}
