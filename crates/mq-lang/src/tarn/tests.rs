use super::interpreter::ExecutionPools;
use super::*;
use crate::module::resolver::std_resolver::StdModuleResolver;
use crate::range::Range;
use crate::{DictMap, Shared, SharedCell};
use crate::{Token, TokenKind, arena::Arena, token_alloc};
use proptest::prelude::*;
use rstest::rstest;

fn compile_and_run(program: &Program, token_arena: TokenArena) -> Result<RuntimeValue, Error> {
    compile_and_run_full(
        program,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        token_arena,
    )
}

fn compile_and_run_with_input(
    program: &Program,
    input: RuntimeValue,
    token_arena: TokenArena,
) -> Result<RuntimeValue, Error> {
    compile_and_run_full(program, input, &HostFunctions::default(), None, token_arena)
}

fn compile_and_run_full(
    program: &Program,
    input: RuntimeValue,
    host_functions: &HostFunctions,
    timeout: Option<Duration>,
    token_arena: TokenArena,
) -> Result<RuntimeValue, Error> {
    let compiled = compiler::compile_program(program, token_arena, ModuleLoader::new(StdModuleResolver))?;
    Ok(interpreter::run_with_globals(
        &compiled,
        input,
        host_functions,
        timeout,
        Options::default().max_call_stack_depth,
        &[],
    )?)
}

#[test]
fn default_call_stack_depth_matches_the_build_profile() {
    assert_eq!(
        Options::default().max_call_stack_depth,
        if cfg!(debug_assertions) { 256 } else { 10_000 }
    );
}

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
    interpreter::run_with_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        max_call_stack_depth,
        &[],
    )
}

#[rstest]
#[case::random_string(r#"random_string(1000001, "x")"#)]
#[case::set_array("set([], 1000000, 1)")]
#[case::insert_array("insert([], 1000000, 1)")]
#[case::insert_string(r#"insert("", 1000000, "x")"#)]
#[case::del_array("del([], 1000000)")]
#[case::del_string(r#"del("", 1000000)"#)]
fn vm_rejects_unbounded_builtin_allocations(#[case] code: &str) {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();

    assert!(
        compile_and_run(&program, token_arena).is_err(),
        "{code} should return a VM error"
    );
}

#[test]
fn vm_mutating_negative_indices_saturate_to_zero() {
    assert_eq!(
        run(r#"set(["a", "b"], -1, "z")"#),
        RuntimeValue::Array(Shared::new(vec!["z".into(), "b".into()]))
    );
    assert_eq!(
        run(r#"insert(["a", "b"], -1, "z")"#),
        RuntimeValue::Array(Shared::new(vec!["z".into(), "a".into(), "b".into()]))
    );
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

#[test]
fn caught_error_from_an_array_literal_does_not_corrupt_the_enclosing_array() {
    // The try body starts (and abandons) its own array before erroring, so unwinding must
    // discard those partial operands or the outer array's accumulator gets clobbered.
    assert_eq!(
        run("[1, (try: [2, 3, 1 / 0] catch: 99), 4]"),
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(99.into()),
            RuntimeValue::Number(4.into()),
        ]))
    );
}

#[test]
fn caught_error_from_a_dict_literal_does_not_corrupt_the_enclosing_dict() {
    assert_eq!(
        run(r#"{"a": 1, "b": (try: {"x": 1, "y": 1 / 0} catch: 99), "c": 4}"#),
        RuntimeValue::Dict(Shared::new(DictMap::from_iter([
            (crate::Ident::new("a"), RuntimeValue::Number(1.into())),
            (crate::Ident::new("b"), RuntimeValue::Number(99.into())),
            (crate::Ident::new("c"), RuntimeValue::Number(4.into())),
        ])))
    );
}

fn run_with_prelude(code: &str) -> RuntimeValue {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let compiled =
        compiler::compile_program_with_builtin_prelude(&program, token_arena, ModuleLoader::new(StdModuleResolver))
            .unwrap();
    match interpreter::run_with_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        Options::default().max_call_stack_depth,
        &[],
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
    assert!(compiled.chunks[0].static_closures[0].upvalues.is_none());
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
            .any(|op| matches!(op, OpCode::CallStaticExact1(_)))
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
            .any(|op| matches!(op, OpCode::ReturnBinaryLocalConst { .. }))
    );
}

#[test]
fn loop_header_comparisons_use_a_compact_branch_opcode() {
    use super::bytecode::{BinaryOp, OpCode};

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("var i = 3 | while(i > 0): i -= 1; | i", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::JumpIfFalseLocalConst { op: BinaryOp::Gt, .. }))
    );
    assert_eq!(
        run("var i = 3 | while(i > 0): i -= 1; | i"),
        RuntimeValue::Number(0.into())
    );
}

#[test]
fn local_return_preserves_auto_call_semantics() {
    // A function parameter can itself be callable, so its final expression carries
    // `MaybeAutoCall` before returning. The bytecode optimizer's direct `ReturnLocal`
    // coverage belongs in `bytecode::tests`, where the instruction shape is explicit.
    assert_eq!(
        run("let identity = fn(x): x; | identity(42)"),
        RuntimeValue::Number(42.into())
    );
}

#[test]
fn local_constant_assignment_uses_update_opcode() {
    use super::bytecode::{BinaryOp, OpCode};

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("var x = 1 | x += 2 | x", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::UpdateLocalConst { op: BinaryOp::Add, .. }))
    );
    assert_eq!(run("var x = 1 | x += 2 | x"), RuntimeValue::Number(3.into()));
}

#[test]
fn constant_assignment_uses_set_local_const_opcode() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("var x = 1 | x", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::SetLocalConst { .. }))
    );
    assert_eq!(run("var x = 1 | x"), RuntimeValue::Number(1.into()));
}

#[test]
fn local_constant_binary_return_uses_return_opcode() {
    use super::bytecode::{BinaryOp, OpCode};

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let double = fn(x): x * 2; | double(3)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(compiled.chunks.iter().any(|chunk| {
        chunk
            .code
            .iter()
            .any(|op| matches!(op, OpCode::ReturnBinaryLocalConst { op: BinaryOp::Mul, .. }))
    }));
    assert_eq!(
        run("let double = fn(x): x * 2; | double(3)"),
        RuntimeValue::Number(6.into())
    );
}

#[test]
fn local_assignment_uses_local_update_opcode() {
    use super::bytecode::{BinaryOp, OpCode};

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("var x = 1 | var y = 2 | x += y | x", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::UpdateLocalLocal { op: BinaryOp::Add, .. }))
    );
    assert_eq!(
        run("var x = 1 | var y = 2 | x += y | x"),
        RuntimeValue::Number(3.into())
    );
}

#[test]
fn top_level_function_literal_produces_a_callable_value() {
    assert!(matches!(run("fn(x): x;"), RuntimeValue::VmClosure(_)));
}

#[test]
fn vm_closure_equals_itself() {
    if let RuntimeValue::Array(arr) = run("let f = fn(x): x + 1; | [f, f]") {
        assert_eq!(arr[0], arr[1]);
    } else {
        panic!("expected an array");
    }
}

#[test]
fn vm_closure_alias_equals_original() {
    if let RuntimeValue::Array(arr) = run("let f = fn(x): x + 1; | let g = f | [f, g]") {
        assert_eq!(arr[0], arr[1]);
    } else {
        panic!("expected an array");
    }
}

#[test]
fn vm_closure_distinct_equivalent_closures_are_equal() {
    // Same compiled body, distinct `Shared` allocations, ignoring captured upvalue content.
    if let RuntimeValue::Array(arr) = run("let fns = foreach(i, [1, 2, 3]): fn(): i;; | fns") {
        assert_eq!(arr[0], arr[1]);
        assert_eq!(arr[1], arr[2]);
    } else {
        panic!("expected an array");
    }

    // Same compiled body, distinct allocations, equal bound arguments.
    let partials =
        run_with_prelude("def add(x, y): x + y; | let g = partial(add, 1) | let h = partial(add, 1) | [g, h]");
    if let RuntimeValue::Array(arr) = partials {
        assert_eq!(arr[0], arr[1]);
    } else {
        panic!("expected an array");
    }
}

#[test]
fn vm_closures_compare_correctly_inside_collections() {
    let value = run("let f = fn(x): x + 1; | let g = fn(x): x + 2; | [[f], [f], [g]]");
    if let RuntimeValue::Array(arr) = value {
        assert_eq!(arr[0], arr[1]);
        assert_ne!(arr[0], arr[2]);
    } else {
        panic!("expected an array");
    }
}

