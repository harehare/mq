use super::*;
use crate::Selector;
use crate::ast::node::{self as ast, Args, MatchArm, Param, Pattern};
use crate::error::runtime::RuntimeError;
use crate::number::{INFINITE, NAN, Number};
use crate::range::Range;
use crate::{
    AstExpr, AstNode, DefaultModuleLoader, Ident, IdentWithToken, Program, Token, TokenKind, arena::Arena,
    error::InnerError, token_alloc,
};
use crate::{Shared, SharedCell};
use proptest::prelude::*;
use rstest::rstest;
use smallvec::{SmallVec, smallvec};
use std::f64::consts::PI;

#[rstest]
#[case::selector_chain(".h1 | .text")]
#[case::builtin_calls("upcase(.) | trim(.)")]
#[cfg(not(feature = "debugger"))]
fn non_tail_pipe_stage_fuses_into_tee_local(#[case] code: &str) {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::TeeLocal(_))),
        "{code:?} should fuse into TeeLocal, got {:?}",
        compiled.chunks[0].code
    );
    assert!(
        !compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::SetLocal(_)))
    );
}

#[test]
fn tee_local_fusion_preserves_pipe_result() {
    assert_eq!(
        run_with_prelude(r#""  hello  " | trim() | upcase()"#),
        RuntimeValue::String(Shared::new("HELLO".to_string()))
    );
}

#[test]
fn node_selectors_use_compact_bytecode_instructions() {
    use super::bytecode::{NodeSelectorKind, OpCode};

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(".h1 | .code | .[0]", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::SelectorMatchHeading(1)))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::SelectorMatchKind(NodeSelectorKind::Code)))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::SelectorMatch(_)))
    );
}

#[rstest::fixture]
fn token_arena() -> Shared<SharedCell<Arena<Shared<Token>>>> {
    let token_arena = Shared::new(SharedCell::new(Arena::new(10)));
    token_alloc(
        &token_arena,
        &Shared::new(Token {
            kind: TokenKind::Eof,
            range: Range::default(),
            module_id: 1.into(),
        }),
    );
    token_arena
}

fn ast_node(expr: AstExpr) -> Shared<AstNode> {
    Shared::new(AstNode {
        token_id: 0.into(),
        expr: Shared::new(expr),
    })
}

fn ast_call(name: &str, args: Args) -> Shared<AstNode> {
    Shared::new(AstNode {
        token_id: 0.into(),
        expr: Shared::new(ast::Expr::Call(IdentWithToken::new(name), args)),
    })
}

// The shared table keeps VM and evaluator cases aligned.
crate::eval_table_cases!(
    evaluator_table_cases_run_on_vm,
    token_arena,
    runtime_values,
    program,
    expected,
    {
        let host_functions = HostFunctions::default();
        let vm_result = compile_and_run_many(
            &program,
            runtime_values.into_iter(),
            EngineRunContext {
                host_functions: &host_functions,
                // Hand-built AST cases must never leave the VM test worker running forever.
                timeout: Some(std::time::Duration::from_secs(1)),
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: DefaultModuleLoader::default(),
                global_bindings: &[],
                session: None,
            },
        );

        match expected {
            Ok(expected_values) => assert_eq!(
                vm_result.expect("VM should accept a successful evaluator table case"),
                expected_values,
            ),
            Err(_) => assert!(vm_result.is_err(), "VM should reject an evaluator error case"),
        }
    }
);

fn run(code: &str) -> RuntimeValue {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    compile_and_run(&program, token_arena).unwrap()
}

fn run_with_input(code: &str, input: RuntimeValue) -> RuntimeValue {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    compile_and_run_with_input(&program, input, token_arena).unwrap()
}

fn run_with_max_depth(code: &str, max_call_stack_depth: u32) -> Result<RuntimeValue, interpreter::VmError> {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
    interpreter::run(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        max_call_stack_depth,
    )
}

#[test]
fn try_catch_body_counts_toward_call_stack_depth() {
    // Each recursive level wraps its call in `try`/`catch`, so entering the try body is an
    // extra native call frame per level on top of the recursive call itself. With a call-stack
    // limit of 5, recursion through this native frame reaches the limit after only 2 levels
    // (it's caught by the nearest `catch`, producing `1 + (1 + (-1))` = 0) instead of behaving
    // just like the untracked, plain-recursion case below (which reaches `f(4)` = 4 uncapped).
    let code = "def f(n): if (n <= 0): 0 else: try: 1 + f(n - 1) catch(e): -1; | f(4)";
    assert_eq!(run_with_max_depth(code, 5).unwrap(), RuntimeValue::Number(0.into()));

    // Plain recursion (no try/catch) only costs one call-stack unit per level, so the same
    // limit comfortably completes all 4 levels.
    let no_try = "def f(n): if (n <= 0): 0 else: 1 + f(n - 1); | f(4)";
    assert_eq!(run_with_max_depth(no_try, 5).unwrap(), RuntimeValue::Number(4.into()));
}

fn run_with_prelude(code: &str) -> RuntimeValue {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let compiled =
        compiler::compile_program_with_builtin_prelude(&program, token_arena, ModuleLoader::new(StdModuleResolver))
            .unwrap();
    match interpreter::run(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
    ) {
        Ok(v) => v,
        Err(e) => panic!("{e}"),
    }
}

#[test]
fn non_capturing_closures_use_chunk_static_storage() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let f = fn(x): x + 1; | f(2)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert_eq!(compiled.chunks[0].static_closures.len(), 1);
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::MakeStaticClosure(0)))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallLocal(_, 1)))
    );
    assert_eq!(compiled.chunks[1].param_shape.fixed_required_arity(), Some(1));
}

#[test]
fn local_binary_expressions_use_compact_bytecode() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let f = fn(x): x * 2; | f(2)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[1]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::BinaryLocalConst { .. }))
    );
}

#[test]
fn top_level_def_calls_use_call_local() {
    use super::bytecode::OpCode;
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        r#"def identity(x): x; | foreach(i, range(0, 1000, 1)): identity(i);"#,
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
    assert!(
        compiled
            .chunks
            .iter()
            .any(|c| c.code.iter().any(|op| matches!(op, OpCode::CallLocal(_, _)))),
        "top-level def call should compile to CallLocal, not the slower CallValue path"
    );
}

#[test]
fn local_array_accesses_use_compact_bytecode() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let values = [1, 2] | let index = 0 | len(values) + get(values, index)",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::ArrayLenLocal(_)))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::ArrayGetLocalAt { .. }))
    );
    assert_eq!(
        run("let values = [1] | let index = 0 | get(values, index) | get(values, index)"),
        RuntimeValue::Number(1.into())
    );
}

#[test]
fn foreach_uses_the_specialized_iteration_opcode() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("foreach(x, [1, 2]): x;", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::ForeachNext { .. }))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::ForeachCollect(_)))
    );
    assert!(
        !compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::ArrayLen | OpCode::ArrayGetAt))
    );
}

#[test]
fn capturing_closures_keep_dynamic_capture_sources() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let x = 1 | let f = fn(): x; | f()", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::MakeClosure(_)))
    );
}

#[test]
fn mutable_local_calls_keep_the_generic_value_call_path() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("var f = fn(x): x + 1; | f(2)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallValue(1)))
    );
    assert!(
        !compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallLocal(_, _)))
    );
}

#[test]
fn dynamic_fixed_call_keeps_the_callee_value_from_before_argument_evaluation() {
    assert_eq!(
        run("var f = fn(x): x + 1; | let update = fn(): f = fn(x): x + 2; | 0; | f(update())"),
        RuntimeValue::Number(1.into())
    );
}

#[test]
fn engine_compiler_loads_only_reachable_soft_builtins() {
    let selected_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let selected_program = crate::parse("range(0, 3, 1) | map(fn(x): x * 2;)", Shared::clone(&selected_arena)).unwrap();
    let selected = compiler::compile_program_for_engine(
        &selected_program,
        selected_arena,
        ModuleLoader::new(StdModuleResolver),
        &[],
    )
    .unwrap();

    let full_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let full_program = crate::parse("range(0, 3, 1) | map(fn(x): x * 2;)", Shared::clone(&full_arena)).unwrap();
    let full =
        compiler::compile_program_with_builtin_prelude(&full_program, full_arena, ModuleLoader::new(StdModuleResolver))
            .unwrap();

    assert!(selected.chunks.len() < full.chunks.len());
}

