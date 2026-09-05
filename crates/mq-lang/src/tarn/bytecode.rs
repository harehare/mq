#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSymbolTable;
use super::value::Closure;
use crate::Ident;
use crate::Shared;
use crate::ast::TokenId;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;
use crate::runtime::runtime_value::RuntimeValue;
use crate::selector::Selector;
use std::fmt;

/// The implicit pipeline value (`.` / `self`) slot.
pub(crate) const SELF_SLOT: u16 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A captured value's source slot.
pub(crate) enum UpvalueSource {
    Local(u16),
    Upvalue(u16),
}

/// Binary operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// A binary operation.
pub(crate) enum BinaryOp {
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
}

/// Compact argument-free node selector.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NodeSelectorKind {
    Blockquote,
    Footnote,
    Toml,
    Yaml,
    Break,
    InlineCode,
    InlineMath,
    Delete,
    Emphasis,
    FootnoteRef,
    Html,
    Image,
    ImageRef,
    MdxJsxTextElement,
    Link,
    LinkRef,
    WikiLink,
    Callout,
    Embed,
    Strong,
    Code,
    Math,
    TableAlign,
    Text,
    HorizontalRule,
    Definition,
    MdxFlowExpression,
    MdxTextExpression,
    MdxJsEsm,
    MdxJsxFlowElement,
    Task,
    Todo,
    Done,
}

impl NodeSelectorKind {
    /// Returns the compact form for an eligible selector.
    pub(crate) fn from_selector(selector: &Selector) -> Option<Self> {
        Some(match selector {
            Selector::Blockquote => Self::Blockquote,
            Selector::Footnote => Self::Footnote,
            Selector::Toml => Self::Toml,
            Selector::Yaml => Self::Yaml,
            Selector::Break => Self::Break,
            Selector::InlineCode => Self::InlineCode,
            Selector::InlineMath => Self::InlineMath,
            Selector::Delete => Self::Delete,
            Selector::Emphasis => Self::Emphasis,
            Selector::FootnoteRef => Self::FootnoteRef,
            Selector::Html => Self::Html,
            Selector::Image => Self::Image,
            Selector::ImageRef => Self::ImageRef,
            Selector::MdxJsxTextElement => Self::MdxJsxTextElement,
            Selector::Link => Self::Link,
            Selector::LinkRef => Self::LinkRef,
            Selector::WikiLink => Self::WikiLink,
            Selector::Callout => Self::Callout,
            Selector::Embed => Self::Embed,
            Selector::Strong => Self::Strong,
            Selector::Code => Self::Code,
            Selector::Math => Self::Math,
            Selector::TableAlign => Self::TableAlign,
            Selector::Text => Self::Text,
            Selector::HorizontalRule => Self::HorizontalRule,
            Selector::Definition => Self::Definition,
            Selector::MdxFlowExpression => Self::MdxFlowExpression,
            Selector::MdxTextExpression => Self::MdxTextExpression,
            Selector::MdxJsEsm => Self::MdxJsEsm,
            Selector::MdxJsxFlowElement => Self::MdxJsxFlowElement,
            Selector::Task => Self::Task,
            Selector::Todo => Self::Todo,
            Selector::Done => Self::Done,
            _ => return None,
        })
    }

    /// Converts the compact selector to its generic form.
    pub(crate) fn as_selector(self) -> Selector {
        match self {
            Self::Blockquote => Selector::Blockquote,
            Self::Footnote => Selector::Footnote,
            Self::Toml => Selector::Toml,
            Self::Yaml => Selector::Yaml,
            Self::Break => Selector::Break,
            Self::InlineCode => Selector::InlineCode,
            Self::InlineMath => Selector::InlineMath,
            Self::Delete => Selector::Delete,
            Self::Emphasis => Selector::Emphasis,
            Self::FootnoteRef => Selector::FootnoteRef,
            Self::Html => Selector::Html,
            Self::Image => Selector::Image,
            Self::ImageRef => Selector::ImageRef,
            Self::MdxJsxTextElement => Selector::MdxJsxTextElement,
            Self::Link => Selector::Link,
            Self::LinkRef => Selector::LinkRef,
            Self::WikiLink => Selector::WikiLink,
            Self::Callout => Selector::Callout,
            Self::Embed => Selector::Embed,
            Self::Strong => Selector::Strong,
            Self::Code => Selector::Code,
            Self::Math => Selector::Math,
            Self::TableAlign => Selector::TableAlign,
            Self::Text => Selector::Text,
            Self::HorizontalRule => Selector::HorizontalRule,
            Self::Definition => Selector::Definition,
            Self::MdxFlowExpression => Selector::MdxFlowExpression,
            Self::MdxTextExpression => Selector::MdxTextExpression,
            Self::MdxJsEsm => Selector::MdxJsEsm,
            Self::MdxJsxFlowElement => Selector::MdxJsxFlowElement,
            Self::Task => Selector::Task,
            Self::Todo => Selector::Todo,
            Self::Done => Selector::Done,
        }
    }
}

