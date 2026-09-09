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

/// Compile-time frame metadata for a capture-free static call with a common exact arity.
///
/// The dedicated call opcodes carrying this target avoid indexing the chunk table before a
/// callee frame starts. Chunks whose locals are captured retain the generic call path, because
/// their per-slot cell layout cannot be represented by this compact payload.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StaticExactCallTarget {
    pub(crate) chunk_index: u16,
    pub(crate) local_count: u16,
}

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
    /// Copies one local slot to another without using the operand stack.
    CopyLocal {
        source: u16,
        destination: u16,
    },
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
    SelectorMatchWithArgs(Box<(Selector, u16)>),
    CallBuiltin(Ident, u16),
    /// Calls a capture-free fixed-arity chunk through the checked fallback path.
    CallStatic(u16, u16),
    /// Calls a capture-free fixed-arity chunk with exactly its declared arguments.
    CallStaticExact(u16, u16),
    /// Calls a capture-free zero-argument chunk with its frame metadata embedded.
    CallStaticExact0(StaticExactCallTarget),
    /// Calls a capture-free one-argument chunk with its frame metadata embedded.
    CallStaticExact1(StaticExactCallTarget),
    /// Calls a capture-free two-argument chunk with its frame metadata embedded.
    CallStaticExact2(StaticExactCallTarget),
    /// Calls a capture-free fixed-arity chunk with the pipeline value as its first argument.
    CallStaticImplicitSelf(u16, u16),
    /// Recursively calls the current fixed-arity chunk through the checked fallback path.
    CallSelf(u16),
    /// Recursively calls the current chunk with exactly its declared arguments.
    CallSelfExact(u16),
    /// Recursively calls the current zero-argument chunk.
    CallSelfExact0,
    /// Recursively calls the current one-argument chunk.
    CallSelfExact1,
    /// Recursively calls the current two-argument chunk.
    CallSelfExact2,
    /// Recursively calls the current chunk with the pipeline value as its first argument.
    CallSelfImplicitSelf(u16),
    CallLocal(u16, u16),
    /// Calls an immutable upvalue without first placing its closure on the operand stack.
    CallUpvalue(u16, u16),
    CallValue(u16),
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
    /// Sorted local slots whose cells are captured by a nested closure or default expression.
    /// All remaining slots can stay as direct values in the interpreter frame.
    captured_local_slots: Vec<u16>,
}

impl Chunk {
    /// Adds a reusable non-capturing closure.
    pub(crate) fn push_static_closure(&mut self, target_chunk: u16) -> u16 {
        self.static_closures.push(Shared::new(Closure {
            chunk_index: target_chunk,
            upvalues: None,
        }));
        (self.static_closures.len() - 1) as u16
    }

    /// Computes the local slots that a closure or default expression captures.
    ///
    /// This runs after bytecode optimization, so the interpreter can choose its local storage
    /// layout without rescanning instructions each time a frame is entered.
    pub(crate) fn refresh_captured_local_slots(&mut self) {
        let mut captured = vec![false; self.local_count as usize];
        let mut mark_sources = |sources: &[UpvalueSource]| {
            for source in sources {
                if let UpvalueSource::Local(slot) = source
                    && let Some(captured) = captured.get_mut(*slot as usize)
                {
                    *captured = true;
                }
            }
        };
        for opcode in &self.code {
            if let OpCode::MakeClosure(payload) = opcode {
                mark_sources(&payload.1);
            }
        }
        for binding in &self.param_shape.bindings {
            if let ParamBinding::Optional(_, _, sources) = binding {
                mark_sources(sources);
            }
        }
        self.captured_local_slots = captured
            .into_iter()
            .enumerate()
            .filter_map(|(slot, is_captured)| is_captured.then_some(slot as u16))
            .collect();
    }

    /// Returns whether any local can outlive the current frame.
    pub(crate) fn captures_local_slots(&self) -> bool {
        !self.captured_local_slots.is_empty()
    }

