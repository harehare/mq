use super::bytecode::Chunk;
use crate::runtime::runtime_value::RuntimeValue;
use crate::{Shared, SharedCell};

/// A shared VM value cell.
pub(crate) type Cell = Shared<SharedCell<StackValue>>;

#[derive(Clone)]
/// A value held on the VM operand stack.
pub(crate) enum StackValue {
    Value(RuntimeValue),
    Closure(Shared<Closure>),
}

/// A closure on the VM operand stack.
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

/// A VM closure stored as a runtime value.
#[derive(Clone)]
pub(crate) struct VmClosureValue {
    pub(crate) chunks: Shared<Vec<Chunk>>,
    pub(crate) chunk_index: u16,
    pub(crate) upvalues: Vec<Cell>,
    pub(crate) bound_args: Vec<RuntimeValue>,
}

impl VmClosureValue {
    /// Converts a stack closure to a runtime closure.
    pub(crate) fn from_closure(chunks: &Shared<Vec<Chunk>>, closure: &Closure) -> Self {
        Self {
            chunks: Shared::clone(chunks),
            chunk_index: closure.chunk_index,
            upvalues: closure.upvalues.clone(),
            bound_args: Vec::new(),
        }
    }
}

/// Creates a shared VM cell.
pub(crate) fn new_cell(value: StackValue) -> Cell {
    Shared::new(SharedCell::new(value))
}

/// One frame's local slots. `Boxed` slots support captures.
pub(crate) enum Locals {
    #[cfg(not(feature = "sync"))]
    Flat(Vec<std::cell::RefCell<StackValue>>),
    Boxed(Vec<Cell>),
}

impl Locals {
    /// Creates a non-capturing frame.
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

    /// Creates a capture-capable frame.
    pub(crate) fn boxed(count: usize) -> Self {
        Locals::Boxed(
            (0..count)
                .map(|_| new_cell(StackValue::Value(RuntimeValue::None)))
                .collect(),
        )
    }

    /// Returns the number of local slots.
    pub(crate) fn len(&self) -> usize {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => slots.len(),
            Locals::Boxed(slots) => slots.len(),
        }
    }

    /// Clears slots from `from` onward.
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

    /// Reads a local slot.
    pub(crate) fn get(&self, slot: u16) -> StackValue {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => slots[slot as usize].borrow().clone(),
            Locals::Boxed(slots) => read_cell(&slots[slot as usize]),
        }
    }

    /// Reads a local slot when it is in range.
    pub(crate) fn get_checked(&self, slot: u16) -> Option<StackValue> {
        ((slot as usize) < self.len()).then(|| self.get(slot))
    }

    /// Writes a local slot.
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

    /// Returns a captured cell.
    pub(crate) fn cell(&self, slot: u16) -> &Cell {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(_) => unreachable!("a non-capturing chunk's locals can't be captured"),
            Locals::Boxed(slots) => &slots[slot as usize],
        }
    }

    /// Reads an array slot's length and element.
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

    /// Appends to an array stored in a local slot.
    pub(crate) fn append_to_array_at(&self, slot: u16, value: RuntimeValue) -> Result<(), &'static str> {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                let mut borrowed = slots[slot as usize].borrow_mut();
                let StackValue::Value(RuntimeValue::Array(array)) = &mut *borrowed else {
                    return Err("ForeachCollect accumulator is not an array");
                };
                crate::runtime::runtime_value::array_mut(array).push(value);
                Ok(())
            }
            Locals::Boxed(slots) => append_to_array_cell(&slots[slot as usize], value),
        }
    }
}

/// Reads a VM cell.
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

/// Writes a VM cell.
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

/// Reads an array cell's length and element.
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

/// Appends to an array cell.
pub(crate) fn append_to_array_cell(cell: &Cell, value: RuntimeValue) -> Result<(), &'static str> {
    #[cfg(not(feature = "sync"))]
    {
        let mut stored = cell.borrow_mut();
        let StackValue::Value(RuntimeValue::Array(array)) = &mut *stored else {
            return Err("ForeachCollect accumulator is not an array");
        };
        crate::runtime::runtime_value::array_mut(array).push(value);
    }
    #[cfg(feature = "sync")]
    {
        let mut stored = cell.write().unwrap();
        let StackValue::Value(RuntimeValue::Array(array)) = &mut *stored else {
            return Err("ForeachCollect accumulator is not an array");
        };
        crate::runtime::runtime_value::array_mut(array).push(value);
    }
    Ok(())
}
