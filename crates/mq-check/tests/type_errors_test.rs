//! Tests for type error detection
//!
//! This test file demonstrates which type errors are currently detected
//! and which ones are not yet implemented.

use mq_check::{TypeChecker, TypeCheckerOptions, TypeError};
use mq_hir::Hir;
use rstest::rstest;

fn create_hir(code: &str) -> Hir {
    let mut hir = Hir::default();
    // Disable builtins before adding code to avoid type checking builtin functions
    hir.builtin.disabled = true;
    hir.add_code(None, code);
    hir
}

fn check_types(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::new();
    checker.check(&hir)
}

fn check_types_with_builtins(code: &str) -> Vec<TypeError> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    TypeChecker::new().check(&hir)
}

#[test]
fn test_binary_op_type_mismatch() {
    let result = check_types(r#"1 + true"#);
    println!("Binary op type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for number + boolean");
}

#[test]
fn test_if_else_type_mismatch() {
    // mq is dynamically typed, if/else branches with different types are allowed
    let result = check_types(r#"if (true): 42 else: "string";"#);
    assert!(
        result.is_empty(),
        "if/else branches with different types should be allowed in dynamically typed mq: {:?}",
        result
    );
}

#[test]
fn test_heterogeneous_array_allowed() {
    // mq is dynamically typed, heterogeneous arrays (used as tuples) are valid
    let result = check_types(r#"[1, "string", true]"#);
    println!("Heterogeneous array: {:?}", result);
    assert!(
        result.is_empty(),
        "Heterogeneous arrays should be allowed (tuple pattern): {:?}",
        result
    );
}

#[test]
fn test_dict_heterogeneous_values_allowed() {
    // With row polymorphism, each field has its own type in a Record
    // {"a": 1, "b": "string"} → Record({a: int, b: string}, RowEmpty)
    let result = check_types(r#"{"a": 1, "b": "string"}"#);
    println!("Dict with heterogeneous values: {:?}", result);
    assert!(
        result.is_empty(),
        "Dict with mixed value types should be allowed via row polymorphism"
    );
}

#[test]
fn test_function_arity_mismatch() {
    let result = check_types("def add(x, y): x + y;\n| add(1, 2, 3)");
    println!("Function arity mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected arity mismatch error");
}

#[test]
fn test_function_arity_mismatch_hint_names_function_and_counts() {
    // `add(1)` here is not the sole expression after a pipe (which would make the
    // call site ambiguous re: piped-input arity), so this reliably hits the
    // `TypeError::WrongArity` path in constraint.rs.
    let result = check_types("def add(x, y): x + y; | [add(1)]");

    let has_hint = result.iter().any(|err| {
        matches!(
            err,
            TypeError::WrongArity { context: Some(hint), .. } if hint.contains("add") && hint.contains('2') && hint.contains('1')
        )
    });
    assert!(
        has_hint,
        "Expected a WrongArity error with a hint naming the function and arg counts, got: {result:?}"
    );
}

#[test]
fn test_function_type_arity_mismatch_hint_shows_param_counts() {
    // Unifying two function *types* with different arities (e.g. passing a lambda of the wrong
    // arity as a callback) hits the WrongArity path in
    // unify.rs, which has no call-site function name available, unlike the
    // constraint.rs path exercised above.
    let result = check_types_with_builtins("let g = fn(x, y): x; | sort_by([1], g)");

    let has_hint = result.iter().any(|err| {
        matches!(
            err,
            TypeError::WrongArity { context: Some(hint), .. } if hint.contains('1') && hint.contains('2')
        )
    });
    assert!(
        has_hint,
        "Expected a WrongArity error with a hint naming the param counts, got: {result:?}"
    );
}

#[test]
fn test_match_pattern_type_mismatch() {
    // Matching a number against a string pattern should produce a type error
    let result = check_types(r#"match (1): | "hello": "matched" end"#);
    println!("Match pattern type mismatch: {:?}", result);
    assert!(
        !result.is_empty(),
        "Expected type error for matching number against string pattern"
    );
}

#[test]
fn test_match_arm_body_type_mismatch() {
    // mq is dynamically typed, match arms with different body types are allowed.
    // Filter out exhaustiveness errors (tested separately) to focus on type compatibility.
    let result: Vec<_> = check_types(r#"match (1): | 1: "one" | 2: 222 end"#)
        .into_iter()
        .filter(|e| !matches!(e, TypeError::NonExhaustiveMatch { .. }))
        .collect();
    println!("Match arm body type mismatch: {:?}", result);
    assert!(
        result.is_empty(),
        "Match arms with different body types should be allowed in dynamically typed mq: {:?}",
        result
    );
}

#[test]
fn test_success_simple_literal() {
    let result = check_types("42");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty());
}

#[test]
fn test_success_simple_variable() {
    assert!(check_types("let x = 42 | x").is_empty());
}

#[test]
fn test_success_simple_function() {
    assert!(check_types("def id(x): x;").is_empty());
}

#[test]
fn test_success_homogeneous_array() {
    assert!(check_types("[1, 2, 3]").is_empty());
}

#[test]
fn test_success_homogeneous_dict() {
    assert!(check_types(r#"{"a": 1, "b": 2}"#).is_empty());
}

#[test]
fn test_success_match_consistent_patterns() {
    // Filter out exhaustiveness errors (tested separately) to focus on type compatibility.
    let result: Vec<_> = check_types(r#"match (1): | 1: "one" | 2: "two" end"#)
        .into_iter()
        .filter(|e| !matches!(e, TypeError::NonExhaustiveMatch { .. }))
        .collect();
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(
        result.is_empty(),
        "Match with consistent pattern/body types should succeed"
    );
}

#[test]
fn test_success_match_variable_pattern() {
    let result = check_types(r#"match (1): | x: x end"#);
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Match with variable pattern should succeed");
}

#[test]
fn test_success_while_loop() {
    let result = check_types("while (true): 1;");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "While loop with bool condition should succeed");
}

#[test]
fn test_while_condition_type_mismatch() {
    let result = check_types("while (42): 1;");
    println!("While condition type mismatch: {:?}", result);
    assert!(
        !result.is_empty(),
        "Expected type error for while with non-bool condition"
    );
}

#[test]
fn test_success_until_loop() {
    let result = check_types("var x = 0 | until (x > 0): x = x + 1;");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Until loop should succeed");
}

#[test]
fn test_narrowing_equality_then_branch() {
    // x == "foo" narrows x to String in then-branch; len() accepts String/Array
    let result = check_types(r#"def f(x): if (x == "hello"): len(x) else: 0;"#);
    assert!(
        result.is_empty(),
        "Equality narrowing should allow len(x) in then-branch: {result:?}"
    );
}

#[test]
fn test_narrowing_neq_none_removes_none() {
    // x != none removes None from union in the then-branch
    let result = check_types(r#"def f(x): if (x != none): len(x) else: 0;"#);
    assert!(
        result.is_empty(),
        "x != none should narrow away None in then-branch: {result:?}"
    );
}

#[test]
fn test_narrowing_type_call_string() {
    // type(x) == "string" narrows x to String
    let result = check_types(r#"def f(x): if (type(x) == "string"): upcase(x) else: 0;"#);
    assert!(
        result.is_empty(),
        "type(x) == 'string' should narrow x to String: {result:?}"
    );
}

#[test]
fn test_narrowing_type_call_number() {
    // type(x) == "number" narrows x to Number
    let result = check_types(r#"def f(x): if (type(x) == "number"): x + 1 else: 0;"#);
    assert!(
        result.is_empty(),
        "type(x) == 'number' should narrow x to Number: {result:?}"
    );
}

#[test]
fn test_narrowing_match_arm_string_pattern() {
    // match with a String pattern should narrow the matched variable to String inside the arm body
    let result = check_types(r#"def f(x): match (x): | "hello": upcase(x) | _: x end"#);
    assert!(
        result.is_empty(),
        "Match arm with string pattern should narrow variable to String: {result:?}"
    );
}

#[test]
fn test_narrowing_match_arm_number_pattern() {
    // match with a Number pattern should narrow to Number inside the arm body
    let result = check_types(r#"def f(x): match (x): | 42: x + 1 | _: 0 end"#);
    assert!(
        result.is_empty(),
        "Match arm with number pattern should narrow variable to Number: {result:?}"
    );
}

#[test]
fn test_narrowing_match_arm_none_pattern() {
    // match with none pattern narrows to None in that arm
    let result = check_types(r#"def f(x): match (x): | none: 0 | _: 1 end"#);
    assert!(
        result.is_empty(),
        "Match arm with none pattern should succeed: {result:?}"
    );
}

#[test]
fn test_inspect_inferred_types() {
    let code = "def add(a, b): a + b;";

    let hir = create_hir(code);
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    println!("\n=== Inferred Types ===");
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && let Some(name) = &symbol.value
        {
            println!("{}: {}", name, type_scheme);
        }
    }
}

#[test]
fn test_inspect_type_variables() {
    let code = "def identity(x): x;";

    let hir = create_hir(code);
    let mut checker = TypeChecker::new();

    assert!(checker.check(&hir).is_empty());

    println!("\n=== Type Variables ===");
    for (symbol_id, type_scheme) in checker.symbol_types() {
        if let Some(symbol) = hir.symbol(*symbol_id)
            && symbol.is_function()
            && let Some(name) = &symbol.value
        {
            println!("{}: {}", name, type_scheme);
        }
    }
}

#[test]
fn test_narrowing_or_then_branch_same_variable() {
    // is_string(x) || is_number(x) → then-branch: x: String|Number
    // Returning x in the then-branch is always safe regardless of which arm fires.
    let result = check_types(r#"def f(x): if (is_string(x) || is_number(x)): x else: none;"#);
    assert!(
        result.is_empty(),
        "OR narrowing: x is String|Number in then-branch, returning x should succeed: {result:?}"
    );
}

#[test]
fn test_narrowing_or_then_branch_different_variables() {
    // is_string(x) || is_bool(y), different variables, no then-branch narrowing (conservative)
    let result = check_types(r#"def f(x, y): if (is_string(x) || is_bool(y)): x else: none;"#);
    assert!(
        result.is_empty(),
        "OR narrowing over different variables should produce no errors: {result:?}"
    );
}

#[test]
fn test_narrowing_or_else_branch_complement() {
    // is_string(x) || is_number(x), else-branch: x is neither String nor Number
    let result = check_types(r#"def f(x): if (is_string(x) || is_number(x)): x else: x;"#);
    assert!(
        result.is_empty(),
        "OR narrowing complement in else-branch should produce no errors: {result:?}"
    );
}

#[test]
fn test_narrowing_post_while_loop() {
    // After `while (is_string(x))`, the loop exits when condition is false.
    // Post-loop narrowing applies else_narrowings to subsequent code (x has String subtracted).
    let result = check_types(r#"def f(x): while (is_string(x)): x; x"#);
    assert!(
        result.is_empty(),
        "Post-loop narrowing should produce no errors: {result:?}"
    );
}

#[test]
fn test_narrowing_post_until_loop() {
    // `until` inverts `while`'s narrowing polarity: the body runs while the condition is
    // false, so post-loop narrowing (loop exited normally, condition now true) applies
    // then_narrowings to subsequent code instead of else_narrowings.
    let result = check_types(r#"def f(x): until (is_string(x)): x; x"#);
    assert!(
        result.is_empty(),
        "Post-loop narrowing should produce no errors: {result:?}"
    );
}

#[test]
fn test_narrowing_selector_heading_then_branch() {
    // if (.h) uses the structural heading selector as a condition.
    // In the then-branch, the first parameter `x` is narrowed to Markdown.
    // Returning `x` from the then-branch should be type-safe.
    let result = check_types(r#"def f(x): if (.h): x else: none;"#);
    assert!(
        result.is_empty(),
        "Selector narrowing: x should be narrowable to Markdown in the then-branch: {result:?}"
    );
}

#[test]
fn test_narrowing_selector_various_structural() {
    // All structural selectors (.code, .list, .table, etc.) should narrow to Markdown.
    for selector in &[
        ".h", ".h1", ".h2", ".code", ".list", ".table", ".link", ".strong", ".em",
    ] {
        let code = format!("def f(x): if ({selector}): x else: none;");
        let result = check_types(&code);
        assert!(
            result.is_empty(),
            "Selector {selector} narrowing should produce no errors: {result:?}"
        );
    }
}

// Strict Array Mode Tests

/// Helper function to run type checker with strict array mode enabled
fn check_types_strict_array(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::with_options(TypeCheckerOptions {
        strict_array: true,
        ..Default::default()
    });
    checker.check(&hir)
}

/// Helper function to run type checker with tuple mode enabled
fn check_types_tuple(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::with_options(TypeCheckerOptions {
        strict_array: false,
        ..Default::default()
    });
    checker.check(&hir)
}

#[test]
fn test_strict_array_heterogeneous_error() {
    // With strict_array enabled, mixed-type arrays should produce an error
    let result = check_types_strict_array(r#"[1, "hello"]"#);
    assert!(
        !result.is_empty(),
        "Expected HeterogeneousArray error for [1, \"hello\"] in strict mode"
    );
    assert!(
        result.iter().any(|e| matches!(e, TypeError::HeterogeneousArray { .. })),
        "Expected HeterogeneousArray variant, got: {result:?}"
    );
}

#[test]
fn test_strict_array_heterogeneous_three_types() {
    // Multiple different types should also produce an error
    let result = check_types_strict_array(r#"[1, "hello", true]"#);
    assert!(
        result.iter().any(|e| matches!(e, TypeError::HeterogeneousArray { .. })),
        "Expected HeterogeneousArray error for three mixed types: {result:?}"
    );
}

#[test]
fn test_strict_array_homogeneous_ok() {
    // Homogeneous arrays should still pass in strict mode
    let result = check_types_strict_array("[1, 2, 3]");
    assert!(
        result.is_empty(),
        "Homogeneous number array should pass in strict mode: {result:?}"
    );
}

#[test]
fn test_strict_array_homogeneous_strings_ok() {
    // Homogeneous string arrays should pass in strict mode
    let result = check_types_strict_array(r#"["a", "b", "c"]"#);
    assert!(
        result.is_empty(),
        "Homogeneous string array should pass in strict mode: {result:?}"
    );
}

#[test]
fn test_strict_array_empty_ok() {
    // Empty arrays should always pass
    let result = check_types_strict_array("[]");
    assert!(result.is_empty(), "Empty array should pass in strict mode: {result:?}");
}

#[test]
fn test_default_mode_heterogeneous_allowed() {
    // Without strict_array, heterogeneous arrays should still be allowed
    let result = check_types(r#"[1, "hello", true]"#);
    assert!(
        result.is_empty(),
        "Heterogeneous arrays should be allowed in default mode: {result:?}"
    );
}

// Array Element Access Type Propagation Tests

#[test]
fn test_array_element_access_type_error() {
    // v[0] on a number array should propagate the element type (Number),
    // so v[0] + true should be a type error
    let result = check_types("let v = [1, 1, 3] | v[0] + true");
    assert!(
        !result.is_empty(),
        "Expected type error for v[0] + true where v is [Number]: {result:?}"
    );
}

#[test]
fn test_array_element_access_valid() {
    // v[0] on a number array should return Number, so v[0] + 1 should be valid
    let result = check_types("let v = [1, 2, 3] | v[0] + 1");
    assert!(
        result.is_empty(),
        "v[0] + 1 where v is [Number] should succeed: {result:?}"
    );
}

#[test]
fn test_tuple_literal_index_number_element() {
    // In tuple mode, [1, "hello"] is Tuple(Number, String)
    // v[0] should be Number, so v[0] + 1 should be valid
    let result = check_types_tuple(r#"let v = [1, "hello"] | v[0] + 1"#);
    assert!(
        result.is_empty(),
        "v[0] + 1 on tuple (number, string) should succeed: {result:?}"
    );
}

#[rstest]
#[case::four(r#"[[[[1]]]] | self[0][0][0][0] | upcase()"#)]
#[case::five(r#"[[[[[1]]]]] | self[0][0][0][0][0] | upcase()"#)]
fn test_deep_bracket_chain_resolves_element(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(
        !result.is_empty(),
        "deep index chain should resolve to a number: {code}"
    );
}

#[test]
fn test_tuple_literal_index_string_element() {
    // v[1] should be String, so v[1] - 1 should be a type error
    // (subtraction is not defined for strings)
    let result = check_types_tuple(r#"let v = [1, "hello"] | v[1] - 1"#);
    assert!(
        !result.is_empty(),
        "v[1] - 1 on tuple (number, string) should fail: {result:?}"
    );
}

#[test]
fn test_tuple_literal_index_number_valid_arithmetic() {
    // v[0] should be Number, so v[0] - 1 should be valid
    let result = check_types_tuple(r#"let v = [1, "hello"] | v[0] - 1"#);
    assert!(
        result.is_empty(),
        "v[0] - 1 on tuple (number, string) should succeed: {result:?}"
    );
}

#[test]
fn test_tuple_homogeneous_stays_array() {
    // Homogeneous arrays should still be typed as Array, not Tuple
    let result = check_types_tuple("let v = [1, 2, 3] | v[0] + 1");
    assert!(
        result.is_empty(),
        "Homogeneous array in tuple mode should stay as Array: {result:?}"
    );
}

#[test]
fn test_tuple_default_mode_no_tuple_typing() {
    // Without tuple mode, heterogeneous arrays don't have per-element types
    // so this should not produce an error (fresh type var for element)
    let result = check_types(r#"let v = [1, "hello"] | v[0] + 1"#);
    assert!(
        result.is_empty(),
        "Default mode should not track per-element types: {result:?}"
    );
}

#[test]
fn test_tuple_three_elements() {
    // [1, "hello", true] → Tuple(Number, String, Bool)
    // v[0] + 1 should be valid (Number + Number)
    let result = check_types_tuple(r#"let v = [1, "hello", true] | v[0] + 1"#);
    assert!(
        result.is_empty(),
        "v[0] + 1 on 3-element tuple should succeed: {result:?}"
    );
}

#[rstest]
#[case::array_number_none_valid(
    "let v = try: [1, 2, 3] catch: None; | v[0] + 1",
    true,
    "v[0] + 1 on Union(Array(Number), None) should succeed"
)]
#[case::array_number_none_type_error(
    "let v = try: [1, 2, 3] catch: None; | v[0] + true",
    false,
    "v[0] + true on Union(Array(Number), None) should produce a type error"
)]
#[case::array_string_none_valid(
    r#"let v = try: ["a", "b"] catch: None; | upcase(v[0])"#,
    true,
    "upcase(v[0]) on Union(Array(String), None) should succeed"
)]
#[case::two_arrays_union_element_type_error(
    r#"let v = try: [1, 2] catch: ["a", "b"]; | v[0] + 1"#,
    false,
    "v[0] + 1 on Union(Array(Number), Array(String)) should produce a type error"
)]
fn test_union_array_index_access(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {result:?}", description);
}

#[rstest]
#[case::tuple_number_string_none_index0_valid(
    r#"let v = try: [1, "hello"] catch: None; | v[0] + 1"#,
    true,
    "v[0] + 1 on Union(Tuple(Number, String), None) should succeed"
)]
#[case::tuple_number_string_none_index1_type_error(
    r#"let v = try: [1, "hello"] catch: None; | v[1] - 1"#,
    false,
    "v[1] - 1 on Union(Tuple(Number, String), None) should produce a type error"
)]
fn test_union_tuple_index_access(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types_tuple(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {result:?}", description);
}

// Tests for dict destructuring with explicit key:binding syntax `{key: var}`
#[rstest]
#[case::shorthand_no_error(
    r#"let {a} = {"a": 1} | a + 1"#,
    true,
    "shorthand dict destructuring with number field used as number should be ok"
)]
#[case::shorthand_type_error(
    r#"let {a} = {"a": 1} | a + true"#,
    false,
    "shorthand dict destructuring: number field used as boolean should error"
)]
#[case::explicit_key_no_error(
    r#"let {a: xxx} = {"a": 1} | xxx + 1"#,
    true,
    "explicit key dict destructuring with number field used as number should be ok"
)]
#[case::explicit_key_type_error(
    r#"let {a: xxx} = {"a": 1, "b": 2} | xxx + true"#,
    false,
    "explicit key dict destructuring: number field used as boolean should error"
)]
fn test_dict_destructuring_type_check(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {result:?}", description);
}

#[rstest]
// After `| none:`, wildcard arm narrows x away from None → String only → upcase is valid
#[case::none_then_wildcard_valid(
    r#"def f(x): match (x): | none: "nothing" | _: upcase(x) end"#,
    true,
    "wildcard arm after none arm: x narrowed to non-None, upcase should succeed"
)]
// After `| :string::`, wildcard arm narrows x away from String → no upcase
#[case::type_label_then_wildcard_valid(
    r#"def f(x): match (x): | :string:: upcase(x) | _: x end"#,
    true,
    "wildcard arm after :string: arm should succeed (x is non-String)"
)]
// String literal `"foo"` does NOT trigger subtraction; x in wildcard can still be String
#[case::string_literal_no_subtraction(
    r#"def f(x): match (x): | "foo": "got foo" | _: upcase(x) end"#,
    true,
    "literal string pattern should not subtract String type from wildcard arm"
)]
// Multiple preceding whole-type arms still don't break string-only operations in wildcard
#[case::two_whole_type_arms_before_wildcard(
    r#"def f(x): match (x): | none: 0 | :string:: upcase(x) | _: x end"#,
    true,
    "multiple whole-type arms before wildcard should not cause errors"
)]
// Markdown node-kind labels (`:h1`, `:code`, ...) should type-check like other type labels
#[case::node_kind_label_then_wildcard_valid(
    r#"def f(x): match (x): | :h1:: "top" | _: x end"#,
    true,
    "wildcard arm after :h1: label arm should succeed"
)]
// Attribute selectors (`.lang`, `.depth`, ...) in a guard/body read the matched node
#[case::node_kind_guard_attribute_access_valid(
    r#"def f(x): match (x): | :code if (.lang == "rust"): upcase(.value) | :code: .value | _: x end"#,
    true,
    "guard/body reading attributes of a node-kind pattern should not error"
)]
fn test_cross_arm_narrowing(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {result:?}", description);
}

