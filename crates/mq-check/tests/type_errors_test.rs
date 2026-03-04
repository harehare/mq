//! Tests for type error detection
//!
//! This test file demonstrates which type errors are currently detected
//! and which ones are not yet implemented.

use mq_check::{TypeChecker, TypeCheckerOptions, TypeError};
use mq_hir::Hir;

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

#[test]
fn test_binary_op_type_mismatch() {
    let result = check_types(r#"1 + true"#);
    println!("Binary op type mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected type error for number + boolean");
}

#[test]
fn test_if_else_type_mismatch() {
    // mq is dynamically typed — if/else branches with different types are allowed
    let result = check_types(r#"if (true): 42 else: "string";"#);
    assert!(
        result.is_empty(),
        "if/else branches with different types should be allowed in dynamically typed mq: {:?}",
        result
    );
}

#[test]
fn test_heterogeneous_array_allowed() {
    // mq is dynamically typed — heterogeneous arrays (used as tuples) are valid
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
    let result = check_types("def add(x, y): x + y;\n| add(1)");
    println!("Function arity mismatch: {:?}", result);
    assert!(!result.is_empty(), "Expected arity mismatch error");
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
    // mq is dynamically typed — match arms with different body types are allowed
    let result = check_types(r#"match (1): | 1: "one" | 2: 222 end"#);
    println!("Match arm body type mismatch: {:?}", result);
    assert!(
        result.is_empty(),
        "Match arms with different body types should be allowed in dynamically typed mq: {:?}",
        result
    );
}

// ============================================================================
// Expected Success Cases (should always pass)
// ============================================================================

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
    let result = check_types(r#"match (1): | 1: "one" | 2: "two" end"#);
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
fn test_success_macro_definition() {
    let result = check_types("macro inc(x): x + 1;");
    for e in &result {
        eprintln!("Error: {}", e);
    }
    assert!(result.is_empty(), "Macro definition should succeed");
}

// --- Type Narrowing Tests ---

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
    // is_string(x) || is_bool(y) — different variables, no then-branch narrowing (conservative)
    let result = check_types(r#"def f(x, y): if (is_string(x) || is_bool(y)): x else: none;"#);
    assert!(
        result.is_empty(),
        "OR narrowing over different variables should produce no errors: {result:?}"
    );
}

#[test]
fn test_narrowing_or_else_branch_complement() {
    // is_string(x) || is_number(x) — else-branch: x is neither String nor Number
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

// ============================================================================
// Strict Array Mode Tests
// ============================================================================

/// Helper function to run type checker with strict array mode enabled
fn check_types_strict_array(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::with_options(TypeCheckerOptions {
        strict_array: true,
        tuple: false,
    });
    checker.check(&hir)
}

/// Helper function to run type checker with tuple mode enabled
fn check_types_tuple(code: &str) -> Vec<TypeError> {
    let hir = create_hir(code);
    let mut checker = TypeChecker::with_options(TypeCheckerOptions {
        strict_array: false,
        tuple: true,
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

// ============================================================================
// Array Element Access Type Propagation Tests
// ============================================================================

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

// ============================================================================
// Tuple Mode Tests
// ============================================================================

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
