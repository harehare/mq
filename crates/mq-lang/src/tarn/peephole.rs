//! Bytecode-level (peephole) rewrites, run per [`Chunk`] right after compilation.
//!
//! Fuses adjacent stack ops into superinstructions, drops dead `Push`/`Pop` pairs, and collapses
//! no-op jumps.

use super::bytecode::{
    Chunk, LineEntry, OpCode, ParamBinding, SELF_SLOT, StaticExactCallTarget, TryCatchInfo, UpvalueSource, jump_target,
};
use crate::ast::TokenId;
use crate::runtime::runtime_value::RuntimeValue;

/// Applies local bytecode rewrites.
pub(crate) fn optimize_chunks(chunks: &mut [Chunk]) {
    for chunk in chunks {
        optimize_chunk(chunk);
    }
}

/// Rewrites common capture-free exact static calls after local capture metadata is finalized.
pub(crate) fn specialize_static_exact_calls(chunks: &mut [Chunk]) {
    let targets: Vec<Option<StaticExactCallTarget>> = chunks
        .iter()
        .enumerate()
        .map(|(chunk_index, chunk)| {
            // A generator call must produce a coroutine, not run the chunk directly, so it can
            // never take this embedded-metadata fast path.
            (!chunk.captures_local_slots() && !chunk.is_generator).then_some(StaticExactCallTarget {
                chunk_index: chunk_index as u16,
                local_count: chunk.local_count,
            })
        })
        .collect();

    for chunk in chunks {
        for op in &mut chunk.code {
            let OpCode::CallStaticExact(chunk_index, argc) = op else {
                continue;
            };
            let Some(target) = targets.get(*chunk_index as usize).copied().flatten() else {
                continue;
            };
            *op = match *argc {
                0 => OpCode::CallStaticExact0(target),
                1 => OpCode::CallStaticExact1(target),
                2 => OpCode::CallStaticExact2(target),
                _ => continue,
            };
        }
    }
}