#[test]
fn engine_compiler_skips_soft_builtins_shadowed_by_user_definitions() {
    let shadowed_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let shadowed_program = crate::parse(
        "def identity(x): x; | foreach(i, range(0, 3, 1)): identity(i);",
        Shared::clone(&shadowed_arena),
    )
    .unwrap();
    let shadowed = compiler::compile_program_for_engine(
        &shadowed_program,
        shadowed_arena,
        ModuleLoader::new(StdModuleResolver),
        &[],
    )
    .unwrap();

    let plain_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let plain_program = crate::parse(
        "def local_identity(x): x; | foreach(i, range(0, 3, 1)): local_identity(i);",
        Shared::clone(&plain_arena),
    )
    .unwrap();
    let plain =
        compiler::compile_program_for_engine(&plain_program, plain_arena, ModuleLoader::new(StdModuleResolver), &[])
            .unwrap();

    // The soft builtin with the same name must not be compiled just because it exists
    // in `builtin.mq`; both equivalent user functions should yield the same chunks.
    assert_eq!(shadowed.chunks.len(), plain.chunks.len());
}

#[test]
fn engine_compiler_loads_only_reachable_module_exports() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("include \"csv\" | csv_parse(true)", Shared::clone(&token_arena)).unwrap();
    let compiled =
        compiler::compile_program_for_engine(&program, token_arena, ModuleLoader::new(StdModuleResolver), &[]).unwrap();

    // The top-level chunk and `csv_parse`; the remaining CSV exports are not reachable.
    assert_eq!(compiled.chunks.len(), 2);
}

#[test]
fn engine_compiler_reachable_prelude_cache_is_correct_across_different_queries() {
    fn compile_and_run_for_engine(code: &str) -> RuntimeValue {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let compiled =
            compiler::compile_program_for_engine(&program, token_arena, ModuleLoader::new(StdModuleResolver), &[])
                .unwrap();
        interpreter::run(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            crate::eval::Options::default().max_call_stack_depth,
        )
        .unwrap()
    }

    // Interleaves queries reaching disjoint soft-builtin sets, so a stale
    // `builtin_dependency_graph` cache entry from one would break another.
    assert_eq!(compile_and_run_for_engine("is_array(1)"), RuntimeValue::Boolean(false));
    assert_eq!(
        compile_and_run_for_engine("first([1, 2, 3])"),
        RuntimeValue::Number(1.into())
    );
    assert_eq!(compile_and_run_for_engine("is_array([1])"), RuntimeValue::Boolean(true));
}

#[test]
#[cfg(not(feature = "debugger"))]
fn non_tail_let_and_var_do_not_round_trip_self_through_local_zero() {
    use super::bytecode::{OpCode, SELF_SLOT};

    for code in [
        "let a = 1 | let b = 2 | a + b",
        "var a = 1 | var b = 2 | a + b",
        "[1, 2] | let [a, b] = [10, 20] | a + b",
        "def f(x): let a = x * 2 | let b = a + 1 | b; | f(5)",
    ] {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

        let has_self_round_trip = compiled.chunks.iter().any(|chunk| {
            chunk.code.windows(2).any(|pair| {
                matches!(
                    pair,
                    [OpCode::GetLocal(a), OpCode::SetLocal(b)] if *a == SELF_SLOT && *b == SELF_SLOT
                )
            })
        });
        assert!(
            !has_self_round_trip,
            "{code:?} should not round-trip self through GetLocal(SELF_SLOT)/SetLocal(SELF_SLOT)"
        );
    }
}

#[test]
fn non_tail_let_and_var_preserve_self_across_bindings() {
    assert_eq!(
        run(r#""hello" | let a = 1 | var b = 2 | len()"#),
        RuntimeValue::Number(5.into())
    );
    assert_eq!(
        run("[1, 2, 3] | let [a, b] = [10, 20] | len()"),
        RuntimeValue::Number(3.into())
    );
    assert_eq!(
        run("def f(x): let a = x * 2 | let b = a + 1 | b; | f(5)"),
        RuntimeValue::Number(11.into())
    );
}

#[test]
fn let_shadowing_a_loop_local_reuses_its_slot_instead_of_hanging() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let x = 0 | loop: let x = x + 1 | if(x > 5): break else: x;;",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let result = compile_and_run_full(
        &program,
        RuntimeValue::None,
        &HostFunctions::default(),
        Some(std::time::Duration::from_secs(5)),
        token_arena,
    )
    .unwrap();
    assert_eq!(result, RuntimeValue::Number(5.0.into()));
}

#[test]
fn let_shadowing_mutates_a_prior_closures_capture() {
    let result = run("let x = 1 | let f = fn(): x; | let x = 2 | f()");
    assert_eq!(result, RuntimeValue::Number(2.0.into()));
}

