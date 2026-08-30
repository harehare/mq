#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSymbolTable;
use super::value::Closure;
use crate::Ident;
use crate::Shared;
use crate::ast::TokenId;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;
use crate::eval::runtime_value::RuntimeValue;
use crate::selector::Selector;
use std::fmt;

/// Every chunk reserves local slot 0 for the implicit pipeline value (`.` / `self`),
/// declared before params/other locals so its index is stable across all chunks.
pub(crate) const SELF_SLOT: u16 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpvalueSource {
    Local(u16),
    Upvalue(u16),
}

/// How one declared parameter binds an argument, mirroring `ast::Param`/the tree-walker's
/// `call_fn` binding loop exactly (see `interpreter::bind_params`).
#[derive(Debug, Clone)]
pub(crate) enum ParamBinding {
    Required(u16),
    Optional(u16, u16, Vec<UpvalueSource>),
    Variadic(u16),
}

impl ParamBinding {
    pub(crate) fn slot(&self) -> u16 {
        match self {
            ParamBinding::Required(slot) | ParamBinding::Optional(slot, ..) | ParamBinding::Variadic(slot) => *slot,
        }
    }
}

impl ParamShape {
    /// Returns the arity when every parameter is required and occupies its declared slot.
    pub(crate) fn fixed_required_arity(&self) -> Option<usize> {
        (!self.has_variadic && self.required == self.bindings.len()).then_some(self.required)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ParamShape {
    pub(crate) bindings: Vec<ParamBinding>,
    /// Count of `bindings` that are `Required` (no default, not variadic) — `call_fn` needs
    /// this precomputed to decide the implicit-`.`/arity outcome before binding anything.
    pub(crate) required: usize,
    pub(crate) has_variadic: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum OpCode {
    /// Marks an AST evaluation boundary at which the debugger may pause.
    #[cfg(feature = "debugger")]
    StmtBoundary(TokenId),
    Const(u16),
    PushNone,
    GetLocal(u16),
    SetLocal(u16),
    GetUpvalue(u16),
    SetUpvalue(u16),
    MakeClosure(u16, Vec<UpvalueSource>),
    /// Pushes a shared closure with no captured cells.
    MakeStaticClosure(u16),
    /// Pops and discards the top value (used to drop a flattened module statement's
    /// value, since only its side effect — defining a name — matters).
    Pop,
    /// Duplicates the top stack value (used by `&&`/`||` to test an operand's
    /// truthiness without losing the value it may short-circuit to).
    Dup,
    /// Swaps the top two stack values.
    Swap,
    Jump(i32),
    JumpIfFalse(i32),
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Neg,
    /// Pops a value and pushes its truthiness negation.
    Not,
    /// Pushes a fresh empty `Array`, for `array(...)` construction (incl. `...spread`).
    ArrayNew,
    /// Pops a value and an `Array` beneath it, appends the value, pushes the array back.
    ArrayPush,
    /// Pops a source and an `Array` accumulator beneath it, extends the accumulator with the
    /// source's elements (`...spread` inside `array(...)`/`[...]`); `None` contributes
    /// nothing. Pushes the accumulator back.
    ArraySpread,
    /// Like `ArraySpread`, but for `dict{...}`/`{...}` build: pops a `Dict` source, extends
    /// the `Array` accumulator with `[key, value]` pairs (matching the plain-entry encoding
    /// `compile_dict_call` also uses); `None` contributes nothing.
    DictSpread,
    /// Normalizes `foreach`'s operand into an `Array` (`String` -> per-char array; anything
    /// else errors).
    ToForeachIterable,
    /// Reads the length of an `Array` value directly, without going through the `len`
    /// builtin's generic dispatch (`foreach`'s per-iteration hot path).
    ArrayLen,
    /// Reads `array[index]` (or `None` if out of range) directly, without the `get`
    /// builtin's clone-on-write mutation path.
    ArrayGetAt,
    /// Pops an index and an `Array`, pushes a new `Array` of the elements from that index
    /// onward (the `..rest` binding in an `ArrayRest` match pattern).
    ArraySliceFrom,
    /// Pops a value, pushes whether its runtime type name matches (`match`'s `:type`
    /// pattern); `none` is checked structurally rather than by `RuntimeValue::name()`
    /// (which capitalizes it).
    TypeCheck(Ident),
    /// Reads an OS environment variable named by the string constant at this index.
    GetEnvVar(u16),
    /// Reads a name defined via `Engine::define_value`/`define_string_value`, which write
    /// directly into the tree-walker's root `Env` rather than any compiled program — the one
    /// case where the VM falls back to a real dynamic lookup instead of a static slot, against
    /// the runtime `Env` the interpreter seeds from `global_bindings` before execution starts.
    GetExternalGlobal(Ident),
    /// Pops `n` values, `Display`s each, concatenates, and pushes the resulting `String`
    /// (mq string-interpolation semantics).
    InterpString(u16),
    /// Pops a `Markdown` value and pushes the result of matching `selector` against it
    /// (`None` for any other value shape).
    SelectorMatch(Selector),
    /// Like `SelectorMatch`, but pops `argc` filter-argument values (pushed before the
    /// subject) for selectors with runtime arguments (`.h(1..2)`, `.code("rust")`).
    SelectorMatchWithArgs(Selector, u8),
    CallBuiltin(Ident, u8),
    /// Calls an immutable local binding without materializing the callee on the operand stack.
    CallLocal(u16, u8),
    CallValue(u8),
    /// "Paren-free" call: calls the top-of-stack value with zero args if it's a
    /// closure/native function needing at most one (the implicit `.`); otherwise leaves it
    /// unchanged. Mirrors `Evaluator::maybe_auto_call_pipeline_ident`.
    MaybeAutoCall,
    /// Pops a catch closure then a try closure (both 0/1-arg, no-upvalue-restriction
    /// closures compiled like any nested `fn`); runs the try closure, and on error runs
    /// the catch closure instead (passing it a `{"message": ...}` dict if it takes a
    /// parameter). Loop control raised by the nested try chunk bypasses the catch and
    /// jumps to the enclosing loop's patched target.
    TryCatch {
        has_binder: bool,
        break_acc_slot: Option<u16>,
        break_offset: Option<i32>,
        continue_offset: Option<i32>,
    },
    /// Exits a loop outside this nested chunk. The enclosing `TryCatch` owns the actual
    /// jump target and accumulator slot.
    FlowBreak(bool),
    /// Continues a loop outside this nested chunk. The enclosing `TryCatch` owns the
    /// actual jump target.
    FlowContinue,
    /// Raised when a `let`/`var` destructuring pattern doesn't match its value (e.g.
    /// `let [a, b] = [1]`) — mirrors `RuntimeError::DestructuringFailed`.
    RaiseDestructuringFailed,
    Return,
}

#[derive(Debug, Default)]
pub(crate) struct Chunk {
    pub(crate) code: Vec<OpCode>,
    pub(crate) constants: Vec<RuntimeValue>,
    /// Non-capturing closures allocated once when the program is compiled.
    pub(crate) static_closures: Vec<Shared<Closure>>,
    pub(crate) local_count: u16,
    /// Source names for local slots. Kept in non-debug builds too because legacy dynamic
    /// builtins such as `get_variable` resolve names against the current lexical scope.
    pub(crate) local_names: Vec<Ident>,
    /// Source names for captured slots; see [`Self::local_names`].
    pub(crate) upvalue_names: Vec<Ident>,
    /// Run-length-encoded `pc -> TokenId` map: `(pc_start, token_id)` pairs, one per run
    /// of consecutive instructions attributed to the same source token. Looked up via
    /// `token_at` to recover error spans.
    pub(crate) lines: Vec<(usize, TokenId)>,
    /// AST nodes addressable by `StmtBoundary`, omitted from non-debugger builds.
    #[cfg(feature = "debugger")]
    pub(crate) debug_nodes: Vec<(TokenId, Shared<Node>)>,
    /// Static source-name to local/upvalue resolution for paused-frame evaluation.
    #[cfg(feature = "debugger")]
    pub(crate) debug_symbols: DebugSymbolTable,
    /// How this chunk's declared parameters bind call arguments — empty for chunks that
    /// aren't a compiled `ast::Params` function body (the top-level program, `try`/`catch`
    /// closures, ...), which `CallValue` never targets.
    pub(crate) param_shape: ParamShape,
}

impl Chunk {
    /// Adds a non-capturing closure and returns its chunk-local index.
    pub(crate) fn push_static_closure(&mut self, target_chunk: u16) -> u16 {
        self.static_closures.push(Shared::new(Closure {
            chunk_index: target_chunk,
            upvalues: Vec::new(),
        }));
        (self.static_closures.len() - 1) as u16
    }

    /// Whether a closure or parameter default can retain one of this frame's local cells.
    pub(crate) fn captures_local_slots(&self) -> bool {
        self.code.iter().any(|op| {
            matches!(
                op,
                OpCode::MakeClosure(_, sources)
                    if sources.iter().any(|source| matches!(source, UpvalueSource::Local(_)))
            )
        }) || self.param_shape.bindings.iter().any(|binding| {
            matches!(
                binding,
                ParamBinding::Optional(_, _, sources)
                    if sources.iter().any(|source| matches!(source, UpvalueSource::Local(_)))
            )
        })
    }

    pub(crate) fn push_const(&mut self, value: RuntimeValue) -> u16 {
        self.constants.push(value);
        (self.constants.len() - 1) as u16
    }

    /// Emits `op`, attributing it to `token_id` in the line table (only recorded when it
    /// differs from the previous instruction's, keeping the table run-length-encoded).
    pub(crate) fn emit(&mut self, op: OpCode, token_id: TokenId) -> usize {
        let pc = self.code.len();
        if self.lines.last().map(|(_, t)| *t) != Some(token_id) {
            self.lines.push((pc, token_id));
        }
        self.code.push(op);
        pc
    }

    /// The `TokenId` attributed to the instruction at `pc`, for error reporting.
    pub(crate) fn token_at(&self, pc: usize) -> Option<TokenId> {
        self.lines
            .partition_point(|(start, _)| *start <= pc)
            .checked_sub(1)
            .map(|i| self.lines[i].1)
    }

    /// Rewrites a previously-emitted `Jump`/`JumpIfFalse` at `at` so its offset lands on
    /// the instruction that will be emitted next.
    pub(crate) fn patch_jump(&mut self, at: usize) {
        let offset = (self.code.len() - at - 1) as i32;
        match &mut self.code[at] {
            OpCode::Jump(o) | OpCode::JumpIfFalse(o) => *o = offset,
            _ => unreachable!("patch_jump target is not a jump instruction"),
        }
    }

    /// Patches a `TryCatch` control-flow break target to the next instruction.
    pub(crate) fn patch_try_break(&mut self, at: usize) {
        let offset = (self.code.len() - at - 1) as i32;
        match &mut self.code[at] {
            OpCode::TryCatch { break_offset, .. } => *break_offset = Some(offset),
            _ => unreachable!("flow-break target is not a TryCatch instruction"),
        }
    }

    /// Patches a `TryCatch` control-flow continue target to the next instruction.
    pub(crate) fn patch_try_continue(&mut self, at: usize) {
        self.patch_try_continue_to(at, self.code.len());
    }

    /// Patches a `TryCatch` control-flow continue target to `target`.
    pub(crate) fn patch_try_continue_to(&mut self, at: usize, target: usize) {
        let offset = target as i32 - at as i32 - 1;
        match &mut self.code[at] {
            OpCode::TryCatch { continue_offset, .. } => *continue_offset = Some(offset),
            _ => unreachable!("flow-continue target is not a TryCatch instruction"),
        }
    }

    /// Offset for a backward jump from the next-emitted instruction to `target`.
    pub(crate) fn backward_offset(&self, target: usize) -> i32 {
        (target as i32) - (self.code.len() as i32) - 1
    }
}

/// A structural bytecode error emitted by the compiler's post-generation verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BytecodeError {
    EmptyChunk(usize),
    MissingReturn(usize),
    ConstantOutOfBounds { chunk: usize, pc: usize, index: u16 },
    LocalOutOfBounds { chunk: usize, pc: usize, slot: u16 },
    ChunkOutOfBounds { chunk: usize, pc: usize, target: u16 },
    StaticClosureOutOfBounds { chunk: usize, pc: usize, index: u16 },
    JumpOutOfBounds { chunk: usize, pc: usize, target: isize },
}

impl fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChunk(chunk) => write!(f, "chunk {chunk} has no instructions"),
            Self::MissingReturn(chunk) => write!(f, "chunk {chunk} does not end in Return"),
            Self::ConstantOutOfBounds { chunk, pc, index } => {
                write!(f, "chunk {chunk} pc {pc} references constant {index} out of bounds")
            }
            Self::LocalOutOfBounds { chunk, pc, slot } => {
                write!(f, "chunk {chunk} pc {pc} references local slot {slot} out of bounds")
            }
            Self::ChunkOutOfBounds { chunk, pc, target } => {
                write!(f, "chunk {chunk} pc {pc} references chunk {target} out of bounds")
            }
            Self::StaticClosureOutOfBounds { chunk, pc, index } => {
                write!(
                    f,
                    "chunk {chunk} pc {pc} references static closure {index} out of bounds"
                )
            }
            Self::JumpOutOfBounds { chunk, pc, target } => {
                write!(f, "chunk {chunk} pc {pc} jumps to {target} out of bounds")
            }
        }
    }
}

