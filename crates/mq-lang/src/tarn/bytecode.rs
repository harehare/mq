#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSymbolTable;
use crate::Ident;
#[cfg(feature = "debugger")]
use crate::Shared;
use crate::ast::TokenId;
#[cfg(feature = "debugger")]
use crate::ast::node::Node;
use crate::eval::runtime_value::RuntimeValue;
use crate::selector::Selector;

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