#[test]
fn vm_closure_stored_in_a_dict_is_callable_once_retrieved() {
    let result = run_with_prelude(r#"def f(): 1; | let d = {"name": "x", "func": f} | d["func"]()"#);
    assert_eq!(result, RuntimeValue::Number(1.0.into()));
}

#[test]
fn partial_works_on_a_vm_closure() {
    let result = run_with_prelude("def add(x, y): x + y; | let add5 = partial(add, 5) | add5(3)");
    assert_eq!(result, RuntimeValue::Number(8.0.into()));
}

#[rstest]
#[case("1 + 2", 3.0)]
#[case("10 - 4", 6.0)]
#[case("3 * 4", 12.0)]
#[case("10 / 4", 2.5)]
#[case("10 % 3", 1.0)]
#[case("-5 + 2", -3.0)]
fn arithmetic(#[case] code: &str, #[case] expected: f64) {
    assert_eq!(run(code), RuntimeValue::Number(expected.into()));
}

#[test]
fn paren_free_prelude_builtin_still_resolves_after_load_builtin_module() {
    let mut engine = crate::DefaultEngine::default();
    engine.load_builtin_module();
    let result = engine
        .eval(r#"sort(["b", "a", "c"]) | first"#, std::iter::once(RuntimeValue::None))
        .unwrap();
    assert_eq!(result.values()[0], RuntimeValue::String(Shared::new("a".to_string())));
}

#[rstest]
#[case("1 < 2", true)]
#[case("2 < 1", false)]
#[case("2 <= 2", true)]
#[case("3 > 2", true)]
#[case("2 == 2", true)]
#[case("2 != 3", true)]
fn comparisons(#[case] code: &str, #[case] expected: bool) {
    assert_eq!(run(code), RuntimeValue::Boolean(expected));
}

#[rstest]
#[case::if_true("if (1 < 2): 10 else: 20", 10.0)]
#[case::if_false("if (2 < 1): 10 else: 20", 20.0)]
#[case::let_binding("let x = 5 | x + 1", 6.0)]
#[case::while_loop("var i = 0 | while (i < 5): i = i + 1; | i", 5.0)]
#[case::recursive_fibonacci(
    "def fibonacci(x): if (x < 2): x else: fibonacci(x - 1) + fibonacci(x - 2); | fibonacci(10)",
    55.0
)]
#[case::closure_captures_outer_local("let x = 10 | let f = fn(y): x + y; | f(5)", 15.0)]
#[case::nested_closure_captures_through_two_levels(
    "let x = 1 | let make = fn(y): fn(z): x + y + z;; | let add_y = make(2) | add_y(3)",
    6.0
)]
#[case::while_break("var x = 0 | while(x < 10): x += 1 | if(x == 3): break else: x;", 2.0)]
#[case::while_continue("var x = 0 | while(x < 4): x += 1 | if(x == 3): continue else: x;", 4.0)]
#[case::loop_break_with_value("loop: break: 42;", 42.0)]
#[case::builtin_shadow_recursion_calls_builtin("def add(a, b): add(a, b) + 1; | add(1, 2)", 4.0)]
#[case::foreach_sums_elements("var total = 0 | foreach(x, array(1, 2, 3, 4)): total = total + x; | total", 10.0)]
#[case::foreach_continue_skips_element(
    "var total = 0 | foreach(x, array(1, 2, 3, 4)): if (x == 2): continue else: total = total + x; | total",
    8.0
)]
#[case::foreach_break_stops_early(
    "var total = 0 | foreach(x, array(1, 2, 3, 4)): if (x == 3): break else: total = total + x; | total",
    3.0
)]
#[case::foreach_collects_results_array("len(foreach(x, array(10, 20, 30)): x * 2;)", 3.0)]
#[case::foreach_results_array_last_elem("get(foreach(x, array(10, 20, 30)): x * 2;, 2)", 60.0)]
#[case::foreach_break_with_value_bypasses_array("foreach(x, array(1, 2, 3)): if (x == 2): break: 999 else: x;", 999.0)]
#[case::try_without_error("try: 5 catch: 99;", 5.0)]
#[case::try_bare_catch_on_error("try: 1 / 0 catch: 99;", 99.0)]
#[case::try_catch_binder_sees_message("try: 1 / 0 catch(e): len(get(e, \"message\"));", 16.0)]
#[case::match_ident_binding_with_guard("match (5) do | x if (x > 3): x * 10 | _: 0 end", 50.0)]
#[case::match_wildcard_fallback("match (99) do | 1: 100 | _: 0 end", 0.0)]
#[case::array_spread_len("len([1, 2, ...[3, 4, 5], 6])", 6.0)]
#[case::array_spread_in_foreach_iterates_all("len(foreach(x, [0, ...[1, 2, 3]]): x + 1;)", 4.0)]
#[case::match_array_rest_pattern("match([1, 2, 3, 4]) do | [first, ..rest]: len(rest) end", 3.0)]
#[case::match_array_rest_binds_first("match([10, 20, 30]) do | [first, ..rest]: first end", 10.0)]
#[case::match_array_exact_pattern("match([1, 2]) do | [a, b]: a + b | _: -1 end", 3.0)]
#[case::match_array_exact_wrong_len("match([1, 2, 3]) do | [a, b]: a + b | _: -1 end", -1.0)]
#[case::match_or_pattern_hit("match(2) do | 1 || 2 || 3: 1 | _: 0 end", 1.0)]
#[case::match_or_pattern_miss("match(9) do | 1 || 2 || 3: 1 | _: 0 end", 0.0)]
#[case::match_dict_pattern(r#"match({"x": 10}) do | {x: v}: v end"#, 10.0)]
#[case::match_dict_pattern_missing_key(r#"match({"x": 10}) do | {y: v}: v | _: -1 end"#, -1.0)]
#[case::match_dict_pattern_key_present_but_none(r#"match({"x": None}) do | {x: v}: -1 | _: -2 end"#, -1.0)]
#[case::or_pattern_shares_binding_slot_alt1("match([1]) do | [a] || [a, _]: a end", 1.0)]
#[case::or_pattern_shares_binding_slot_alt2("match([9, 8]) do | [a] || [a, _]: a end", 9.0)]
#[case::or_pattern_shares_binding_slot_dict_variant(r#"match({"a": 1}) do | {a: v} || {b: v}: v end"#, 1.0)]
#[case::or_pattern_nested_or_reuses_outer_slot_first_alt("match([1]) do | [a] || [a || b]: a end", 1.0)]
#[case::or_pattern_nested_or_reuses_outer_slot_second_alt("match([5]) do | [0] || [a || b]: a end", 5.0)]
#[case::match_guard_sees_pattern_bound_var("match(5) do | x if (x > 3): x | _: -1 end", 5.0)]
#[case::or_pattern_guard_sees_the_matching_alternatives_binding(
    "match([9, 8]) do | [a] || [a, _] if (a > 5): a | _: -1 end",
    9.0
)]
#[case::foreach_loop_var_shadow_does_not_mutate_outer("let x = 100 | foreach(x, [1, 2, 3]): x; | x", 100.0)]
#[case::default_param_used_when_supplied("def add(a, b = 10): a + b; | add(1, 2)", 3.0)]
#[case::default_param_used_when_omitted("def add(a, b = 10): a + b; | add(1)", 11.0)]
#[case::multiple_default_params_all_omitted("def msg(a, b = 2, c = 3): a + b + c; | msg(1)", 6.0)]
#[case::multiple_default_params_some_supplied("def msg(a, b = 2, c = 3): a + b + c; | msg(1, 20)", 24.0)]
#[case::default_param_can_reference_earlier_param("def f(a, b = a + 1): a + b; | f(10)", 21.0)]
#[case::variadic_param_collects_remaining_args("def sum_all(*xs): len(xs); | sum_all(1, 2, 3, 4)", 4.0)]
#[case::variadic_param_collects_nothing_when_absent("def sum_all(*xs): len(xs); | sum_all()", 0.0)]
#[case::required_and_variadic_together("def f(first, *rest): first + len(rest); | f(100, 1, 2, 3)", 103.0)]
#[case::implicit_self_fills_missing_required_arg("def double(x): x * 2; | 21 | double()", 42.0)]
#[case::let_array_destruct("let [a, b] = [1, 2] | add(a, b)", 3.0)]
#[case::let_array_wildcard("let [_, b] = [1, 2] | b", 2.0)]
#[case::let_array_rest("let [first, ..rest] = [1, 2, 3] | len(rest)", 2.0)]
#[case::var_array_destruct_then_reassign("var [a, b] = [1, 2] | a = 10 | a + b", 12.0)]
#[case::module_inline_function("module math: def mysum(a, b): a + b; end | math::mysum(1, 2)", 3.0)]
#[case::module_extension(
    "module math: def mysum(a, b): a + b; end module math: def mymul(a, b): a * b; end | math::mysum(2, 3) + math::mymul(2, 3)",
    11.0
)]
#[case::module_inline_let("module constants: let pi = 314 end | constants::pi", 314.0)]
#[case::qualified_access_from_a_nested_closure(
    "module math: def mysum(a, b): a + b; end | let f = fn(x): math::mysum(x, 1); | f(41)",
    42.0
)]
#[case::qualified_access_from_a_doubly_nested_closure(
    "module math: def mysum(a, b): a + b; end | let f = fn(x): fn(y): math::mysum(x, y);; | let g = f(40) | g(2)",
    42.0
)]
fn programs_yield_number(#[case] code: &str, #[case] expected: f64) {
    assert_eq!(run(code), RuntimeValue::Number(expected.into()));
}

#[rstest]
#[case::arithmetic_and_assignment("var total = 1 | foreach(x, [2, 3, 4]): total += x; | total")]
#[case::recursive_closure("let make = fn(x): fn(y): x + y;; | let add_two = make(2) | add_two(40)")]
#[case::defaults_and_variadic("def f(first, rest = 2, *tail): first + rest + len(tail); | f(10, 20, 1, 2)")]
#[case::try_continue("var total = 0 | foreach(x, array(1, 2, 3)) do try: continue catch: 0 end | total")]
#[case::match_and_destructuring(
    "let [first, ..rest] = [10, 20, 30] | match(rest) do | [a, b]: first + a + b | _: 0 end"
)]
#[case::module_resolution("module math: def twice(x): x * 2; end | math::twice(21)")]
#[case::string_interpolation(r#"let name = "mq" | s"hello, ${name}!""#)]
#[case::array_spread("len([0, ...[1, 2, 3], 4])")]
#[case::builtin_map(r#"range(0, 20, 1) | map(fn(x): x * 2;)"#)]
#[case::builtin_filter(r#"range(0, 20, 1) | filter(fn(x): x % 3 == 0;)"#)]
#[case::builtin_fold(r#"def sum(acc, x): add(acc, x); | fold(range(0, 20, 1), 0, sum)"#)]
#[case::builtin_chain(r#"range(0, 50, 1) | filter(fn(x): x % 2 == 0;) | map(fn(x): x * 3;) | filter(fn(x): x > 10;)"#)]
#[case::foreach_closures_share_the_loop_variables_captured_cell(
    "let fns = foreach(i, [1, 2, 3]): fn(): i;; | fns | map(fn(f): f();)"
)]
#[case::three_level_nested_closure(
    "let x = 1 | let make = fn(y): fn(z): fn(w): x + y + z + w;;; | let step1 = make(2) | let step2 = step1(3) | step2(4)"
)]
#[case::sibling_closures_capture_independent_lets(
    "let a = 1 | let b = 2 | let f = fn(): a; | let g = fn(): b; | f() + g()"
)]
#[case::closure_over_var_sees_later_mutation("var x = 1 | let f = fn(): x; | x = 99 | f()")]
#[case::implicit_self_visible_inside_a_closure_created_by_a_def(
    "def outer(): let g = fn(): . + 1; | g(); | 41 | outer()"
)]
#[case::implicit_self_fill_combined_with_variadic("def f(first, *rest): first + len(rest); | 5 | f()")]
#[case::shadowing_across_if_branches_does_not_leak_to_the_outer_binding(
    "let x = 1 | let inner_a = fn(): let x = 2 | x; | let inner_b = fn(): let x = 3 | x; | let r = if(true): inner_a() else: inner_b() | r + x"
)]
#[case::shadowing_in_sibling_closures_does_not_leak_between_them(
    "let f = fn(): let x = 1 | x; | let g = fn(): let x = 2 | x; | f() + g()"
)]
fn compiled_engine_matches_tree_walker(#[case] code: &str) {
    assert_vm_matches_tree_walker(code, vec![RuntimeValue::None]);
}