#[rstest]
// dot and self resolve to dynamic() at root level
#[case::dot_at_root(".", true, "dot is dynamic()")]
#[case::self_at_root("self", true, "self is dynamic()")]
// dynamic() satisfies string parameters
#[case::upcase_dot("upcase(.)", true, "dynamic -> string")]
#[case::upcase_ref("upcase", true, "ref uses dynamic piped input")]
#[case::downcase_dot("downcase(.)", true, "dynamic -> string")]
#[case::trim_dot("trim(.)", true, "dynamic -> string")]
#[case::ltrim_dot("ltrim(.)", true, "dynamic -> string")]
#[case::rtrim_dot("rtrim(.)", true, "dynamic -> string")]
// dynamic() satisfies number parameters
#[case::abs_dot("abs(.)", true, "dynamic -> number")]
#[case::floor_dot("floor(.)", true, "dynamic -> number")]
#[case::ceil_dot("ceil(.)", true, "dynamic -> number")]
// dynamic() satisfies polymorphic parameters
#[case::to_number_dot("to_number(.)", true, "dynamic satisfies any")]
#[case::to_string_dot("to_string(.)", true, "dynamic satisfies any")]
// 0-arg calls use dynamic() as implicit first arg
#[case::len_zero_args("len()", true, "dynamic as implicit arg")]
#[case::trim_zero_args("trim()", true, "dynamic as implicit arg")]
// binary calls: dynamic piped + explicit args
#[case::split_dot("split(., \"x\")", true, "dynamic as string, separator explicit")]
#[case::starts_with_dot("starts_with(., \"hello\")", true, "dynamic as string")]
#[case::replace_two_explicit("replace(\"l\", \"r\")", true, "dynamic piped + 2 explicit = 3 args")]
#[case::word_wrap_dot("word_wrap(., 10)", true, "dynamic as string, width explicit")]
#[case::truncate_dot("truncate(., 10, \"...\")", true, "dynamic as string, width and ellipsis explicit")]
// pipe chains: dynamic pinned to concrete type, then further typed ops
#[case::chain_to_len("upcase(.) | len", true, "dynamic -> string -> number")]
#[case::chain_two_string_ops("upcase(.) | trim(.)", true, "string -> string")]
#[case::chain_len_to_abs("len() | abs(.)", true, "number -> number")]
#[case::chain_floor_to_string("floor(.) | to_string", true, "number -> string")]
#[case::chain_three_ops("upcase(.) | trim(.) | len", true, "three-op chain")]
#[case::chain_len_to_to_string("len() | to_string", true, "number -> string")]
// errors: concrete type mismatch, dynamic() does not shield downstream errors
#[case::concrete_type_error("1 + true", false, "concrete: bool ≠ number")]
#[case::number_piped_to_string("42 | upcase(.)", false, "number piped, upcase needs string")]
#[case::string_piped_to_number("\"hello\" | abs(.)", false, "string piped, abs needs number")]
#[case::bool_piped_to_string("true | downcase", false, "bool piped, downcase needs string")]
#[case::string_pinned_abs_fails("upcase(.) | abs(.)", false, "upcase pins to string, abs needs number")]
#[case::number_pinned_upcase_fails("len() | upcase(.)", false, "len pins to number, upcase needs string")]
#[case::replace_arity_error("replace(\"l\")", false, "1 explicit + dynamic = 2, needs 3")]
#[case::word_wrap_width_wrong_type("word_wrap(\"hello\", \"wide\")", false, "width must be a number")]
#[case::truncate_ellipsis_wrong_type("truncate(\"hello\", 3, 42)", false, "ellipsis must be a string")]
fn test_dynamic_piped_input(#[case] code: &str, #[case] should_succeed: bool, #[case] description: &str) {
    let result = check_types_with_builtins(code);
    assert_eq!(result.is_empty(), should_succeed, "{}: {result:?}", description);
}