#[test]
fn builtin_receives_a_top_level_closure_as_its_implicit_self() {
    assert_eq!(
        run("fn(x): x; | type()"),
        RuntimeValue::String(Shared::new("function".to_string()))
    );
}

#[cfg(not(feature = "debugger"))]
#[test]
fn breakpoint_is_a_no_op_when_the_debugger_feature_is_disabled() {
    assert_eq!(run("1 | breakpoint() | . + 1"), RuntimeValue::Number(2.0.into()));
}

#[test]
fn top_level_def_calls_use_call_static() {
    use super::bytecode::OpCode;
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        r#"def identity(x): x; | foreach(i, range(0, 1000, 1)): identity(i);"#,
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
    assert!(
        compiled.chunks.iter().any(|c| c.code.iter().any(|op| matches!(
            op,
            OpCode::CallStaticExact0(_) | OpCode::CallStaticExact1(_) | OpCode::CallStaticExact2(_)
        ))),
        "capture-free common-arity top-level def call should compile to a specialized CallStaticExact opcode"
    );
}

#[test]
fn fixed_static_calls_specialize_the_exact_and_implicit_self_forms() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "def identity(x): x; | identity(1) | identity()",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallStaticExact1(_)))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallStaticImplicitSelf(_, 0)))
    );
    assert_eq!(
        run("def identity(x): x; | identity(1) | identity()"),
        RuntimeValue::Number(1.into())
    );
}

#[test]
fn fixed_static_calls_bind_zero_and_two_arguments_without_generic_binding() {
    use super::bytecode::OpCode;

    let source = "def constant(): 7; | def add(left, right): left + right; | constant() + add(20, 22)";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallStaticExact0(_)))
    );
    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallStaticExact2(_)))
    );
    assert_eq!(run(source), RuntimeValue::Number(49.into()));
}

#[test]
fn static_calls_with_captured_locals_keep_the_generic_exact_opcode() {
    use super::bytecode::OpCode;

    let source = "let f = fn(value): fn(): value;; | let read = f(42) | read()";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallStaticExact(_, 1)))
    );
    assert_eq!(run(source), RuntimeValue::Number(42.into()));
}

#[test]
fn fixed_static_arity_mismatches_keep_the_checked_call_form() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("def identity(x): x; | identity(1, 2)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallStatic(_, 2)))
    );
}

#[test]
fn defaulted_named_calls_keep_the_generic_local_form() {
    use super::bytecode::OpCode;

    let source = "def add(value, step = 1): value + step; | add()";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallLocal(_, 0))),
        "defaulted parameters must use the generic binder rather than a fixed-arity opcode"
    );
    assert_eq!(
        run_with_input(source, RuntimeValue::Number(10.into())),
        RuntimeValue::Number(11.into())
    );
}

#[test]
fn generator_calls_stay_on_the_coroutine_aware_generic_paths() {
    use super::bytecode::OpCode;

    let source = "def g(value): yield: value | if (false): g() else: None; | let stream = g(42) | next(stream)";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled.chunks[0]
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallLocal(_, 1))),
        "a generator call must not use a direct static frame-enter opcode"
    );
    assert!(
        compiled
            .chunks
            .iter()
            .any(|chunk| { chunk.code.iter().any(|op| matches!(op, OpCode::CallSelf(0))) })
    );
    assert_eq!(dict_field(&run(source), "value"), RuntimeValue::Number(42.into()));
}

#[test]
fn fixed_arity_recursive_def_uses_call_self_without_capturing_itself() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "def count(n): if (n == 0): 0 else: count(n - 1); | count(10)",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
    let recursive_chunk = compiled
        .chunks
        .iter()
        .find(|chunk| chunk.code.iter().any(|op| matches!(op, OpCode::CallSelfExact1)))
        .expect("recursive body should use CallSelfExact1");

    assert!(recursive_chunk.upvalue_names.is_empty());
    assert_eq!(
        run("def count(n): if (n == 0): 0 else: count(n - 1); | count(10)"),
        RuntimeValue::Number(0.0.into())
    );
}

#[test]
fn fixed_arity_recursive_def_specializes_implicit_self_calls() {
    use super::bytecode::OpCode;

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "def identity(x): if (true): x else: identity(); | 42 | identity()",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(compiled.chunks.iter().any(|chunk| {
        chunk
            .code
            .iter()
            .any(|op| matches!(op, OpCode::CallSelfImplicitSelf(0)))
    }));
    assert_eq!(
        run("def identity(x): if (true): x else: identity(); | 42 | identity()"),
        RuntimeValue::Number(42.into())
    );
}

#[test]
fn tail_recursive_call_respects_call_stack_depth() {
    // This terminates even without a recursion guard. Before tail calls counted toward the
    // configured limit, it returned `3` instead of reporting the exhausted depth.
    let code = "def count(n): if (n >= 3): n else: count(n + 1); | count(0)";
    assert!(matches!(
        run_with_max_depth(code, 2),
        Err(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::RecursionError(2))
    ));
}

/// Direct builtin calls preserve argument order through the specialized common-arity paths.
#[rstest]
#[case("type(42)", RuntimeValue::String(Shared::new("number".to_string())))]
#[case("sub(5, 3)", RuntimeValue::Number(2.0.into()))]
fn direct_builtin_calls_with_common_arities_preserve_results(#[case] code: &str, #[case] expected: RuntimeValue) {
    assert_eq!(run(code), expected);
}

#[test]
fn immutable_function_upvalue_calls_use_call_upvalue() {
    use super::bytecode::OpCode;

    let source = "let increment = fn(x): x + 1; | let apply = fn(x): increment(x); | apply(41)";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(
        compiled
            .chunks
            .iter()
            .any(|chunk| chunk.code.iter().any(|op| matches!(op, OpCode::CallUpvalue(_, 1))))
    );
    assert_eq!(run(source), RuntimeValue::Number(42.0.into()));
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
        &compiler::ResolvedModuleVars::default(),
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
        &compiler::ResolvedModuleVars::default(),
    )
    .unwrap();

    let plain_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let plain_program = crate::parse(
        "def local_identity(x): x; | foreach(i, range(0, 3, 1)): local_identity(i);",
        Shared::clone(&plain_arena),
    )
    .unwrap();
    let plain = compiler::compile_program_for_engine(
        &plain_program,
        plain_arena,
        ModuleLoader::new(StdModuleResolver),
        &[],
        &compiler::ResolvedModuleVars::default(),
    )
    .unwrap();

    // The soft builtin with the same name must not be compiled just because it exists
    // in `builtin.mq`; both equivalent user functions should yield the same chunks.
    assert_eq!(shadowed.chunks.len(), plain.chunks.len());
}

#[test]
fn engine_compiler_loads_only_reachable_module_exports() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("include \"csv\" | csv_parse(true)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program_for_engine(
        &program,
        token_arena,
        ModuleLoader::new(StdModuleResolver),
        &[],
        &compiler::ResolvedModuleVars::default(),
    )
    .unwrap();

    // The top-level chunk and `csv_parse`; the remaining CSV exports are not reachable.
    assert_eq!(compiled.chunks.len(), 2);
}

#[test]
fn engine_compiler_reachable_prelude_cache_is_correct_across_different_queries() {
    fn compile_and_run_for_engine(code: &str) -> RuntimeValue {
        let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
        let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
        let compiled = compiler::compile_program_for_engine(
            &program,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
            &compiler::ResolvedModuleVars::default(),
        )
        .unwrap();
        interpreter::run_with_globals(
            &compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            Options::default().max_call_stack_depth,
            &[],
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

#[test]
fn auto_call_accounts_for_arguments_bound_by_partial() {
    assert_eq!(
        run_with_input(
            "def add(x, y): x + y; | let add5 = partial(add, 5) | add5",
            RuntimeValue::Number(3.0.into()),
        ),
        RuntimeValue::Number(8.0.into())
    );
}

#[rstest]
#[case::while_loop("var checks = 0 | def condition(): checks += 1 | checks < 3; | while(condition()): .; | checks")]
#[case::until_loop("var checks = 0 | def condition(): checks += 1 | checks >= 3; | until(condition()): .; | checks")]
fn conditional_loop_evaluates_its_condition_once_before_the_first_body(#[case] code: &str) {
    assert_eq!(run(code), RuntimeValue::Number(3.0.into()));
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
#[case::implicit_self_is_bound_before_a_default_is_evaluated(
    "def add(value, increment = value + 1): value + increment; | 20 | add()",
    41.0
)]
#[case::explicit_argument_takes_precedence_over_implicit_self_with_a_default(
    "def add(value, increment = value + 1): value + increment; | 99 | add(20)",
    41.0
)]
#[case::paren_free_qualified_call_uses_implicit_self_before_a_default(
    "module math: def increment(value, step = 1): value + step; end | 10 | math::increment",
    11.0
)]
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