// Documents real (matching) behavior, not an aspirational "fresh cell per iteration"
// semantics: both engines reuse the same captured binding for `foreach`'s loop variable
// across iterations, so every closure created inside the loop sees the final value.
#[test]
fn foreach_closures_share_the_loop_variables_captured_cell_exact_values() {
    assert_eq!(
        run_with_prelude("let fns = foreach(i, [1, 2, 3]): fn(): i;; | fns | map(fn(f): f();)"),
        RuntimeValue::Array(
            vec![
                RuntimeValue::Number(3.0.into()),
                RuntimeValue::Number(3.0.into()),
                RuntimeValue::Number(3.0.into()),
            ]
            .into()
        )
    );
}

#[rstest]
#[case::self_and_pipe(". + 1 | . * 2", RuntimeValue::Number(42.0.into()))]
#[case::multiple_inputs(". * .", RuntimeValue::Number(7.0.into()))]
#[case::markdown_selector(".h1", heading(1))]
fn compiled_engine_matches_tree_walker_with_input(#[case] code: &str, #[case] input: RuntimeValue) {
    assert_vm_matches_tree_walker(code, vec![input]);
}

#[test]
fn compiled_engine_matches_tree_walker_for_nodes_aggregation() {
    assert_vm_matches_tree_walker(
        ". * 10 | nodes | len()",
        vec![
            RuntimeValue::Number(1.0.into()),
            RuntimeValue::Number(2.0.into()),
            RuntimeValue::Number(3.0.into()),
        ],
    );
}

#[rstest]
#[case::self_is_the_input_value(".", 42.0)]
#[case::self_threads_through_pipe(". + 1 | . * 2", 86.0)]
#[case::self_inside_function_call_is_caller_self("def f(): . + 1; | 42 | f()", 43.0)]
#[case::let_preserves_pipeline_value("let saved = 10 | .", 42.0)]
#[case::assignment_preserves_pipeline_value("var saved = 10 | saved = 20 | .", 42.0)]
#[case::as_preserves_pipeline_value(". + 1 as saved | .", 42.0)]
#[case::top_level_def_preserves_pipeline_value("def identity(): .; | identity()", 42.0)]
#[case::unless_runs_for_falsy_condition("unless(false): . + 1;", 43.0)]
#[case::until_runs_until_condition_is_true("until(. >= 45): . + 1;", 45.0)]
fn programs_with_number_input_yield_number(#[case] code: &str, #[case] expected: f64) {
    assert_eq!(
        run_with_input(code, RuntimeValue::Number(42.0.into())),
        RuntimeValue::Number(expected.into())
    );
}

#[rstest]
#[case::builtin_fallback_string_concat(r#""a" + "b""#, "ab")]
#[case::while_break_with_value("var x = 0 | while(x < 10): x += 1 | if(x == 5): break: \"found\" else: x;", "found")]
#[case::string_interpolation_expr(r#"let name = "World" | s"Hello, ${name}!""#, "Hello, World!")]
#[case::string_interpolation_number(r#"let n = 1 + 2 | s"sum=${n}""#, "sum=3")]
#[case::match_literal_arm("match (2) do | 1: \"one\" | 2: \"two\" | _: \"other\" end", "two")]
#[case::match_type_pattern(
    "match (array(1, 2, 3)) do | :array: \"is_array\" | :number: \"is_number\" | _: \"other\" end",
    "is_array"
)]
#[case::let_dict_destruct(r#"let {name: n} = {"name": "Alice"} | n"#, "Alice")]
fn programs_yield_string(#[case] code: &str, #[case] expected: &str) {
    assert_eq!(run(code), RuntimeValue::String(Shared::new(expected.to_string())));
}

#[test]
fn false_if_without_else_yields_none() {
    assert_eq!(run("if(false): 1;"), RuntimeValue::None);
}

#[test]
fn interpolation_can_reference_self() {
    assert_eq!(
        run_with_input(r#"s"value=${self}""#, RuntimeValue::Number(42.0.into())),
        RuntimeValue::String(Shared::new("value=42".to_string()))
    );
}

fn heading(depth: u8) -> RuntimeValue {
    RuntimeValue::new_markdown(mq_markdown::Node::Heading(mq_markdown::Heading {
        values: vec![],
        position: None,
        depth,
    }))
}

#[test]
fn selector_matches_heading_of_the_right_depth() {
    assert_ne!(run_with_input(".h1", heading(1)), RuntimeValue::None);
}

#[test]
fn selector_does_not_match_heading_of_a_different_depth() {
    assert_eq!(run_with_input(".h1", heading(2)), RuntimeValue::None);
}

#[test]
fn selector_does_not_match_non_markdown_input() {
    assert_eq!(
        run_with_input(".h1", RuntimeValue::Number(1.0.into())),
        RuntimeValue::None
    );
}

fn text_node(value: &str) -> mq_markdown::Node {
    mq_markdown::Node::Text(mq_markdown::Text {
        value: value.to_string(),
        position: None,
    })
}

/// Reference output from the tree-walking evaluator.
fn tree_walk_eval(code: &str, input: RuntimeValue) -> RuntimeValue {
    tree_walk_eval_many(code, vec![input]).remove(0)
}

fn tree_walk_eval_many(code: &str, inputs: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
    let mut engine = crate::DefaultEngine::default();
    engine.evaluator.load_builtin_module_full().unwrap();
    let compiled = engine.compile(code).unwrap();
    engine.evaluator.eval(compiled.program(), inputs.into_iter()).unwrap()
}

fn vm_engine_eval_many(code: &str, inputs: Vec<RuntimeValue>) -> Vec<RuntimeValue> {
    let mut engine = crate::DefaultEngine::default();
    engine.load_builtin_module();
    let compiled = engine.compile(code).unwrap();
    engine
        .eval_compiled(&compiled, inputs.into_iter())
        .unwrap()
        .values()
        .clone()
}

fn assert_vm_matches_tree_walker(code: &str, inputs: Vec<RuntimeValue>) {
    assert_eq!(
        vm_engine_eval_many(code, inputs.clone()),
        tree_walk_eval_many(code, inputs),
        "VM and tree-walker disagreed for: {code}"
    );
}

#[rstest]
#[case::first_iteration_self_is_the_incoming_value("while(. == 0): is_none(.);")]
#[case::completes_normally("while(. < 5): . + 1;")]
#[case::cond_false_from_the_start("while(. > 100): . + 1;")]
#[case::first_iteration_break_with_value("while(true): break: 999;")]
#[case::first_iteration_bare_continue("var i = 0 | while(i < 3): i += 1 | if (i == 1): continue else: i;;")]
#[case::later_iteration_bare_break("var i = 0 | while(i < 5): i += 1 | if (i == 3): break else: i;;")]
#[case::later_iteration_bare_continue("var i = 0 | while(i < 5): i += 1 | if (i == 3): continue else: i;;")]
#[case::until_completes_normally("until(. >= 5): . + 1;")]
#[case::until_first_iteration_self_is_the_incoming_value("until(. != 0): is_none(.);")]
fn while_until_matches_tree_walker(#[case] code: &str) {
    assert_vm_matches_tree_walker(code, vec![RuntimeValue::Number(0.0.into())]);
}

/// A closure created inside a loop/match block captures the block's *slot*, not a
/// per-iteration snapshot — both engines agree a loop variable is one mutable binding
/// reused every iteration (closures built in different iterations observe the same,
/// final value), while a closure built before the loop keeps its own outer binding.
#[rstest]
#[case::foreach_loop_var_is_one_binding_shared_by_every_closure(
    "let fns = foreach(x, [1, 2, 3]): fn(): x;; | foreach(f, fns): f();"
)]
#[case::foreach_body_local_is_one_binding_shared_by_every_closure(
    "let fns = foreach(x, [1, 2, 3]): let z = x * 10 | fn(): z;; | foreach(f, fns): f();"
)]
#[case::while_body_local_is_one_binding_shared_by_every_closure(
    "var i = 0 | var fns = [] | while (i < 3): let f = fn(): i; | fns = fns + [f] | i += 1; | foreach(f, fns): f();"
)]
#[case::until_body_local_is_one_binding_shared_by_every_closure(
    "var i = 0 | var fns = [] | until (i >= 3): let f = fn(): i; | fns = fns + [f] | i += 1; | foreach(f, fns): f();"
)]
#[case::loop_body_local_closure_sees_the_value_at_capture_time("loop: let f = fn(): 42; | break: f();")]
#[case::match_arm_closure_captures_the_pattern_bound_var(
    "match([1, 2]) do | [a, b]: do let f = fn(): a + b; | f() end end"
)]
#[case::or_pattern_closure_captures_whichever_alternative_matched(
    "match([9, 8]) do | [a] || [a, _]: do let f = fn(): a; | f() end end"
)]
#[case::closure_built_before_a_loop_keeps_its_own_outer_binding(
    "let x = 100 | let f = fn(): x; | foreach(x, [1, 2, 3]): x; | f()"
)]
fn closures_over_scoped_bindings_match_the_tree_walker(#[case] code: &str) {
    assert_vm_matches_tree_walker(code, vec![RuntimeValue::None]);
}