/// Parameter binding.
#[derive(Debug, Clone)]
/// Parameter binding metadata.
pub(crate) enum ParamBinding {
    Required(u16),
    Optional(u16, u16, Vec<UpvalueSource>),
    Variadic(u16),
}

impl ParamBinding {
    /// Returns the binding's local slot.
    pub(crate) fn slot(&self) -> u16 {
        match self {
            ParamBinding::Required(slot) | ParamBinding::Optional(slot, ..) | ParamBinding::Variadic(slot) => *slot,
        }
    }
}

impl ParamShape {
    /// Returns the arity for an all-required parameter list.
    pub(crate) fn fixed_required_arity(&self) -> Option<usize> {
        (!self.has_variadic && self.required == self.bindings.len()).then_some(self.required)
    }
}

#[derive(Debug, Clone, Default)]
/// Parameter metadata for a compiled function.
pub(crate) struct ParamShape {
    pub(crate) bindings: Vec<ParamBinding>,
    pub(crate) required: usize,
    pub(crate) has_variadic: bool,
}

#[derive(Debug, Clone)]
/// A VM instruction.
pub(crate) enum OpCode {
    /// Debugger stop point.
    #[cfg(feature = "debugger")]
    StmtBoundary(TokenId),
    /// Unconditional debugger stop for `breakpoint()`.
    #[cfg(feature = "debugger")]
    Breakpoint(TokenId),
    Const(u16),
    PushNone,
    GetLocal(u16),
    SetLocal(u16),
    /// Stores the top stack value without popping it.
    TeeLocal(u16),
    GetUpvalue(u16),
    SetUpvalue(u16),
    MakeClosure(Box<(u16, Vec<UpvalueSource>)>),
    MakeStaticClosure(u16),
    Pop,
    Dup,
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
    BinaryLocalLocal {
        op: BinaryOp,
        left: u16,
        right: u16,
    },
    BinaryLocalConst {
        op: BinaryOp,
        local: u16,
        constant: u16,
    },
    Neg,
    Not,
    ArrayNew,
    ArrayPush,
    ArraySpread,
    DictSpread,
    ToForeachIterable,
    ArrayLen,
    ArrayGetAt,
    ArrayLenLocal(u16),
    ArrayGetLocalAt {
        array_slot: u16,
        index_slot: u16,
    },
    /// Advances a `foreach` iteration or exits the loop.
    ForeachNext {
        array_slot: u16,
        index_slot: u16,
        value_slot: u16,
        exit_offset: i32,
    },
    ForeachCollect(u16),
    ArraySliceFrom,
    /// Dict-pattern key test: if present, stores the value in `value_slot` and pushes `true`;
    /// else pushes `false`. Fuses what used to be separate `has` + `get` builtin calls.
    DictGetLocalOrFail {
        subject_slot: u16,
        key: Ident,
        value_slot: u16,
    },
    TypeCheck(Ident),
    GetEnvVar(u16),
    /// Looks up an Engine-defined global by name.
    GetExternalGlobal(Ident),
    InterpString(u16),
    SelectorMatch(Box<Selector>),
    SelectorMatchKind(NodeSelectorKind),
    SelectorMatchHeading(u8),
    SelectorMatchWithArgs(Box<(Selector, u8)>),
    CallBuiltin(Ident, u8),
    CallLocal(u16, u8),
    CallValue(u8),
    /// Invokes a pipeline value only when it is callable without explicit arguments.
    MaybeAutoCall,
    /// Executes a `try` closure and invokes its catch closure on errors.
    TryCatch(Box<TryCatchInfo>),
    /// Propagates `break` from a nested `try` closure.
    FlowBreak(bool),
    /// Propagates `continue` from a nested `try` closure.
    FlowContinue,
    RaiseDestructuringFailed,
    Return,
}

