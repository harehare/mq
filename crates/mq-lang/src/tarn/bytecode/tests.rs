use super::*;
use rstest::rstest;

fn native(name: &str) -> &'static BuiltinFunction {
    crate::runtime::builtin::get_builtin_functions(&Ident::new(name)).expect("native builtin")
}

#[test]
fn push_const_reuses_safe_values_within_each_chunk() {
    let values = [
        RuntimeValue::Number(3.0.into()),
        RuntimeValue::Boolean(true),
        RuntimeValue::from("repeated"),
        RuntimeValue::Symbol(Ident::new("repeated")),
        RuntimeValue::from(vec![1_u8, 2_u8]),
        RuntimeValue::None,
    ];
    let (mut first, mut first_index) = (Chunk::default(), ConstantIndex::default());
    for (expected, value) in values.iter().enumerate() {
        assert_eq!(first_index.push(&mut first, value.clone()), expected as u16);
        assert_eq!(first_index.push(&mut first, value.clone()), expected as u16);
    }
    assert_eq!(first.constants.len(), values.len());

    let (mut second, mut second_index) = (Chunk::default(), ConstantIndex::default());
    assert_eq!(second_index.push(&mut second, values[0].clone()), 0);
}

#[test]
fn push_const_keeps_signed_zero_and_runtime_objects_distinct() {
    let (mut chunk, mut index) = (Chunk::default(), ConstantIndex::default());
    assert_eq!(index.push(&mut chunk, RuntimeValue::Number(0.0.into())), 0);
    assert_eq!(index.push(&mut chunk, RuntimeValue::Number((-0.0).into())), 1);
    assert_eq!(index.push(&mut chunk, RuntimeValue::Number(0.0.into())), 0);

    let array = RuntimeValue::empty_array();
    assert_eq!(index.push(&mut chunk, array.clone()), 2);
    assert_eq!(index.push(&mut chunk, array), 3);

    let function = RuntimeValue::NativeFunction(Ident::new("len"));
    assert_eq!(index.push(&mut chunk, function.clone()), 4);
    assert_eq!(index.push(&mut chunk, function), 5);
}

#[test]
fn shared_string_constant_is_not_changed_by_mutating_a_loaded_copy() {
    let (mut chunk, mut constants) = (Chunk::default(), ConstantIndex::default());
    let index = constants.push(&mut chunk, RuntimeValue::from("original"));
    assert_eq!(constants.push(&mut chunk, RuntimeValue::from("original")), index);
    let RuntimeValue::String(mut loaded) = chunk.constants[index as usize].clone() else {
        panic!("expected a string constant");
    };
    crate::runtime::runtime_value::string_mut(&mut loaded).push_str(" changed");
    assert_eq!(chunk.constants[index as usize], RuntimeValue::from("original"));
}

#[test]
fn opcode_stays_compact() {
    assert!(std::mem::size_of::<OpCode>() <= 16);
}

#[test]
fn capture_slots_selects_the_last_declaration() {
    let first = Ident::new("first");
    let second = Ident::new("second");
    let chunk = Chunk {
        local_names: vec![first, second, first],
        ..Default::default()
    };

    assert_eq!(
        super::super::interpreter::capture_slots(&chunk, &[first, second]),
        vec![(first, 2), (second, 1)]
    );
}

#[test]
fn captured_local_metadata_contains_only_closure_and_default_sources() {
    let mut chunk = Chunk {
        local_count: 5,
        code: vec![OpCode::MakeClosure(Box::new((0, vec![UpvalueSource::Local(3)])))],
        param_shape: ParamShape {
            bindings: vec![ParamBinding::Optional(1, 0, vec![UpvalueSource::Local(1)])],
            ..Default::default()
        },
        ..Default::default()
    };

    chunk.refresh_captured_local_slots();

    assert_eq!(chunk.captured_local_slots(), &[1, 3]);
}

#[test]
fn verifier_rejects_invalid_constant_and_jump_targets() {
    let invalid_constant = Chunk {
        code: vec![OpCode::Const(0), OpCode::Return],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[invalid_constant]),
        Err(BytecodeError::ConstantOutOfBounds { .. })
    ));

    let invalid_jump = Chunk {
        code: vec![OpCode::Jump(4), OpCode::Return],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[invalid_jump]),
        Err(BytecodeError::JumpOutOfBounds { .. })
    ));

    let invalid_static_closure = Chunk {
        code: vec![OpCode::MakeStaticClosure(0), OpCode::Return],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[invalid_static_closure]),
        Err(BytecodeError::StaticClosureOutOfBounds { .. })
    ));
}

