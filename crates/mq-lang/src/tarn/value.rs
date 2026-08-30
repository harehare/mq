#[cfg(feature = "tarn")]
use super::bytecode::Chunk;
use crate::eval::runtime_value::RuntimeValue;
use crate::{Shared, SharedCell};

pub(crate) type Cell = Shared<SharedCell<StackValue>>;

#[derive(Clone)]
pub(crate) enum StackValue {
    Value(RuntimeValue),
    Closure(Shared<Closure>),
}

pub(crate) struct Closure {
    pub(crate) chunk_index: u16,
    pub(crate) upvalues: Vec<Cell>,
}

impl std::fmt::Debug for Closure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Closure")
            .field("chunk_index", &self.chunk_index)
            .field("upvalue_count", &self.upvalues.len())
            .finish()
    }
}

/// A VM closure that has crossed into plain-`RuntimeValue` territory — stored in an
/// array/dict, passed to a native builtin like `partial`, or returned from `Engine::eval`.
/// Unlike `StackValue::Closure` (only ever a transient value *on the VM stack*, scoped to
/// one `run_chunk` call), this is `RuntimeValue::VmClosure`'s payload: it must be callable
/// independent of any particular execution's borrow of the chunk table, so it holds its own
/// `Shared` handle to it (see `CompiledProgram::chunks`) rather than a borrowed slice.
#[cfg(feature = "tarn")]
#[derive(Clone)]
pub(crate) struct VmClosureValue {
    pub(crate) chunks: Shared<Vec<Chunk>>,
    pub(crate) chunk_index: u16,
    pub(crate) upvalues: Vec<Cell>,
    /// Args already supplied via `partial` — prepended (positionally) to whatever args a
    /// later call site supplies, then bound the same as any ordinary call. Empty for a
    /// closure that hasn't been partially applied.
    pub(crate) bound_args: Vec<RuntimeValue>,
}

#[cfg(feature = "tarn")]
impl VmClosureValue {
    pub(crate) fn from_closure(chunks: &Shared<Vec<Chunk>>, closure: &Closure) -> Self {
        Self {
            chunks: Shared::clone(chunks),
            chunk_index: closure.chunk_index,
            upvalues: closure.upvalues.clone(),
            bound_args: Vec::new(),
        }
    }
}

pub(crate) fn new_cell(value: StackValue) -> Cell {
    Shared::new(SharedCell::new(value))
}

pub(crate) fn read_cell(cell: &Cell) -> StackValue {
    #[cfg(not(feature = "sync"))]
    {
        cell.borrow().clone()
    }
    #[cfg(feature = "sync")]
    {
        cell.read().unwrap().clone()
    }
}

pub(crate) fn write_cell(cell: &Cell, value: StackValue) {
    #[cfg(not(feature = "sync"))]
    {
        *cell.borrow_mut() = value;
    }
    #[cfg(feature = "sync")]
    {
        *cell.write().unwrap() = value;
    }
}