/// Payload for [`OpCode::TryCatch`].
#[derive(Debug, Clone)]
/// `try`/`catch` instruction metadata.
pub(crate) struct TryCatchInfo {
    pub(crate) has_binder: bool,
    pub(crate) break_acc_slot: Option<u16>,
    pub(crate) break_offset: Option<i32>,
    pub(crate) continue_offset: Option<i32>,
}

/// A build-dependent memoized boolean.
#[derive(Debug, Default)]
struct BoolCache(
    #[cfg(not(feature = "sync"))] std::cell::Cell<Option<bool>>,
    #[cfg(feature = "sync")] std::sync::OnceLock<bool>,
);

impl BoolCache {
    fn get_or_init(&self, f: impl FnOnce() -> bool) -> bool {
        #[cfg(not(feature = "sync"))]
        {
            if let Some(value) = self.0.get() {
                return value;
            }
            let value = f();
            self.0.set(Some(value));
            value
        }
        #[cfg(feature = "sync")]
        {
            *self.0.get_or_init(f)
        }
    }
}

/// A run of instructions attributed to one source token.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LineEntry {
    pub(crate) pc_start: usize,
    pub(crate) token_id: TokenId,
}

#[derive(Debug, Default)]
/// A compiled bytecode chunk.
pub(crate) struct Chunk {
    pub(crate) code: Vec<OpCode>,
    pub(crate) constants: Vec<RuntimeValue>,
    pub(crate) static_closures: Vec<Shared<Closure>>,
    pub(crate) local_count: u16,
    pub(crate) local_names: Vec<Ident>,
    pub(crate) local_mutable: Vec<bool>,
    pub(crate) upvalue_names: Vec<Ident>,
    pub(crate) lines: Vec<LineEntry>,
    #[cfg(feature = "debugger")]
    pub(crate) debug_nodes: Vec<(TokenId, Shared<Node>)>,
    #[cfg(feature = "debugger")]
    pub(crate) debug_symbols: DebugSymbolTable,
    pub(crate) param_shape: ParamShape,
    captures_local_slots_cache: BoolCache,
}

impl Chunk {
    /// Adds a reusable non-capturing closure.
    pub(crate) fn push_static_closure(&mut self, target_chunk: u16) -> u16 {
        self.static_closures.push(Shared::new(Closure {
            chunk_index: target_chunk,
            upvalues: Vec::new(),
        }));
        (self.static_closures.len() - 1) as u16
    }

    /// Returns whether locals can outlive the current frame.
    pub(crate) fn captures_local_slots(&self) -> bool {
        self.captures_local_slots_cache.get_or_init(|| {
            self.code.iter().any(|op| {
                matches!(
                    op,
                    OpCode::MakeClosure(payload)
                        if payload.1.iter().any(|source| matches!(source, UpvalueSource::Local(_)))
                )
            }) || self.param_shape.bindings.iter().any(|binding| {
                matches!(
                    binding,
                    ParamBinding::Optional(_, _, sources)
                        if sources.iter().any(|source| matches!(source, UpvalueSource::Local(_)))
                )
            })
        })
    }

    /// Adds a constant and returns its index.
    pub(crate) fn push_const(&mut self, value: RuntimeValue) -> u16 {
        self.constants.push(value);
        (self.constants.len() - 1) as u16
    }

    /// Appends an instruction and its source token.
    pub(crate) fn emit(&mut self, op: OpCode, token_id: TokenId) -> usize {
        let pc = self.code.len();
        if self.lines.last().map(|entry| entry.token_id) != Some(token_id) {
            self.lines.push(LineEntry { pc_start: pc, token_id });
        }
        self.code.push(op);
        pc
    }

    /// Returns the source token for an instruction.
    pub(crate) fn token_at(&self, pc: usize) -> Option<TokenId> {
        self.lines
            .partition_point(|entry| entry.pc_start <= pc)
            .checked_sub(1)
            .map(|i| self.lines[i].token_id)
    }

    /// Patches a jump to the current instruction.
    pub(crate) fn patch_jump(&mut self, at: usize) {
        let offset = (self.code.len() - at - 1) as i32;
        match &mut self.code[at] {
            OpCode::Jump(o) | OpCode::JumpIfFalse(o) => *o = offset,
            OpCode::ForeachNext { exit_offset, .. } => *exit_offset = offset,
            _ => unreachable!("patch_jump target is not a jump instruction"),
        }
    }

