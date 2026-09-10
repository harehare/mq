use super::bytecode::Chunk;
use crate::number::Number;
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
    /// Absent for the common capture-free closure. Capturing closures share their cells with
    /// call frames, avoiding a deep copy on every call.
    pub(crate) upvalues: Option<Shared<Vec<Cell>>>,
}

impl std::fmt::Debug for Closure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Closure")
            .field("chunk_index", &self.chunk_index)
            .field(
                "upvalue_count",
                &self.upvalues.as_ref().map_or(0, |upvalues| upvalues.len()),
            )
            .finish()
    }
}

/// A VM closure stored as a runtime value.
#[derive(Clone)]
pub(crate) struct VmClosureValue {
    pub(crate) chunks: Shared<Vec<Chunk>>,
    pub(crate) chunk_index: u16,
    pub(crate) upvalues: Option<Shared<Vec<Cell>>>,
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

/// One frame's local slots.
///
/// A capturing frame only allocates shared cells for slots a nested closure actually captures;
/// its other slots retain the direct-value representation used by non-capturing frames.
pub(crate) enum Locals {
    #[cfg(not(feature = "sync"))]
    /// A contiguous, exclusively owned local region for a non-capturing frame.
    Flat(Vec<StackValue>),
    #[cfg(not(feature = "sync"))]
    /// Direct slots plus cells at the sparse set of captured slot positions.
    Hybrid {
        slots: Vec<StackValue>,
        captured: Vec<Option<Cell>>,
    },
    Boxed(Vec<Cell>),
}

impl Locals {
    /// Creates a non-capturing frame.
    pub(crate) fn flat(count: usize) -> Self {
        #[cfg(not(feature = "sync"))]
        {
            Locals::Flat((0..count).map(|_| StackValue::Value(RuntimeValue::None)).collect())
        }
        #[cfg(feature = "sync")]
        {
            Locals::boxed(count)
        }
    }

    /// Creates a capture-capable frame.
    pub(crate) fn for_captured_slots(count: usize, captured_slots: &[u16]) -> Self {
        if captured_slots.len() == count {
            return Locals::boxed(count);
        }
        #[cfg(not(feature = "sync"))]
        {
            let mut captured = vec![None; count];
            for &slot in captured_slots {
                if let Some(cell) = captured.get_mut(slot as usize) {
                    *cell = Some(new_cell(StackValue::Value(RuntimeValue::None)));
                }
            }
            Locals::Hybrid {
                slots: (0..count).map(|_| StackValue::Value(RuntimeValue::None)).collect(),
                captured,
            }
        }
        #[cfg(feature = "sync")]
        {
            let _ = captured_slots;
            Locals::boxed(count)
        }
    }