// partial is typed from the arity of its function argument
#[rstest]
#[case::two_params("def f(a, b): a + b; | partial(f, 1)", true)]
#[case::five_params_one_bound("def f(a, b, c, d, e): a; | partial(f, 1)", true)]
#[case::five_params_four_bound("def f(a, b, c, d, e): a; | partial(f, 1, 2, 3, 4)", true)]
#[case::six_params_three_bound("def f(a, b, c, d, e, g): a; | let p = partial(f, 1, 2, 3) | p(4, 5, 6)", true)]
#[case::reapplied(
    "def f(a, b, c): a + b + c; | let p = partial(f, 1) | let q = partial(p, 2) | q(3)",
    true
)]
#[case::lambda("let f = fn(a, b, c): a + b + c; | let p = partial(f, 1) | p(2, 3)", true)]
#[case::all_bound("def f(a, b, c): a; | partial(f, 1, 2, 3)", false)]
#[case::too_many_bound("def f(a, b): a; | partial(f, 1, 2)", false)]
#[case::not_a_function("partial(\"x\", 1)", false)]
#[case::no_arguments("partial()", false)]
#[case::piped_function("def f(a, b, c): a; | f | partial(1)", true)]
#[case::piped_function_five_params("def f(a, b, c, d, e): a; | f | partial(1)", true)]
#[case::piped_function_multiple_bound("def f(a, b, c): a; | f | partial(1, 2)", true)]
#[case::piped_function_all_bound("def f(a, b): a; | f | partial(1, 2)", false)]
#[case::piped_non_function("\"abc\" | partial(1)", false)]
#[case::explicit_function_all_bound_ignores_piped("def f(a, b): a; | f | partial(f, 1, 2)", false)]
#[case::piped_function_variable_bound("def f(a, b, c): a; | let x = 1 | f | partial(x, 2)", true)]
#[case::piped_function_variable_bound_called(
    "def f(a, b, c): a + b + c; | let x = 1 | let p = do f | partial(x, 2) end | p(3)",
    true
)]
#[case::piped_function_variable_all_bound("def f(a, b): a; | let x = 1 | f | partial(x, 2)", false)]
#[case::variable_function_ignores_piped("def f(a, b, c): a; | let g = f | let x = 1 | \"abc\" | partial(g, x)", true)]
fn test_partial_arity(#[case] code: &str, #[case] should_succeed: bool) {
    let result = check_types_with_builtins(code);
    assert_eq!(result.is_empty(), should_succeed, "{code}: {result:?}");
}