    /// Patches a `try` break target to the current instruction.
    pub(crate) fn patch_try_break(&mut self, at: usize) {
        let offset = (self.code.len() - at - 1) as i32;
        match &mut self.code[at] {
            OpCode::TryCatch(info) => info.break_offset = Some(offset),
            _ => unreachable!("flow-break target is not a TryCatch instruction"),
        }
    }

    /// Patches a `try` continue target.
    pub(crate) fn patch_try_continue_to(&mut self, at: usize, target: usize) {
        let offset = target as i32 - at as i32 - 1;
        match &mut self.code[at] {
            OpCode::TryCatch(info) => info.continue_offset = Some(offset),
            _ => unreachable!("flow-continue target is not a TryCatch instruction"),
        }
    }

    /// Returns an offset from the next instruction to `target`.
    pub(crate) fn backward_offset(&self, target: usize) -> i32 {
        (target as i32) - (self.code.len() as i32) - 1
    }
}

/// A structural bytecode error emitted by the compiler's post-generation verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
/// A bytecode verification failure.
pub(crate) enum BytecodeError {
    EmptyChunk(usize),
    MissingReturn(usize),
    TooManyChunks(usize),
    TooManyConstants { chunk: usize, count: usize },
    TooManyLocals { chunk: usize, count: usize },
    TooManyUpvalues { chunk: usize, count: usize },
    TooManyStaticClosures { chunk: usize, count: usize },
    ConstantOutOfBounds { chunk: usize, pc: usize, index: u16 },
    LocalOutOfBounds { chunk: usize, pc: usize, slot: u16 },
    UpvalueOutOfBounds { chunk: usize, pc: usize, index: u16 },
    ChunkOutOfBounds { chunk: usize, pc: usize, target: u16 },
    StaticClosureOutOfBounds { chunk: usize, pc: usize, index: u16 },
    JumpOutOfBounds { chunk: usize, pc: usize, target: isize },
}

