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

/// One frame's local slots. `Boxed` cells support upvalue capture (a nested closure can hold
/// a `Shared` handle to one past this frame's lifetime); `Flat` is for the common case where
/// `Chunk::captures_local_slots()` is false — same `RefCell`-style borrow/clone access as
/// `Boxed`, but inline in the `Vec` instead of behind a separate `Rc` allocation, so a
/// read/write is one memory access instead of two. `Flat` is unavailable under `sync`: a
/// cached program's pooled frames must stay `Send + Sync` there, and plain `RefCell` isn't
/// `Sync` (matching why `Boxed` uses `RwLock` under `sync` via `SharedCell`).
pub(crate) enum Locals {
    #[cfg(not(feature = "sync"))]
    Flat(Vec<std::cell::RefCell<StackValue>>),
    Boxed(Vec<Cell>),
}

impl Locals {
    pub(crate) fn flat(count: usize) -> Self {
        #[cfg(not(feature = "sync"))]
        {
            Locals::Flat(
                (0..count)
                    .map(|_| std::cell::RefCell::new(StackValue::Value(RuntimeValue::None)))
                    .collect(),
            )
        }
        #[cfg(feature = "sync")]
        {
            Locals::boxed(count)
        }
    }

    pub(crate) fn boxed(count: usize) -> Self {
        Locals::Boxed(
            (0..count)
                .map(|_| new_cell(StackValue::Value(RuntimeValue::None)))
                .collect(),
        )
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => slots.len(),
            Locals::Boxed(slots) => slots.len(),
        }
    }

    /// Resets slots in `range` to `None`, leaving the rest untouched (for a pooled frame whose
    /// leading slots a caller is about to overwrite anyway).
    pub(crate) fn reset_from(&self, from: usize) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                for slot in &slots[from.min(slots.len())..] {
                    *slot.borrow_mut() = StackValue::Value(RuntimeValue::None);
                }
            }
            Locals::Boxed(slots) => {
                for slot in &slots[from.min(slots.len())..] {
                    write_cell(slot, StackValue::Value(RuntimeValue::None));
                }
            }
        }
    }

    pub(crate) fn get(&self, slot: u16) -> StackValue {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => slots[slot as usize].borrow().clone(),
            Locals::Boxed(slots) => read_cell(&slots[slot as usize]),
        }
    }

    /// Bounds-checked read, for the debugger's slot-name lookups (which validate the slot
    /// exists in `DebugSymbolTable` before trusting it — everywhere else, a compiled chunk's
    /// slots are already in-bounds by construction).
    pub(crate) fn get_checked(&self, slot: u16) -> Option<StackValue> {
        ((slot as usize) < self.len()).then(|| self.get(slot))
    }

    pub(crate) fn set(&self, slot: u16, value: StackValue) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => *slots[slot as usize].borrow_mut() = value,
            Locals::Boxed(slots) => write_cell(&slots[slot as usize], value),
        }
    }

    /// Like [`Locals::get`], without the bounds check.
    ///
    /// # Safety
    /// `slot` must be `< self.len()` (guaranteed by `bytecode::verify_chunks` for any
    /// GetLocal/SetLocal/TeeLocal/BinaryLocalLocal/BinaryLocalConst/ArrayLenLocal/
    /// ArrayGetLocalAt opcode slot).
    #[inline(always)]
    pub(crate) unsafe fn get_unchecked(&self, slot: u16) -> StackValue {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => unsafe { slots.get_unchecked(slot as usize) }.borrow().clone(),
            Locals::Boxed(slots) => read_cell(unsafe { slots.get_unchecked(slot as usize) }),
        }
    }

    /// Like [`Locals::set`], without the bounds check. See [`Locals::get_unchecked`].
    #[inline(always)]
    pub(crate) unsafe fn set_unchecked(&self, slot: u16, value: StackValue) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => *unsafe { slots.get_unchecked(slot as usize) }.borrow_mut() = value,
            Locals::Boxed(slots) => write_cell(unsafe { slots.get_unchecked(slot as usize) }, value),
        }
    }

    /// Only meaningful on `Boxed`: `Chunk::captures_local_slots()` guarantees a `Flat` frame's
    /// slots are never captured by a nested closure, so this is never reached for `Flat`.
    pub(crate) fn cell(&self, slot: u16) -> &Cell {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(_) => unreachable!("a non-capturing chunk's locals can't be captured"),
            Locals::Boxed(slots) => &slots[slot as usize],
        }
    }

    pub(crate) fn array_len_and_element_at(
        &self,
        slot: u16,
        index: usize,
    ) -> Result<(usize, Option<RuntimeValue>), &'static str> {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                let borrowed = slots[slot as usize].borrow();
                let StackValue::Value(RuntimeValue::Array(array)) = &*borrowed else {
                    return Err("ForeachNext array slot is not an array");
                };
                Ok((array.len(), array.get(index).cloned()))
            }
            Locals::Boxed(slots) => array_len_and_element_at_cell(&slots[slot as usize], index),
        }
    }

    pub(crate) fn append_to_array_at(&self, slot: u16, value: RuntimeValue) -> Result<(), &'static str> {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                let mut borrowed = slots[slot as usize].borrow_mut();
                let StackValue::Value(RuntimeValue::Array(array)) = &mut *borrowed else {
                    return Err("ForeachCollect accumulator is not an array");
                };
                crate::eval::runtime_value::array_mut(array).push(value);
                Ok(())
            }
            Locals::Boxed(slots) => append_to_array_cell(&slots[slot as usize], value),
        }
    }
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

/// Reads an array's length and one element without cloning the enclosing array value.
pub(crate) fn array_len_and_element_at_cell(
    cell: &Cell,
    index: usize,
) -> Result<(usize, Option<RuntimeValue>), &'static str> {
    #[cfg(not(feature = "sync"))]
    {
        let stored = cell.borrow();
        let StackValue::Value(RuntimeValue::Array(array)) = &*stored else {
            return Err("ForeachNext array slot is not an array");
        };
        Ok((array.len(), array.get(index).cloned()))
    }
    #[cfg(feature = "sync")]
    {
        let stored = cell.read().unwrap();
        let StackValue::Value(RuntimeValue::Array(array)) = &*stored else {
            return Err("ForeachNext array slot is not an array");
        };
        Ok((array.len(), array.get(index).cloned()))
    }
}

/// Appends to an array held in a VM cell without cloning the enclosing `RuntimeValue`.
pub(crate) fn append_to_array_cell(cell: &Cell, value: RuntimeValue) -> Result<(), &'static str> {
    #[cfg(not(feature = "sync"))]
    {
        let mut stored = cell.borrow_mut();
        let StackValue::Value(RuntimeValue::Array(array)) = &mut *stored else {
            return Err("ForeachCollect accumulator is not an array");
        };
        crate::eval::runtime_value::array_mut(array).push(value);
    }
    #[cfg(feature = "sync")]
    {
        let mut stored = cell.write().unwrap();
        let StackValue::Value(RuntimeValue::Array(array)) = &mut *stored else {
            return Err("ForeachCollect accumulator is not an array");
        };
        crate::eval::runtime_value::array_mut(array).push(value);
    }
    Ok(())
}