#[rstest]
#[case::comparison(r#"let v = len([1, 2]) | if (is_string(v)): (v > 9) else: 1"#)]
#[case::short_circuit(r#"let v = len([1, 2]) | if (is_string(v)): (is_string(1) && (v > 9)) else: 1"#)]
fn test_unreachable_then_branch_reports_nothing(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "unreachable branch must not report: {:?}", result);
}

#[rstest]
#[case::else_of_union_with_pending_member(
    r#"let u = if (true): 3 else: get({"b": "x"}, "b") | if (is_string(u)): upcase(u) else: u + 1"#
)]
#[case::and_right_operand(r#"let u = if (true): 1 else: "a" | if (is_string(u) && len(u) > 0): upcase(u) else: 0"#)]
#[case::or_right_operand(r#"let u = if (true): 1 else: "a" | if (is_string(u) || u > 0): 1 else: 0"#)]
#[case::set_other_value_type(r#"let d = {"a": 1} | set(d, "c", "x") | keys(d)"#)]
fn test_narrowing_and_set_report_nothing_for_valid_programs(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "valid program must not report: {:?}", result);
}

#[rstest]
#[case::condition(r#""abc" | if (is_string()): 1 else: 2"#)]
#[case::branch(r#""abc" | if (true): upcase() else: "x""#)]
#[case::elif(r#""abc" | if (false): 1 elif (is_none()): 2 else: 3"#)]
#[case::in_function(r#"def f(): if (is_none()): None else: upcase(); | "a" | f()"#)]
fn test_piped_input_reaches_if_condition_and_branches(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "implicit input must not report: {:?}", result);
}