    /// Returns the finalized list of local slots that need independently shared cells.
    pub(crate) fn captured_local_slots(&self) -> &[u16] {
        &self.captured_local_slots
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
    TooManyConstants {
        chunk: usize,
        count: usize,
    },
    TooManyLocals {
        chunk: usize,
        count: usize,
    },
    TooManyUpvalues {
        chunk: usize,
        count: usize,
    },
    TooManyStaticClosures {
        chunk: usize,
        count: usize,
    },
    ConstantOutOfBounds {
        chunk: usize,
        pc: usize,
        index: u16,
    },
    LocalOutOfBounds {
        chunk: usize,
        pc: usize,
        slot: u16,
    },
    UpvalueOutOfBounds {
        chunk: usize,
        pc: usize,
        index: u16,
    },
    ChunkOutOfBounds {
        chunk: usize,
        pc: usize,
        target: u16,
    },
    StaticClosureOutOfBounds {
        chunk: usize,
        pc: usize,
        index: u16,
    },
    JumpOutOfBounds {
        chunk: usize,
        pc: usize,
        target: isize,
    },
    ClosureCaptureMismatch {
        chunk: usize,
        pc: usize,
        target: u16,
        expected: usize,
        actual: usize,
    },
    StaticCallTargetInvalid {
        chunk: usize,
        pc: usize,
        target: u16,
    },
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
            Self::ClosureCaptureMismatch {
                chunk,
                pc,
                target,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "chunk {chunk} pc {pc} makes a closure over chunk {target} with {actual} captures, but it expects {expected}"
                )
            }
            Self::StaticCallTargetInvalid { chunk, pc, target } => {
                write!(f, "chunk {chunk} pc {pc} directly calls invalid static chunk {target}")
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

/// Rewrites common capture-free exact static calls after local capture metadata is finalized.
pub(crate) fn specialize_static_exact_calls(chunks: &mut [Chunk]) {
    let targets: Vec<Option<StaticExactCallTarget>> = chunks
        .iter()
        .enumerate()
        .map(|(chunk_index, chunk)| {
            (!chunk.captures_local_slots()).then_some(StaticExactCallTarget {
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
            (OpCode::GetLocal(source), Some(OpCode::SetLocal(destination)))
                if !targets.contains(&pc) && !targets.contains(&(pc + 1)) =>
            {
                old_code[pc] = OpCode::CopyLocal {
                    source: *source,
                    destination: *destination,
                };
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
                OpCode::CopyLocal { source, destination } => {
                    for slot in [source, destination] {
                        if *slot >= chunk.local_count {
                            return Err(BytecodeError::LocalOutOfBounds {
                                chunk: chunk_index,
                                pc,
                                slot: *slot,
                            });
                        }
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
                OpCode::CallUpvalue(index, _) => {
                    if *index as usize >= chunk.upvalue_names.len() {
                        return Err(BytecodeError::UpvalueOutOfBounds {
                            chunk: chunk_index,
                            pc,
                            index: *index,
                        });
                    }
                }
                OpCode::MakeClosure(payload) => {
                    let (target, sources) = payload.as_ref();
                    verify_chunk_target(chunks, chunk_index, pc, *target)?;
                    verify_upvalue_sources(chunk, chunk_index, pc, sources)?;
                    verify_closure_capture_count(chunks, chunk_index, pc, *target, sources.len())?;
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
                    verify_closure_capture_count(
                        chunks,
                        chunk_index,
                        pc,
                        closure.chunk_index,
                        closure.upvalues.as_ref().map_or(0, |upvalues| upvalues.len()),
                    )?;
                }
                OpCode::CallStatic(target, _)
                | OpCode::CallStaticExact(target, _)
                | OpCode::CallStaticImplicitSelf(target, _) => {
                    verify_chunk_target(chunks, chunk_index, pc, *target)?;
                    let callee = &chunks[*target as usize];
                    if !callee.upvalue_names.is_empty() || callee.param_shape.fixed_required_arity().is_none() {
                        return Err(BytecodeError::StaticCallTargetInvalid {
                            chunk: chunk_index,
                            pc,
                            target: *target,
                        });
                    }
                    let arity = callee.param_shape.required;
                    match op {
                        OpCode::CallStaticExact(_, argc) if arity != *argc as usize => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: *target,
                            });
                        }
                        OpCode::CallStaticImplicitSelf(_, argc) if arity == 0 || arity != *argc as usize + 1 => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: *target,
                            });
                        }
                        _ => {}
                    }
                }
                OpCode::CallStaticExact0(target)
                | OpCode::CallStaticExact1(target)
                | OpCode::CallStaticExact2(target) => {
                    verify_chunk_target(chunks, chunk_index, pc, target.chunk_index)?;
                    let callee = &chunks[target.chunk_index as usize];
                    let expected_arity = match op {
                        OpCode::CallStaticExact0(_) => 0,
                        OpCode::CallStaticExact1(_) => 1,
                        OpCode::CallStaticExact2(_) => 2,
                        _ => unreachable!("the outer match limits the opcode variants"),
                    };
                    if !callee.upvalue_names.is_empty()
                        || callee.param_shape.fixed_required_arity() != Some(expected_arity)
                        || callee.captures_local_slots()
                        || callee.local_count != target.local_count
                    {
                        return Err(BytecodeError::StaticCallTargetInvalid {
                            chunk: chunk_index,
                            pc,
                            target: target.chunk_index,
                        });
                    }
                }
                OpCode::CallSelf(_)
                | OpCode::CallSelfExact(_)
                | OpCode::CallSelfExact0
                | OpCode::CallSelfExact1
                | OpCode::CallSelfExact2
                | OpCode::CallSelfImplicitSelf(_) => {
                    let Some(arity) = chunk.param_shape.fixed_required_arity() else {
                        return Err(BytecodeError::StaticCallTargetInvalid {
                            chunk: chunk_index,
                            pc,
                            target: chunk_index as u16,
                        });
                    };
                    match op {
                        OpCode::CallSelfExact(argc) if arity != *argc as usize => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: chunk_index as u16,
                            });
                        }
                        OpCode::CallSelfImplicitSelf(argc) if arity == 0 || arity != *argc as usize + 1 => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: chunk_index as u16,
                            });
                        }
                        OpCode::CallSelfExact0 if arity != 0 => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: chunk_index as u16,
                            });
                        }
                        OpCode::CallSelfExact1 if arity != 1 => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: chunk_index as u16,
                            });
                        }
                        OpCode::CallSelfExact2 if arity != 2 => {
                            return Err(BytecodeError::StaticCallTargetInvalid {
                                chunk: chunk_index,
                                pc,
                                target: chunk_index as u16,
                            });
                        }
                        _ => {}
                    }
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
                let pc = chunk.code.len() - 1;
                verify_chunk_target(chunks, chunk_index, pc, *default_chunk)?;
                verify_upvalue_sources(chunk, chunk_index, pc, sources)?;
                verify_closure_capture_count(chunks, chunk_index, pc, *default_chunk, sources.len())?;
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