fn optimize_chunk(chunk: &mut Chunk) {
    if chunk.code.is_empty() {
        return;
    }

    let has_rewrite = chunk.code.iter().enumerate().any(|(pc, op)| {
        matches!(op, OpCode::BinaryLocalConst { constant, .. } if numeric_constant(&chunk.constants, *constant).is_some())
            || is_fusable_compare_jump(op, chunk.code.get(pc + 1))
            || {
                matches!(
                    (op, chunk.code.get(pc + 1)),
                    (OpCode::Const(_), Some(OpCode::Pop))
                        | (OpCode::Const(_), Some(OpCode::SetLocal(_)))
                        | (OpCode::GetLocal(_), Some(OpCode::SetLocal(_)))
                        | (OpCode::SetLocal(_), Some(OpCode::GetLocal(_)))
                        | (OpCode::SetLocalAndCopy { .. }, Some(OpCode::Jump(_)))
                        | (OpCode::BinaryLocalConst { .. }, Some(OpCode::SetLocal(_)))
                        | (OpCode::BinaryLocalLocal { .. }, Some(OpCode::SetLocal(_)))
                        | (OpCode::GetLocal(_), Some(OpCode::Return))
                        | (OpCode::BinaryLocalLocal { .. }, Some(OpCode::Return))
                        | (OpCode::BinaryLocalConst { .. }, Some(OpCode::Return))
                        | (OpCode::ForeachCollect(_), Some(OpCode::Jump(_)))
                        | (OpCode::Jump(0), _)
                )
            }
            || matches!(
                (op, chunk.code.get(pc + 1), chunk.code.get(pc + 2)),
                (OpCode::SetLocal(source), Some(OpCode::GetLocal(read)), Some(OpCode::SetLocal(_))) if source == read
            )
            || matches!(
                (op, chunk.code.get(pc + 1), chunk.code.get(pc + 2), chunk.code.get(pc + 3)),
                (OpCode::SetLocal(source), Some(OpCode::GetLocal(read)), Some(OpCode::SetLocal(_)), Some(OpCode::Jump(_))) if source == read
            )
    });
    if !has_rewrite {
        return;
    }

    let mut old_code = std::mem::take(&mut chunk.code);
    let old_lines = std::mem::take(&mut chunk.lines);
    let mut keep = vec![true; old_code.len()];
    let targets = jump_targets(&old_code);

    let mut pc = 0;
    while pc < old_code.len() {
        if let (
            OpCode::SetLocal(source),
            Some(OpCode::GetLocal(read)),
            Some(OpCode::SetLocal(destination)),
            Some(OpCode::Jump(offset)),
        ) = (
            &old_code[pc],
            old_code.get(pc + 1),
            old_code.get(pc + 2),
            old_code.get(pc + 3),
        ) && source == read
            && !targets.contains(&(pc + 1))
            && !targets.contains(&(pc + 2))
            && !targets.contains(&(pc + 3))
        {
            old_code[pc] = OpCode::SetLocalAndCopyAndJump {
                source: *source,
                destination: *destination,
                // The fused op moves three pcs before the old jump.
                offset: *offset + 3,
            };
            keep[pc + 1] = false;
            keep[pc + 2] = false;
            keep[pc + 3] = false;
            pc += 4;
            continue;
        }
        if let (
            OpCode::BinaryLocalConst { op, local, constant },
            Some(OpCode::ForeachCollect(accumulator_slot)),
            Some(OpCode::Jump(offset)),
        ) = (&old_code[pc], old_code.get(pc + 1), old_code.get(pc + 2))
            && let Some(constant) = numeric_constant(&chunk.constants, *constant)
            && !targets.contains(&(pc + 1))
            && !targets.contains(&(pc + 2))
        {
            old_code[pc] = OpCode::ForeachBinaryLocalNumberConstAndJump {
                op: *op,
                local: *local,
                constant,
                accumulator_slot: *accumulator_slot,
                // The fused op moves two pcs before the old jump.
                offset: *offset + 2,
            };
            keep[pc + 1] = false;
            keep[pc + 2] = false;
            pc += 3;
            continue;
        }
        if let (OpCode::SetLocal(source), Some(OpCode::GetLocal(read)), Some(OpCode::SetLocal(destination))) =
            (&old_code[pc], old_code.get(pc + 1), old_code.get(pc + 2))
            && source == read
            && !targets.contains(&(pc + 1))
            && !targets.contains(&(pc + 2))
        {
            old_code[pc] = OpCode::SetLocalAndCopy {
                source: *source,
                destination: *destination,
            };
            keep[pc + 1] = false;
            keep[pc + 2] = false;
            pc += 3;
            continue;
        }
        // A fused instruction stays at the first instruction's pc, so a branch that enters
        // there still observes the same combined operation. The second instruction must not be
        // a target: entering there can depend on an intermediate operand-stack value.
        match (&old_code[pc], old_code.get(pc + 1)) {
            (OpCode::SetLocalAndCopy { source, destination }, Some(OpCode::Jump(offset)))
                if !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = OpCode::SetLocalAndCopyAndJump {
                    source: *source,
                    destination: *destination,
                    // The fused op stays one pc before the old jump.
                    offset: *offset + 1,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::Const(_), Some(OpCode::Pop)) if !targets.contains(&pc) && !targets.contains(&(pc + 1)) => {
                keep[pc] = false;
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::Const(constant), Some(OpCode::SetLocal(local))) if !targets.contains(&(pc + 1)) => {
                old_code[pc] = OpCode::SetLocalConst {
                    local: *local,
                    constant: *constant,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::GetLocal(source), Some(OpCode::SetLocal(target)))
                if source == target && !targets.contains(&pc) && !targets.contains(&(pc + 1)) =>
            {
                keep[pc] = false;
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::GetLocal(source), Some(OpCode::SetLocal(destination))) if !targets.contains(&(pc + 1)) => {
                old_code[pc] = OpCode::CopyLocal {
                    source: *source,
                    destination: *destination,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::SetLocal(set_slot), Some(OpCode::GetLocal(get_slot)))
                if set_slot == get_slot && !targets.contains(&(pc + 1)) =>
            {
                let slot = *set_slot;
                old_code[pc] = OpCode::TeeLocal(slot);
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::BinaryLocalConst { op, local, constant }, Some(OpCode::SetLocal(destination)))
                if local == destination && !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = numeric_constant(&chunk.constants, *constant).map_or(
                    OpCode::UpdateLocalConst {
                        op: *op,
                        local: *local,
                        constant: *constant,
                    },
                    |constant| OpCode::UpdateLocalNumberConst {
                        op: *op,
                        local: *local,
                        constant,
                    },
                );
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::BinaryLocalLocal { op, left, right }, Some(OpCode::SetLocal(destination)))
                if left == destination && !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = OpCode::UpdateLocalLocal {
                    op: *op,
                    local: *left,
                    value: *right,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::GetLocal(slot), Some(OpCode::Return)) if !targets.contains(&(pc + 1)) => {
                old_code[pc] = OpCode::ReturnLocal(*slot);
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::BinaryLocalLocal { op, left, right }, Some(OpCode::Return)) if !targets.contains(&(pc + 1)) => {
                old_code[pc] = OpCode::ReturnBinaryLocalLocal {
                    op: *op,
                    left: *left,
                    right: *right,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::BinaryLocalConst { op, local, constant }, Some(OpCode::Return))
                if !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = numeric_constant(&chunk.constants, *constant).map_or(
                    OpCode::ReturnBinaryLocalConst {
                        op: *op,
                        local: *local,
                        constant: *constant,
                    },
                    |constant| OpCode::ReturnBinaryLocalNumberConst {
                        op: *op,
                        local: *local,
                        constant,
                    },
                );
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::Jump(0), _) => {
                keep[pc] = false;
                pc += 1;
            }
            (OpCode::BinaryLocalLocal { op, left, right }, Some(OpCode::JumpIfFalse(offset)))
                if op.is_comparison() && !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = OpCode::JumpIfFalseLocalLocal {
                    op: *op,
                    left: *left,
                    right: *right,
                    // The fused op keeps the `BinaryLocalLocal`'s old pc, one slot earlier than
                    // the `JumpIfFalse` this offset was written for; +1 keeps the same target.
                    offset: *offset + 1,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::BinaryLocalConst { op, local, constant }, Some(OpCode::JumpIfFalse(offset)))
                if op.is_comparison() && !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = numeric_constant(&chunk.constants, *constant).map_or(
                    OpCode::JumpIfFalseLocalConst {
                        op: *op,
                        local: *local,
                        constant: *constant,
                        offset: *offset + 1,
                    },
                    |constant| OpCode::JumpIfFalseLocalNumberConst {
                        op: *op,
                        local: *local,
                        constant,
                        offset: *offset + 1,
                    },
                );
                keep[pc + 1] = false;
                pc += 2;
            }
            (OpCode::ForeachCollect(slot), Some(OpCode::Jump(offset))) if !targets.contains(&(pc + 1)) => {
                old_code[pc] = OpCode::ForeachCollectAndJump {
                    slot: *slot,
                    // The fused instruction stays one pc earlier than the old jump.
                    offset: *offset + 1,
                };
                keep[pc + 1] = false;
                pc += 2;
            }
            _ => {
                if let OpCode::BinaryLocalConst { op, local, constant } = &old_code[pc]
                    && let Some(constant) = numeric_constant(&chunk.constants, *constant)
                {
                    old_code[pc] = OpCode::BinaryLocalNumberConst {
                        op: *op,
                        local: *local,
                        constant,
                    };
                }
                pc += 1;
            }
        }
    }

    compact(chunk, old_code, &old_lines, &keep);
}