#[test]
fn test_piped_input_type_reaches_if_branches() {
    let result = check_types_with_builtins(r#""abc" | if (true): abs() else: 1"#);
    assert!(!result.is_empty(), "abs of a string must still be reported");
}

#[rstest]
#[case::eq_other_types(r#"let x = [1] | if (x == "List"): 1 else: 2"#)]
#[case::eq_record_and_string(r#"{"a": 1} == "s""#)]
#[case::concat_other_element_types(r#"[1] + ["a"]"#)]
#[case::concat_records(r#"[{a: 1}] + [{a: "x"}]"#)]
#[case::append_other_type(r#"[1] + "a""#)]
#[case::accumulate(r#"var p = [] | p += [{t: "a", v: "x"}] | p += [{t: "b", v: ["y"]}] | p"#)]
fn test_values_of_other_types_are_not_reported(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "valid program must not report: {:?}", result);
}

#[rstest]
#[case::branch_not_pinned(r#"var cur = "" | if (true): [1] else: do cur = "b" | cur end | cur"#)]
#[case::self_assignment_keeps_type(r#"def f(s): var cur = "" | cur = cur + s | cur end | f("a") | upcase(f("b"))"#)]
#[case::accumulate_in_loop(
    r#"def f(s): var cur = "" | var i = 0 | let _ = while (i < 3): do cur = cur + s[i] | i += 1 | cur end end | cur end | f("abc")"#
)]
fn test_unsettled_branch_and_reassignment_report_nothing(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "valid program must not report: {:?}", result);
}