/// This carries the former evaluator's `test_default_params_with_self` semantics
/// through the compiled VM. (Its test constructed a parameter named `self` directly
/// in the AST; `self` is reserved in source syntax.) The first parameter receives the
/// pipeline value, then the omitted optional parameter receives its default.
#[test]
fn implicit_self_and_default_parameter_match_previous_evaluator_semantics() {
    assert_eq!(
        run_with_input(
            r#"def format(value, prefix = "[LOG]"): [prefix, value]; | format()"#,
            RuntimeValue::String(Shared::new("message".to_string())),
        ),
        RuntimeValue::Array(
            vec![
                RuntimeValue::String(Shared::new("[LOG]".to_string())),
                RuntimeValue::String(Shared::new("message".to_string())),
            ]
            .into(),
        ),
    );
}

/// Covers the parameter binder's distinct decisions rather than only individual examples:
/// whether the pipeline value occupies the first required slot, which supplied arguments
/// follow it, and when defaults and a variadic tail take over.
#[rstest]
#[case::implicit_self_then_chained_defaults(10.0, "def f(a, b = a + 1, c = b + 1): a + b + c; | f()", 33.0)]
#[case::implicit_self_precedes_a_partial_explicit_argument_list(
    10.0,
    "def f(a, b, c = 1): a * 100 + b * 10 + c; | f(2)",
    1021.0
)]
#[case::all_optional_parameters_prefer_a_zero_argument_call_over_implicit_self(
    9.0,
    "def f(a = 1, b = 2): a * 10 + b; | f()",
    12.0
)]
#[case::implicit_self_with_optional_and_variadic_parameters(
    4.0,
    "def f(a, b = 2, *rest): a * 100 + b * 10 + len(rest); | f()",
    420.0
)]
#[case::explicit_arguments_fill_optional_and_variadic_parameters_without_implicit_self(
    99.0,
    "def f(a, b = 2, *rest): a * 100 + b * 10 + len(rest); | f(1, 3, 4, 5)",
    132.0
)]
#[case::optional_before_variadic_does_not_consume_implicit_self(
    9.0,
    "def f(a = 3, *rest): a * 10 + len(rest); | f()",
    30.0
)]
#[case::default_expression_preserves_the_callers_self(10.0, "def f(a, b = . + a): a + b; | f()", 30.0)]
#[case::dynamic_closure_call_uses_the_same_implicit_self_and_default_binding(
    10.0,
    "let f = fn(a, b = a + 1): a + b; | f()",
    21.0
)]
#[case::paren_free_local_closure_call_uses_the_same_binding(10.0, "let f = fn(a, b = 1): a + b; | f", 11.0)]
#[case::default_expression_captures_an_enclosing_binding(
    0.0,
    "let step = 2 | let f = fn(a, b = step): a + b; | f(40)",
    42.0
)]
#[case::recursive_defaulted_call_rebinds_the_default_on_each_invocation(
    0.0,
    "def f(n, step = 1): if(n == 0): 0 else: step + f(n - 1); | f(3)",
    3.0
)]
fn parameter_binding_matrix(#[case] input: f64, #[case] code: &str, #[case] expected: f64) {
    assert_eq!(
        run_with_input(code, RuntimeValue::Number(input.into())),
        RuntimeValue::Number(expected.into()),
    );
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
#[case::unresolved_name_in_an_unreachable_branch("if(false): undefined_name else: 1")]
#[case::unresolved_name_in_an_uncalled_function("def f(): undefined_name; | 1")]
fn compiled_engine_executes_supported_constructs(#[case] code: &str) {
    assert_vm_executes(code, vec![RuntimeValue::None]);
}

#[test]
fn inline_module_destructuring_executes() {
    assert_vm_executes(
        r#"module constants: let [pi, ..digits] = [314, 1, 5, 9] | let {major: version} = {"major": 8} end | constants::pi + len(constants::digits) + constants::version"#,
        vec![RuntimeValue::None],
    );
}

#[test]
fn nested_module_paths_execute() {
    assert_vm_executes(
        "module parent: module child: let answer = 40 | def add_two(): answer + 2; end end | parent::child::answer + parent::child::add_two()",
        vec![RuntimeValue::None],
    );
}

#[test]
fn calls_with_256_arguments_execute() {
    let arguments = (0..256).map(|_| "1").collect::<Vec<_>>().join(", ");
    let code = format!("let count = fn(*args): len(args); | count({arguments})");
    assert_vm_executes(&code, vec![RuntimeValue::None]);
}

#[test]
fn dynamic_fixed_arity_call_handles_256_arguments_without_shifting_them() {
    let parameters = (1..=256)
        .map(|index| format!("a{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let arguments = (0..256).map(|_| "1").collect::<Vec<_>>().join(", ");
    let code = format!("var f = fn({parameters}): a256; | f({arguments})");
    assert_eq!(run(&code), RuntimeValue::Number(1.into()));
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
fn compiled_engine_executes_with_input(#[case] code: &str, #[case] input: RuntimeValue) {
    assert_vm_executes(code, vec![input]);
}

#[test]
fn compiled_engine_aggregates_nodes() {
    assert_eq!(
        vm_engine_eval_many(
            ". * 10 | nodes | len()",
            vec![
                RuntimeValue::Number(1.0.into()),
                RuntimeValue::Number(2.0.into()),
                RuntimeValue::Number(3.0.into()),
            ],
        ),
        vec![RuntimeValue::Number(3.0.into())],
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

fn assert_vm_executes(code: &str, inputs: Vec<RuntimeValue>) {
    let input_count = inputs.len();
    let values = vm_engine_eval_many(code, inputs);
    assert_eq!(
        values.len(),
        input_count,
        "VM returned an unexpected result count for: {code}"
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
fn while_until_executes(#[case] code: &str) {
    assert_vm_executes(code, vec![RuntimeValue::Number(0.0.into())]);
}

/// A closure created inside a loop/match block captures the block's *slot*, not a
/// per-iteration snapshot: a loop variable is one mutable binding reused every
/// iteration, while a closure built before the loop keeps its own outer binding.
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
fn closures_over_scoped_bindings_execute(#[case] code: &str) {
    assert_vm_executes(code, vec![RuntimeValue::None]);
}

#[rstest]
#[case::top_level_destructuring_let_visible_to_a_sibling_def("let [x] = [1] | def f(): x; | f()")]
#[case::top_level_array_rest_let_visible_to_a_sibling_def("let [x, ..rest] = [1, 2, 3] | def f(): rest; | f()")]
#[case::top_level_as_binding_visible_to_a_sibling_def("1 as x | def f(): x; | f()")]
#[case::top_level_destructuring_var_visible_to_a_sibling_def("var [x] = [1] | def f(): x; | f()")]
#[case::inline_module_let_visible_to_a_sibling_function("module m: let x = 1 | def f(): x; end | m::f()")]
fn forward_declared_top_level_bindings_are_visible_to_a_sibling_def(#[case] code: &str) {
    assert_vm_executes(code, vec![RuntimeValue::None]);
}

#[test]
fn bare_soft_builtin_reference_inside_an_imported_module_becomes_reachable() {
    let code = r#"import "table" | table::tables(to_markdown("| id | v |\n| - | - |\n| 1 | 2 |\n| 1 | 3 |\n")) | first(self) | table::pivot_wider(self, 1, 2)"#;
    assert_vm_executes(code, vec![RuntimeValue::None]);
}

#[test]
fn nodes_capture_uses_the_latest_slot_for_a_name_rebound_by_repeated_destructuring() {
    let code = "let [x] = [1] | let [x] = [2] | nodes | x";
    let inputs = vec![RuntimeValue::Number(1.0.into())];
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let results = compile_and_run_many(
        &program,
        inputs.clone().into_iter(),
        EngineRunContext {
            host_functions: &HostFunctions::default(),
            timeout: None,
            max_call_stack_depth: Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
            preresolved_module_vars: compiler::ResolvedModuleVars::default(),
        },
    )
    .unwrap();
    assert_eq!(results, vec![RuntimeValue::Number(2.0.into())]);
}

#[cfg(not(feature = "debugger"))]
#[test]
fn cached_nodes_capture_reuses_precomputed_slots() {
    let mut engine = crate::DefaultEngine::default();
    let compiled = engine.compile("let [x] = [1] | let [x] = [2] | nodes | x").unwrap();

    let results = engine
        .eval_compiled(
            &compiled,
            vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(2.0.into())].into_iter(),
        )
        .unwrap();

    assert_eq!(results.values(), &[RuntimeValue::Number(2.0.into())]);
    assert!(compiled.cached_vm_program().flatten().is_some());
}

#[rstest]
#[case::while_first_iteration_bare_break("while(true): break;")]
#[case::until_first_iteration_bare_break("until(false): break;")]
#[case::while_first_iteration_bare_break_through_try("while(true): try: break catch: 1;;")]
#[case::until_first_iteration_bare_break_through_try("until(false): try: break catch: 1;;")]
fn bare_break_before_any_completed_iteration_returns_none(#[case] code: &str) {
    assert_eq!(
        run_with_input(code, RuntimeValue::Number(7.0.into())),
        RuntimeValue::None
    );
}

#[rstest]
#[case::while_first_iteration_break_with_value("while(true): break: 999;")]
#[case::until_first_iteration_break_with_value("until(false): break: 999;")]
#[case::while_first_iteration_break_with_value_through_try("while(true): try: break: 999 catch: 1;;")]
#[case::until_first_iteration_break_with_value_through_try("until(false): try: break: 999 catch: 1;;")]
fn break_with_value_before_any_completed_iteration_returns_its_value(#[case] code: &str) {
    assert_eq!(
        run_with_input(code, RuntimeValue::Number(7.0.into())),
        RuntimeValue::Number(999.0.into())
    );
}

#[test]
fn nodes_aggregates_per_input_results_into_one_run() {
    // `nodes` (see `split_at_nodes`/`run_nodes_aggregate`) collects every input's
    // per-input result into one array and runs the rest of the program against that
    // array once, rather than once per input. `len()` here only makes sense read that
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
            max_call_stack_depth: Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
            preresolved_module_vars: compiler::ResolvedModuleVars::default(),
        },
    )
    .unwrap();
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
                max_call_stack_depth: Options::default().max_call_stack_depth,
                token_arena,
                module_loader: ModuleLoader::new(StdModuleResolver),
                global_bindings: &[],
                session: None,
                preresolved_module_vars: compiler::ResolvedModuleVars::default(),
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
            max_call_stack_depth: Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
            preresolved_module_vars: compiler::ResolvedModuleVars::default(),
        },
    )
    .unwrap();
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
            max_call_stack_depth: Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
            preresolved_module_vars: compiler::ResolvedModuleVars::default(),
        },
    )
    .unwrap();
    assert_eq!(results.len(), 1);
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
        values: vec![matching_child.clone(), text_node("no match anywhere")],
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
            max_call_stack_depth: Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
            preresolved_module_vars: compiler::ResolvedModuleVars::default(),
        },
    )
    .unwrap();
    assert_eq!(results, vec![RuntimeValue::new_markdown(matching_child)]);
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
            max_call_stack_depth: Options::default().max_call_stack_depth,
            token_arena,
            module_loader: ModuleLoader::new(StdModuleResolver),
            global_bindings: &[],
            session: None,
            preresolved_module_vars: compiler::ResolvedModuleVars::default(),
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