/// Drops closures stored into slots nothing reads, and those slots. Invalid when locals are
/// read by name after the run; `seeded` slots are filled by position, so they keep their numbers.
pub(crate) fn drop_unread_static_closures(chunks: &mut [Chunk], seeded: usize) {
    for (index, chunk) in chunks.iter_mut().enumerate() {
        drop_unread_static_closures_in(chunk, if index == 0 { seeded } else { 0 });
    }
}

fn drop_unread_static_closures_in(chunk: &mut Chunk, seeded: usize) {
    let mut uses = vec![0u32; chunk.local_count as usize];
    let mut count = |slot: &mut u16| {
        if let Some(uses) = uses.get_mut(*slot as usize) {
            *uses += 1;
        }
    };
    for op in &mut chunk.code {
        op.for_each_local_slot_mut(&mut count);
    }
    for_each_param_slot_mut(chunk, &mut count);

    let targets = jump_targets(&chunk.code);
    let mut keep = vec![true; chunk.code.len()];
    let mut dead = vec![false; chunk.local_count as usize];
    for pc in 0..chunk.code.len().saturating_sub(1) {
        if let (OpCode::MakeStaticClosure(_), OpCode::SetLocal(slot)) = (&chunk.code[pc], &chunk.code[pc + 1])
            && *slot != SELF_SLOT
            && uses.get(*slot as usize) == Some(&1)
            && !targets.contains(&(pc + 1))
        {
            keep[pc] = false;
            keep[pc + 1] = false;
            let is_seeded = usize::from(*slot) <= usize::from(SELF_SLOT) + seeded;
            dead[*slot as usize] = !is_seeded;
        }
    }
    if keep.iter().all(|keep| *keep) {
        return;
    }
    let old_code = std::mem::take(&mut chunk.code);
    let old_lines = std::mem::take(&mut chunk.lines);
    compact(chunk, old_code, &old_lines, &keep);

    if dead.iter().any(|dead| *dead) {
        remove_local_slots(chunk, &dead);
    }
}

/// Keeps slot order, so `self` and parameters keep their numbers.
fn remove_local_slots(chunk: &mut Chunk, dead: &[bool]) {
    let mut renumbered = Vec::with_capacity(dead.len());
    let mut next = 0u16;
    for dead in dead {
        renumbered.push(next);
        if !dead {
            next += 1;
        }
    }
    let mut renumber = |slot: &mut u16| *slot = renumbered[*slot as usize];
    for op in &mut chunk.code {
        op.for_each_local_slot_mut(&mut renumber);
    }
    for_each_param_slot_mut(chunk, &mut renumber);
    retain_live(&mut chunk.local_names, dead);
    retain_live(&mut chunk.local_mutable, dead);
    chunk.local_count = next;
}

