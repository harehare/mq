//! Turns the last read of a local into a move.
//!
//! A `GetLocal` clones the value, so a callee that receives an array through a local keeps
//! the caller's copy alive and every in-place update copies the whole array. When no later
//! instruction can read the slot before it is overwritten, the read becomes a move that hands
//! the value over and leaves `None` behind: `GetLocal` becomes `TakeLocal`, `CopyLocal` becomes
//! `MoveLocal`, and a fused `CallUpvalueLocal` argument is flagged `take`.

use crate::tarn::bytecode::{Chunk, OpCode, SELF_SLOT, jump_target};

/// Rewrites last-use reads in every chunk into moves.
pub(crate) fn move_last_uses(chunks: &mut [Chunk]) {
    for chunk in chunks {
        move_last_uses_in(chunk);
    }
}

fn move_last_uses_in(chunk: &mut Chunk) {
    let candidates = candidate_slots(chunk);
    if candidates.is_empty() {
        return;
    }

    let (reads, writes) = local_effects(chunk, &candidates);
    let live_in = liveness(chunk, &reads, &writes);

    for pc in 0..chunk.code.len() {
        let (slot, moved) = match chunk.code[pc] {
            OpCode::GetLocal(slot) => (slot, OpCode::TakeLocal(slot)),
            OpCode::CopyLocal { source, destination } if source != destination => {
                (source, OpCode::MoveLocal { source, destination })
            }
            OpCode::CallUpvalueLocal {
                index,
                local,
                take: false,
            } => (
                local,
                OpCode::CallUpvalueLocal {
                    index,
                    local,
                    take: true,
                },
            ),
            _ => continue,
        };
        if let Ok(bit) = candidates.binary_search(&slot)
            && !live_after(chunk, &live_in, pc, bit)
        {
            chunk.code[pc] = moved;
        }
    }
}

/// Slots a move could never be observed through: read by `GetLocal` or `CopyLocal`, and neither
/// `self` nor shared with a closure.
fn candidate_slots(chunk: &Chunk) -> Vec<u16> {
    let mut slots: Vec<u16> = chunk
        .code
        .iter()
        .filter_map(|op| match op {
            OpCode::GetLocal(slot)
            | OpCode::CopyLocal { source: slot, .. }
            | OpCode::CallUpvalueLocal {
                local: slot,
                take: false,
                ..
            } if *slot != SELF_SLOT && !chunk.captured_local_slots().contains(slot) => Some(*slot),
            _ => None,
        })
        .collect();
    slots.sort_unstable();
    slots.dedup();
    slots
}

/// The candidate slots each instruction reads and the ones it fully overwrites.
///
/// An operand of any other instruction counts as a read, which keeps the slot live longer and
/// so can only suppress a move.
fn local_effects(chunk: &Chunk, candidates: &[u16]) -> (BitRows, BitRows) {
    let bit_of = |slot: u16| candidates.binary_search(&slot).ok();
    let mut reads = BitRows::new(chunk.code.len(), candidates.len());
    let mut writes = BitRows::new(chunk.code.len(), candidates.len());
    let mut operands = Vec::new();

    for (pc, op) in chunk.code.iter().enumerate() {
        match op {
            OpCode::SetLocal(slot) | OpCode::SetLocalConst { local: slot, .. } => {
                if let Some(bit) = bit_of(*slot) {
                    writes.insert(pc, bit);
                }
            }
            OpCode::CopyLocal { source, destination } => {
                if let Some(bit) = bit_of(*source) {
                    reads.insert(pc, bit);
                }
                if let Some(bit) = bit_of(*destination) {
                    writes.insert(pc, bit);
                }
            }
            _ => {
                operands.clear();
                op.clone().for_each_local_slot_mut(|slot| operands.push(*slot));
                for bit in operands.iter().filter_map(|slot| bit_of(*slot)) {
                    reads.insert(pc, bit);
                }
            }
        }
    }
    (reads, writes)
}