#[test]
fn expired_timeout_rejects_a_short_query_before_it_runs() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("1", Shared::clone(&token_arena)).unwrap();
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

#[test]
fn try_depth_limit_returns_its_unstarted_frame_to_the_pool() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("try: 1 catch(e): 2;", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    let (result, pools) = interpreter::run_with_globals_and_pools(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        0,
        &[],
        ExecutionPools::default(),
    );

    assert!(
        matches!(result, Err(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::RecursionError(0)))
    );
    // The top-level frame and the try frame both have no captures and are reusable.
    assert_eq!(pools.pooled_local_frame_count(), 2);
}

/// Regression: mq call depth used to equal native Rust stack depth, so a high
/// `max_call_stack_depth` could overflow the OS thread stack instead of hitting
/// `RecursionError`. The trampoline's `Vec<Frame>` is heap-bound, so this must just complete.
#[rstest]
#[case::plain_recursion("def f(n): if (n <= 0): 0 else: 1 + f(n - 1); | f(100000)")]
#[case::recursion_through_try_catch("def f(n): if (n <= 0): 0 else: try: 1 + f(n - 1) catch(e): -1; | f(100000)")]
fn deep_non_tail_recursion_does_not_overflow_the_native_stack(#[case] code: &str) {
    assert_eq!(
        run_with_max_depth(code, 1_000_000).unwrap(),
        RuntimeValue::Number(100000.into())
    );
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
        fn on_boundary(&mut self, event: DebugEvent) -> Result<(), super::interpreter::VmError> {
            self.0.push(event);
            Ok(())
        }

        fn on_explicit_breakpoint(&mut self, event: DebugEvent) -> Result<(), super::interpreter::VmError> {
            self.0.push(event);
            Ok(())
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
        Options::default().max_call_stack_depth,
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
fn debugger_hook_exposes_closure_bindings() {
    use super::interpreter::{DebugEvent, DebugHook};

    #[derive(Default)]
    struct Recorder(Vec<DebugEvent>);

    impl DebugHook for Recorder {
        fn on_boundary(&mut self, event: DebugEvent) -> Result<(), super::interpreter::VmError> {
            self.0.push(event);
            Ok(())
        }

        fn on_explicit_breakpoint(&mut self, event: DebugEvent) -> Result<(), super::interpreter::VmError> {
            self.0.push(event);
            Ok(())
        }
    }

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("let f = fn(x): x; | breakpoint() | f(1)", Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();
    let mut recorder = Recorder::default();
    interpreter::run_with_debug_hook_and_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        Options::default().max_call_stack_depth,
        &[],
        &mut recorder,
    )
    .unwrap();

    assert!(recorder.0.iter().any(|event| {
        event
            .bindings
            .iter()
            .any(|(name, value)| *name == crate::Ident::new("f") && matches!(value, RuntimeValue::VmClosure(_)))
    }));
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
        Options::default().max_call_stack_depth,
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
        Options::default().max_call_stack_depth,
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
        Options::default().max_call_stack_depth,
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
        Options::default().max_call_stack_depth,
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
#[case::pipeline_self_cannot_fill_two_required_params("def add(a, b): a + b; | 10 | add()", 2, 0)]
#[case::variadic_function_still_requires_more_than_one_missing_required_param(
    "def add(a, b, *rest): a + b + len(rest); | add()",
    2,
    0
)]
#[case::too_many_required("def add(a, b): a + b; | add(1, 2, 3)", 2, 3)]
#[case::too_many_optional("def add(a, b = 1): a + b; | add(1, 2, 3)", 2, 3)]
#[case::too_many_zero_arity("def constant(): 1; | constant(1)", 0, 1)]
fn invalid_function_arity_reports_the_declared_bounds(
    #[case] code: &str,
    #[case] expected: usize,
    #[case] actual: usize,
) {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(
        matches!(err, Error::Vm(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::ArityMismatch { expected: got_expected, actual: got_actual } if got_expected == expected && got_actual == actual))
    );
}

#[test]
fn large_call_arity_error_reports_the_full_argument_count() {
    let arguments = (0..256).map(|_| "1").collect::<Vec<_>>().join(", ");
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        &format!("var f = fn(value): value; | f({arguments})"),
        Shared::clone(&token_arena),
    )
    .unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(
        matches!(err, Error::Vm(interpreter::VmError::Located(inner, _)) if matches!(*inner, interpreter::VmError::ArityMismatch { expected: 1, actual: 256 }))
    );
}

#[cfg(not(feature = "debugger"))]
#[test]
fn cached_program_restores_execution_pools_after_each_run() {
    let mut engine = crate::DefaultEngine::default();
    let compiled = engine.compile("let value = 1 | value + 1").unwrap();

    for _ in 0..2 {
        assert_eq!(
            engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::Number(2.into())]
        );
        assert!(
            compiled
                .cached_vm_program()
                .flatten()
                .is_some_and(|cached| cached.has_available_execution_pools())
        );
    }
}

#[cfg(not(feature = "debugger"))]
#[test]
fn unchanged_engine_globals_reuse_their_vm_snapshot() {
    let state = VmState::<DefaultModuleResolver>::default();
    state.define("answer".into(), RuntimeValue::Number(42.into()));

    let (first, first_key) = state.global_bindings_snapshot_with_key();
    let (second, second_key) = state.global_bindings_snapshot_with_key();
    assert!(Shared::ptr_eq(&first, &second));
    assert_eq!(first_key, second_key);

    state.define("answer".into(), RuntimeValue::Number(43.into()));
    let (updated, updated_key) = state.global_bindings_snapshot_with_key();
    assert!(!Shared::ptr_eq(&first, &updated));
    assert_ne!(first_key, updated_key);
    assert_eq!(updated.as_ref(), &[("answer".into(), RuntimeValue::Number(43.into()))]);
}