fn retain_live<T>(values: &mut Vec<T>, dead: &[bool]) {
    let mut dead = dead.iter();
    values.retain(|_| !dead.next().copied().unwrap_or(false));
}

fn for_each_param_slot_mut(chunk: &mut Chunk, visit: &mut impl FnMut(&mut u16)) {
    for binding in &mut chunk.param_shape.bindings {
        match binding {
            ParamBinding::Required(slot) | ParamBinding::Variadic(slot) => visit(slot),
            ParamBinding::Optional(slot, _, sources) => {
                visit(slot);
                for source in sources {
                    if let UpvalueSource::Local(slot) = source {
                        visit(slot);
                    }
                }
            }
        }
    }
}

fn compact(chunk: &mut Chunk, old_code: Vec<OpCode>, old_lines: &[LineEntry], keep: &[bool]) {
    let old_to_new = old_to_new_pc_map(keep);
    let mut new_code = Vec::with_capacity(old_code.len());
    let mut new_lines: Vec<LineEntry> = Vec::with_capacity(old_lines.len());
    for (old_pc, op) in old_code.into_iter().enumerate() {
        if !keep[old_pc] {
            continue;
        }
        let new_pc = new_code.len();
        let token_id = token_at(old_lines, old_pc);
        if new_lines.last().map(|entry| entry.token_id) != Some(token_id) {
            new_lines.push(LineEntry {
                pc_start: new_pc,
                token_id,
            });
        }
        new_code.push(rewrite_targets(op, old_pc, new_pc, &old_to_new));
    }
    chunk.code = new_code;
    chunk.lines = new_lines;
}

fn numeric_constant(constants: &[RuntimeValue], index: u16) -> Option<i32> {
    match constants.get(index as usize) {
        Some(RuntimeValue::Number(value)) => {
            let number = value.value();
            let integer = number as i32;
            (number == integer as f64 && !(number == 0.0 && number.is_sign_negative())).then_some(integer)
        }
        _ => None,
    }
}

/// Whether `op` immediately followed by `next` is a comparison feeding a plain `JumpIfFalse`,
/// the shape every `if`/`while`/`until` condition compiles to, and so can fuse into a single
/// compare-and-branch instruction with no boolean ever pushed to the operand stack.
fn is_fusable_compare_jump(op: &OpCode, next: Option<&OpCode>) -> bool {
    let Some(OpCode::JumpIfFalse(_)) = next else {
        return false;
    };
    match op {
        OpCode::BinaryLocalLocal { op, .. } | OpCode::BinaryLocalConst { op, .. } => op.is_comparison(),
        _ => false,
    }
}

fn jump_targets(code: &[OpCode]) -> std::collections::BTreeSet<usize> {
    let mut targets = std::collections::BTreeSet::new();
    for (pc, op) in code.iter().enumerate() {
        match op {
            OpCode::Jump(offset)
            | OpCode::JumpIfFalse(offset)
            | OpCode::JumpIfFalseLocalLocal { offset, .. }
            | OpCode::JumpIfFalseLocalConst { offset, .. }
            | OpCode::JumpIfFalseLocalNumberConst { offset, .. }
            | OpCode::SetLocalAndCopyAndJump { offset, .. }
            | OpCode::ForeachCollectAndJump { offset, .. }
            | OpCode::ForeachBinaryLocalNumberConstAndJump { offset, .. } => {
                if let Some(target) = jump_target(pc, *offset) {
                    targets.insert(target);
                }
            }
            OpCode::ForeachNext { exit_offset, .. } => {
                if let Some(target) = jump_target(pc, *exit_offset) {
                    targets.insert(target);
                }
            }
            OpCode::TryCatch(info) => {
                if let Some(offset) = info.break_offset
                    && let Some(target) = jump_target(pc, offset)
                {
                    targets.insert(target);
                }
                if let Some(offset) = info.continue_offset
                    && let Some(target) = jump_target(pc, offset)
                {
                    targets.insert(target);
                }
            }
            _ => {}
        }
    }
    targets
}

fn old_to_new_pc_map(keep: &[bool]) -> Vec<usize> {
    let mut map = vec![0; keep.len() + 1];
    let mut next = keep.iter().filter(|keep| **keep).count();
    map[keep.len()] = next;
    for pc in (0..keep.len()).rev() {
        if keep[pc] {
            next -= 1;
        }
        map[pc] = next;
    }
    map
}