/// Backward dataflow: the candidate slots live on entry to each instruction.
fn liveness(chunk: &Chunk, reads: &BitRows, writes: &BitRows) -> BitRows {
    let mut live_in = BitRows::new(chunk.code.len(), reads.bits);
    let mut successors = Vec::new();
    let mut changed = true;
    while changed {
        changed = false;
        for pc in (0..chunk.code.len()).rev() {
            successors.clear();
            successors_of(pc, &chunk.code[pc], chunk.code.len(), &mut successors);
            for word in 0..live_in.words {
                let live_out = successors.iter().fold(0, |acc, next| acc | live_in.word(*next, word));
                let updated = reads.word(pc, word) | (live_out & !writes.word(pc, word));
                changed |= live_in.replace_word(pc, word, updated);
            }
        }
    }
    live_in
}

/// Whether `bit` is live on entry to any successor of `pc`.
fn live_after(chunk: &Chunk, live_in: &BitRows, pc: usize, bit: usize) -> bool {
    let mut successors = Vec::new();
    successors_of(pc, &chunk.code[pc], chunk.code.len(), &mut successors);
    successors.iter().any(|next| live_in.contains(*next, bit))
}

/// Control-flow successors of `op`, mirroring the bytecode verifier's traversal.
fn successors_of(pc: usize, op: &OpCode, code_len: usize, out: &mut Vec<usize>) {
    let mut push = |target: Option<usize>| {
        if let Some(target) = target
            && target < code_len
        {
            out.push(target);
        }
    };
    match op {
        OpCode::Return
        | OpCode::ReturnLocal(_)
        | OpCode::ReturnBinaryLocalLocal { .. }
        | OpCode::ReturnBinaryLocalConst { .. }
        | OpCode::ReturnBinaryLocalNumberConst { .. }
        | OpCode::FlowBreak(_)
        | OpCode::FlowContinue
        | OpCode::RaiseDestructuringFailed => {}
        OpCode::Jump(offset) => push(jump_target(pc, *offset)),
        OpCode::SetLocalAndCopyAndJump { offset, .. }
        | OpCode::ForeachCollectAndJump { offset, .. }
        | OpCode::ForeachBinaryLocalNumberConstAndJump { offset, .. } => push(jump_target(pc, *offset)),
        OpCode::JumpIfFalse(offset)
        | OpCode::JumpIfTrue(offset)
        | OpCode::JumpIfFalseLocalLocal { offset, .. }
        | OpCode::JumpIfFalseLocalConst { offset, .. }
        | OpCode::JumpIfFalseLocalNumberConst { offset, .. } => {
            push(Some(pc + 1));
            push(jump_target(pc, *offset));
        }
        OpCode::ForeachNext { exit_offset, .. } => {
            push(Some(pc + 1));
            push(jump_target(pc, *exit_offset));
        }
        OpCode::TryCatch(info) => {
            push(Some(pc + 1));
            push(info.break_offset.and_then(|offset| jump_target(pc, offset)));
            push(info.continue_offset.and_then(|offset| jump_target(pc, offset)));
        }
        _ => push(Some(pc + 1)),
    }
}

/// A fixed-width bit set per instruction.
struct BitRows {
    bits: usize,
    words: usize,
    data: Vec<u64>,
}

impl BitRows {
    fn new(rows: usize, bits: usize) -> Self {
        let words = bits.div_ceil(64);
        Self {
            bits,
            words,
            data: vec![0; rows * words],
        }
    }

    fn word(&self, row: usize, word: usize) -> u64 {
        self.data[row * self.words + word]
    }

    /// Stores `value`, returning whether it differs from what was there.
    fn replace_word(&mut self, row: usize, word: usize, value: u64) -> bool {
        std::mem::replace(&mut self.data[row * self.words + word], value) != value
    }

    fn insert(&mut self, row: usize, bit: usize) {
        self.data[row * self.words + bit / 64] |= 1 << (bit % 64);
    }

    fn contains(&self, row: usize, bit: usize) -> bool {
        self.word(row, bit / 64) & (1 << (bit % 64)) != 0
    }
}

#[cfg(all(test, any(feature = "mqc", not(feature = "debugger"))))]
mod tests {
    use crate::arena::Arena;
    use crate::module::resolver::std_resolver::StdModuleResolver;
    use crate::runtime::host::HostFunctions;
    use crate::runtime::runtime_value::RuntimeValue;
    use crate::tarn::bytecode::{BinaryOp, Chunk, OpCode, TryCatchInfo};
    use crate::tarn::compiler::{
        CompiledProgram, ResolvedModuleVars, compile_program_for_engine, compile_uninstrumented_program_for_engine,
    };
    use crate::tarn::interpreter::run_with_globals;
    use crate::tarn::last_use::{BitRows, move_last_uses, successors_of};
    use crate::tarn::{Options, TokenArena};
    use crate::{ModuleLoader, Shared, SharedCell};
    use rstest::rstest;