impl std::error::Error for BytecodeError {}

/// Applies semantics-preserving local rewrites after compilation.
///
/// This pass deliberately does not fold expressions: that belongs to the AST optimizer.
/// It only removes compiler artifacts and retargets jumps without crossing source-debug
/// boundaries or changing observable evaluation order.
pub(crate) fn optimize_chunks(chunks: &mut [Chunk]) {
    for chunk in chunks {
        optimize_chunk(chunk);
    }
}

fn optimize_chunk(chunk: &mut Chunk) {
    if chunk.code.is_empty() {
        return;
    }

    let old_code = std::mem::take(&mut chunk.code);
    let old_lines = std::mem::take(&mut chunk.lines);
    let mut keep = vec![true; old_code.len()];
    let targets = jump_targets(&old_code);

    let mut pc = 0;
    while pc < old_code.len() {
        match (&old_code[pc], old_code.get(pc + 1)) {
            (OpCode::Const(_), Some(OpCode::Pop)) if !targets.contains(&pc) && !targets.contains(&(pc + 1)) => {
                keep[pc] = false;
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
            (OpCode::Jump(0), _) => {
                keep[pc] = false;
                pc += 1;
            }
            _ => pc += 1,
        }
    }

    let old_to_new = old_to_new_pc_map(&keep);
    let mut new_code = Vec::with_capacity(old_code.len());
    let mut new_lines = Vec::with_capacity(old_lines.len());
    for (old_pc, op) in old_code.into_iter().enumerate() {
        if !keep[old_pc] {
            continue;
        }
        let new_pc = new_code.len();
        let token_id = token_at(&old_lines, old_pc);
        if new_lines.last().map(|(_, token)| *token) != Some(token_id) {
            new_lines.push((new_pc, token_id));
        }
        new_code.push(rewrite_targets(op, old_pc, new_pc, &old_to_new));
    }
    chunk.code = new_code;
    chunk.lines = new_lines;
}

fn jump_targets(code: &[OpCode]) -> std::collections::BTreeSet<usize> {
    let mut targets = std::collections::BTreeSet::new();
    for (pc, op) in code.iter().enumerate() {
        match op {
            OpCode::Jump(offset) | OpCode::JumpIfFalse(offset) => {
                if let Some(target) = jump_target(pc, *offset) {
                    targets.insert(target);
                }
            }
            OpCode::TryCatch {
                break_offset: Some(offset),
                continue_offset,
                ..
            } => {
                if let Some(target) = jump_target(pc, *offset) {
                    targets.insert(target);
                }
                if let Some(offset) = continue_offset
                    && let Some(target) = jump_target(pc, *offset)
                {
                    targets.insert(target);
                }
            }
            OpCode::TryCatch {
                continue_offset: Some(offset),
                ..
            } => {
                if let Some(target) = jump_target(pc, *offset) {
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

fn token_at(lines: &[(usize, TokenId)], pc: usize) -> TokenId {
    lines
        .partition_point(|(start, _)| *start <= pc)
        .checked_sub(1)
        .map(|index| lines[index].1)
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
        OpCode::TryCatch {
            has_binder,
            break_acc_slot,
            break_offset,
            continue_offset,
        } => OpCode::TryCatch {
            has_binder,
            break_acc_slot,
            break_offset: break_offset.map(rewrite),
            continue_offset: continue_offset.map(rewrite),
        },
        other => other,
    }
}

fn jump_target(pc: usize, offset: i32) -> Option<usize> {
    pc.checked_add(1)?.checked_add_signed(offset as isize)
}

/// Verifies generated bytecode before it becomes executable.
pub(crate) fn verify_chunks(chunks: &[Chunk]) -> Result<(), BytecodeError> {
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        if chunk.code.is_empty() {
            return Err(BytecodeError::EmptyChunk(chunk_index));
        }
        if !matches!(chunk.code.last(), Some(OpCode::Return)) {
            return Err(BytecodeError::MissingReturn(chunk_index));
        }
        for (pc, op) in chunk.code.iter().enumerate() {
            match op {
                OpCode::Const(index) | OpCode::GetEnvVar(index) => {
                    if *index as usize >= chunk.constants.len() {
                        return Err(BytecodeError::ConstantOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            index: *index,
                        });
                    }
                }
                OpCode::GetLocal(slot) | OpCode::SetLocal(slot) | OpCode::CallLocal(slot, _) => {
                    if *slot >= chunk.local_count {
                        return Err(BytecodeError::LocalOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            slot: *slot,
                        });
                    }
                }
                OpCode::MakeClosure(target, _) => verify_chunk_target(chunks, chunk_index, pc, *target)?,
                OpCode::MakeStaticClosure(index) => {
                    let Some(closure) = chunk.static_closures.get(*index as usize) else {
                        return Err(BytecodeError::StaticClosureOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            index: *index,
                        });
                    };
                    verify_chunk_target(chunks, chunk_index, pc, closure.chunk_index)?;
                }
                OpCode::Jump(offset) | OpCode::JumpIfFalse(offset) => {
                    verify_jump_target(chunk, chunk_index, pc, *offset)?;
                }
                OpCode::TryCatch {
                    break_offset,
                    continue_offset,
                    ..
                } => {
                    if let Some(offset) = break_offset {
                        verify_jump_target(chunk, chunk_index, pc, *offset)?;
                    }
                    if let Some(offset) = continue_offset {
                        verify_jump_target(chunk, chunk_index, pc, *offset)?;
                    }
                }
                _ => {}
            }
        }
        for binding in &chunk.param_shape.bindings {
            if binding.slot() >= chunk.local_count {
                return Err(BytecodeError::LocalOutOfBounds {
                    chunk: chunk_index,
                    pc: chunk.code.len() - 1,
                    slot: binding.slot(),
                });
            }
        }
    }
    Ok(())
}

fn verify_chunk_target(chunks: &[Chunk], chunk: usize, pc: usize, target: u16) -> Result<(), BytecodeError> {
    if target as usize >= chunks.len() {
        return Err(BytecodeError::ChunkOutOfBounds { chunk, pc, target });
    }
    Ok(())
}

fn verify_jump_target(chunk: &Chunk, chunk_index: usize, pc: usize, offset: i32) -> Result<(), BytecodeError> {
    let Some(target) = jump_target(pc, offset) else {
        return Err(BytecodeError::JumpOutOfBounds {
            chunk: chunk_index,
            pc,
            target: -1,
        });
    };
    if target >= chunk.code.len() {
        return Err(BytecodeError::JumpOutOfBounds {
            chunk: chunk_index,
            pc,
            target: target as isize,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