#[cfg(not(feature = "debugger"))]
#[test]
fn cached_program_reflects_updated_global_in_module_var_initializer() {
    let mut engine = crate::DefaultEngine::default();
    engine.define_value("g", RuntimeValue::Number(1.into())).unwrap();
    let compiled = engine.compile("module m: let x = g end | m::x").unwrap();

    assert_eq!(
        engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap()
            .values(),
        &[RuntimeValue::Number(1.into())]
    );

    // `x` was baked into the cached bytecode from `g`'s value at compile time; changing `g`
    // must invalidate that cache, not just the plain-global lookup environment.
    engine.define_value("g", RuntimeValue::Number(2.into())).unwrap();

    assert_eq!(
        engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap()
            .values(),
        &[RuntimeValue::Number(2.into())]
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
    fn generated_compiled_programs_execute(
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
        assert_vm_executes(&code, vec![RuntimeValue::Number(f64::from(input).into())]);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn generated_nested_closures_execute(
        x in -50i16..50,
        y in -50i16..50,
        z in -50i16..50,
        w in -50i16..50,
    ) {
        let code = format!(
            "let x = {x} | let make = fn(y): fn(z): x + y + z + {w};; | let step = make({y}) | step({z})"
        );
        assert_vm_executes(&code, vec![RuntimeValue::None]);
    }

    #[test]
    fn generated_foreach_closures_execute(
        values in proptest::collection::vec(-30i16..30, 1..8),
    ) {
        let elements = values.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ");
        let code =
            format!("let fns = foreach(i, [{elements}]): fn(): i * 2;; | fns | map(fn(f): f();)");
        assert_vm_executes(&code, vec![RuntimeValue::None]);
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
fn assigning_to_an_immutable_captured_binding_is_a_compile_error() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(
        "let x = 1 | let change = fn(): x = 2; | change()",
        Shared::clone(&token_arena),
    )
    .unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(matches!(
        err,
        Error::Compile(compiler::CompileError::AssignToImmutable(..))
    ));
}

#[test]
fn assigning_to_an_as_binding_is_a_compile_error() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("1 as x | x = 2", Shared::clone(&token_arena)).unwrap();
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

/// External modules can contain inline modules, which remain qualified beneath the import alias.
#[rstest::fixture]
fn module_with_nested_inline_module() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(
        dir.path().join("parent.mq"),
        "module child: let answer = 40 | def add_two(): answer + 2; end",
    )
    .unwrap();
    dir
}

fn run_with_local_module(dir: &tempfile::TempDir, code: &str) -> RuntimeValue {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let resolver =
        crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap();
    interpreter::run_with_globals(
        &compiled,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        Options::default().max_call_stack_depth,
        &[],
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

#[rstest]
fn import_alias_prefixes_nested_inline_module_access(module_with_nested_inline_module: tempfile::TempDir) {
    assert_eq!(
        run_with_local_module(
            &module_with_nested_inline_module,
            r#"import "parent" as parent | parent::child::answer + parent::child::add_two()"#,
        ),
        RuntimeValue::Number(82.into())
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

/// A local module containing a remote `include`/`import` must still hit the top-level-only
/// HTTP boundary when compiled through Tarn.
#[rstest]
#[case::nested_include(
    "nested_remote_include.mq",
    r#"include "https://example.invalid/remote.mq""#,
    r#"include "nested_remote_include""#
)]
#[case::nested_import(
    "nested_remote_import.mq",
    r#"import "https://example.invalid/remote.mq""#,
    r#"import "nested_remote_import" as m"#
)]
#[cfg(feature = "http-import")]
fn nested_remote_module_directive_is_blocked_under_tarn(
    #[case] local_file: &str,
    #[case] local_source: &str,
    #[case] code: &str,
) {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join(local_file), local_source).unwrap();

    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let resolver =
        crate::module::resolver::local_fs_resolver::LocalFsModuleResolver::new(Some(vec![dir.path().to_path_buf()]));
    let err = compiler::compile_program(&program, token_arena, ModuleLoader::new(resolver)).unwrap_err();

    assert!(
        matches!(
            err,
            compiler::CompileError::Module(crate::ModuleError::HttpImportNotAllowed(_))
        ),
        "{code:?}: {err:?}"
    );
}

// Bytecode-level generator/coroutine tests. Source-level coverage follows below; these tests
// hand-build `Chunk`s to exercise suspension and resumption states that are awkward to construct
// from a single surface program.

fn generator_program(chunks: Vec<bytecode::Chunk>) -> compiler::CompiledProgram {
    compiler::CompiledProgram {
        chunks: Shared::new(chunks),
        token_arena: Shared::new(SharedCell::new(Arena::new(1))),
        #[cfg(feature = "debugger")]
        debug_sources: Vec::new(),
    }
}

fn run_generator_program(program: &compiler::CompiledProgram) -> Result<RuntimeValue, interpreter::VmError> {
    interpreter::run_with_globals(
        program,
        RuntimeValue::None,
        &HostFunctions::default(),
        None,
        Options::default().max_call_stack_depth,
        &[],
    )
}

fn dict_field(value: &RuntimeValue, key: &str) -> RuntimeValue {
    let RuntimeValue::Dict(map) = value else {
        panic!("expected a dict, got {value:?}");
    };
    map.get(&crate::Ident::new(key)).cloned().unwrap_or(RuntimeValue::None)
}

// `Chunk`'s private `captured_local_slots` field rules out `..Default::default()` from outside
// `bytecode`, so tests build via `Chunk::default()` plus field assignment instead.
fn chunk(code: Vec<bytecode::OpCode>, constants: Vec<RuntimeValue>, local_count: u16) -> bytecode::Chunk {
    let mut c = bytecode::Chunk::default();
    c.code = code;
    c.constants = constants;
    c.local_count = local_count;
    c
}

/// Chunk 1 in every test below: yields `1`, then `2`, then completes with `3`.
fn yield_1_2_return_3() -> bytecode::Chunk {
    use bytecode::OpCode;
    let mut c = chunk(
        vec![
            OpCode::Const(0),
            OpCode::Yield,
            OpCode::Const(1),
            OpCode::Yield,
            OpCode::Const(2),
            OpCode::Return,
        ],
        vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ],
        1,
    );
    c.is_generator = true;
    c
}

/// Chunk 0: calls the generator at chunk 1, then `next()`s it `n` times, returning the last
/// `{ value, done }` result.
fn drive_n_times(n: u32) -> bytecode::Chunk {
    use bytecode::OpCode;
    let mut code = vec![OpCode::CallStatic(1, 0), OpCode::SetLocal(1)];
    for i in 0..n {
        code.push(OpCode::GetLocal(1));
        code.push(OpCode::Resume(1));
        if i + 1 < n {
            code.push(OpCode::Pop);
        }
    }
    code.push(OpCode::Return);
    chunk(code, Vec::new(), 2)
}