/// Ensures a closure's capture count matches the target chunk's upvalue count, since
/// `GetUpvalue`/`SetUpvalue` index the runtime `upvalues` array unchecked. Call only after
/// `verify_chunk_target` confirms `target` is in bounds.
fn verify_closure_capture_count(
    chunks: &[Chunk],
    chunk_index: usize,
    pc: usize,
    target: u16,
    capture_count: usize,
) -> Result<(), BytecodeError> {
    let expected = chunks[target as usize].upvalue_names.len();
    if capture_count != expected {
        return Err(BytecodeError::ClosureCaptureMismatch {
            chunk: chunk_index,
            pc,
            target,
            expected,
            actual: capture_count,
        });
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
    fn peephole_fuses_local_copy_without_changing_jump_targets() {
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
                OpCode::GetLocal(1),
                OpCode::Return,
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
    #[case::get_local(vec![OpCode::GetLocal(0), OpCode::Pop, OpCode::Return])]
    #[case::set_local(vec![OpCode::PushNone, OpCode::SetLocal(0), OpCode::Return])]
    #[case::tee_local(vec![OpCode::PushNone, OpCode::TeeLocal(0), OpCode::Pop, OpCode::Return])]
    #[case::copy_local(vec![
        OpCode::CopyLocal {
            source: 0,
            destination: 0,
        },
        OpCode::Return,
    ])]
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

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn widened_call_counts_keep_opcode_size() {
        // `CallBuiltin(Ident, ..)` already determines the enum's 64-bit layout, so widening
        // call counts from `u8` to `u16` must not inflate the instruction stream.
        assert_eq!(std::mem::size_of::<OpCode>(), 16);
    }
}