fn token_at(lines: &[LineEntry], pc: usize) -> TokenId {
    lines
        .partition_point(|entry| entry.pc_start <= pc)
        .checked_sub(1)
        .map(|index| lines[index].token_id)
        .unwrap_or_else(|| TokenId::new(0))
}

fn rewrite_targets(op: OpCode, old_pc: usize, new_pc: usize, map: &[usize]) -> OpCode {
    let rewrite = |offset: i32| {
        let old_target = jump_target(old_pc, offset).expect("compiler-generated jump must not underflow");
        (map[old_target] as i32) - (new_pc as i32) - 1
    };
    match op {
        OpCode::Jump(offset) => OpCode::Jump(rewrite(offset)),
        OpCode::JumpIfFalse(offset) => OpCode::JumpIfFalse(rewrite(offset)),
        OpCode::JumpIfFalseLocalLocal {
            op,
            left,
            right,
            offset,
        } => OpCode::JumpIfFalseLocalLocal {
            op,
            left,
            right,
            offset: rewrite(offset),
        },
        OpCode::JumpIfFalseLocalConst {
            op,
            local,
            constant,
            offset,
        } => OpCode::JumpIfFalseLocalConst {
            op,
            local,
            constant,
            offset: rewrite(offset),
        },
        OpCode::JumpIfFalseLocalNumberConst {
            op,
            local,
            constant,
            offset,
        } => OpCode::JumpIfFalseLocalNumberConst {
            op,
            local,
            constant,
            offset: rewrite(offset),
        },
        OpCode::SetLocalAndCopyAndJump {
            source,
            destination,
            offset,
        } => OpCode::SetLocalAndCopyAndJump {
            source,
            destination,
            offset: rewrite(offset),
        },
        OpCode::ForeachNext {
            array_slot,
            index_slot,
            value_slot,
            exit_offset,
        } => OpCode::ForeachNext {
            array_slot,
            index_slot,
            value_slot,
            exit_offset: rewrite(exit_offset),
        },
        OpCode::ForeachCollectAndJump { slot, offset } => OpCode::ForeachCollectAndJump {
            slot,
            offset: rewrite(offset),
        },
        OpCode::ForeachBinaryLocalNumberConstAndJump {
            op,
            local,
            constant,
            accumulator_slot,
            offset,
        } => OpCode::ForeachBinaryLocalNumberConstAndJump {
            op,
            local,
            constant,
            accumulator_slot,
            offset: rewrite(offset),
        },
        OpCode::TryCatch(info) => OpCode::TryCatch(Box::new(TryCatchInfo {
            break_offset: info.break_offset.map(rewrite),
            continue_offset: info.continue_offset.map(rewrite),
            ..*info
        })),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::super::bytecode::{BinaryOp, ParamShape, verify_chunks};
    use super::*;
    use crate::runtime::runtime_value::RuntimeValue;

    #[test]
    fn peephole_removes_unused_constants_local_moves_and_empty_jumps() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::Const(0),
                OpCode::Pop,
                OpCode::GetLocal(0),
                OpCode::SetLocal(0),
                OpCode::Jump(0),
                OpCode::PushNone,
                OpCode::Return,
            ],
            constants: vec![RuntimeValue::Number(1.into())],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(chunk.code.as_slice(), [OpCode::PushNone, OpCode::Return]));
    }

    #[test]
    fn peephole_fuses_constant_assignment_and_local_return() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::Const(0),
                OpCode::SetLocal(0),
                OpCode::GetLocal(0),
                OpCode::Return,
            ],
            constants: vec![RuntimeValue::Number(1.into())],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [OpCode::SetLocalConst { local: 0, constant: 0 }, OpCode::ReturnLocal(0)]
        ));
    }

    #[test]
    fn peephole_keeps_a_constant_assignment_when_the_store_is_a_jump_target() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::Const(0),
                OpCode::Jump(1),
                OpCode::Const(0),
                OpCode::SetLocal(0),
                OpCode::GetLocal(0),
                OpCode::Return,
            ],
            constants: vec![RuntimeValue::Number(1.into())],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(!chunk.code.iter().any(|op| matches!(op, OpCode::SetLocalConst { .. })));
    }

    #[test]
    fn peephole_returns_a_local_without_using_the_operand_stack() {
        let mut chunk = Chunk {
            code: vec![OpCode::GetLocal(0), OpCode::Return],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(chunk.code.as_slice(), [OpCode::ReturnLocal(0)]));
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[test]
    fn peephole_returns_a_local_constant_binary_expression_without_using_the_operand_stack() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::BinaryLocalConst {
                    op: BinaryOp::Mul,
                    local: 0,
                    constant: 0,
                },
                OpCode::Return,
            ],
            constants: vec![RuntimeValue::Number(2.into())],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [OpCode::ReturnBinaryLocalNumberConst {
                op: BinaryOp::Mul,
                local: 0,
                constant: _,
            }]
        ));
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[rstest::rstest]
    #[case::number(RuntimeValue::Number(2.into()), true)]
    #[case::negative_integer(RuntimeValue::Number((-2).into()), true)]
    #[case::fraction(RuntimeValue::Number(crate::number::Number::new(2.5)), false)]
    #[case::negative_zero(RuntimeValue::Number(crate::number::Number::new(-0.0)), false)]
    #[case::out_of_range(RuntimeValue::Number(crate::number::Number::new(i32::MAX as f64 + 1.0)), false)]
    #[case::string(RuntimeValue::String(crate::Shared::new("two".to_string())), false)]
    fn peephole_keeps_numeric_local_constant_fusion(#[case] constant: RuntimeValue, #[case] numeric: bool) {
        let mut chunk = Chunk {
            code: vec![
                OpCode::BinaryLocalConst {
                    op: BinaryOp::Add,
                    local: 0,
                    constant: 0,
                },
                OpCode::SetLocal(0),
                OpCode::ReturnLocal(0),
            ],
            constants: vec![constant],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert_eq!(
            matches!(chunk.code.first(), Some(OpCode::UpdateLocalNumberConst { .. })),
            numeric
        );
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[test]
    fn peephole_keeps_a_return_target_that_needs_its_operand() {
        let mut chunk = Chunk {
            code: vec![OpCode::Jump(1), OpCode::GetLocal(0), OpCode::Return],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [OpCode::Jump(1), OpCode::GetLocal(0), OpCode::Return]
        ));
    }

    #[test]
    fn peephole_keeps_a_set_local_get_local_pair_when_its_second_instruction_is_a_jump_target() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::JumpIfFalse(1),
                OpCode::SetLocal(0),
                OpCode::GetLocal(0),
                OpCode::Return,
            ],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [OpCode::JumpIfFalse(1), OpCode::SetLocal(0), OpCode::ReturnLocal(0),]
        ));
    }

    #[test]
    fn peephole_fuses_local_copy_and_local_return_at_jump_targets() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::Jump(2),
                OpCode::GetLocal(0),
                OpCode::SetLocal(1),
                OpCode::GetLocal(1),
                OpCode::Return,
            ],
            local_count: 2,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [
                OpCode::Jump(1),
                OpCode::CopyLocal {
                    source: 0,
                    destination: 1,
                },
                OpCode::ReturnLocal(1),
            ]
        ));
    }

    #[test]
    fn peephole_fuses_a_loop_header_comparison() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::BinaryLocalConst {
                    op: BinaryOp::Gt,
                    local: 0,
                    constant: 0,
                },
                OpCode::JumpIfFalse(2),
                // This backedge targets the comparison at pc 0. The fused instruction remains
                // at pc 0, so the backedge must not inhibit fusion.
                OpCode::Jump(-3),
                OpCode::Jump(1),
                OpCode::PushNone,
                OpCode::Return,
            ],
            constants: vec![RuntimeValue::Number(0.into())],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [
                OpCode::JumpIfFalseLocalNumberConst {
                    op: BinaryOp::Gt,
                    local: 0,
                    constant: _,
                    offset: 2,
                },
                OpCode::Jump(-2),
                OpCode::Jump(1),
                OpCode::PushNone,
                OpCode::Return,
            ]
        ));
    }

    #[test]
    fn peephole_fuses_a_store_followed_by_a_local_copy() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::PushNone,
                OpCode::SetLocal(0),
                OpCode::GetLocal(0),
                OpCode::SetLocal(1),
                OpCode::GetLocal(1),
                OpCode::Return,
            ],
            local_count: 2,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [
                OpCode::PushNone,
                OpCode::SetLocalAndCopy {
                    source: 0,
                    destination: 1,
                },
                OpCode::ReturnLocal(1),
            ]
        ));
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[test]
    fn peephole_fuses_a_store_copy_loop_backedge() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::PushNone,
                OpCode::SetLocal(0),
                OpCode::GetLocal(0),
                OpCode::SetLocal(1),
                OpCode::Jump(-5),
                OpCode::Return,
            ],
            local_count: 2,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [
                OpCode::PushNone,
                OpCode::SetLocalAndCopyAndJump {
                    source: 0,
                    destination: 1,
                    offset: -2,
                },
                OpCode::Return,
            ]
        ));
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[test]
    fn peephole_fuses_a_foreach_collect_backedge() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::PushNone,
                OpCode::ForeachCollect(0),
                OpCode::Jump(-3),
                OpCode::Return,
            ],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [
                OpCode::PushNone,
                OpCode::ForeachCollectAndJump { slot: 0, offset: -2 },
                OpCode::Return,
            ]
        ));
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[rstest::rstest]
    #[case::number(RuntimeValue::Number(1.into()), true)]
    #[case::string(RuntimeValue::String(crate::Shared::new("item".to_string())), false)]
    fn peephole_inlines_numeric_foreach_body_constants(#[case] constant: RuntimeValue, #[case] inline: bool) {
        let mut chunk = Chunk {
            code: vec![
                OpCode::BinaryLocalConst {
                    op: BinaryOp::Add,
                    local: 0,
                    constant: 0,
                },
                OpCode::ForeachCollect(1),
                OpCode::Jump(-3),
                OpCode::Return,
            ],
            constants: vec![constant],
            local_count: 2,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert_eq!(
            matches!(
                chunk.code.first(),
                Some(OpCode::ForeachBinaryLocalNumberConstAndJump {
                    op: BinaryOp::Add,
                    local: 0,
                    accumulator_slot: 1,
                    offset: -1,
                    ..
                })
            ),
            inline
        );
        assert_eq!(verify_chunks(&[chunk]), Ok(()));
    }

    #[test]
    fn peephole_rewrites_try_catch_offsets_past_removed_dead_code() {
        let mut chunk = Chunk {
            code: vec![
                OpCode::Const(0),
                OpCode::Pop,
                OpCode::TryCatch(Box::new(TryCatchInfo {
                    has_binder: false,
                    break_acc_slot: None,
                    break_completed_iteration_slot: None,
                    break_offset: Some(0),
                    continue_offset: Some(1),
                })),
                OpCode::PushNone,
                OpCode::PushNone,
                OpCode::Return,
            ],
            constants: vec![RuntimeValue::Number(1.into())],
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert!(matches!(
            chunk.code.as_slice(),
            [
                OpCode::TryCatch(info),
                OpCode::PushNone,
                OpCode::PushNone,
                OpCode::Return,
            ] if info.break_offset == Some(0) && info.continue_offset == Some(1)
        ));
    }

    /// `chunks[0]` runs `code`; `chunks[1]` is the `def` its static closure points at.
    fn def_program(code: Vec<OpCode>, local_count: u16) -> Vec<Chunk> {
        let mut main = Chunk {
            code,
            local_count,
            local_names: (0..local_count)
                .map(|slot| crate::Ident::new(&format!("l{slot}")))
                .collect(),
            local_mutable: vec![false; local_count as usize],
            ..Default::default()
        };
        main.push_static_closure(1);
        let function = Chunk {
            code: vec![OpCode::PushNone, OpCode::Return],
            local_count: 1,
            ..Default::default()
        };
        vec![main, function]
    }

    fn slot_names(chunk: &Chunk) -> Vec<String> {
        chunk.local_names.iter().map(|name| name.as_str()).collect()
    }

    #[rstest::rstest]
    #[case::unread(
        vec![OpCode::MakeStaticClosure(0), OpCode::SetLocal(1), OpCode::GetLocal(0), OpCode::Return],
        2,
        vec![OpCode::GetLocal(0), OpCode::Return],
        &["l0"],
    )]
    #[case::later_slots_renumbered(
        vec![
            OpCode::MakeStaticClosure(0),
            OpCode::SetLocal(1),
            OpCode::PushNone,
            OpCode::SetLocal(2),
            OpCode::GetLocal(2),
            OpCode::Return,
        ],
        3,
        vec![OpCode::PushNone, OpCode::SetLocal(1), OpCode::GetLocal(1), OpCode::Return],
        &["l0", "l2"],
    )]
    #[case::jump_over_the_pair_retargeted(
        vec![
            OpCode::Jump(2),
            OpCode::MakeStaticClosure(0),
            OpCode::SetLocal(1),
            OpCode::GetLocal(0),
            OpCode::Return,
        ],
        2,
        vec![OpCode::Jump(0), OpCode::GetLocal(0), OpCode::Return],
        &["l0"],
    )]
    #[case::several_defs(
        vec![
            OpCode::MakeStaticClosure(0),
            OpCode::SetLocal(1),
            OpCode::MakeStaticClosure(0),
            OpCode::SetLocal(2),
            OpCode::MakeStaticClosure(0),
            OpCode::SetLocal(3),
            OpCode::GetLocal(2),
            OpCode::Return,
        ],
        4,
        vec![OpCode::MakeStaticClosure(0), OpCode::SetLocal(1), OpCode::GetLocal(1), OpCode::Return],
        &["l0", "l2"],
    )]
    fn unread_static_closures_are_dropped(
        #[case] code: Vec<OpCode>,
        #[case] local_count: u16,
        #[case] expected: Vec<OpCode>,
        #[case] names: &[&str],
    ) {
        let mut chunks = def_program(code, local_count);
        drop_unread_static_closures(&mut chunks, 0);
        assert_eq!(format!("{:?}", chunks[0].code), format!("{expected:?}"));
        assert_eq!(chunks[0].local_count as usize, names.len());
        assert_eq!(slot_names(&chunks[0]), names);
        assert_eq!(chunks[0].local_mutable.len(), names.len());
        assert_eq!(verify_chunks(&chunks), Ok(()));
    }

    #[rstest::rstest]
    #[case::read_later(vec![OpCode::MakeStaticClosure(0), OpCode::SetLocal(1), OpCode::GetLocal(1), OpCode::Return])]
    #[case::called_through_the_slot(vec![
        OpCode::MakeStaticClosure(0),
        OpCode::SetLocal(1),
        OpCode::CallLocal(1, 0),
        OpCode::Return,
    ])]
    #[case::captured(vec![
        OpCode::MakeStaticClosure(0),
        OpCode::SetLocal(1),
        OpCode::MakeClosure(Box::new((1, vec![UpvalueSource::Local(1)]))),
        OpCode::Return,
    ])]
    #[case::redefined(vec![
        OpCode::MakeStaticClosure(0),
        OpCode::SetLocal(1),
        OpCode::MakeStaticClosure(0),
        OpCode::SetLocal(1),
        OpCode::GetLocal(0),
        OpCode::Return,
    ])]
    #[case::store_is_a_jump_target(vec![
        OpCode::PushNone,
        OpCode::Jump(1),
        OpCode::MakeStaticClosure(0),
        OpCode::SetLocal(1),
        OpCode::GetLocal(0),
        OpCode::Return,
    ])]
    #[case::self_slot(vec![OpCode::MakeStaticClosure(0), OpCode::SetLocal(0), OpCode::GetLocal(0), OpCode::Return])]
    #[case::other_value_stored(vec![OpCode::PushNone, OpCode::SetLocal(1), OpCode::GetLocal(0), OpCode::Return])]
    fn read_or_unsafe_stores_are_kept(#[case] code: Vec<OpCode>) {
        let mut chunks = def_program(code.clone(), 2);
        drop_unread_static_closures(&mut chunks, 0);
        assert_eq!(format!("{:?}", chunks[0].code), format!("{code:?}"));
        assert_eq!(chunks[0].local_count, 2);
    }

    #[test]
    fn seeded_slots_keep_their_numbers() {
        let mut chunks = def_program(
            vec![
                OpCode::MakeStaticClosure(0),
                OpCode::SetLocal(1),
                OpCode::GetLocal(2),
                OpCode::Return,
            ],
            3,
        );
        drop_unread_static_closures(&mut chunks, 1);
        assert_eq!(
            format!("{:?}", chunks[0].code),
            format!("{:?}", vec![OpCode::GetLocal(2), OpCode::Return])
        );
        assert_eq!(chunks[0].local_count, 3);
    }

    #[test]
    fn parameter_slots_and_default_captures_follow_renumbering() {
        let mut chunks = def_program(
            vec![
                OpCode::MakeStaticClosure(0),
                OpCode::SetLocal(3),
                OpCode::GetLocal(4),
                OpCode::Return,
            ],
            5,
        );
        chunks[0].param_shape = ParamShape {
            bindings: vec![
                ParamBinding::Required(1),
                ParamBinding::Optional(2, 1, vec![UpvalueSource::Local(1)]),
            ],
            required: 1,
            has_variadic: false,
        };
        drop_unread_static_closures(&mut chunks, 0);
        assert_eq!(
            format!("{:?}", chunks[0].code),
            format!("{:?}", vec![OpCode::GetLocal(3), OpCode::Return])
        );
        assert_eq!(slot_names(&chunks[0]), ["l0", "l1", "l2", "l4"]);
        assert!(matches!(
            chunks[0].param_shape.bindings.as_slice(),
            [ParamBinding::Required(1), ParamBinding::Optional(2, 1, sources)]
                if matches!(sources.as_slice(), [UpvalueSource::Local(1)])
        ));
    }

    #[test]
    fn source_lines_follow_removed_instructions() {
        let token = |id| crate::ast::TokenId::new(id);
        let mut chunks = def_program(
            vec![
                OpCode::MakeStaticClosure(0),
                OpCode::SetLocal(1),
                OpCode::GetLocal(0),
                OpCode::Return,
            ],
            2,
        );
        chunks[0].lines = vec![
            LineEntry {
                pc_start: 0,
                token_id: token(1),
            },
            LineEntry {
                pc_start: 2,
                token_id: token(2),
            },
        ];
        drop_unread_static_closures(&mut chunks, 0);
        assert!(matches!(
            chunks[0].lines.as_slice(),
            [LineEntry { pc_start: 0, token_id }] if *token_id == token(2)
        ));
    }
}