// Deliberate divergence from the tree-walker (which returns None here) — not worth the
// per-iteration cost of matching it exactly.
#[rstest]
#[case::while_first_iteration_bare_break("while(true): break;", 7.0)]
#[case::until_first_iteration_bare_break("until(false): break;", 7.0)]
fn bare_break_before_any_completed_iteration_keeps_the_incoming_value(#[case] code: &str, #[case] input: f64) {
    assert_eq!(
        run_with_input(code, RuntimeValue::Number(input.into())),
        RuntimeValue::Number(input.into())
    );
}

#[test]
fn nodes_aggregates_per_input_results_into_one_run() {
    // `nodes` (see `split_at_nodes`/`run_nodes_aggregate`) collects every input's
    // per-input result into one array and runs the rest of the program against that
    // array once, rather than once per input — `len()` here only makes sense read that
    // way (3 individual numbers each have no `len`, but an array of 3 does).
    let code = "nodes | len()";
    let inputs = vec![
        RuntimeValue::Number(1.0.into()),
        RuntimeValue::Number(2.0.into()),
        RuntimeValue::Number(3.0.into()),
    ];
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let results = compile_and_run_many(
        &program,
        inputs.clone().into_iter(),
        EngineRunContext {
            host_functions: &HostFunctions::default(),
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
        },
    )
    .unwrap();
    assert_eq!(results, tree_walk_eval_many(code, inputs));
    assert_eq!(results, vec![RuntimeValue::Number(3.0.into())]);
}

#[cfg(feature = "debugger")]
#[test]
fn nodes_split_also_works_through_the_debugger_hooked_entry_point() {
    let code = "nodes | len()";
    let inputs = vec![
        RuntimeValue::Number(1.0.into()),
        RuntimeValue::Number(2.0.into()),
        RuntimeValue::Number(3.0.into()),
    ];
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
    let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> = Shared::new(SharedCell::new(Box::new(
        crate::runtime::debugger::DefaultDebuggerHandler,
    )));
    let results = compile_and_run_debugged(
        &program,
        inputs.into_iter(),
        DebugRunContext {
            engine: EngineRunContext {
                host_functions: &HostFunctions::default(),
                timeout: None,
                max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
                session: None,
            },
            debugger,
            handler,
            source: Source {
                name: None,
                code: code.to_string(),
            },
        },
    )
    .unwrap();
    assert_eq!(results, vec![RuntimeValue::Number(3.0.into())]);
}

#[test]
fn nodes_runs_the_pre_nodes_portion_once_per_input_first() {
    let code = ". * 10 | nodes | len()";
    let inputs = vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(2.0.into())];
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let results = compile_and_run_many(
        &program,
        inputs.clone().into_iter(),
        EngineRunContext {
            host_functions: &HostFunctions::default(),
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
        },
    )
    .unwrap();
    assert_eq!(results, tree_walk_eval_many(code, inputs));
    assert_eq!(results, vec![RuntimeValue::Number(2.0.into())]);
}

#[test]
fn markdown_fragment_input_that_matches_at_the_top_runs_only_once() {
    let fragment = mq_markdown::Node::Fragment(mq_markdown::Fragment {
        values: vec![text_node("a"), text_node("b")],
    });
    let code = r#"s"[${to_string(.)}]""#;
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let results = compile_and_run_many(
        &program,
        std::iter::once(RuntimeValue::new_markdown(fragment.clone())),
        EngineRunContext {
            host_functions: &HostFunctions::default(),
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
        },
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0], tree_walk_eval(code, RuntimeValue::new_markdown(fragment)));
    assert_eq!(results[0].to_string(), "[a\nb]");
}

#[test]
fn markdown_selector_recurses_into_a_non_matching_container_to_find_matches_below() {
    let matching_child = mq_markdown::Node::Heading(mq_markdown::Heading {
        values: vec![],
        position: None,
        depth: 1,
    });
    let outer = mq_markdown::Node::Heading(mq_markdown::Heading {
        values: vec![matching_child, text_node("no match anywhere")],
        position: None,
        depth: 2,
    });
    let code = ".h1";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let results = compile_and_run_many(
        &program,
        std::iter::once(RuntimeValue::new_markdown(outer.clone())),
        EngineRunContext {
            host_functions: &HostFunctions::default(),
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
        },
    )
    .unwrap();
    assert_eq!(results[0], tree_walk_eval(code, RuntimeValue::new_markdown(outer)));
}

#[test]
fn non_fragment_markdown_input_still_runs_the_query_once() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(".h1", Shared::clone(&token_arena)).unwrap();
    let results = compile_and_run_many(
        &program,
        std::iter::once(heading(1)),
        EngineRunContext {
            host_functions: &HostFunctions::default(),
            timeout: None,
            max_call_stack_depth: crate::eval::Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
        },
    )
    .unwrap();
    assert_ne!(results[0], RuntimeValue::None);
}

#[test]
fn runtime_error_carries_a_source_token() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("1 | 1 / 0", Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    let Error::Vm(vm_err) = err else {
        panic!("expected a VM error, got {err}");
    };
    assert!(
        vm_err.token_id().is_some(),
        "division-by-zero error should carry a source token"
    );
}

#[test]
fn timeout_enabled_execution_still_checks_the_deadline() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("loop: 1;", Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run_full(
        &program,
        RuntimeValue::None,
        &HostFunctions::default(),
        Some(std::time::Duration::ZERO),
        token_arena,
    )
    .unwrap_err();

    assert!(matches!(
        err,
        Error::Vm(interpreter::VmError::Located(inner, _))
            if matches!(*inner, interpreter::VmError::Timeout(_))
    ));
}