#[rstest]
#[case::at_the_limit(usize::from(u16::MAX), false)]
#[case::over_the_limit(usize::from(u16::MAX) + 1, true)]
fn verifier_limits_local_slots_to_what_local_count_can_hold(#[case] slots: usize, #[case] rejected: bool) {
    let chunk = Chunk {
        code: vec![OpCode::Return],
        local_names: vec![Ident::default(); slots],
        local_count: slots as u16,
        ..Default::default()
    };
    assert_eq!(
        matches!(verify_chunks(&[chunk]), Err(BytecodeError::TooManyLocals { .. })),
        rejected
    );
}

#[test]
fn stack_verifier_stops_when_its_shared_work_budget_is_exhausted() {
    let chunk = Chunk {
        code: vec![OpCode::PushNone, OpCode::Return],
        ..Default::default()
    };
    let mut remaining_work = 0;
    assert_eq!(
        verify_stack_effects(&chunk, 3, &mut remaining_work),
        Err(BytecodeError::VerificationWorkLimit { chunk: 3 })
    );

    let mut remaining_work = 2;
    assert_eq!(verify_stack_effects(&chunk, 3, &mut remaining_work), Ok(()));
    assert_eq!(remaining_work, 0);
}

#[rstest]
#[case::pop(vec![OpCode::Pop, OpCode::Return], 0, 1, 0, 0)]
#[case::binary(vec![OpCode::PushNone, OpCode::Add, OpCode::Return], 1, 2, 1, 0)]
#[case::call_builtin(vec![OpCode::CallBuiltin(Ident::new("f"), 1), OpCode::Return], 0, 1, 0, 0)]
#[case::call_native(
        vec![OpCode::CallNative { func: native("len"), ident: Ident::new("len"), argc: 1 }, OpCode::Return],
        0,
        1,
        0,
        0
    )]
// TeeLocal peeks, it doesn't duplicate: a second Pop should underflow.
#[case::tee_local_does_not_duplicate(
        vec![OpCode::PushNone, OpCode::TeeLocal(0), OpCode::Pop, OpCode::Pop, OpCode::Return],
        3,
        1,
        0,
        1,
    )]
fn verifier_rejects_stack_underflow(
    #[case] code: Vec<OpCode>,
    #[case] pc: usize,
    #[case] required: usize,
    #[case] available: usize,
    #[case] local_count: u16,
) {
    let chunk = Chunk {
        code,
        local_count,
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::StackUnderflow {
            chunk: 0,
            pc: actual_pc,
            required: actual_required,
            available: actual_available,
        }) if actual_pc == pc && actual_required == required && actual_available == available
    ));
}

#[test]
fn verifier_uses_the_lowest_height_at_a_stack_polymorphic_join() {
    let chunk = Chunk {
        code: vec![
            OpCode::PushNone,
            OpCode::JumpIfFalse(2),
            OpCode::PushNone,
            OpCode::Jump(0),
            OpCode::Pop,
            OpCode::Return,
        ],
        ..Default::default()
    };

    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::StackUnderflow {
            pc: 4,
            available: 0,
            ..
        })
    ));
}

#[rstest]
#[case::get_local(vec![OpCode::GetLocal(0), OpCode::Pop, OpCode::Return])]
#[case::take_local(vec![OpCode::TakeLocal(0), OpCode::Pop, OpCode::Return])]
#[case::return_local(vec![OpCode::ReturnLocal(0)])]
#[case::set_local(vec![OpCode::PushNone, OpCode::SetLocal(0), OpCode::Return])]
#[case::set_index_local(vec![
        OpCode::PushNone,
        OpCode::PushNone,
        OpCode::SetIndexLocal(0),
        OpCode::Return,
    ])]
#[case::add_assign_local(vec![OpCode::PushNone, OpCode::AddAssignLocal(0), OpCode::Return])]
#[case::set_local_and_copy(vec![
        OpCode::PushNone,
        OpCode::SetLocalAndCopy { source: 0, destination: 0 },
        OpCode::Return,
    ])]
#[case::set_local_const(vec![OpCode::SetLocalConst { local: 0, constant: 0 }, OpCode::Return])]
#[case::tee_local(vec![OpCode::PushNone, OpCode::TeeLocal(0), OpCode::Pop, OpCode::Return])]
#[case::copy_local(vec![
        OpCode::CopyLocal {
            source: 0,
            destination: 0,
        },
        OpCode::Return,
    ])]