#[test]
fn test_parenthesized_call_of_a_variable_is_not_a_slice() {
    let code = "def make(p): fn(input, pos): p; | let f = make(1) | f(\"a\", 2)";
    let result = check_types_with_builtins(code);
    assert!(
        result.is_empty(),
        "calling a variable must not be read as a slice: {:?}",
        result
    );
}

#[rstest]
#[case::node_upcase(r##"upcase(to_md_fragment(["h"]))""##)]
#[case::node_starts_with(r##"starts_with(first(to_markdown("# h")), "h")""##)]
#[case::node_len(r##"len(to_md_fragment(["h"]))""##)]
#[case::node_regex(r##"regex_match(first(to_markdown("# h")), "h")""##)]
#[case::none_starts_with(r#"starts_with(None, "a")"#)]
#[case::none_index(r#"index(None, "a")"#)]
#[case::token_count_none("token_count(None)")]
#[case::to_text_any("to_text(2)")]
#[case::to_markdown_string_array(r##"to_markdown_string(to_markdown("# h"))""##)]
#[case::url_encode_number("url_encode(2)")]
#[case::has_other_type(r#"has([1, 2], "a")"#)]
#[case::has_other_key(r#"has({"a": 1}, 2)"#)]
#[case::index_other_type(r#"index([1, 2], "a")"#)]
#[case::coalesce_other_type(r#"42 ?? "fallback""#)]
#[case::repeat_string(r#"2 * "ab""#)]
#[case::is_nan_string(r#"is_nan("a")"#)]
#[case::code_without_language("to_code(2, None)")]
#[case::contains_none("contains(None, 2)")]
#[case::contains_other_type(r#"contains([1, 2], "a")"#)]
#[case::in_none("in(None, 2)")]
#[case::bsearch_other_type(r#"bsearch([1, 2], "a")"#)]
#[case::indices_in_array("indices([1, 2], 1)")]
#[case::indices_in_none(r#"indices(None, "a")"#)]
#[case::shift_string(r#"shift_left("abc", 1)"#)]
#[case::order_arrays("[1, 2] < [1, 3]")]
#[case::order_none("None < None")]
#[case::insert_into_string(r#"insert("ab", 1, "x")"#)]
#[case::set_other_key_type(r#"set({"a": 1}, :sym, "x")"#)]
#[case::update_node_text(r##"update(to_md_fragment(["h"]), "x")"##)]
fn test_builtin_accepts_what_the_runtime_accepts(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "the runtime accepts it: {:?}", result);
}