    /// Creates a frame whose every slot is a shared cell.
    fn boxed(count: usize) -> Self {
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
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, .. } => slots.len(),
            Locals::Boxed(slots) => slots.len(),
        }
    }

    /// Clears slots from `from` onward.
    pub(crate) fn reset_from(&mut self, from: usize) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                let from = from.min(slots.len());
                for slot in &mut slots[from..] {
                    *slot = StackValue::Value(RuntimeValue::None);
                }
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                let from = from.min(slots.len());
                for index in from..slots.len() {
                    if let Some(cell) = &captured[index] {
                        write_cell(cell, StackValue::Value(RuntimeValue::None));
                    } else {
                        slots[index] = StackValue::Value(RuntimeValue::None);
                    }
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
            Locals::Flat(slots) => slots[slot as usize].clone(),
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => captured[slot as usize]
                .as_ref()
                .map_or_else(|| slots[slot as usize].clone(), read_cell),
            Locals::Boxed(slots) => read_cell(&slots[slot as usize]),
        }
    }

    /// Reads a local slot when it is in range.
    pub(crate) fn get_checked(&self, slot: u16) -> Option<StackValue> {
        ((slot as usize) < self.len()).then(|| self.get(slot))
    }

    /// Writes a local slot.
    pub(crate) fn set(&mut self, slot: u16, value: StackValue) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => slots[slot as usize] = value,
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                if let Some(cell) = &captured[slot as usize] {
                    write_cell(cell, value);
                } else {
                    slots[slot as usize] = value;
                }
            }
            Locals::Boxed(slots) => write_cell(&slots[slot as usize], value),
        }
    }

    /// Like [`Locals::get`], without the bounds check.
    ///
    /// # Safety
    /// `slot` must be `< self.len()` (guaranteed by `bytecode::verify_chunks` for any
    /// GetLocal/SetLocal/TeeLocal/BinaryLocalLocal/BinaryLocalConst/UpdateLocalConst/UpdateLocalLocal/ArrayLenLocal/
    /// ArrayGetLocalAt opcode slot).
    #[inline(always)]
    pub(crate) unsafe fn get_unchecked(&self, slot: u16) -> StackValue {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                // SAFETY: inherited from `Locals::get_unchecked`'s caller contract.
                unsafe { slots.get_unchecked(slot as usize) }.clone()
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                // SAFETY: inherited from `Locals::get_unchecked`'s caller contract.
                match unsafe { captured.get_unchecked(slot as usize) } {
                    Some(cell) => read_cell(cell),
                    // SAFETY: inherited from `Locals::get_unchecked`'s caller contract.
                    None => unsafe { slots.get_unchecked(slot as usize) }.clone(),
                }
            }
            Locals::Boxed(slots) => {
                // SAFETY: inherited from `Locals::get_unchecked`'s caller contract.
                read_cell(unsafe { slots.get_unchecked(slot as usize) })
            }
        }
    }

    /// Like [`Locals::set`], without the bounds check. See [`Locals::get_unchecked`].
    #[inline(always)]
    pub(crate) unsafe fn set_unchecked(&mut self, slot: u16, value: StackValue) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                // SAFETY: inherited from `Locals::set_unchecked`'s caller contract.
                *unsafe { slots.get_unchecked_mut(slot as usize) } = value;
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                // SAFETY: inherited from `Locals::set_unchecked`'s caller contract.
                if let Some(cell) = unsafe { captured.get_unchecked(slot as usize) } {
                    write_cell(cell, value);
                } else {
                    // SAFETY: inherited from `Locals::set_unchecked`'s caller contract.
                    *unsafe { slots.get_unchecked_mut(slot as usize) } = value;
                }
            }
            Locals::Boxed(slots) => {
                // SAFETY: inherited from `Locals::set_unchecked`'s caller contract.
                write_cell(unsafe { slots.get_unchecked(slot as usize) }, value);
            }
        }
    }

    /// Returns a captured cell.
    pub(crate) fn cell(&self, slot: u16) -> &Cell {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(_) => unreachable!("a non-capturing chunk's locals can't be captured"),
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { captured, .. } => captured[slot as usize]
                .as_ref()
                .expect("bytecode attempted to capture a local slot without a cell"),
            Locals::Boxed(slots) => &slots[slot as usize],
        }
    }

    /// Appends to an array stored in a local slot.
    pub(crate) fn append_to_array_at(&mut self, slot: u16, value: RuntimeValue) -> Result<(), &'static str> {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                let StackValue::Value(RuntimeValue::Array(array)) = &mut slots[slot as usize] else {
                    return Err("ForeachCollect accumulator is not an array");
                };
                crate::runtime::runtime_value::array_mut(array).push(value);
                Ok(())
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => match &captured[slot as usize] {
                Some(cell) => append_to_array_cell(cell, value),
                None => {
                    let StackValue::Value(RuntimeValue::Array(array)) = &mut slots[slot as usize] else {
                        return Err("ForeachCollect accumulator is not an array");
                    };
                    crate::runtime::runtime_value::array_mut(array).push(value);
                    Ok(())
                }
            },
            Locals::Boxed(slots) => append_to_array_cell(&slots[slot as usize], value),
        }
    }

    /// Advances a `foreach` loop and updates its index, loop value, and implicit-self slots.
    ///
    /// # Safety
    /// `array_slot`, `index_slot`, and `value_slot` must be valid local slots. The bytecode
    /// verifier establishes this for every `ForeachNext` instruction before execution.
    #[inline(always)]
    pub(crate) unsafe fn foreach_next(
        &mut self,
        array_slot: u16,
        index_slot: u16,
        value_slot: u16,
        self_slot: u16,
    ) -> Result<Option<RuntimeValue>, &'static str> {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                let index_value = {
                    let index = unsafe { slots.get_unchecked(index_slot as usize) };
                    let StackValue::Value(RuntimeValue::Number(index)) = index else {
                        return Err("ForeachNext has invalid loop state");
                    };
                    index.value()
                };

                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                let value = {
                    let array = unsafe { slots.get_unchecked(array_slot as usize) };
                    let StackValue::Value(RuntimeValue::Array(array)) = array else {
                        return Err("ForeachNext array slot is not an array");
                    };
                    if index_value >= array.len() as f64 {
                        return Ok(None);
                    }
                    array.get(index_value as usize).cloned().unwrap_or(RuntimeValue::None)
                };

                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                *unsafe { slots.get_unchecked_mut(index_slot as usize) } =
                    StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0)));
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                *unsafe { slots.get_unchecked_mut(value_slot as usize) } = StackValue::Value(value.clone());
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                *unsafe { slots.get_unchecked_mut(self_slot as usize) } = StackValue::Value(value.clone());
                Ok(Some(value))
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { .. } => {
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                let index = unsafe { self.get_unchecked(index_slot) };
                let StackValue::Value(RuntimeValue::Number(index)) = index else {
                    return Err("ForeachNext has invalid loop state");
                };
                let index_value = index.value();
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                let array = unsafe { self.get_unchecked(array_slot) };
                let StackValue::Value(RuntimeValue::Array(array)) = array else {
                    return Err("ForeachNext array slot is not an array");
                };
                if index_value >= array.len() as f64 {
                    return Ok(None);
                }
                let value = array.get(index_value as usize).cloned().unwrap_or(RuntimeValue::None);
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                unsafe {
                    self.set_unchecked(
                        index_slot,
                        StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0))),
                    );
                    self.set_unchecked(value_slot, StackValue::Value(value.clone()));
                    self.set_unchecked(self_slot, StackValue::Value(value.clone()));
                }
                Ok(Some(value))
            }
            Locals::Boxed(slots) => {
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                let index = read_cell(unsafe { slots.get_unchecked(index_slot as usize) });
                let StackValue::Value(RuntimeValue::Number(index)) = index else {
                    return Err("ForeachNext has invalid loop state");
                };
                let index_value = index.value();
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                let array = read_cell(unsafe { slots.get_unchecked(array_slot as usize) });
                let StackValue::Value(RuntimeValue::Array(array)) = array else {
                    return Err("ForeachNext array slot is not an array");
                };
                if index_value >= array.len() as f64 {
                    return Ok(None);
                }
                let value = array.get(index_value as usize).cloned().unwrap_or(RuntimeValue::None);

                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                write_cell(
                    unsafe { slots.get_unchecked(index_slot as usize) },
                    StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0))),
                );
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                write_cell(
                    unsafe { slots.get_unchecked(value_slot as usize) },
                    StackValue::Value(value.clone()),
                );
                // SAFETY: inherited from `Locals::foreach_next`'s caller contract.
                write_cell(
                    unsafe { slots.get_unchecked(self_slot as usize) },
                    StackValue::Value(value.clone()),
                );
                Ok(Some(value))
            }
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