    fn arena() -> TokenArena {
        Shared::new(SharedCell::new(Arena::new(100)))
    }

    /// Compiles with every local observable, so no read is turned into a move.
    fn compile_without_moves(source: &str) -> CompiledProgram {
        let token_arena = arena();
        let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
        compile_program_for_engine(
            &program,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
            &ResolvedModuleVars::default(),
        )
        .unwrap()
    }

    /// Compiles the way a plain run does, with last-use moves.
    fn compile_with_moves(source: &str) -> CompiledProgram {
        let token_arena = arena();
        let program = crate::parse(source, Shared::clone(&token_arena)).unwrap();
        compile_uninstrumented_program_for_engine(
            &program,
            token_arena,
            ModuleLoader::new(StdModuleResolver),
            &[],
            &[],
            &[],
            &ResolvedModuleVars::default(),
        )
        .unwrap()
    }

    fn run(compiled: &CompiledProgram) -> RuntimeValue {
        run_with_globals(
            compiled,
            RuntimeValue::None,
            &HostFunctions::default(),
            None,
            Options::default().max_call_stack_depth,
            &[],
        )
        .unwrap()
    }

    fn all_ops(compiled: &CompiledProgram) -> impl Iterator<Item = &OpCode> {
        compiled.chunks.iter().flat_map(|chunk| chunk.code.iter())
    }

    fn is_move(op: &OpCode) -> bool {
        matches!(
            op,
            OpCode::TakeLocal(_) | OpCode::MoveLocal { .. } | OpCode::CallUpvalueLocal { take: true, .. }
        )
    }

    fn count_moves(compiled: &CompiledProgram) -> usize {
        all_ops(compiled).filter(|op| is_move(op)).count()
    }

    /// Moves of the local named `name` in the chunk that declares it.
    fn moves_of_named_local(compiled: &CompiledProgram, name: &str) -> usize {
        compiled
            .chunks
            .iter()
            .filter_map(|chunk| {
                let slot = chunk.local_names.iter().rposition(|local| local.to_string() == name)? as u16;
                Some(
                    chunk
                        .code
                        .iter()
                        .filter(|op| match op {
                            OpCode::TakeLocal(taken) => *taken == slot,
                            OpCode::MoveLocal { source, .. } => *source == slot,
                            OpCode::CallUpvalueLocal { local, take: true, .. } => *local == slot,
                            _ => false,
                        })
                        .count(),
                )
            })
            .sum()
    }