#[test]
fn next_before_first_resume_runs_to_the_first_yield() {
    let result = run_generator_program(&generator_program(vec![drive_n_times(1), yield_1_2_return_3()])).unwrap();
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(1.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn repeated_next_resumes_after_the_previous_yield() {
    let result = run_generator_program(&generator_program(vec![drive_n_times(2), yield_1_2_return_3()])).unwrap();
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(2.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn next_past_the_last_yield_completes_the_coroutine() {
    let result = run_generator_program(&generator_program(vec![drive_n_times(3), yield_1_2_return_3()])).unwrap();
    assert_eq!(dict_field(&result, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(true));
}

#[test]
fn next_after_completion_is_idempotent() {
    let result = run_generator_program(&generator_program(vec![drive_n_times(4), yield_1_2_return_3()])).unwrap();
    assert_eq!(dict_field(&result, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(true));
}

#[test]
fn next_after_failure_reraises_the_same_error() {
    use bytecode::OpCode;
    let mut failing_generator = chunk(
        vec![
            OpCode::Const(0),
            OpCode::Yield,
            OpCode::Const(0),
            OpCode::Const(1),
            OpCode::Div,
            OpCode::Return,
        ],
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(0.into())],
        1,
    );
    failing_generator.is_generator = true;
    let err = run_generator_program(&generator_program(vec![drive_n_times(3), failing_generator])).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("division by zero"));
}

#[test]
fn cloning_a_coroutine_shares_its_progress() {
    use bytecode::OpCode;
    let main = chunk(
        vec![
            OpCode::CallStatic(1, 0),
            OpCode::Dup,
            OpCode::Resume(1),
            OpCode::Pop,
            OpCode::Resume(1),
            OpCode::Return,
        ],
        Vec::new(),
        1,
    );
    let result = run_generator_program(&generator_program(vec![main, yield_1_2_return_3()])).unwrap();
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(2.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

/// A generator that, after its first yield, resumes itself via a captured upvalue holding its
/// own coroutine handle. `next()` on it must observe `Running` and fail.
#[test]
fn reentrant_next_on_a_running_coroutine_errors() {
    use bytecode::{OpCode, UpvalueSource};
    let mut generator = chunk(
        vec![
            OpCode::Const(0),
            OpCode::Yield,
            OpCode::GetUpvalue(0),
            OpCode::Resume(1),
            OpCode::Return,
        ],
        vec![RuntimeValue::Number(1.into())],
        1,
    );
    generator.is_generator = true;
    generator.upvalue_names = vec![crate::Ident::new("s")];

    let mut main = chunk(
        vec![
            OpCode::PushNone,
            OpCode::SetLocal(1),
            OpCode::MakeClosure(Box::new((1, vec![UpvalueSource::Local(1)]))),
            OpCode::CallValue(0),
            OpCode::TeeLocal(1),
            OpCode::Resume(1),
            OpCode::Pop,
            OpCode::GetLocal(1),
            OpCode::Resume(1),
            OpCode::Return,
        ],
        Vec::new(),
        2,
    );
    main.refresh_captured_local_slots();

    let err = run_generator_program(&generator_program(vec![main, generator])).unwrap_err();
    assert_eq!(err.to_string(), "coroutine is already running");
}

#[test]
fn dropping_a_self_referencing_suspended_coroutine_releases_its_frames() {
    let value = run("var s = None | let g = fn(): yield: 1 | s; | s = g() | next(s) | s");
    let RuntimeValue::Coroutine(handle) = &value else {
        panic!("expected a coroutine, got {value:?}");
    };
    let weak = Shared::downgrade(handle);

    drop(value);

    assert!(
        weak.upgrade().is_none(),
        "suspended coroutine retained a self-reference cycle"
    );
}

#[rstest]
#[case::reassigned_to_none("s = None", RuntimeValue::None)]
#[case::reassigned_to_a_number("s = 42", RuntimeValue::Number(42.into()))]
#[case::reassigned_to_a_string("s = \"done\"", RuntimeValue::from("done"))]
fn outer_write_to_a_captured_self_reference_is_visible_after_resume(
    #[case] reassign: &str,
    #[case] expected: RuntimeValue,
) {
    let code = format!(
        "var s = None | let g = fn(): yield: 0 | yield: s; | s = g() | next(s) | let saved = s | {reassign} | next(saved)"
    );
    let result = run(&code);
    assert_eq!(
        dict_field(&result, "value"),
        expected,
        "the resumed generator must observe the outer scope's reassignment of its captured `s`"
    );
}

#[test]
fn generator_assignment_to_a_captured_self_reference_survives_the_next_resume() {
    let result = run(
        "var s = None | let g = fn(): s = 42 | yield: 0 | yield: s; | s = g() | let saved = s | next(saved) | next(saved)",
    );
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(42.into()));
}

#[rstest]
#[case::array("[s]")]
#[case::dict(r#"{"s": s}"#)]
fn dropping_a_coroutine_nested_in_a_captured_container_releases_its_frames(#[case] container: &str) {
    let code = format!(
        "var s = None | var holder = [] | let g = fn(): yield: 0 | holder; | s = g() | holder = {container} | next(s) | s"
    );
    let value = run(&code);
    let RuntimeValue::Coroutine(handle) = &value else {
        panic!("expected a coroutine, got {value:?}");
    };
    let weak = Shared::downgrade(handle);

    drop(value);

    assert!(
        weak.upgrade().is_none(),
        "suspended coroutine retained a self-reference cycle through a captured container"
    );
}

#[rstest]
#[case::array("[s]", "0")]
#[case::dict(r#"{"s": s}"#, "\"s\"")]
fn suspending_does_not_erase_a_self_reference_from_an_aliased_captured_container(
    #[case] container: &str,
    #[case] key: &str,
) {
    let code = format!(
        "var s = None | var holder = [] | let g = fn(): yield: 0 | holder; | s = g() | holder = {container} | next(s) | get(holder, {key})"
    );
    let value = run(&code);
    assert!(
        matches!(value, RuntimeValue::Coroutine(_)),
        "suspending must not erase the coroutine from the caller's own captured container, got {value:?}"
    );
}

// End-to-end generator tests compiled from real `yield`/`next()` source (Phase 4: lexer, CST,
// AST, HIR-free compiler wiring all land together so every commit stays green).

/// Drives `stream` (bound by `def_and_binding`) with `n` `next()` calls, discarding all but the
/// last, and returns its `{ value, done }` dict.
fn run_yield_source(def_and_binding: &str, n: u32) -> RuntimeValue {
    let mut code = format!("{def_and_binding} | var s = stream");
    for _ in 0..n {
        code.push_str(" | s | next(s)");
    }
    run(&code)
}

#[test]
fn range_example_yields_then_completes() {
    let def = "def range(n): var i = 0 | while (i < n): yield: i | i += 1;; | let stream = range(3)";
    assert_eq!(
        dict_field(&run_yield_source(def, 1), "value"),
        RuntimeValue::Number(0.into())
    );
    assert_eq!(
        dict_field(&run_yield_source(def, 2), "value"),
        RuntimeValue::Number(1.into())
    );
    assert_eq!(
        dict_field(&run_yield_source(def, 3), "value"),
        RuntimeValue::Number(2.into())
    );
    let fourth = run_yield_source(def, 4);
    assert_eq!(dict_field(&fourth, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&fourth, "done"), RuntimeValue::Boolean(true));
}

#[test]
fn generator_completion_discards_the_function_return_value() {
    let def = "def g(): yield: 1 | 42; | let stream = g()";
    let result = run_yield_source(def, 2);
    assert_eq!(dict_field(&result, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(true));
}

#[test]
fn bare_yield_produces_none_value() {
    let result = run("def g(): yield; | let s = g() | next(s)");
    assert_eq!(dict_field(&result, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn next_without_an_argument_resumes_the_pipeline_coroutine() {
    let result = run("def g(): yield: 1; | let stream = g() | stream | next()");
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(1.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[rstest]
#[case::stored("def g(): yield: 1; | let advance = next | let s = g() | advance(s)", 1)]
#[case::passed(
    "def apply(f, value): f(value); | def g(): yield: 2; | let s = g() | apply(next, s)",
    2
)]
#[case::piped("def g(): yield: 3; | let advance = next | let s = g() | s | advance()", 3)]
#[case::captured(
    "def g(): yield: 4; | let advance = next | let apply = fn(stream): advance(stream); | let s = g() | apply(s)",
    4
)]
#[case::contained(
    "def g(): yield: 5; | let advances = [next] | let advance = advances[0] | let s = g() | advance(s)",
    5
)]
fn next_is_first_class_across_call_paths(#[case] code: &str, #[case] expected: i64) {
    let result = run(code);
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(expected.into()));
}

#[rstest]
#[case::stored_and_piped(
    "def g(): let value = yield: 1 | yield: value; | let resume = send | let s = g() | next(s) | s | resume(42)",
    42
)]
#[case::passed(
    "def apply(f, stream, value): f(stream, value); | def g(): let value = yield: 1 | yield: value; | let s = g() | next(s) | apply(send, s, 99)",
    99
)]
#[case::captured(
    "def g(): let value = yield: 1 | yield: value; | let resume = send | let apply = fn(stream, value): resume(stream, value); | let s = g() | next(s) | apply(s, 100)",
    100
)]
#[case::contained(
    "def g(): let value = yield: 1 | yield: value; | let resumes = [send] | let resume = resumes[0] | let s = g() | next(s) | resume(s, 101)",
    101
)]
fn send_is_first_class_across_call_paths(#[case] code: &str, #[case] expected: i64) {
    let result = run(code);
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(expected.into()));
}

#[rstest]
#[case::next("def next(stream): stream + 1; | let advance = next | advance(41)", 42)]
#[case::send("def send(stream, value): stream + value; | let resume = send | resume(40, 2)", 42)]
fn local_resume_names_shadow_first_class_builtins(#[case] code: &str, #[case] expected: i64) {
    assert_eq!(run(code), RuntimeValue::Number(expected.into()));
}