#[cfg(feature = "debugger")]
#[test]
fn debugger_metadata_tracks_boundaries_and_static_slots() {
    use super::bytecode::OpCode;
    use super::debug_symbols::DebugSlot;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let outer = 10 | let f = fn(inner): outer + inner; | f(2)",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::StmtBoundary(_)))
    );
    assert_eq!(
        compiled.chunks[0]
            .debug_symbols
            .bindings()
            .iter()
            .find(|(name, _)| *name == crate::Ident::new("outer"))
            .map(|(_, slot)| *slot),
        Some(DebugSlot::Local(1))
    );
    assert!(
        compiled.chunks[1]
            .debug_symbols
            .bindings()
            .contains(&(crate::Ident::new("outer"), DebugSlot::Upvalue(0)))
    );
    assert!(
        compiled.chunks[1]
            .debug_symbols
            .bindings()
            .contains(&(crate::Ident::new("inner"), DebugSlot::Local(1)))
    );
}

#[cfg(feature = "debugger")]
#[test]
fn debugger_hook_receives_live_bindings_and_call_stack() {
    use super::interpreter::{DebugEvent, DebugHook};

    #[derive(Default)]
    struct Recorder(Vec<DebugEvent>);

    impl DebugHook for Recorder {
        fn on_boundary(&mut self, event: DebugEvent) {
            self.0.push(event);
        }

        fn on_explicit_breakpoint(&mut self, event: DebugEvent) {
            self.0.push(event);
        }
    }

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let outer = 10 | let f = fn(inner): outer + inner; | f(2)",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
    let mut recorder = Recorder::default();

    let result = interpreter::run_with_debug_hook_and_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
        &[],
        &mut recorder,
    )
    .unwrap();

    assert_eq!(result, RuntimeValue::Number(12.0.into()));
    let function_event = recorder
        .0
        .iter()
        .find(|event| {
            event
                .bindings
                .contains(&(crate::Ident::new("inner"), RuntimeValue::Number(2.0.into())))
        })
        .expect("function body should emit a boundary with its parameter");
    assert!(
        function_event
            .bindings
            .contains(&(crate::Ident::new("outer"), RuntimeValue::Number(10.0.into())))
    );
    assert_eq!(function_event.call_stack.len(), 1);
    assert_eq!(function_event.token_id, function_event.node.token_id);
    assert_eq!(function_event.current_value, RuntimeValue::None);
}

#[cfg(feature = "debugger")]
#[test]
fn vm_debugger_hook_adapts_breakpoints_to_existing_handler() {
    use crate::{DebugContext, DebuggerAction, DebuggerHandler, Source, get_token};
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RecordingHandler {
        inner_values: Arc<Mutex<Vec<RuntimeValue>>>,
        #[cfg(feature = "debug-trace")]
        operand_stacks: Arc<Mutex<Vec<Vec<RuntimeValue>>>>,
    }

    impl DebuggerHandler for RecordingHandler {
        fn on_breakpoint_hit(&self, _breakpoint: &crate::Breakpoint, context: &DebugContext) -> DebuggerAction {
            if let Some((_, value)) = context
                .vm_bindings()
                .into_iter()
                .find(|(name, _)| *name == crate::Ident::new("inner"))
            {
                self.inner_values.lock().unwrap().push(value);
            }
            #[cfg(feature = "debug-trace")]
            self.operand_stacks.lock().unwrap().push(context.operand_stack.clone());
            DebuggerAction::Continue
        }
    }

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let outer = 10 |\nlet f = fn(inner):\nouter + inner; |\nf(2)",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(
        &program,
        Shared::clone(&token_arena),
        ModuleLoader::new(StdModuleResolver),
    )
    .unwrap();
    let function_token_id = compiled.chunks[1].debug_nodes[0].0;
    let function_line = get_token(Shared::clone(&token_arena), function_token_id)
        .range
        .start
        .line as usize;

    let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
    debugger.write().unwrap().activate();
    debugger.write().unwrap().add_breakpoint_with_options(
        function_line,
        None,
        None,
        Some("is_expected(inner) && outer == 10".to_string()),
        None,
        None,
    );
    let inner_values = Arc::new(Mutex::new(Vec::new()));
    #[cfg(feature = "debug-trace")]
    let operand_stacks = Arc::new(Mutex::new(Vec::new()));
    let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
        Shared::new(SharedCell::new(Box::new(RecordingHandler {
            inner_values: Arc::clone(&inner_values),
            #[cfg(feature = "debug-trace")]
            operand_stacks: Arc::clone(&operand_stacks),
        })));
    let mut host_functions = HostFunctions::default();
    host_functions.insert("is_expected", |args: &[RuntimeValue]| {
        Ok(RuntimeValue::Boolean(matches!(
            args.first(),
            Some(RuntimeValue::Number(number)) if number.value() == 2.0
        )))
    });
    let mut hook = debugger::VmDebuggerHook::new(
        debugger,
        handler,
        token_arena,
        Source {
            name: None,
            code: String::new(),
        },
        Default::default(),
        ModuleLoader::new(StdModuleResolver),
        host_functions,
    );

    interpreter::run_with_debug_hook_and_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
        &[],
        &mut hook,
    )
    .unwrap();

    assert!(inner_values.lock().unwrap().contains(&RuntimeValue::Number(2.0.into())));
    #[cfg(feature = "debug-trace")]
    assert!(!operand_stacks.lock().unwrap().is_empty());
}

#[cfg(feature = "debugger")]
#[rstest]
#[case::local_slot("let x = 1 |\nx + 1", 0, "x", false, true, 42)]
#[case::captured_upvalue("let x = 1 |\nlet f = fn():\n  x + 1; |\nf()", 1, "x", true, true, 42)]
#[case::top_level_global_scope("let x = 1 |\nx + 1", 0, "x", true, true, 42)]
#[case::unknown_binding("let x = 1 |\nx + 1", 0, "missing", false, false, 2)]
fn vm_debugger_hook_applies_live_frame_writes(
    #[case] code: &str,
    #[case] target_chunk: usize,
    #[case] name: &str,
    #[case] prefer_upvalue: bool,
    #[case] write_expected: bool,
    #[case] expected: i64,
) {
    use crate::{DebugContext, DebuggerAction, DebuggerHandler, Source, get_token};

    #[derive(Debug)]
    struct MutatingHandler {
        name: String,
        prefer_upvalue: bool,
        write_expected: bool,
    }

    impl DebuggerHandler for MutatingHandler {
        fn on_breakpoint_hit(&self, _breakpoint: &crate::Breakpoint, context: &DebugContext) -> DebuggerAction {
            assert_eq!(
                context.set_vm_variable(&self.name, RuntimeValue::Number(41.into()), self.prefer_upvalue),
                self.write_expected
            );
            DebuggerAction::Continue
        }
    }

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(
        &program,
        Shared::clone(&token_arena),
        ModuleLoader::new(StdModuleResolver),
    )
    .unwrap();
    let line = compiled.chunks[target_chunk]
        .debug_nodes
        .iter()
        .map(|(token_id, _)| get_token(Shared::clone(&token_arena), *token_id).range.start.line as usize)
        .max()
        .unwrap();

    let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
    debugger.write().unwrap().activate();
    debugger
        .write()
        .unwrap()
        .add_breakpoint_with_options(line, None, None, None, None, None);
    let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
        Shared::new(SharedCell::new(Box::new(MutatingHandler {
            name: name.to_string(),
            prefer_upvalue,
            write_expected,
        })));
    let mut hook = debugger::VmDebuggerHook::new(
        debugger,
        handler,
        token_arena,
        Source {
            name: None,
            code: String::new(),
        },
        Default::default(),
        ModuleLoader::new(StdModuleResolver),
        HostFunctions::default(),
    );

    let result = interpreter::run_with_debug_hook_and_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
        &[],
        &mut hook,
    )
    .unwrap();

    assert_eq!(result, RuntimeValue::Number(expected.into()));
}