impl fmt::Display for BytecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyChunk(chunk) => write!(f, "chunk {chunk} has no instructions"),
            Self::MissingReturn(chunk) => write!(f, "chunk {chunk} does not end in Return"),
            Self::TooManyChunks(count) => write!(f, "bytecode has {count} chunks; the VM limit is 65536"),
            Self::TooManyConstants { chunk, count } => {
                write!(f, "chunk {chunk} has {count} constants; the VM limit is 65536")
            }
            Self::TooManyLocals { chunk, count } => {
                write!(f, "chunk {chunk} has {count} local slots; the VM limit is 65536")
            }
            Self::TooManyUpvalues { chunk, count } => {
                write!(f, "chunk {chunk} has {count} upvalues; the VM limit is 65536")
            }
            Self::TooManyStaticClosures { chunk, count } => {
                write!(f, "chunk {chunk} has {count} static closures; the VM limit is 65536")
            }
            Self::ConstantOutOfBounds { chunk, pc, index } => {
                write!(f, "chunk {chunk} pc {pc} references constant {index} out of bounds")
            }
            Self::LocalOutOfBounds { chunk, pc, slot } => {
                write!(f, "chunk {chunk} pc {pc} references local slot {slot} out of bounds")
            }
            Self::UpvalueOutOfBounds { chunk, pc, index } => {
                write!(f, "chunk {chunk} pc {pc} references upvalue {index} out of bounds")
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

/// Applies local bytecode rewrites.
pub(crate) fn optimize_chunks(chunks: &mut [Chunk]) {
    for chunk in chunks {
        optimize_chunk(chunk);
    }
}

fn optimize_chunk(chunk: &mut Chunk) {
    if chunk.code.is_empty() {
        return;
    }

    let has_rewrite = chunk.code.iter().enumerate().any(|(pc, op)| {
        matches!(
            (op, chunk.code.get(pc + 1)),
            (OpCode::Const(_), Some(OpCode::Pop))
                | (OpCode::GetLocal(_), Some(OpCode::SetLocal(_)))
                | (OpCode::SetLocal(_), Some(OpCode::GetLocal(_)))
                | (OpCode::Jump(0), _)
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
            (OpCode::SetLocal(set_slot), Some(OpCode::GetLocal(get_slot)))
                if set_slot == get_slot && !targets.contains(&pc) && !targets.contains(&(pc + 1)) =>
            {
                let slot = *set_slot;
                old_code[pc] = OpCode::TeeLocal(slot);
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

fn jump_targets(code: &[OpCode]) -> std::collections::BTreeSet<usize> {
    let mut targets = std::collections::BTreeSet::new();
    for (pc, op) in code.iter().enumerate() {
        match op {
            OpCode::Jump(offset) | OpCode::JumpIfFalse(offset) => {
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
        OpCode::TryCatch(info) => OpCode::TryCatch(Box::new(TryCatchInfo {
            break_offset: info.break_offset.map(rewrite),
            continue_offset: info.continue_offset.map(rewrite),
            ..*info
        })),
        other => other,
    }
}

pub(crate) fn jump_target(pc: usize, offset: i32) -> Option<usize> {
    pc.checked_add(1)?.checked_add_signed(offset as isize)
}

/// Verifies generated bytecode.
pub(crate) fn verify_chunks(chunks: &[Chunk]) -> Result<(), BytecodeError> {
    if chunks.len() > usize::from(u16::MAX) + 1 {
        return Err(BytecodeError::TooManyChunks(chunks.len()));
    }
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        if chunk.code.is_empty() {
            return Err(BytecodeError::EmptyChunk(chunk_index));
        }
        if !matches!(chunk.code.last(), Some(OpCode::Return)) {
            return Err(BytecodeError::MissingReturn(chunk_index));
        }
        let max_entries = usize::from(u16::MAX) + 1;
        if chunk.constants.len() > max_entries {
            return Err(BytecodeError::TooManyConstants {
                chunk: chunk_index,
                count: chunk.constants.len(),
            });
        }
        if chunk.local_names.len() > max_entries {
            return Err(BytecodeError::TooManyLocals {
                chunk: chunk_index,
                count: chunk.local_names.len(),
            });
        }
        if chunk.upvalue_names.len() > max_entries {
            return Err(BytecodeError::TooManyUpvalues {
                chunk: chunk_index,
                count: chunk.upvalue_names.len(),
            });
        }
        if chunk.static_closures.len() > max_entries {
            return Err(BytecodeError::TooManyStaticClosures {
                chunk: chunk_index,
                count: chunk.static_closures.len(),
            });
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
                OpCode::GetLocal(slot)
                | OpCode::SetLocal(slot)
                | OpCode::TeeLocal(slot)
                | OpCode::CallLocal(slot, _)
                | OpCode::ForeachCollect(slot)
                | OpCode::ArrayLenLocal(slot) => {
                    if *slot >= chunk.local_count {
                        return Err(BytecodeError::LocalOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            slot: *slot,
                        });
                    }
                }
                OpCode::BinaryLocalLocal { left, right, .. } => {
                    for slot in [left, right] {
                        if *slot >= chunk.local_count {
                            return Err(BytecodeError::LocalOutOfBounds {
                                chunk: chunk_index,
                                pc,
                                slot: *slot,
                            });
                        }
                    }
                }
                OpCode::BinaryLocalConst { local, constant, .. } => {
                    if *local >= chunk.local_count {
                        return Err(BytecodeError::LocalOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            slot: *local,
                        });
                    }
                    if *constant as usize >= chunk.constants.len() {
                        return Err(BytecodeError::ConstantOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            index: *constant,
                        });
                    }
                }
                OpCode::ArrayGetLocalAt { array_slot, index_slot } => {
                    for slot in [array_slot, index_slot] {
                        if *slot >= chunk.local_count {
                            return Err(BytecodeError::LocalOutOfBounds {
                                chunk: chunk_index,
                                pc,
                                slot: *slot,
                            });
                        }
                    }
                }
                OpCode::DictGetLocalOrFail {
                    subject_slot,
                    value_slot,
                    ..
                } => {
                    for slot in [subject_slot, value_slot] {
                        if *slot >= chunk.local_count {
                            return Err(BytecodeError::LocalOutOfBounds {
                                chunk: chunk_index,
                                pc,
                                slot: *slot,
                            });
                        }
                    }
                }
                OpCode::ForeachNext {
                    array_slot,
                    index_slot,
                    value_slot,
                    exit_offset,
                } => {
                    for slot in [array_slot, index_slot, value_slot] {
                        if *slot >= chunk.local_count {
                            return Err(BytecodeError::LocalOutOfBounds {
                                chunk: chunk_index,
                                pc,
                                slot: *slot,
                            });
                        }
                    }
                    verify_jump_target(chunk, chunk_index, pc, *exit_offset)?;
                }
                OpCode::GetUpvalue(index) | OpCode::SetUpvalue(index) => {
                    if *index as usize >= chunk.upvalue_names.len() {
                        return Err(BytecodeError::UpvalueOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            index: *index,
                        });
                    }
                }
                OpCode::MakeClosure(payload) => {
                    verify_chunk_target(chunks, chunk_index, pc, payload.0)?;
                    verify_upvalue_sources(chunk, chunk_index, pc, &payload.1)?;
                }
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
                OpCode::TryCatch(info) => {
                    if let Some(slot) = info.break_acc_slot
                        && slot >= chunk.local_count
                    {
                        return Err(BytecodeError::LocalOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            slot,
                        });
                    }
                    if let Some(offset) = info.break_offset {
                        verify_jump_target(chunk, chunk_index, pc, offset)?;
                    }
                    if let Some(offset) = info.continue_offset {
                        verify_jump_target(chunk, chunk_index, pc, offset)?;
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
            if let ParamBinding::Optional(_, default_chunk, sources) = binding {
                verify_chunk_target(chunks, chunk_index, chunk.code.len() - 1, *default_chunk)?;
                verify_upvalue_sources(chunk, chunk_index, chunk.code.len() - 1, sources)?;
            }
        }
    }
    Ok(())
}

fn verify_upvalue_sources(
    chunk: &Chunk,
    chunk_index: usize,
    pc: usize,
    sources: &[UpvalueSource],
) -> Result<(), BytecodeError> {
    for source in sources {
        match source {
            UpvalueSource::Local(slot) if *slot >= chunk.local_count => {
                return Err(BytecodeError::LocalOutOfBounds {
                    chunk: chunk_index,
                    pc,
                    slot: *slot,
                });
            }
            UpvalueSource::Upvalue(index) if *index as usize >= chunk.upvalue_names.len() => {
                return Err(BytecodeError::UpvalueOutOfBounds {
                    chunk: chunk_index,
                    pc,
                    index: *index,
                });
            }
            _ => {}
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
    use rstest::rstest;

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
    fn peephole_fuses_set_local_get_local_into_tee_local() {
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
            [OpCode::Const(0), OpCode::TeeLocal(0), OpCode::Return]
        ));
    }

    #[test]
    fn peephole_does_not_fuse_set_local_get_local_across_a_jump_target() {
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
            [
                OpCode::JumpIfFalse(1),
                OpCode::SetLocal(0),
                OpCode::GetLocal(0),
                OpCode::Return
            ]
        ));
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

    #[test]
    fn opcode_stays_compact() {
        assert_eq!(std::mem::size_of::<OpCode>(), 16);
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
    #[case::get_local(vec![OpCode::GetLocal(0), OpCode::Pop, OpCode::Return])]
    #[case::set_local(vec![OpCode::PushNone, OpCode::SetLocal(0), OpCode::Return])]
    #[case::tee_local(vec![OpCode::PushNone, OpCode::TeeLocal(0), OpCode::Pop, OpCode::Return])]
    #[case::call_local(vec![OpCode::CallLocal(0, 0), OpCode::Pop, OpCode::Return])]
    #[case::foreach_collect(vec![OpCode::ForeachCollect(0), OpCode::Return])]
    #[case::array_len_local(vec![OpCode::ArrayLenLocal(0), OpCode::Pop, OpCode::Return])]
    #[case::binary_local_local(vec![
        OpCode::BinaryLocalLocal { op: BinaryOp::Add, left: 0, right: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
    #[case::binary_local_const(vec![
        OpCode::BinaryLocalConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
    #[case::array_get_local_at(vec![
        OpCode::ArrayGetLocalAt { array_slot: 0, index_slot: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
    #[case::foreach_next(vec![
        OpCode::ForeachNext { array_slot: 0, index_slot: 0, value_slot: 0, exit_offset: 1 },
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

    #[rstest]
    #[case::const_(vec![OpCode::Const(0), OpCode::Pop, OpCode::Return])]
    #[case::get_env_var(vec![OpCode::GetEnvVar(0), OpCode::Pop, OpCode::Return])]
    #[case::binary_local_const(vec![
        OpCode::BinaryLocalConst { op: BinaryOp::Add, local: 0, constant: 0 },
        OpCode::Pop,
        OpCode::Return,
    ])]
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
}