#[test]
fn local_next_definition_still_shadows_pipeline_resume() {
    assert_eq!(
        run("def next(stream): stream + 1; | 41 | next()"),
        RuntimeValue::Number(42.into())
    );
}

#[test]
fn yield_after_a_nested_call_returns_still_suspends_correctly() {
    let code = "def helper(x): x * 2; | def g(): var a = helper(3) | yield: a; | let s = g() | next(s)";
    let result = run(code);
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(6.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn yield_inside_try_catch_suspends_and_resumes_through_the_try_frame() {
    let def = "def g(): try: yield: 1 catch: yield: -1 | yield: 2; | let stream = g()";
    assert_eq!(
        dict_field(&run_yield_source(def, 1), "value"),
        RuntimeValue::Number(1.into())
    );
    // Confirm the *second* next() correctly resumes past the try body, not just the first.
    assert_eq!(
        dict_field(&run_yield_source(def, 2), "value"),
        RuntimeValue::Number(2.into())
    );
}

#[test]
fn suspended_generator_frames_do_not_consume_an_unrelated_call_depth() {
    // Entering `try` pushes a synthetic VM frame. After its `yield`, that frame belongs to the
    // coroutine rather than the caller that invoked `next()`: a separate one-frame call must
    // still fit under this limit.
    let code = "def g(): try: yield: 1 catch: 0; | let s = g() | next(s) | def f(): 42; | f()";
    assert_eq!(run_with_max_depth(code, 2).unwrap(), RuntimeValue::Number(42.into()));
}

fn contains_recursion_error(error: &interpreter::VmError, max_depth: u32) -> bool {
    match error {
        interpreter::VmError::RecursionError(actual) => *actual == max_depth,
        interpreter::VmError::Located(inner, _) => contains_recursion_error(inner, max_depth),
        interpreter::VmError::CoroutineFailed(inner, _) => contains_recursion_error(inner, max_depth),
        _ => false,
    }
}

#[rstest]
#[case::one(1)]
#[case::four(4)]
#[case::eight(8)]
fn recursively_resumed_generators_respect_call_stack_depth(#[case] max_depth: u32) {
    // Every invocation creates and resumes a child before yielding. This recursively enters
    // `coroutine::resume` on the Rust stack, so each created generator frame must count toward
    // the VM recursion limit before it reaches its first yield.
    let code = "def g(n): if (n <= 0): yield: 0 else: let child = g(n - 1) | next(child) | yield: n; \
                | let stream = g(20) \
                | next(stream)";
    let error = run_with_max_depth(code, max_depth).unwrap_err();
    assert!(
        contains_recursion_error(&error, max_depth),
        "expected RecursionError, got {error}"
    );
}

#[test]
fn yield_inside_foreach_suspends_once_per_element() {
    let def = "def g(): foreach (x, array(10, 20, 30)): yield: x;; | let stream = g()";
    assert_eq!(
        dict_field(&run_yield_source(def, 2), "value"),
        RuntimeValue::Number(20.into())
    );
}

#[test]
fn generator_closure_mutates_captured_state_across_suspensions() {
    // `fn` (not `def`) capturing an outer `var`, mutated between yields. Closures/upvalues
    // must survive suspend/resume, and the mutation must be visible to the caller afterward.
    let code = "var total = 0 \
                | let g = fn(): total += 1 | yield: total | total += 1 | yield: total; \
                | let s = g() \
                | next(s) \
                | s | next(s) \
                | total";
    assert_eq!(run(code), RuntimeValue::Number(2.into()));
}

#[test]
fn suspended_generator_with_a_captured_self_reference_is_released() {
    // Suspending `g` captures `s`, whose value is the coroutine itself. The suspension path
    // must downgrade that back-edge; otherwise the coroutine state, frame, and captured cell
    // keep one another alive after the program drops its last external reference.
    let stream = run("var s = None | let g = fn(): yield: s; | s = g() | next(s) | s");
    let RuntimeValue::Coroutine(handle) = &stream else {
        panic!("expected the program to return its coroutine");
    };
    let weak = Shared::downgrade(handle);

    drop(stream);

    assert!(
        weak.upgrade().is_none(),
        "the suspended coroutine must not retain itself"
    );
}

#[test]
fn unstarted_generator_with_a_captured_self_reference_is_released() {
    // Same self-reference as above, but `s` is never `next()`-ed: `g`'s frame captures `s`'s
    // cell, and `s = g()` writes the coroutine into that very cell before it ever suspends.
    // `downgrade_self_references` (suspend-time only) can't reach this; the write itself must
    // break the cycle.
    let stream = run("var s = None | let g = fn(): yield: s; | s = g() | s");
    let RuntimeValue::Coroutine(handle) = &stream else {
        panic!("expected the program to return its coroutine");
    };
    let weak = Shared::downgrade(handle);

    drop(stream);

    assert!(
        weak.upgrade().is_none(),
        "the unstarted coroutine must not retain itself"
    );
}

#[rstest]
#[case::array("[s]")]
#[case::dict(r#"{"stream": s}"#)]
fn unstarted_generator_nested_in_a_captured_container_is_released(#[case] container: &str) {
    let code = format!("var holder = [] | let g = fn(): yield: holder; | let s = g() | holder = {container} | s");
    let stream = run(&code);
    let RuntimeValue::Coroutine(handle) = &stream else {
        panic!("expected the program to return its coroutine");
    };
    let weak = Shared::downgrade(handle);

    drop(stream);

    assert!(
        weak.upgrade().is_none(),
        "the unstarted coroutine must not retain itself through a captured container"
    );
}

#[rstest]
#[case::array("[s]", "holder[0]")]
#[case::dict(r#"{"stream": s}"#, r#"holder["stream"]"#)]
fn generator_reads_its_own_coroutine_back_through_a_captured_container(
    #[case] container: &str,
    #[case] read_expr: &str,
) {
    let code = format!(
        "var holder = [] | let g = fn(): yield: 0 | yield: {read_expr}; | let s = g() | holder = {container} | next(s) | next(s)"
    );
    let result = run(&code);
    assert!(
        matches!(dict_field(&result, "value"), RuntimeValue::Coroutine(_)),
        "the generator must read back its own coroutine through the captured container, got {:?}",
        dict_field(&result, "value")
    );
}

#[rstest]
#[case::unstarted(
    "var a = None | var b = None \
     | let ga = fn(): yield: b; \
     | let gb = fn(): yield: a; \
     | a = ga() | b = gb() | [a, b]"
)]
#[case::suspended(
    "var a = None | var b = None \
     | let ga = fn(): yield: 0 | yield: b; \
     | let gb = fn(): yield: 0 | yield: a; \
     | a = ga() | b = gb() | next(a) | next(b) | [a, b]"
)]
fn dropping_two_mutually_capturing_coroutines_releases_both(#[case] code: &str) {
    let value = run(code);
    let RuntimeValue::Array(pair) = &value else {
        panic!("expected an array, got {value:?}");
    };
    let RuntimeValue::Coroutine(ga) = &pair[0] else {
        panic!("expected a coroutine, got {:?}", pair[0]);
    };
    let RuntimeValue::Coroutine(gb) = &pair[1] else {
        panic!("expected a coroutine, got {:?}", pair[1]);
    };
    let (weak_ga, weak_gb) = (Shared::downgrade(ga), Shared::downgrade(gb));

    drop(value);

    assert!(weak_ga.upgrade().is_none(), "ga must not retain gb -> ga -> gb");
    assert!(weak_gb.upgrade().is_none(), "gb must not retain ga -> gb -> ga");
}

#[rstest]
#[case::a_reads_b("next(a)")]
#[case::b_reads_a("next(b)")]
fn mutually_capturing_coroutines_still_read_each_other_after_the_cycle_is_broken(#[case] read_expr: &str) {
    let code = format!(
        "var a = None | var b = None \
         | let ga = fn(): yield: b; \
         | let gb = fn(): yield: a; \
         | a = ga() | b = gb() | {read_expr}"
    );
    let result = run(&code);
    assert!(
        matches!(dict_field(&result, "value"), RuntimeValue::Coroutine(_)),
        "must still read back the peer through the broken cycle, got {:?}",
        dict_field(&result, "value")
    );
}