#[cfg(feature = "debugger")]
#[test]
fn breakpoint_builtin_pauses_unconditionally_with_no_registered_breakpoints() {
    use crate::{DebugContext, DebuggerAction, DebuggerHandler, Source};
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct RecordingHandler(Arc<Mutex<Vec<RuntimeValue>>>);

    impl DebuggerHandler for RecordingHandler {
        fn on_breakpoint_hit(&self, _breakpoint: &crate::Breakpoint, context: &DebugContext) -> DebuggerAction {
            self.0.lock().unwrap().push(context.current_value.clone());
            DebuggerAction::Continue
        }
    }

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("1 | breakpoint() | . + 10", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(
        &program,
        Shared::clone(&token_arena),
        ModuleLoader::new(StdModuleResolver),
    )
    .unwrap();

    let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
    debugger.write().unwrap().activate();
    assert!(debugger.read().unwrap().list_breakpoints().is_empty());

    let hit_values = Arc::new(Mutex::new(Vec::new()));
    let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
        Shared::new(SharedCell::new(Box::new(RecordingHandler(Arc::clone(&hit_values)))));
    let mut hook = debugger::VmDebuggerHook::new(
        debugger,
        handler,
        token_arena,
        Source {
            name: None,
            code: String::new(),
        },
        Default::default(),
        ModuleLoader::new(StdModuleResolver),
        HostFunctions::default(),
    );

    let result = interpreter::run_with_debug_hook_and_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
        &[],
        &mut hook,
    )
    .unwrap();

    assert_eq!(result, RuntimeValue::Number(11.0.into()));
    assert_eq!(
        hit_values.lock().unwrap().as_slice(),
        &[RuntimeValue::Number(1.0.into())]
    );
}

#[cfg(feature = "debugger")]
#[test]
fn vm_debugger_hook_evaluates_hit_conditions_and_logpoints() {
    use crate::{DebugContext, DebuggerHandler, Source, get_token};
    use std::sync::{Arc, Mutex};

    #[derive(Debug)]
    struct LogHandler(Arc<Mutex<Vec<String>>>);

    impl DebuggerHandler for LogHandler {
        fn on_log_point(&self, _breakpoint: &crate::Breakpoint, message: &str, _context: &DebugContext) {
            self.0.lock().unwrap().push(message.to_string());
        }
    }

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let outer = 10 |\nlet f = fn(inner):\nouter + inner; |\nf(2)",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(
        &program,
        Shared::clone(&token_arena),
        ModuleLoader::new(StdModuleResolver),
    )
    .unwrap();
    let function_token_id = compiled.chunks[1].debug_nodes[0].0;
    let function_line = get_token(Shared::clone(&token_arena), function_token_id)
        .range
        .start
        .line as usize;

    let debugger = Shared::new(SharedCell::new(crate::Debugger::new()));
    debugger.write().unwrap().activate();
    debugger.write().unwrap().add_breakpoint_with_options(
        function_line,
        None,
        None,
        None,
        Some("1".to_string()),
        Some("inner=${inner}, outer=${outer}".to_string()),
    );
    let messages = Arc::new(Mutex::new(Vec::new()));
    let handler: Shared<SharedCell<Box<dyn DebuggerHandler>>> =
        Shared::new(SharedCell::new(Box::new(LogHandler(Arc::clone(&messages)))));
    let mut hook = debugger::VmDebuggerHook::new(
        debugger,
        handler,
        token_arena,
        Source {
            name: None,
            code: String::new(),
        },
        Default::default(),
        ModuleLoader::new(StdModuleResolver),
        HostFunctions::default(),
    );

    interpreter::run_with_debug_hook_and_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
        &[],
        &mut hook,
    )
    .unwrap();

    assert!(
        messages
            .lock()
            .unwrap()
            .iter()
            .any(|message| message == "inner=2, outer=10")
    );
}

#[test]
fn host_function_is_called_when_not_a_builtin_or_local() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("double(21)", Shared::clone(&token_arena)).unwrap();
    let mut host_functions = HostFunctions::default();
    host_functions.insert("double", |args: &[RuntimeValue]| {
        let RuntimeValue::Number(n) = &args[0] else {
            return Err("expected a number".into());
        };
        Ok(RuntimeValue::Number((n.value() * 2.0).into()))
    });
    let result = compile_and_run_full(&program, RuntimeValue::None, &host_functions, None, token_arena).unwrap();
    assert_eq!(result, RuntimeValue::Number(42.0.into()));
}

#[test]
fn undefined_call_with_no_matching_host_function_errors() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("totally_undefined_name(1)", Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(matches!(err, Error::Vm(_)));
}

#[rstest]
#[case::missing_required("def add(a, b): a + b; | add()", 2, 0)]
#[case::too_many_required("def add(a, b): a + b; | add(1, 2, 3)", 2, 3)]
#[case::too_many_optional("def add(a, b = 1): a + b; | add(1, 2, 3)", 2, 3)]
#[case::too_many_zero_arity("def constant(): 1; | constant(1)", 0, 1)]
fn invalid_function_arity_reports_the_declared_bounds(#[case] code: &str, #[case] expected: u8, #[case] actual: u8) {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(
        matches!(err, Error::Vm(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::ArityMismatch { expected: got_expected, actual: got_actual } if got_expected == expected && got_actual == actual))
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn generated_programs_preserve_arithmetic_closure_and_foreach_semantics(
        left in -100i16..100,
        right in -100i16..100,
        scale in -10i16..10,
        values in proptest::collection::vec(-50i16..50, 0..12),
    ) {
        let arithmetic = format!("({left} + {right}) * {scale}");
        let arithmetic_expected = (i32::from(left) + i32::from(right)) * i32::from(scale);
        prop_assert_eq!(run(&arithmetic), RuntimeValue::Number(f64::from(arithmetic_expected).into()));

        let closure = format!("let base = {left} | let add_base = fn(value): base + value; | add_base({right})");
        let closure_expected = i32::from(left) + i32::from(right);
        prop_assert_eq!(run(&closure), RuntimeValue::Number(f64::from(closure_expected).into()));

        let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        let foreach = format!("var total = 0 | foreach(value, [{elements}]): total += value; | total");
        let foreach_expected: i32 = values.iter().map(|value| i32::from(*value)).sum();
        prop_assert_eq!(run(&foreach), RuntimeValue::Number(f64::from(foreach_expected).into()));
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn generated_compiled_programs_match_the_tree_walker(
        initial in -100i16..100,
        scale in -10i16..10,
        offset in -100i16..100,
        input in -100i16..100,
        values in proptest::collection::vec(-50i16..50, 0..12),
    ) {
        let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        let code = format!(
            "var total = {initial} | foreach(value, [{elements}]): total += value * {scale}; | let finish = fn(extra): total + extra + {offset}; | finish(.)"
        );
        assert_vm_matches_tree_walker(&code, vec![RuntimeValue::Number(f64::from(input).into())]);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn generated_nested_closures_match_the_tree_walker(
        x in -50i16..50,
        y in -50i16..50,
        z in -50i16..50,
        w in -50i16..50,
    ) {
        let code = format!(
            "let x = {x} | let make = fn(y): fn(z): x + y + z + {w};; | let step = make({y}) | step({z})"
        );
        assert_vm_matches_tree_walker(&code, vec![RuntimeValue::None]);
    }

    #[test]
    fn generated_foreach_closures_match_the_tree_walker(
        values in proptest::collection::vec(-30i16..30, 1..8),
    ) {
        let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        let code =
            format!("let fns = foreach(i, [{elements}]): fn(): i * 2;; | fns | map(fn(f): f();)");
        assert_vm_matches_tree_walker(&code, vec![RuntimeValue::None]);
    }
}

#[test]
fn break_crossing_a_try_boundary_exits_the_enclosing_loop() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("while(true): try: break catch: 1;;", Shared::clone(&token_arena)).unwrap();
    assert_eq!(compile_and_run(&program, token_arena).unwrap(), RuntimeValue::None);
}

#[rstest]
#[case::while_loop("var x = 0 | while(x < 4) do x += 1 | try: continue catch: 0 end | x", 4.0)]
#[case::foreach_loop(
    "var total = 0 | foreach(x, array(1, 2, 3, 4)) do try: continue catch: 0 end | total",
    0.0
)]
fn continue_crossing_try_boundary_reaches_the_enclosing_loop(#[case] code: &str, #[case] expected: f64) {
    assert_eq!(run(code), RuntimeValue::Number(expected.into()));
}

#[test]
fn let_destructuring_mismatch_errors() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let [a, b] = [1] | a + b", Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(
        matches!(err, Error::Vm(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::DestructuringFailed))
    );
}