#[case::move_local(vec![
        OpCode::MoveLocal {
            source: 0,
            destination: 0,
        },
        OpCode::Return,
    ])]
#[case::call_local(vec![OpCode::CallLocal(0, 0), OpCode::Pop, OpCode::Return])]
#[case::foreach_collect(vec![OpCode::ForeachCollect(0), OpCode::Return])]
#[case::foreach_collect_and_jump(vec![
        OpCode::ForeachCollectAndJump { slot: 0, offset: 1 },
        OpCode::Return,
        OpCode::Return,
    ])]
#[case::foreach_binary_local_number_const_and_jump(vec![
        OpCode::ForeachBinaryLocalNumberConstAndJump {
            op: BinaryOp::Add,
            local: 0,
            constant: 0,
            accumulator_slot: 0,
            offset: 1,
        },
        OpCode::Return,
        OpCode::Return,
    ])]
#[case::array_len_local(vec![OpCode::ArrayLenLocal(0), OpCode::Pop, OpCode::Return])]
#[case::array_new_with_capacity_local(vec![
        OpCode::ArrayNewWithCapacityLocal(0),
        OpCode::Pop,
        OpCode::Return,
    ])]
#[case::binary_local_local(vec![
        OpCode::BinaryLocalLocal { op: BinaryOp::Add, left: 0, right: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
#[case::update_local_local(vec![
        OpCode::UpdateLocalLocal { op: BinaryOp::Add, local: 0, value: 0 },
        OpCode::Return,
    ])]
#[case::return_binary_local_local(vec![OpCode::ReturnBinaryLocalLocal {
        op: BinaryOp::Add,
        left: 0,
        right: 0,
    }])]
#[case::binary_local_const(vec![
        OpCode::BinaryLocalConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
#[case::binary_local_number_const(vec![
        OpCode::BinaryLocalNumberConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
#[case::update_local_const(vec![
        OpCode::UpdateLocalConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Return,
    ])]
#[case::update_local_number_const(vec![
        OpCode::UpdateLocalNumberConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Return,
    ])]
#[case::jump_if_false_local_number_const(vec![
        OpCode::JumpIfFalseLocalNumberConst {
            op: BinaryOp::Eq,
            local: 0,
            constant: 0,
            offset: 1,
        },
        OpCode::Return,
        OpCode::Return,
    ])]
#[case::return_binary_local_const(vec![OpCode::ReturnBinaryLocalConst {
        op: BinaryOp::Add,
        local: 0,
        constant: 0,
    }])]
#[case::return_binary_local_number_const(vec![OpCode::ReturnBinaryLocalNumberConst {
        op: BinaryOp::Add,
        local: 0,
        constant: 0,
    }])]
#[case::array_get_local_at(vec![
        OpCode::ArrayGetLocalAt { array_slot: 0, index_slot: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
#[case::foreach_next(vec![
        OpCode::ForeachNext { array_slot: 0, index_slot: 0, value_slot: 0, exit_offset: 1 },
        OpCode::Return,
    ])]
#[case::take_local(vec![OpCode::TakeLocal(0), OpCode::Pop, OpCode::Return])]
#[case::move_local_source(vec![
        OpCode::MoveLocal { source: 0, destination: 0 },
        OpCode::Return,
    ])]
#[case::call_builtin_local(vec![
        OpCode::CallBuiltinLocal { func: native("len"), builtin: Ident::new("len"), local: 0 },
        OpCode::Return,
    ])]
fn verifier_rejects_out_of_bounds_local_slots(#[case] code: Vec<OpCode>) {
    let chunk = Chunk {
        code,
        local_count: 0,
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::LocalOutOfBounds { .. })
    ));
}

#[test]
fn verifier_rejects_out_of_bounds_param_binding_slot() {
    let chunk = Chunk {
        code: vec![OpCode::Return],
        local_count: 0,
        param_shape: ParamShape {
            bindings: vec![ParamBinding::Required(0)],
            required: 1,
            has_variadic: false,
        },
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::LocalOutOfBounds { .. })
    ));
}

#[test]
fn verifier_rejects_non_string_environment_name() {
    let chunk = Chunk {
        code: vec![OpCode::GetEnvVar(0), OpCode::Return],
        constants: vec![RuntimeValue::None],
        local_count: 1,
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::EnvNameNotString { index: 0, .. })
    ));
}

