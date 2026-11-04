//! Valid programs around tagged records that the checker must not report, each also run to confirm
//! it is valid, and invalid ones that it must report.

use std::time::Duration;

use mq_check::TypeChecker;
use mq_hir::Hir;
use rstest::rstest;

fn check(code: &str) -> Vec<String> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    TypeChecker::new().check(&hir).iter().map(ToString::to_string).collect()
}

fn run(code: &str) -> Result<(), String> {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(Duration::from_millis(500));
    engine
        .eval(code, mq_lang::null_input().into_iter())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

const DIV: &str = r#"def div(a, b): if (b == 0): {ok: false, error: "zero"} else: {ok: true, value: a / b} end"#;
const SHAPES: &str = r#"def shape(n): if (n == 0): {kind: "circle", radius: 2} elif (n == 1): {kind: "rect", w: 2, h: 3} else: {kind: "dot"} end"#;

#[rstest]
// reading the tag itself
#[case::tag_read_in_branch(format!(r#"{DIV}
let r = div(4, 2)
| if (r["ok"]): r["ok"] && true else: r["ok"] == false"#))]
#[case::tag_compared_after_use(format!(r#"{DIV}
let r = div(4, 2)
| let v = if (r["ok"]): r["value"] else: 0
| v + 1"#))]
// results used in other positions
#[case::tag_stored_in_variable(format!(r#"{DIV}
let r = div(4, 2)
| let ok = r["ok"]
| ok"#))]
#[case::tag_passed_to_function(format!(r#"{DIV}
let r = div(4, 2)
| def show(b): if (b): "yes" else: "no";
| show(r["ok"])"#))]
#[case::tag_in_string(format!(r#"{DIV}
let r = div(4, 2)
| to_string(r["ok"]) + "!""#))]
#[case::tag_as_array_element(format!(r#"{DIV}
let r = div(4, 2)
| [r["ok"], true]"#))]
#[case::tag_in_dict_value(r#"{a: {ok: true}} | {b: .["a"]["ok"]}"#.to_string())]
// collections of results
#[case::map_over_results(format!(r#"{DIV}
[div(4, 2), div(1, 0)] | map(fn(r): if (r["ok"]): r["value"] else: -1;)"#))]
#[case::filter_results(format!(r#"{DIV}
[div(4, 2), div(1, 0)] | filter(fn(r): r["ok"];) | len()"#))]
#[case::foreach_results(format!(r#"{DIV}
foreach (r, [div(4, 2), div(1, 0)]): if (r["ok"]): r["value"] + 1 else: len(r["error"]) end"#))]
#[case::literal_array_of_results(r#"[{ok: true, value: 1}, {ok: false, error: "e"}] | map(fn(r): if (r["ok"]): r["value"] + 1 else: len(r["error"]);)"#.to_string())]
#[case::array_of_same_shape(r#"[{ok: true, v: 1}, {ok: false, v: 2}] | map(fn(r): r["v"] + 1;)"#.to_string())]
// several statements and scopes
#[case::shadowed_result(format!(r#"{DIV}
let r = div(4, 2)
| let r = div(1, 0)
| if (r["ok"]): r["value"] + 1 else: len(r["error"])"#))]
#[case::nested_if(format!(r#"{DIV}
let r = div(4, 2)
| if (r["ok"]): if (r["value"] > 1): r["value"] + 1 else: 0 else: len(r["error"])"#))]
#[case::tag_test_inside_loop_body(format!(r#"{DIV}
var total = 0
| foreach (n, [1, 2, 0]): let r = div(4, n) | if (r["ok"]): total += r["value"] else: total += 0
| total"#))]
#[case::while_with_result(format!(r#"{DIV}
var i = 1
| var acc = 0
| while (i < 4): let r = div(8, i) | if (r["ok"]): acc += r["value"] else: acc += 0 | i += 1 end
| acc"#))]
#[case::try_around_result(format!(r#"{DIV}
let r = div(4, 2)
| try: if (r["ok"]): r["value"] + 1 else: error(r["error"]) catch(e): 0"#))]
// functions returning and passing results
#[case::result_through_function(format!(r#"{DIV}
def safe(a, b): div(a, b);
let r = safe(4, 2)
| if (r["ok"]): r["value"] + 1 else: len(r["error"])"#))]
#[case::function_taking_result(format!(r#"{DIV}
def unwrap_or(r, d): if (r["ok"]): r["value"] else: d;
unwrap_or(div(4, 2), 0) + 1"#))]
#[case::function_taking_result_literal(r#"def unwrap_or(r, d): if (r["ok"]): r["value"] else: d;
unwrap_or({ok: true, value: 3}, 0) + unwrap_or({ok: false, error: "e"}, 1)"#.to_string())]
#[case::chained_results(format!(r#"{DIV}
def step(r): if (r["ok"]): div(r["value"], 2) else: r;
step(step(div(8, 2)))["ok"]"#))]
#[case::result_returned_from_branch(format!(r#"{DIV}
def both(a): let r = div(a, 2) | if (r["ok"]): {{ok: true, value: r["value"] + 1}} else: r;
let x = both(4)
| if (x["ok"]): x["value"] + 1 else: len(x["error"])"#))]
#[case::predicate_with_two_levels(format!(r#"{DIV}
def is_ok(r): r["ok"];
def good(r): is_ok(r);
let r = div(4, 2)
| if (good(r)): r["value"] + 1 else: len(r["error"])"#))]
#[case::negated_predicate(format!(r#"{DIV}
def failed(r): !r["ok"];
let r = div(4, 2)
| if (failed(r)): len(r["error"]) else: r["value"] + 1"#))]
#[case::predicate_with_string_tag(format!(r#"{SHAPES}
def is_circle(s): s["kind"] == "circle";
let s = shape(0)
| if (is_circle(s)): s["radius"] * 2 else: 0"#))]
// three variants and string tags
#[case::three_variants(format!(r#"{SHAPES}
let s = shape(1)
| if (s["kind"] == "circle"): s["radius"] * 2 elif (s["kind"] == "rect"): s["w"] * s["h"] else: 0"#))]
#[case::three_variants_negated(format!(r#"{SHAPES}
let s = shape(1)
| if (s["kind"] != "dot"): 1 else: 0"#))]
#[case::variant_without_field(format!(r#"{SHAPES}
let s = shape(2)
| if (s["kind"] == "dot"): 0 else: 1"#))]
#[case::tag_in_match(format!(r#"{SHAPES}
let s = shape(0)
| match (s["kind"]): | "circle": 1 | "rect": 2 | _: 3 end"#))]
// literal arguments and defaults
#[case::literal_selects_branch(r#"def area(s): if (s["kind"] == "circle"): s["radius"] * 2 else: s["w"] * s["h"];
area({kind: "circle", radius: 2}) + area({kind: "rect", w: 2, h: 3})"#.to_string())]
#[case::literal_with_extra_fields(r#"def area(s): if (s["kind"] == "circle"): s["radius"] * 2 else: 0;
area({kind: "circle", radius: 2, note: "x", n: [1, 2]})"#.to_string())]
#[case::literal_with_unrelated_branch(r#"def f(r): if (r["ok"]): r["value"] + 1 else: r["error"] - 1;
f({ok: true, value: 1, error: "never read"})"#.to_string())]
#[case::default_for_missing(r#"def f(r): (r["value"] ?? 0) + 1;
f({other: 1})"#.to_string())]
#[case::missing_checked_first(r#"def f(r): if (is_none(r["value"])): 0 else: r["value"] + 1;
f({other: 1}) + f({value: 2})"#.to_string())]
#[case::missing_tag_is_falsy(r#"def f(r): if (r["ok"]): r["value"] + 1 else: 0;
f({value: 1})"#.to_string())]
#[case::key_check_first(r#"def f(r): if (contains(keys(r), "value")): r["value"] + 1 else: 0;
f({other: 1})"#.to_string())]
// variables that change
#[case::tag_reassigned_before_test(r#"var r = {ok: true, value: 1}
| r = {ok: false, error: "e"}
| if (r["ok"]): r["value"] else: len(r["error"])"#.to_string())]
#[case::tag_field_assigned(r#"var r = {ok: true, value: 1}
| r["ok"] = false
| r["error"] = "e"
| if (r["ok"]): 1 else: len(r["error"])"#.to_string())]
#[case::argument_changed_before_call(r#"def f(r): if (r["ok"]): r["value"] + 1 else: 0;
var d = {ok: true, value: "s"}
| d = {ok: true, value: 1}
| f(d)"#.to_string())]
// shapes that use the same keys
#[case::same_keys_same_type(r#"def mk(b): if (b): {ok: true, v: 1} else: {ok: false, v: 2};
let r = mk(true)
| if (r["ok"]): r["v"] + 1 else: r["v"] - 1"#.to_string())]
#[case::tag_with_nullable_field(r#"def mk(b): if (b): {ok: true, v: 1} else: {ok: false, v: None};
let r = mk(true)
| if (r["ok"]): r["v"] + 1 else: 0"#.to_string())]
#[case::nested_tagged_records(r#"def mk(b): if (b): {ok: true, inner: {ok: true, n: 1}} else: {ok: false, inner: {ok: false, s: "x"}};
let r = mk(true)
| if (r["ok"]): r["inner"]["n"] + 1 else: len(r["inner"]["s"])"#.to_string())]
fn test_valid_program_is_not_reported(#[case] code: String) {
    if let Err(error) = run(&code) {
        panic!("the program is meant to be valid but fails: {error}\n{code}");
    }
    let errors = check(&code);
    assert!(errors.is_empty(), "false positive: {errors:?}\n{code}");
}

#[rstest]
#[case::wrong_op_in_ok(format!(r#"{DIV}
let r = div(4, 2)
| if (r["ok"]): r["value"] - "s" else: 0"#))]
#[case::wrong_op_in_error(format!(r#"{DIV}
let r = div(4, 2)
| if (r["ok"]): 0 else: r["error"] - 1"#))]
#[case::wrong_op_in_elif(format!(r#"{SHAPES}
let s = shape(1)
| if (s["kind"] == "circle"): s["radius"] * 2 elif (s["kind"] == "rect"): s["w"] - "x" else: 0"#))]
#[case::wrong_op_in_map(format!(r#"{DIV}
[div(4, 2)] | map(fn(r): if (r["ok"]): r["value"] - "s" else: 0;)"#))]
#[case::wrong_op_through_predicate(format!(r#"{DIV}
def is_ok(r): r["ok"];
let r = div(4, 2)
| if (is_ok(r)): r["value"] - "s" else: 0"#))]
#[case::wrong_literal_argument(r#"def f(r): if (r["ok"]): r["value"] - 1 else: 0;
f({ok: true, value: "s"})"#.to_string())]
#[case::missing_field_in_literal(r#"def f(r): if (r["ok"]): r["value"] - 1 else: 0;
f({ok: true})"#.to_string())]
#[case::misspelled_field(format!(r#"{DIV}
let r = div(4, 2)
| if (r["ok"]): r["valeu"] + 1 else: 0"#))]
fn test_invalid_program_is_reported(#[case] code: String) {
    let errors = check(&code);
    assert!(!errors.is_empty(), "missed error\n{code}");
}