#[rstest]
#[case::number("foreach (x, 4): x end")]
#[case::bool("foreach (x, true): x end")]
#[case::dict(r#"foreach (x, {"a": 1}): x end"#)]
fn test_foreach_over_something_that_is_not_a_sequence_is_reported(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(!result.is_empty(), "the runtime fails on {code}");
}

#[rstest]
#[case::array("foreach (x, [1, 2]): x + 1 end")]
#[case::string(r#"foreach (c, "ab"): upcase(c) end"#)]
fn test_foreach_over_a_sequence_is_not_reported(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "{:?}", result);
}

#[test]
fn test_a_comparison_gives_a_bool_before_its_operands_are_known() {
    let result = check_types_with_builtins("let v0 = abs(4) | let v1 = v0 | sort((v0 > v1))");
    assert!(!result.is_empty(), "sort of a bool must be reported");
}

#[rstest]
#[case::operand_of_a_deferred_call(r#"let d = {"a": true, "b": "x"} | d["a"] + len(d["b"])"#)]
#[case::deferred_call_as_operand(r#"let d = {"a": true, "b": "x"} | upcase(d["b"]) + d["a"]"#)]
fn test_an_operation_waiting_for_a_call_is_checked_once_it_is_known(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(!result.is_empty(), "bool + number must be reported");
}