#[rstest]
#[case::zero(0)]
#[case::three(3)]
fn verifier_rejects_invalid_resume_arity(#[case] argc: u8) {
    let chunk = Chunk {
        code: vec![OpCode::PushNone, OpCode::PushNone, OpCode::Resume(argc), OpCode::Return],
        local_count: 1,
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::InvalidResumeArity { argc: actual, .. }) if actual == argc
    ));
}

#[rstest]
#[case::const_(vec![OpCode::Const(0), OpCode::Pop, OpCode::Return])]
#[case::get_env_var(vec![OpCode::GetEnvVar(0), OpCode::Pop, OpCode::Return])]
#[case::set_local_const(vec![OpCode::SetLocalConst { local: 0, constant: 0 }, OpCode::Return])]
#[case::binary_local_const(vec![
        OpCode::BinaryLocalConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
#[case::update_local_const(vec![
        OpCode::UpdateLocalConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Return,
    ])]
#[case::return_binary_local_const(vec![OpCode::ReturnBinaryLocalConst {
        op: BinaryOp::Add,
        local: 0,
        constant: 0,
    }])]
fn verifier_rejects_out_of_bounds_constant_index(#[case] code: Vec<OpCode>) {
    let chunk = Chunk {
        code,
        local_count: 1,
        constants: Vec::new(),
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::ConstantOutOfBounds { .. })
    ));
}

#[test]
fn verifier_rejects_values_that_exceed_u16_index_capacity() {
    let chunk = Chunk {
        code: vec![OpCode::Return],
        constants: vec![RuntimeValue::None; usize::from(u16::MAX) + 2],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::TooManyConstants { .. })
    ));
}

#[test]
fn verifier_rejects_invalid_closure_capture_and_try_slots() {
    let invalid_capture = Chunk {
        code: vec![
            OpCode::MakeClosure(Box::new((1, vec![UpvalueSource::Local(0)]))),
            OpCode::Return,
        ],
        ..Default::default()
    };
    let closure_target = Chunk {
        code: vec![OpCode::Return],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[invalid_capture, closure_target]),
        Err(BytecodeError::LocalOutOfBounds { .. })
    ));

    let invalid_try_slot = Chunk {
        code: vec![
            OpCode::TryCatch(Box::new(TryCatchInfo {
                has_binder: false,
                break_acc_slot: Some(0),
                break_completed_iteration_slot: None,
                break_offset: None,
                continue_offset: None,
            })),
            OpCode::Return,
        ],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[invalid_try_slot]),
        Err(BytecodeError::LocalOutOfBounds { .. })
    ));
}

#[test]
fn verifier_rejects_invalid_optional_parameter_default_references() {
    let invalid_capture = Chunk {
        code: vec![OpCode::Return],
        local_count: 1,
        param_shape: ParamShape {
            bindings: vec![ParamBinding::Optional(0, 1, vec![UpvalueSource::Local(1)])],
            required: 0,
            has_variadic: false,
        },
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[
            invalid_capture,
            Chunk {
                code: vec![OpCode::Return],
                ..Default::default()
            }
        ]),
        Err(BytecodeError::LocalOutOfBounds { .. })
    ));

    let invalid_target = Chunk {
        code: vec![OpCode::Return],
        local_count: 1,
        param_shape: ParamShape {
            bindings: vec![ParamBinding::Optional(0, 1, Vec::new())],
            required: 0,
            has_variadic: false,
        },
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[invalid_target]),
        Err(BytecodeError::ChunkOutOfBounds { .. })
    ));
}