#[rstest]
#[case::unstarted(
    "var a = None | var b = None | var c = None \
     | let ga = fn(): yield: b; \
     | let gb = fn(): yield: c; \
     | let gc = fn(): yield: a; \
     | a = ga() | b = gb() | c = gc() | [a, b, c]"
)]
#[case::suspended(
    "var a = None | var b = None | var c = None \
     | let ga = fn(): yield: 0 | yield: b; \
     | let gb = fn(): yield: 0 | yield: c; \
     | let gc = fn(): yield: 0 | yield: a; \
     | a = ga() | b = gb() | c = gc() | next(a) | next(b) | next(c) | [a, b, c]"
)]
fn dropping_three_mutually_capturing_coroutines_releases_all(#[case] code: &str) {
    // A -> B -> C -> A: longer than the direct pairwise case.
    let value = run(code);
    let RuntimeValue::Array(trio) = &value else {
        panic!("expected an array, got {value:?}");
    };
    let RuntimeValue::Coroutine(ga) = &trio[0] else {
        panic!("expected a coroutine, got {:?}", trio[0]);
    };
    let RuntimeValue::Coroutine(gb) = &trio[1] else {
        panic!("expected a coroutine, got {:?}", trio[1]);
    };
    let RuntimeValue::Coroutine(gc) = &trio[2] else {
        panic!("expected a coroutine, got {:?}", trio[2]);
    };
    let (weak_ga, weak_gb, weak_gc) = (Shared::downgrade(ga), Shared::downgrade(gb), Shared::downgrade(gc));

    drop(value);

    assert!(weak_ga.upgrade().is_none(), "ga must not retain gb -> gc -> ga");
    assert!(weak_gb.upgrade().is_none(), "gb must not retain gc -> ga -> gb");
    assert!(weak_gc.upgrade().is_none(), "gc must not retain ga -> gb -> gc");
}

#[test]
fn a_coroutine_capturing_another_non_cyclically_is_unaffected() {
    // `ga` captures the coroutine `gb`, but `gb` doesn't capture `ga` back: not a cycle, so the
    // mutual-capture check must not touch it.
    let code = "let g = fn(): yield: 1; | let gb = g() | let ga = fn(): yield: gb; | next(ga())";
    let result = run(code);
    assert!(
        matches!(dict_field(&result, "value"), RuntimeValue::Coroutine(_)),
        "a non-cyclic capture must be unaffected by mutual-cycle detection, got {:?}",
        dict_field(&result, "value")
    );
}

#[test]
fn calling_a_generator_does_not_execute_it() {
    // Calling `g()` alone (no `next()`) must produce a coroutine, not run the body, so `marker`
    // stays unset.
    let code = "var marker = 0 | def g(): marker = 1 | yield: 1; | let s = g() | marker";
    assert_eq!(run(code), RuntimeValue::Number(0.into()));
}

#[test]
fn ordinary_functions_are_unaffected_by_generator_support() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let code = "def add(a, b): a + b; | add(1, 2)";
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let compiled = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap();

    assert!(compiled.chunks.iter().all(|chunk| !chunk.is_generator));
    assert!(compiled.chunks.iter().all(|chunk| {
        !chunk
            .code
            .iter()
            .any(|op| matches!(op, bytecode::OpCode::Yield | bytecode::OpCode::Resume(_)))
    }));
}

#[test]
fn yield_outside_a_function_is_a_compile_error() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse("yield: 1", Shared::clone(&token_arena)).unwrap();
    let err = compiler::compile_program(&program, token_arena, ModuleLoader::new(StdModuleResolver)).unwrap_err();
    assert!(matches!(err, compiler::CompileError::YieldOutsideFunction(_)));
}

#[test]
fn generator_call_with_a_defaulted_argument_still_produces_a_coroutine() {
    let result = run("def g(x = 1): yield: x; | let s = g() | s | next(s)");
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(1.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn self_recursive_generator_call_produces_a_coroutine_instead_of_running_inline() {
    let def = "def g(n): yield: n | if (n > 0): g(n - 1) else: None;";
    let first = run_yield_source(&format!("{def} | let stream = g(1)"), 1);
    assert_eq!(dict_field(&first, "value"), RuntimeValue::Number(1.into()));
    assert_eq!(dict_field(&first, "done"), RuntimeValue::Boolean(false));

    let second = run_yield_source(&format!("{def} | let stream = g(1)"), 2);
    assert_eq!(dict_field(&second, "done"), RuntimeValue::Boolean(true));
    assert_eq!(dict_field(&second, "value"), RuntimeValue::None);
}

#[test]
fn send_resumes_a_suspended_yield_to_the_given_value() {
    let code = "def g(): let a = yield: 1 | yield: a + 1; | let s = g() | next(s) | send(s, 10)";
    let result = run(code);
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(11.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn send_without_an_explicit_stream_resumes_the_pipeline_coroutine() {
    let code = "def g(): let a = yield: 1 | yield: a + 1; | let s = g() | next(s) | s | send(10)";
    assert_eq!(dict_field(&run(code), "value"), RuntimeValue::Number(11.into()));
}

#[test]
fn send_to_a_not_yet_started_coroutine_discards_the_value() {
    let result = run("def g(): yield: 1; | let s = g() | send(s, 99)");
    assert_eq!(dict_field(&result, "value"), RuntimeValue::Number(1.into()));
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(false));
}

#[test]
fn local_send_definition_still_shadows_pipeline_resume() {
    assert_eq!(
        run("def send(stream, v): stream + v; | 40 | send(2)"),
        RuntimeValue::Number(42.into())
    );
}

#[test]
fn status_reports_each_lifecycle_state() {
    assert_eq!(
        run("def g(): yield: 1; | status(g())"),
        RuntimeValue::Symbol(crate::Ident::new("created"))
    );
    assert_eq!(
        run("def g(): yield: 1; | let s = g() | next(s) | status(s)"),
        RuntimeValue::Symbol(crate::Ident::new("suspended"))
    );
    assert_eq!(
        run("def g(): yield: 1; | let s = g() | next(s) | next(s) | status(s)"),
        RuntimeValue::Symbol(crate::Ident::new("completed"))
    );
    let failing = "def g(): yield: 1 | 1 / 0; | let s = g() | next(s) | try: next(s) catch: 0 | status(s)";
    assert_eq!(run(failing), RuntimeValue::Symbol(crate::Ident::new("failed")));
}

#[test]
fn close_forces_a_suspended_coroutine_to_completion() {
    let result = run("def g(): yield: 1; | let s = g() | next(s) | close(s) | next(s)");
    assert_eq!(dict_field(&result, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(true));
}

#[test]
fn close_on_a_completed_coroutine_is_a_no_op() {
    let result = run("def g(): yield: 1; | let s = g() | next(s) | next(s) | close(s) | next(s)");
    assert_eq!(dict_field(&result, "value"), RuntimeValue::None);
    assert_eq!(dict_field(&result, "done"), RuntimeValue::Boolean(true));
}

#[test]
fn close_on_a_failed_coroutine_still_reraises_its_error() {
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let code = "def g(): yield: 1 | 1 / 0; | let s = g() | next(s) | try: next(s) catch: 0 | close(s) | next(s)";
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    assert!(err.to_string().to_lowercase().contains("division by zero"));
}

/// `close`'s `Running` check mirrors `next`'s reentrancy check: a generator that closes itself
/// (via a captured `var`) mid-resume must fail the same way a self-`next()` does.
#[test]
fn closing_a_running_coroutine_errors() {
    let code = "var s = None | let g = fn(): yield: 1 | close(s) | yield: 2; | s = g() | next(s) | next(s)";
    let token_arena = Shared::new(SharedCell::new(Arena::new(100)));
    let program = crate::parse(code, Shared::clone(&token_arena)).unwrap();
    let err = compile_and_run(&program, token_arena).unwrap_err();
    // `builtin::Error`'s `Display` is deliberately empty (the real message is built by
    // `to_runtime_error`), so unwrap the error shape instead of formatting it. The failure
    // surfaces through the generator's own `CoroutineFailed`, wrapping a `Located` `Builtin` error.
    fn close_error_message(e: &interpreter::VmError) -> Option<&str> {
        match e {
            interpreter::VmError::Located(inner, _) => close_error_message(inner),
            interpreter::VmError::CoroutineFailed(inner, _) => close_error_message(inner),
            interpreter::VmError::Builtin(crate::runtime::builtin::Error::Runtime(msg)) => Some(msg.as_str()),
            _ => None,
        }
    }
    let Error::Vm(vm_err) = &err else {
        panic!("expected a VM error, got {err:?}")
    };
    let message = close_error_message(vm_err).unwrap_or_else(|| panic!("expected a close error, got {err:?}"));
    assert!(message.contains("cannot close a running coroutine"));
}
