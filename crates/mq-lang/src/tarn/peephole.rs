//! Bytecode-level (peephole) rewrites, run per [`Chunk`] right after compilation.
//!
//! The bytecode/IR counterpart to the AST-to-AST passes in [`crate::optimizer`]: fuses adjacent
//! stack ops into superinstructions, drops dead `Push`/`Pop` pairs, and collapses no-op jumps.

use super::bytecode::{Chunk, LineEntry, OpCode, StaticExactCallTarget, TryCatchInfo, jump_target};
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

    let old_to_new = old_to_new_pc_map(&keep);
    let mut new_code = Vec::with_capacity(old_code.len());
    let mut new_lines: Vec<LineEntry> = Vec::with_capacity(old_lines.len());
    for (old_pc, op) in old_code.into_iter().enumerate() {
        if !keep[old_pc] {
            continue;
        }
        let new_pc = new_code.len();
        let token_id = token_at(&old_lines, old_pc);
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

fn numeric_constant(constants: &[RuntimeValue], index: u16) -> Option<crate::number::Number> {
    match constants.get(index as usize) {
        Some(RuntimeValue::Number(number)) => Some(*number),
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
    use super::super::bytecode::{BinaryOp, verify_chunks};
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
    #[case::string(RuntimeValue::String(crate::Shared::new("two".to_string())), false)]
    fn peephole_inlines_only_numeric_local_constants(#[case] constant: RuntimeValue, #[case] inline: bool) {
        let mut chunk = Chunk {
            code: vec![
                OpCode::BinaryLocalConst {
                    op: BinaryOp::Add,
                    local: 0,
                    constant: 0,
                },
                OpCode::SetLocal(0),
                OpCode::Return,
            ],
            constants: vec![constant],
            local_count: 1,
            ..Default::default()
        };

        optimize_chunk(&mut chunk);

        assert_eq!(
            matches!(chunk.code.first(), Some(OpCode::UpdateLocalNumberConst { .. })),
            inline
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
}