#[test]
fn assigning_to_a_let_bound_name_is_a_compile_error() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let x = 1 | x = 2", Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(matches!(
        err,
        Error::Compile(compiler::CompileError::AssignToImmutable(..))
    ));
}

#[test]
fn assigning_to_a_var_bound_name_still_works() {
    assert_eq!(run("var x = 1 | x = 2 | x"), RuntimeValue::Number(2.0.into()));
}

#[test]
fn module_hoisting_resolves_regardless_of_source_order() {
    assert_eq!(
        run("def call_math(): math::mysum(10, 5); | call_math() | module math: def mysum(a, b): a + b; end"),
        RuntimeValue::Number(15.0.into())
    );
}

#[test]
fn top_level_let_is_visible_to_a_def_body_compiled_before_it_runs() {
    assert_eq!(run("def f(): x; | let x = 42 | f()"), RuntimeValue::Number(42.0.into()));
}

#[test]
fn include_flattens_a_standard_module_into_the_current_scope() {
    assert_eq!(
        run_with_prelude(r#"include "csv" | csv_needs_quote("trailing space ", ",")"#),
        RuntimeValue::Boolean(true)
    );
    assert_eq!(
        run_with_prelude(r#"include "csv" | csv_needs_quote("plain", ",")"#),
        RuntimeValue::Boolean(false)
    );
}

#[test]
fn include_hoisting_resolves_regardless_of_source_order() {
    assert_eq!(
        run_with_prelude(r#"def f(): csv_needs_quote("a,b", ","); | f() | include "csv""#),
        RuntimeValue::Boolean(true)
    );
}

#[test]
fn import_exposes_functions_only_via_qualified_access() {
    assert_eq!(
        run_with_prelude(r#"import "csv" as csv | csv::csv_needs_quote("a,b", ",")"#),
        RuntimeValue::Boolean(true)
    );
}

#[test]
fn import_default_alias_is_the_module_name() {
    assert_eq!(
        run_with_prelude(r#"import "csv" | csv::csv_needs_quote("a,b", ",")"#),
        RuntimeValue::Boolean(true)
    );
}

#[test]
fn import_hoisting_resolves_regardless_of_source_order() {
    assert_eq!(
        run_with_prelude(r#"def f(): csv::csv_needs_quote("a,b", ","); | f() | import "csv" as csv"#),
        RuntimeValue::Boolean(true)
    );
}

#[test]
fn calling_a_bare_native_function_reference_dispatches_to_the_builtin() {
    assert_eq!(
        run_with_prelude(r#"import "csv" as csv | csv::csv_to_markdown_table([["a", "b"], [1, 2]])"#),
        RuntimeValue::String(Shared::new("| a | b |\n| --- | --- |\n| 1 | 2 |".to_string()))
    );
}

#[test]
fn imported_function_name_is_not_reachable_unqualified() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        r#"import "csv" as csv | csv_needs_quote("a,b", ",")"#,
        Shared::clone(&token_arena),
    )
    .unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(matches!(err, Error::Vm(_)));
}

/// Real `.mq` file with a top-level `let`, since no bundled stdlib module has one.
#[rstest::fixture]
fn module_with_vars() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("mod1.mq"), "def helper(x): x + 1; let base = 10").unwrap();
    dir
}

fn run_with_local_module(dir: &tempfile::TempDir, code: &str) -> RuntimeValue {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let resolver =
        crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap();
    interpreter::run(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        crate::eval::Options::default().max_call_stack_depth,
    )
    .unwrap()
}

/// `include` binds vars unqualified; `import` (top-level or nested) qualifies them too.
#[rstest]
#[case::top_level_include(r#"include "mod1" | helper(base)"#)]
#[case::top_level_import(r#"import "mod1" as m | m::helper(m::base)"#)]
#[case::top_level_import_default_alias(r#"import "mod1" | mod1::helper(mod1::base)"#)]
#[case::import_nested_in_inline_module(r#"module outer: import "mod1" as m end | m::helper(m::base)"#)]
fn module_vars_binding_is_correct(module_with_vars: tempfile::TempDir, #[case] code: &str) {
    assert_eq!(
        run_with_local_module(&module_with_vars, code),
        RuntimeValue::Number(11.into())
    );
}

/// `import`'s vars must not leak unqualified into the importing scope.
#[rstest]
#[case::top_level_import(r#"import "mod1" as m | base"#)]
#[case::import_nested_in_inline_module(r#"module outer: import "mod1" as m end | base"#)]
fn import_does_not_leak_the_bare_var_name(module_with_vars: tempfile::TempDir, #[case] code: &str) {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let resolver = crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![
        module_with_vars.path().to_path_buf(),
    ]));
    let err = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap_err();
    assert!(matches!(err, compiler::CompileError::UndefinedIdent(..)));
}

/// Match arms and loop bodies are their own lexical block: a name bound inside one must
/// not resolve in a later arm, after the match, or after the loop ends.
#[rstest]
#[case::match_arm_binding_does_not_leak_to_a_later_arm("match(1) do | 1: let y = 10 | 2: y end")]
#[case::match_arm_binding_does_not_leak_after_the_match("match(1) do | 1: let y = 10 end | y")]
#[case::foreach_loop_var_does_not_leak_after_the_loop("foreach(x, [1, 2, 3]): x; | x")]
#[case::foreach_body_binding_does_not_leak_after_the_loop("foreach(x, [1, 2, 3]): let z = x * 2; | z")]
#[case::while_body_binding_does_not_leak_after_the_loop("var i = 0 | while (i < 3): let z = i * 2 | i += 1; | z")]
#[case::until_body_binding_does_not_leak_after_the_loop("var i = 0 | until (i >= 3): let z = i * 2 | i += 1; | z")]
#[case::loop_body_binding_does_not_leak_after_the_loop("loop: let z = 42 | break: z; | z")]
#[case::nested_match_arm_binding_does_not_leak_to_an_outer_arm(
    "match(1) do | 1: match(2) do | 2: let n = 99 | 3: n end | 2: n end"
)]
#[case::array_rest_binding_does_not_leak_after_the_match("match([1, 2, 3]) do | [first, ..rest]: rest end | rest")]
#[case::dict_pattern_binding_does_not_leak_after_the_match(r#"match({"x": 1}) do | {x: v}: v end | v"#)]
fn block_scoped_bindings_do_not_leak(#[case] code: &str) {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let err = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap_err();
    assert!(
        matches!(err, compiler::CompileError::UndefinedIdent(..)),
        "{code:?}: {err:?}"
    );
}

/// A module's vars never change `self`, whichever path binds them.
#[rstest]
#[case::top_level_include(r#"77 | include "mod1""#)]
#[case::top_level_import(r#"77 | import "mod1" as m"#)]
#[case::import_nested_in_inline_module(r#"77 | module outer: import "mod1" as m end"#)]
fn module_vars_binding_preserves_self(module_with_vars: tempfile::TempDir, #[case] code: &str) {
    assert_eq!(
        run_with_local_module(&module_with_vars, code),
        RuntimeValue::Number(77.into())
    );
}

/// Non-tail module-var binding must not push and then discard `self`.
#[rstest]
#[case::top_level_include_non_tail(r#"include "mod1" | helper(base)"#)]
#[case::top_level_import_non_tail(r#"import "mod1" as m | m::helper(m::base)"#)]
#[case::import_nested_in_inline_module(r#"module outer: import "mod1" as m end | m::helper(m::base)"#)]
#[cfg(not(feature = "debugger"))]
fn module_vars_binding_does_not_push_and_discard_self(module_with_vars: tempfile::TempDir, #[case] code: &str) {
    use super::bytecode::{OpCode, SELF_SLOT};

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let resolver = crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![
        module_with_vars.path().to_path_buf(),
    ]));
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap();

    let has_wasted_self_push = compiled.chunks.iter().any(|chunk| {
        chunk.code.windows(2).any(|pair| match pair {
            [OpCode::GetLocal(a), OpCode::Pop] => *a == SELF_SLOT,
            [OpCode::GetLocal(a), OpCode::SetLocal(b)] => *a == SELF_SLOT && *b == SELF_SLOT,
            _ => false,
        })
    });
    assert!(
        !has_wasted_self_push,
        "{code:?} should not push self just to discard it"
    );
}