#[test]
fn test_strict_array_heterogeneous_is_an_error() {
    let mut hir = Hir::default();
    hir.add_code(None, r#"[1, "hello"]"#);
    let options = TypeCheckerOptions {
        strict_array: true,
        ..TypeCheckerOptions::default()
    };
    let found = TypeChecker::with_options(options.clone()).check_classified(&hir);
    assert!(
        found
            .iter()
            .any(|c| matches!(c.error, TypeError::HeterogeneousArray { .. })
                && c.severity(&options) == Some(mq_check::Severity::Error)),
        "{found:?}"
    );
}

#[rstest]
#[case::number_to_html("to_html(2)")]
#[case::array_to_html("to_html([1])")]
#[case::non_string_prefix(r#"starts_with(["a"], 1)"#)]
#[case::non_string_suffix(r#"ends_with(["a"], 1)"#)]
#[case::shift_right_array_number(r#"shift_right(["a"], 1)"#)]
#[case::branch_of_a_variable_assigned_later(r#"var x = "a" | if (is_string(x)): x - 1 else: 0 | x = 2"#)]
fn test_runtime_failure_is_a_type_error(#[case] code: &str) {
    assert!(
        !check_types_with_builtins(code).is_empty(),
        "expected a type error for {code}"
    );
}

#[rstest]
#[case::string_to_html(r##"to_html("# h")"##)]
#[case::node_to_html(r##"to_html(first(to_markdown("# h")))"##)]
#[case::string_array_starts_with(r#"starts_with(["a"], "a")"#)]
#[case::number_array_starts_with(r#"starts_with([1], "1")"#)]
#[case::node_array_ends_with(r##"ends_with(to_markdown("# h"), "h")"##)]
#[case::shift_left_appends(r#"shift_left(["a"], 1) | map(fn(x): to_text(x);)"#)]
#[case::shift_right_prepends(r#"shift_right(1, ["a"]) | map(fn(x): to_text(x);)"#)]
#[case::upcase_keeps_the_node(r##"upcase(first(to_markdown("# h"))) | set_check(true)"##)]
#[case::trim_keeps_the_node(r##"trim(first(to_markdown("- h"))) | set_check(true)"##)]
#[case::upcase_string(r#"upcase("a") | len()"#)]
#[case::coalesce_keeps_the_left(r#"let n = 42 ?? "fallback" | n - 1"#)]
#[case::default_that_the_body_takes(r#"def f(x, y = 1): abs(x) + y; | f(1)"#)]
#[case::parameter_that_a_type_fits(r#"def f(x): x + 1 | upcase(x); | f("a")"#)]
#[case::lambda_that_takes_the_argument(r#"def apply(f, v): f(v); | apply(fn(x): abs(x);, 1)"#)]
#[case::lambda_applied_in_a_branch(r#"def apply(f, v): if (true): f(v) else: 0; | apply(fn(x): abs(x);, "a")"#)]
#[case::lambda_without_params(r#"def apply(f, n): f(n); | apply(fn: self + 1, 5)"#)]
#[case::lambda_of_a_builtin_that_takes_the_elements(r#"def f(xs): map(xs, fn(s): abs(s);); | f([1])"#)]
#[case::lambda_of_a_builtin_over_no_elements(r#"def f(xs): map(xs, fn(s): abs(s);); | f([])"#)]
#[case::select_condition_is_not_called(r#"def f(xs): select(xs, fn(x): abs(x);); | f(["a"])"#)]
#[case::lambda_over_the_result_of_a_map_that_takes_it(
    r#"def f(xs): xs | map(fn(s): to_string(s);) | map(fn(t): upcase(t);); | f([1])"#
)]
#[case::call_that_the_body_takes(r#"def f(x): abs(x); | f(1)"#)]
#[case::body_that_tests_the_type(r#"def f(x): if (is_string(x)): upcase(x) else: abs(x); | [f("a"), f(1)]"#)]
#[case::body_that_catches_the_failure(r#"def f(x): try: abs(x) catch: 0; | f("a")"#)]
#[case::parameter_that_is_assigned(r#"def f(x): var y = x | y = 1 | abs(y); | f("a")"#)]
#[case::pipe_into_a_call_that_takes_it(r#"def f(x): x | upcase(); | "a" | f()"#)]
fn test_valid_code_has_no_type_error(#[case] code: &str) {
    let found = check_types_with_builtins(code);
    assert!(
        found.iter().all(TypeError::is_advisory),
        "unexpected type error for {code}: {found:?}"
    );
}

#[rstest]
#[case::first("first([1, 2]) | add(1)")]
#[case::last("last([1, 2]) | add(1)")]
#[case::get("get([1, 2], 0) | add(1)")]
fn test_array_element_access_may_be_none(#[case] code: &str) {
    assert!(
        !check_types_with_builtins(code).is_empty(),
        "an array element can be none, so `{code}` must be rejected"
    );
}

#[rstest]
#[case::first_narrowed("let n = first([1, 2]) | if (is_none(n)): 0 else: n + 1")]
#[case::last_narrowed("let n = last([1, 2]) | if (is_none(n)): 0 else: n + 1")]
#[case::get_narrowed("let n = get([1, 2], 0) | if (is_none(n)): 0 else: n + 1")]
#[case::default("first([1, 2]) | default(0) | add(1)")]
fn test_array_element_access_none_can_be_handled(#[case] code: &str) {
    let result = check_types_with_builtins(code);
    assert!(result.is_empty(), "`{code}` must type check: {result:?}");
}

#[rstest]
#[case::root_call(r#"abs("a")"#, "`abs` cannot be applied to (string)")]
#[case::extra_argument(r#"abs("a", 1)"#, "`abs` cannot be applied to (string, number)")]
#[case::piped(r#""a" | abs()"#, "`abs` cannot be applied to (string)")]
fn test_no_overload_message_leaves_out_the_unknown_query_input(#[case] code: &str, #[case] expected: &str) {
    let errors = check_types_with_builtins(code);
    assert_eq!(
        errors.first().map(ToString::to_string).as_deref(),
        Some(expected),
        "{errors:?}"
    );
}

#[rstest]
#[case::known_reference(r#"var x = "a" | let y = 1 | x = y | x | abs()"#, true)]
#[case::known_reference_old_type_gone(r#"var x = "a" | let y = 1 | x = y | x | upcase()"#, false)]
#[case::unknown_parameter_keeps_previous_type(r#"def f(p): var x = "a" | x = p | x | upcase();"#, true)]
fn test_assigning_a_reference_replaces_the_variable_type(#[case] code: &str, #[case] ok: bool) {
    let errors = check_types_with_builtins(code);
    assert_eq!(errors.is_empty(), ok, "`{code}`: {errors:?}");
}

#[test]
fn test_no_overload_message_keeps_an_explicit_dynamic_argument() {
    let errors = check_types_with_builtins("def f(): abs(next(), 1);");
    assert_eq!(
        errors.first().map(ToString::to_string).as_deref(),
        Some("`abs` cannot be applied to (dynamic(), number)"),
        "{errors:?}"
    );
}