#[test]
fn verifier_rejects_closure_capture_count_mismatches() {
    fn target_wants_one_upvalue() -> Chunk {
        Chunk {
            code: vec![OpCode::GetUpvalue(0), OpCode::Return],
            upvalue_names: vec![Ident::default()],
            ..Default::default()
        }
    }

    let no_captures = Chunk {
        code: vec![OpCode::MakeClosure(Box::new((1, Vec::new()))), OpCode::Return],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[no_captures, target_wants_one_upvalue()]),
        Err(BytecodeError::ClosureCaptureMismatch {
            expected: 1,
            actual: 0,
            ..
        })
    ));

    let too_many_captures = Chunk {
        code: vec![
            OpCode::MakeClosure(Box::new((1, vec![UpvalueSource::Local(0), UpvalueSource::Local(1)]))),
            OpCode::Return,
        ],
        local_count: 2,
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[too_many_captures, target_wants_one_upvalue()]),
        Err(BytecodeError::ClosureCaptureMismatch {
            expected: 1,
            actual: 2,
            ..
        })
    ));

    let static_closure_target_expects_upvalues = Chunk {
        code: vec![OpCode::MakeStaticClosure(0), OpCode::Return],
        static_closures: vec![Shared::new(Closure {
            chunk_index: 1,
            upvalues: None,
        })],
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[static_closure_target_expects_upvalues, target_wants_one_upvalue()]),
        Err(BytecodeError::ClosureCaptureMismatch {
            expected: 1,
            actual: 0,
            ..
        })
    ));

    let optional_default_mismatch = Chunk {
        code: vec![OpCode::Return],
        local_count: 1,
        param_shape: ParamShape {
            bindings: vec![ParamBinding::Optional(0, 1, Vec::new())],
            required: 0,
            has_variadic: false,
        },
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[optional_default_mismatch, target_wants_one_upvalue()]),
        Err(BytecodeError::ClosureCaptureMismatch {
            expected: 1,
            actual: 0,
            ..
        })
    ));
}

#[test]
fn verifier_rejects_a_generator_target_on_the_embedded_exact_call_fast_path() {
    let generator = Chunk {
        code: vec![OpCode::Const(0), OpCode::Yield, OpCode::Return],
        constants: vec![RuntimeValue::Number(1.into())],
        local_count: 1,
        is_generator: true,
        ..Default::default()
    };
    let caller = Chunk {
        code: vec![
            OpCode::CallStaticExact0(StaticExactCallTarget {
                chunk_index: 1,
                local_count: 1,
            }),
            OpCode::Return,
        ],
        local_count: 1,
        ..Default::default()
    };
    assert!(matches!(
        verify_chunks(&[caller, generator]),
        Err(BytecodeError::StaticCallTargetInvalid { .. })
    ));
}

#[rstest]
#[case::upvalue(0, 1, BytecodeError::UpvalueOutOfBounds { chunk: 0, pc: 0, index: 0 })]
#[case::local(1, 0, BytecodeError::LocalOutOfBounds { chunk: 0, pc: 0, slot: 0 })]
fn verifier_rejects_out_of_bounds_call_upvalue_local(
    #[case] upvalue_count: usize,
    #[case] local_count: u16,
    #[case] expected: BytecodeError,
) {
    let chunk = Chunk {
        code: vec![OpCode::CallUpvalueLocal { index: 0, local: 0 }, OpCode::Return],
        upvalue_names: vec![Ident::new("f"); upvalue_count],
        local_count,
        ..Default::default()
    };
    assert_eq!(verify_chunks(&[chunk]), Err(expected));
}

#[test]
fn verifier_accepts_a_generator_target_on_the_checked_static_call_path() {
    let generator = Chunk {
        code: vec![OpCode::Const(0), OpCode::Yield, OpCode::Return],
        constants: vec![RuntimeValue::Number(1.into())],
        local_count: 1,
        is_generator: true,
        ..Default::default()
    };
    let caller = Chunk {
        code: vec![OpCode::CallStatic(1, 0), OpCode::Return],
        local_count: 1,
        ..Default::default()
    };
    assert_eq!(verify_chunks(&[caller, generator]), Ok(()));
}

#[cfg(target_pointer_width = "64")]
#[test]
fn widened_call_counts_keep_opcode_size() {
    // Identifier-carrying instructions determine the enum's 64-bit layout.
    assert!(std::mem::size_of::<OpCode>() <= 16);
}

#[test]
fn verifier_rejects_loop_that_grows_the_stack() {
    let chunk = Chunk {
        code: vec![OpCode::PushNone, OpCode::Jump(-2), OpCode::PushNone, OpCode::Return],
        local_count: 1,
        ..Default::default()
    };

    assert!(matches!(
        verify_chunks(&[chunk]),
        Err(BytecodeError::StackOverflow { chunk: 0, .. })
    ));
}

#[test]
fn verifier_accepts_loop_that_keeps_the_stack_balanced() {
    let chunk = Chunk {
        code: vec![
            OpCode::PushNone,
            OpCode::Pop,
            OpCode::Jump(-3),
            OpCode::PushNone,
            OpCode::Return,
        ],
        local_count: 1,
        ..Default::default()
    };

    assert_eq!(verify_chunks(&[chunk]), Ok(()));
}