    /// Runs `source` with and without moves and checks both give `expected`.
    #[rstest]
    // The moved value is the only owner, so the callee updates it in place.
    #[case::take_through_call("def touch(a, i): set(a, 0, i); | var a = [1, 2, 3] | a = touch(a, 9) | a[0]", "9")]
    #[case::keep_original_while_live(
        "def touch(a, i): set(a, 0, i); | let a = [1, 2, 3] | let b = touch(a, 9) | a[0] * 10 + b[0]",
        "19"
    )]
    #[case::argument_read_again_later("def f(x, n): x[0] + n; | let a = [5, 6] | f(a, len(a))", "7")]
    #[case::argument_read_earlier("def f(n, x): n + x[0]; | let a = [5, 6] | f(len(a), a)", "7")]
    #[case::same_value_passed_twice("def g(a): len(a); | def f(a): g(a) + g(a); | f([1, 2, 3])", "6")]
    #[case::parameter_used_twice("def f(a): len(a) + len(a); | f([1, 2])", "4")]
    #[case::copy_then_read_source("def f(a): let b = a | len(a) + len(b); | f([1, 2])", "4")]
    #[case::copy_then_mutate_keeps_caller_value(
        "def f(h, x): var a = h | a += [x] | a; | let h = [1] | let r = f(h, 2) | len(h) * 10 + len(r)",
        "12"
    )]
    #[case::copy_then_mutate_transfers_ownership(
        "def f(h, x): var a = h | a += [x] | a; | var h = [1] | h = f(h, 2) | h = f(h, 3) | len(h)",
        "3"
    )]
    #[case::self_assignment("var a = [1, 2] | a = a | len(a)", "2")]
    #[case::reassigned_before_read("var a = [1] | let b = a | a = [1, 2, 3] | len(a) + len(b)", "4")]
    #[case::recursion_reads_after_call(
        "def sum(a, i): if (i >= len(a)): 0 else: sum(a, i + 1) + a[i]; | sum([1, 2, 3, 4], 0)",
        "10"
    )]
    #[case::recursion_reads_before_call(
        "def sum(a, i): if (i >= len(a)): 0 else: a[i] + sum(a, i + 1); | sum([1, 2, 3, 4], 0)",
        "10"
    )]
    // A read inside a loop is live across the back edge.
    #[case::loop_reads_every_iteration(
        "var a = [1, 2, 3] | var s = 0 | var i = 0 | let _ = while (i < 3): do s = s + a[i] | i = i + 1 end end | s",
        "6"
    )]
    #[case::loop_passes_to_function_every_iteration(
        "def get(a, i): a[i]; | var a = [1, 2, 3] | var s = 0 | var i = 0 | let _ = while (i < 3): do s = s + get(a, i) | i = i + 1 end end | s",
        "6"
    )]
    #[case::loop_moves_and_reassigns(
        "def bump(a): set(a, 0, a[0] + 1); | var a = [0] | var i = 0 | let _ = while (i < 5): do a = bump(a) | i = i + 1 end end | a[0]",
        "5"
    )]
    #[case::nested_loops(
        "let a = [1, 2] | var s = 0 | var i = 0 | let _ = while (i < 2): do var j = 0 | let _ = while (j < 2): do s = s + a[j] | j = j + 1 end end | i = i + 1 end end | s",
        "6"
    )]
    #[case::branch_moves_in_taken_arm(
        "def id(a): a; | let a = [1, 2] | let r = if (true): id(a) else: a | len(r)",
        "2"
    )]
    #[case::branch_moves_in_other_arm(
        "def id(a): a; | let a = [1, 2] | let r = if (false): id(a) else: 0 | len(a) + r",
        "2"
    )]
    #[case::branch_then_read_after_join(
        "def id(a): a; | let a = [1, 2] | let r = if (true): id(a) else: 0 | len(a) + len(r)",
        "4"
    )]
    // Closure captures share the slot, so they are never moved out from under the closure.
    #[case::closure_sees_original(
        "def touch(a, i): set(a, 0, i); | var a = [1, 2, 3] | let f = fn(): a[0]; | let b = touch(a, 9) | f() * 10 + b[0]",
        "19"
    )]
    #[case::closure_sees_reassignment("var a = [1] | let f = fn(): len(a); | a = [1, 2, 3] | f()", "3")]
    #[case::closure_called_after_last_direct_use(
        "def id(a): a; | var a = [1, 2] | let f = fn(): len(a); | let b = id(a) | len(b) + f()",
        "4"
    )]
    #[case::try_body_failure_keeps_value(
        "def boom(a): error(\"x\"); | var a = [1, 2] | let r = try: boom(a) catch: 0 | len(a) + r",
        "2"
    )]
    #[case::try_with_fallback_reading_value(
        "def boom(a): error(\"x\"); | let a = [1, 2, 3] | try: boom(a) catch: len(a)",
        "3"
    )]
    #[case::foreach_over_moved_local("let a = [1, 2, 3] | let t = foreach(x, a): x * 2; | len(t) + len(a)", "6")]
    #[case::default_parameter_reads_earlier_one("def f(a, b = len(a)): b + len(a); | f([1, 2])", "4")]
    #[case::destructuring("def f(p): let [x, y] = p | x + y + len(p); | f([1, 2])", "5")]
    #[case::string_values("def f(s): s + \"x\"; | var s = \"a\" | s = f(s) | s = f(s) | len(s)", "3")]
    #[case::dict_values("def f(d): set(d, \"k\", 1); | var d = {} | d = f(d) | d[\"k\"]", "1")]
    #[case::number_values("def f(n): n + 1; | var n = 0 | n = f(n) | n = f(n) | n", "2")]
    // A one-argument call to a captured function is a fused CallUpvalueLocal.
    #[case::single_argument_update("def bump(a): set(a, 0, a[0] + 1); | def run(a): bump(a); | run([4])[0]", "5")]
    #[case::single_argument_keeps_original(
        "def bump(a): set(a, 0, a[0] + 1); | def run(a): let b = bump(a) | a[0] * 10 + b[0]; | run([4])",
        "45"
    )]
    #[case::single_argument_in_loop(
        "def bump(a): set(a, 0, a[0] + 1); | def run(a): var i = 0 | let _ = while (i < 5): do a = bump(a) | i = i + 1 end end | a[0]; | run([0])",
        "5"
    )]
    #[case::single_argument_read_each_iteration(
        "def first(a): a[0]; | def run(a): var s = 0 | var i = 0 | let _ = while (i < 3): do s = s + first(a) | i = i + 1 end end | s; | run([2])",
        "6"
    )]
    #[case::single_argument_called_twice(
        "def total(a): a[0] + len(a); | def run(a): total(a) + total(a); | run([5, 6])",
        "14"
    )]
    #[case::single_argument_with_captured_local(
        "def id(p): p; | def run(a): let f = fn(): a[0]; | let b = id(a) | f() + b[0]; | run([3])",
        "6"
    )]
    #[case::many_moves_in_a_row(
        "def f(a): set(a, 0, a[0] + 1); | var a = [0] | a = f(a) | a = f(a) | a = f(a) | a = f(a) | a[0]",
        "4"
    )]
    fn moves_do_not_change_results(#[case] source: &str, #[case] expected: &str) {
        let expected = RuntimeValue::Number(expected.parse::<i64>().unwrap().into());
        assert_eq!(run(&compile_without_moves(source)), expected, "without moves");
        assert_eq!(run(&compile_with_moves(source)), expected, "with moves");
    }

    #[rstest]
    #[case::parameter_passed_on("def touch(a, i): set(a, 0, i); | touch([1], 2)")]
    #[case::copy_of_parameter("def f(h): var a = h | a; | f([1])")]
    #[case::single_argument_call("def bump(a): set(a, 0, 1); | def run(p): bump(p); | run([1])")]
    fn last_use_becomes_a_move(#[case] source: &str) {
        assert_eq!(count_moves(&compile_without_moves(source)), 0);
        assert!(count_moves(&compile_with_moves(source)) > 0);
    }

    #[rstest]
    #[case::read_again_in_loop(
        "def get(a, i): a[i]; | var a = [1, 2, 3] | var i = 0 | let _ = while (i < 3): do get(a, i) | i = i + 1 end end | i"
    )]
    #[case::read_after_call("def id(p): p; | let a = [1] | let b = id(a) | len(a)")]
    #[case::captured_by_closure("def id(p): p; | var a = [1] | let f = fn(): a; | id(a)")]
    #[case::single_argument_call_then_read("def id(p): p; | def run(a): let b = id(a) | len(a); | run([1])")]
    #[case::used_by_default_parameter("def f(a, b = len(a)): a; | f([1])")]
    fn a_still_needed_local_is_not_moved(#[case] source: &str) {
        assert_eq!(moves_of_named_local(&compile_with_moves(source), "a"), 0);
    }

    #[test]
    fn the_last_of_several_reads_is_the_one_moved() {
        let compiled = compile_with_moves("def g(p, n): len(p) + n; | def f(q): g(q, 1) + g(q, 2); | f([1, 2, 3])");
        assert_eq!(moves_of_named_local(&compiled, "q"), 1);
    }

    fn chunk_of(code: Vec<OpCode>, local_count: u16) -> Chunk {
        Chunk {
            code,
            local_count,
            ..Chunk::default()
        }
    }

    fn moved_pcs(chunk: &Chunk) -> Vec<usize> {
        chunk
            .code
            .iter()
            .enumerate()
            .filter(|(_, op)| is_move(op))
            .map(|(pc, _)| pc)
            .collect()
    }

    fn analyzed(code: Vec<OpCode>, local_count: u16) -> Chunk {
        let mut chunks = [chunk_of(code, local_count)];
        move_last_uses(&mut chunks);
        let [chunk] = chunks;
        chunk
    }

    #[rstest]
    #[case::single_read(vec![OpCode::GetLocal(1), OpCode::Return], vec![0])]
    #[case::only_the_last_of_two_reads(vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::GetLocal(1), OpCode::Return], vec![2])]
    #[case::a_store_ends_liveness(
        vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::PushNone, OpCode::SetLocal(1), OpCode::GetLocal(1), OpCode::Return],
        vec![0, 4]
    )]
    #[case::constant_store_ends_liveness(
        vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::SetLocalConst { local: 1, constant: 0 }, OpCode::GetLocal(1), OpCode::Return],
        vec![0, 3]
    )]
    #[case::different_slots_are_independent(
        vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::GetLocal(2), OpCode::Pop, OpCode::GetLocal(1), OpCode::Return],
        vec![2, 4]
    )]
    #[case::self_slot_is_never_moved(vec![OpCode::GetLocal(0), OpCode::Return], vec![])]
    // The back edge makes the read live on every iteration.
    #[case::loop_back_edge(vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::Jump(-3)], vec![])]
    #[case::read_before_loop_that_reads_again(
        vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::GetLocal(1), OpCode::Pop, OpCode::Jump(-3)],
        vec![]
    )]
    #[case::branch_read_in_one_arm_only(
        vec![
            OpCode::GetLocal(1), OpCode::Pop,
            OpCode::PushNone, OpCode::JumpIfFalse(2),
            OpCode::GetLocal(1), OpCode::Pop,
            OpCode::Return,
        ],
        vec![4]
    )]
    #[case::branch_read_after_join(
        vec![
            OpCode::GetLocal(1), OpCode::Pop,
            OpCode::PushNone, OpCode::JumpIfFalse(1),
            OpCode::PushNone,
            OpCode::GetLocal(1), OpCode::Return,
        ],
        vec![5]
    )]
    #[case::fused_call_argument(
        vec![OpCode::CallUpvalueLocal { index: 0, local: 1, take: false }, OpCode::Return],
        vec![0]
    )]
    #[case::fused_call_argument_read_later(
        vec![OpCode::CallUpvalueLocal { index: 0, local: 1, take: false }, OpCode::Pop, OpCode::GetLocal(1), OpCode::Return],
        vec![2]
    )]
    #[case::fused_call_argument_read_by_loop(
        vec![OpCode::CallUpvalueLocal { index: 0, local: 1, take: false }, OpCode::Pop, OpCode::Jump(-3)],
        vec![]
    )]
    #[case::copy_into_other_slot(
        vec![OpCode::CopyLocal { source: 1, destination: 2 }, OpCode::GetLocal(2), OpCode::Return],
        vec![0, 1]
    )]
    #[case::copy_whose_source_is_read_later(
        vec![OpCode::CopyLocal { source: 1, destination: 2 }, OpCode::GetLocal(1), OpCode::Return],
        vec![1]
    )]
    #[case::copy_onto_itself(vec![OpCode::CopyLocal { source: 1, destination: 1 }, OpCode::Return], vec![])]
    #[case::copy_destination_is_a_store(
        vec![OpCode::GetLocal(2), OpCode::Pop, OpCode::CopyLocal { source: 1, destination: 2 }, OpCode::Return],
        vec![0, 2]
    )]
    #[case::fused_operand_counts_as_a_read(
        vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::BinaryLocalLocal { op: BinaryOp::Add, left: 1, right: 2 }, OpCode::Pop, OpCode::Return],
        vec![]
    )]
    #[case::return_local_counts_as_a_read(vec![OpCode::GetLocal(1), OpCode::Pop, OpCode::ReturnLocal(1)], vec![])]
    fn liveness_decides_which_reads_move(#[case] code: Vec<OpCode>, #[case] expected: Vec<usize>) {
        let chunk = analyzed(code, 3);
        assert_eq!(moved_pcs(&chunk), expected);
    }

    #[test]
    fn a_captured_slot_is_never_moved() {
        let mut chunk = chunk_of(vec![OpCode::GetLocal(1), OpCode::Return], 2);
        chunk.captured_local_slots = vec![1];
        let mut chunks = [chunk];
        move_last_uses(&mut chunks);
        assert!(moved_pcs(&chunks[0]).is_empty());
    }

    #[test]
    fn a_move_keeps_the_instruction_count_and_slot() {
        let chunk = analyzed(
            vec![
                OpCode::GetLocal(2),
                OpCode::CopyLocal {
                    source: 1,
                    destination: 3,
                },
                OpCode::Return,
            ],
            4,
        );
        assert_eq!(chunk.code.len(), 3);
        assert!(matches!(chunk.code[0], OpCode::TakeLocal(2)));
        assert!(matches!(
            chunk.code[1],
            OpCode::MoveLocal {
                source: 1,
                destination: 3
            }
        ));
    }

    #[test]
    fn a_chunk_without_candidate_reads_is_left_alone() {
        let chunk = analyzed(vec![OpCode::PushNone, OpCode::SetLocal(1), OpCode::Return], 2);
        assert!(moved_pcs(&chunk).is_empty());
    }

    #[test]
    fn a_wide_chunk_tracks_slots_across_bitset_words() {
        let mut code = Vec::new();
        for slot in 1..=130 {
            code.extend([OpCode::GetLocal(slot), OpCode::Pop]);
        }
        code.push(OpCode::GetLocal(1));
        code.push(OpCode::GetLocal(130));
        code.push(OpCode::Return);
        let chunk = analyzed(code, 131);
        let moved = moved_pcs(&chunk);
        // Slots 2..=129 are read once, in order, so each read is its own last use; 1 and 130 are read again.
        assert_eq!(moved.len(), 128 + 2);
        assert!(!moved.contains(&0));
        assert!(!moved.contains(&(2 * 129)));
    }

    #[rstest]
    #[case::plain(OpCode::PushNone, 4, vec![5])]
    #[case::jump(OpCode::Jump(2), 4, vec![7])]
    #[case::backward_jump(OpCode::Jump(-3), 4, vec![2])]
    #[case::conditional(OpCode::JumpIfFalse(2), 4, vec![5, 7])]
    #[case::return_has_none(OpCode::Return, 4, vec![])]
    #[case::return_local_has_none(OpCode::ReturnLocal(1), 4, vec![])]
    #[case::flow_break_has_none(OpCode::FlowBreak(false), 4, vec![])]
    #[case::fused_conditional(OpCode::JumpIfFalseLocalConst { op: BinaryOp::Lt, local: 1, constant: 0, offset: 1 }, 4, vec![5, 6])]
    #[case::fused_jump_has_only_target(OpCode::SetLocalAndCopyAndJump { source: 1, destination: 2, offset: -2 }, 4, vec![3])]
    #[case::target_past_the_end_is_dropped(OpCode::Jump(10), 4, vec![])]
    #[case::fallthrough_past_the_end_is_dropped(OpCode::PushNone, 7, vec![])]
    fn successors_follow_control_flow(#[case] op: OpCode, #[case] pc: usize, #[case] expected: Vec<usize>) {
        let mut out = Vec::new();
        successors_of(pc, &op, 8, &mut out);
        assert_eq!(out, expected);
    }

    #[test]
    fn try_catch_successors_include_break_and_continue_targets() {
        let info = TryCatchInfo {
            has_binder: false,
            break_acc_slot: None,
            break_completed_iteration_slot: None,
            break_offset: Some(3),
            continue_offset: Some(-3),
        };
        let mut out = Vec::new();
        successors_of(4, &OpCode::TryCatch(Box::new(info)), 10, &mut out);
        assert_eq!(out, vec![5, 8, 2]);
    }

    #[test]
    fn bit_rows_span_word_boundaries() {
        let mut rows = BitRows::new(3, 130);
        for bit in [0, 63, 64, 129] {
            rows.insert(1, bit);
        }
        for bit in [0, 63, 64, 129] {
            assert!(rows.contains(1, bit));
            assert!(!rows.contains(0, bit));
            assert!(!rows.contains(2, bit));
        }
        assert!(!rows.contains(1, 1));
        assert!(!rows.contains(1, 65));
    }

    #[test]
    fn replace_word_reports_whether_it_changed() {
        let mut rows = BitRows::new(1, 64);
        assert!(!rows.replace_word(0, 0, 0));
        assert!(rows.replace_word(0, 0, 5));
        assert!(!rows.replace_word(0, 0, 5));
        assert!(rows.replace_word(0, 0, 0));
    }
}
