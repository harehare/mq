use super::bytecode::Chunk;
use crate::number::Number;
use crate::runtime::runtime_value::{RuntimeValue, array_mut, dict_mut};
use crate::tarn::interpreter::coroutine::{
    CoroutineHandle, CoroutineWeakHandle, downgrade_handle, downgrade_self_references_before_resume, same_handle,
    upgrade_handle,
};
use crate::{Shared, SharedCell};

/// A shared VM value cell.
pub(crate) type Cell = Shared<SharedCell<StackValue>>;

/// A non-owning reference to a [`Cell`], used to resync a suspended coroutine's own downgraded
/// copy of a captured variable with the (still strongly-owned) original cell on next resume.
#[cfg(not(feature = "sync"))]
pub(crate) type WeakCell = std::rc::Weak<SharedCell<StackValue>>;
#[cfg(feature = "sync")]
pub(crate) type WeakCell = std::sync::Weak<SharedCell<StackValue>>;

#[derive(Clone)]
/// A value held on the VM operand stack.
pub(crate) enum StackValue {
    Value(RuntimeValue),
    Closure(Shared<Closure>),
    WeakCoroutine(CoroutineWeakHandle),
    /// Like `WeakCoroutine`, but for a self-reference nested inside a captured array/dict: the
    /// container, with `RuntimeValue::WeakCoroutine` markers standing in for cleared entries.
    NestedWeakCoroutine(RuntimeValue),
}

impl StackValue {
    pub(crate) fn upgraded(&self) -> Self {
        match self {
            StackValue::WeakCoroutine(handle) => upgrade_handle(handle)
                .map(|handle| StackValue::Value(RuntimeValue::Coroutine(handle)))
                .unwrap_or(StackValue::Value(RuntimeValue::None)),
            StackValue::NestedWeakCoroutine(value) => StackValue::Value(resolve_weak_coroutines(value)),
            value => value.clone(),
        }
    }

    pub(crate) fn downgrade_coroutine_reference(&mut self, handle: &CoroutineHandle) {
        match self {
            StackValue::Value(RuntimeValue::Coroutine(value)) if same_handle(value, handle) => {
                *self = StackValue::WeakCoroutine(downgrade_handle(handle));
            }
            StackValue::Value(value) if value_contains_self_reference(value, handle) => {
                let mut value = std::mem::take(value);
                clear_self_reference(&mut value, handle);
                *self = StackValue::NestedWeakCoroutine(value);
            }
            _ => {}
        }
    }
}

/// Whether `value` holds `handle`'s own coroutine, directly or nested in an array/dict.
fn value_contains_self_reference(value: &RuntimeValue, handle: &CoroutineHandle) -> bool {
    match value {
        RuntimeValue::Coroutine(v) => same_handle(v, handle),
        RuntimeValue::Array(array) => array.iter().any(|item| value_contains_self_reference(item, handle)),
        RuntimeValue::Dict(map) => map.values().any(|item| value_contains_self_reference(item, handle)),
        _ => false,
    }
}

/// Collects distinct coroutine handles nested in a value.
fn collect_coroutine_handles(value: &RuntimeValue, handles: &mut Vec<CoroutineHandle>) {
    match value {
        RuntimeValue::Coroutine(handle) => {
            if !handles.iter().any(|existing| same_handle(existing, handle)) {
                handles.push(Shared::clone(handle));
            }
        }
        RuntimeValue::Array(array) => {
            for item in array.iter() {
                collect_coroutine_handles(item, handles);
            }
        }
        RuntimeValue::Dict(map) => {
            for item in map.values() {
                collect_coroutine_handles(item, handles);
            }
        }
        _ => {}
    }
}

/// Collects coroutine handles `value` strongly holds (a bare `WeakCoroutine`/`NestedWeakCoroutine`
/// doesn't count — it isn't an owning edge).
pub(crate) fn collect_coroutine_handles_in_stack_value(value: &StackValue, handles: &mut Vec<CoroutineHandle>) {
    if let StackValue::Value(value) = value {
        collect_coroutine_handles(value, handles);
    }
}

/// Like [`collect_coroutine_handles_in_stack_value`], reading `cell`'s raw content directly.
pub(crate) fn collect_coroutine_handles_in_cell(cell: &Cell, handles: &mut Vec<CoroutineHandle>) {
    #[cfg(not(feature = "sync"))]
    collect_coroutine_handles_in_stack_value(&cell.borrow(), handles);
    #[cfg(feature = "sync")]
    collect_coroutine_handles_in_stack_value(&cell.read().unwrap(), handles);
}

/// Cleans up coroutine cycles after their value is stored.
fn downgrade_coroutine_handles(handles: Vec<CoroutineHandle>) {
    for handle in handles {
        downgrade_self_references_before_resume(&handle);
    }
}

/// Downgrades `handle`'s own coroutine, wherever `value` holds it nested in an array/dict, to a
/// resolvable `RuntimeValue::WeakCoroutine`, breaking the reference cycle a suspended coroutine
/// would otherwise form through a captured container. Direct `RuntimeValue::Coroutine` matches
/// are handled separately (see [`StackValue::downgrade_coroutine_reference`] and
/// [`weak_coroutine_cell`]).
fn clear_self_reference(value: &mut RuntimeValue, handle: &CoroutineHandle) {
    match value {
        RuntimeValue::Array(array) if array.iter().any(|item| value_contains_self_reference(item, handle)) => {
            for item in array_mut(array) {
                clear_self_reference(item, handle);
            }
        }
        RuntimeValue::Dict(map) if map.values().any(|item| value_contains_self_reference(item, handle)) => {
            for item in dict_mut(map).values_mut() {
                clear_self_reference(item, handle);
            }
        }
        RuntimeValue::Coroutine(v) if same_handle(v, handle) => {
            *value = RuntimeValue::WeakCoroutine(downgrade_handle(v));
        }
        _ => {}
    }
}

/// Whether `value` holds a `WeakCoroutine` marker, directly or nested in an array/dict.
fn contains_weak_coroutine(value: &RuntimeValue) -> bool {
    match value {
        RuntimeValue::WeakCoroutine(_) => true,
        RuntimeValue::Array(array) => array.iter().any(contains_weak_coroutine),
        RuntimeValue::Dict(map) => map.values().any(contains_weak_coroutine),
        _ => false,
    }
}

/// Resolves every `WeakCoroutine` marker in `value` back to `Coroutine` (or `None`, if the
/// coroutine is truly gone), rebuilding containers only where a marker was actually found.
pub(crate) fn resolve_weak_coroutines(value: &RuntimeValue) -> RuntimeValue {
    match value {
        RuntimeValue::WeakCoroutine(weak) => upgrade_handle(weak)
            .map(RuntimeValue::Coroutine)
            .unwrap_or(RuntimeValue::None),
        RuntimeValue::Array(array) if array.iter().any(contains_weak_coroutine) => {
            RuntimeValue::Array(Shared::new(array.iter().map(resolve_weak_coroutines).collect()))
        }
        RuntimeValue::Dict(map) if map.values().any(contains_weak_coroutine) => RuntimeValue::Dict(Shared::new(
            map.iter().map(|(k, v)| (*k, resolve_weak_coroutines(v))).collect(),
        )),
        _ => value.clone(),
    }
}

/// Detaches a frame-local reference to `cell` from `handle`'s own coroutine nested inside a
/// captured array/dict, without mutating `cell` itself: `cell` may still be the caller's own
/// binding, and downgrading the self-reference there would corrupt data the caller reads later.
/// Returns a new, privately-owned cell holding a sanitized copy for the frame to use instead, or
/// `None` if `cell` holds no such self-reference. Callers must downgrade a reference to the
/// original `cell` before installing the replacement, so `resume`'s `pending_resyncs` handling
/// can restore whatever the caller later writes.
pub(crate) fn sanitize_nested_self_reference(cell: &Cell, handle: &CoroutineHandle) -> Option<Cell> {
    // `read_cell` upgrades a `WeakCoroutine` back into a literal `Coroutine` for ordinary reads,
    // which would make an already-downgraded cell look like a fresh nested self-reference here
    // and spawn a redundant hop that `weak_coroutine_cell` (checked first, on the raw content)
    // already made unnecessary.
    if is_weak_coroutine(cell) {
        return None;
    }
    let StackValue::Value(mut value) = read_cell(cell) else {
        return None;
    };
    if !value_contains_self_reference(&value, handle) {
        return None;
    }
    clear_self_reference(&mut value, handle);
    Some(new_cell(StackValue::NestedWeakCoroutine(value)))
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
            Locals::Flat(slots) => slots[slot as usize].upgraded(),
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => captured[slot as usize]
                .as_ref()
                .map_or_else(|| slots[slot as usize].upgraded(), read_cell),
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
                unsafe { slots.get_unchecked(slot as usize) }.upgraded()
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                // SAFETY: inherited from `Locals::get_unchecked`'s caller contract.
                match unsafe { captured.get_unchecked(slot as usize) } {
                    Some(cell) => read_cell(cell),
                    // SAFETY: inherited from `Locals::get_unchecked`'s caller contract.
                    None => unsafe { slots.get_unchecked(slot as usize) }.upgraded(),
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

    /// Breaks self-reference cycles in every captured cell this frame's locals hold, returning
    /// `(replacement_cell, weak_original)` pairs for `resume` to resync on next entry (see
    /// [`sanitize_nested_self_reference`] and [`weak_coroutine_cell`]).
    pub(crate) fn downgrade_coroutine_references(&mut self, handle: &CoroutineHandle) -> Vec<(Cell, WeakCell)> {
        let mut resyncs = Vec::new();
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                for slot in slots {
                    slot.downgrade_coroutine_reference(handle);
                }
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                for (slot, cell) in slots.iter_mut().zip(captured) {
                    if let Some(cell) = cell {
                        if let Some(weak_cell) = weak_coroutine_cell(cell, handle) {
                            resyncs.push((Shared::clone(&weak_cell), Shared::downgrade(cell)));
                            *cell = weak_cell;
                        } else if let Some(sanitized) = sanitize_nested_self_reference(cell, handle) {
                            resyncs.push((Shared::clone(&sanitized), Shared::downgrade(cell)));
                            *cell = sanitized;
                        }
                    } else {
                        slot.downgrade_coroutine_reference(handle);
                    }
                }
            }
            Locals::Boxed(slots) => {
                for cell in slots {
                    if let Some(weak_cell) = weak_coroutine_cell(cell, handle) {
                        resyncs.push((Shared::clone(&weak_cell), Shared::downgrade(cell)));
                        *cell = weak_cell;
                    } else if let Some(sanitized) = sanitize_nested_self_reference(cell, handle) {
                        resyncs.push((Shared::clone(&sanitized), Shared::downgrade(cell)));
                        *cell = sanitized;
                    }
                }
            }
        }
        resyncs
    }

    /// Read-only sibling of [`Locals::downgrade_coroutine_references`]: collects coroutine
    /// handles these locals strongly hold, for pairwise-mutual-cycle detection.
    pub(crate) fn collect_coroutine_handles(&self, handles: &mut Vec<CoroutineHandle>) {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                for slot in slots {
                    collect_coroutine_handles_in_stack_value(slot, handles);
                }
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => {
                for (slot, cell) in slots.iter().zip(captured) {
                    match cell {
                        Some(cell) => collect_coroutine_handles_in_cell(cell, handles),
                        None => collect_coroutine_handles_in_stack_value(slot, handles),
                    }
                }
            }
            Locals::Boxed(slots) => {
                for cell in slots {
                    collect_coroutine_handles_in_cell(cell, handles);
                }
            }
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
                array_mut(array).push(value);
                Ok(())
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { slots, captured } => match &captured[slot as usize] {
                Some(cell) => append_to_array_cell(cell, value),
                None => {
                    let StackValue::Value(RuntimeValue::Array(array)) = &mut slots[slot as usize] else {
                        return Err("ForeachCollect accumulator is not an array");
                    };
                    array_mut(array).push(value);
                    Ok(())
                }
            },
            Locals::Boxed(slots) => append_to_array_cell(&slots[slot as usize], value),
        }
    }

    /// Advances a `foreach` loop and updates its index, loop value, and implicit-self slots.
    ///
    /// Returns whether an element was available. The caller only needs this control-flow
    /// result, so keeping the element in the local slots avoids an otherwise unused clone on
    /// every iteration.
    ///
    /// # Safety
    /// `array_slot`, `index_slot`, and `value_slot` must be valid local slots. The bytecode
    /// verifier establishes this for every `ForeachNext` instruction before execution.
    #[inline(always)]
    pub(crate) unsafe fn advance_foreach(
        &mut self,
        array_slot: u16,
        index_slot: u16,
        value_slot: u16,
        self_slot: u16,
    ) -> Result<bool, &'static str> {
        match self {
            #[cfg(not(feature = "sync"))]
            Locals::Flat(slots) => {
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                let index_value = {
                    let index = unsafe { slots.get_unchecked(index_slot as usize) };
                    let StackValue::Value(RuntimeValue::Number(index)) = index else {
                        return Err("ForeachNext has invalid loop state");
                    };
                    index.value()
                };

                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                let value = {
                    let array = unsafe { slots.get_unchecked(array_slot as usize) };
                    let StackValue::Value(RuntimeValue::Array(array)) = array else {
                        return Err("ForeachNext array slot is not an array");
                    };
                    if index_value >= array.len() as f64 {
                        return Ok(false);
                    }
                    array.get(index_value as usize).cloned().unwrap_or(RuntimeValue::None)
                };

                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                *unsafe { slots.get_unchecked_mut(index_slot as usize) } =
                    StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0)));
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                *unsafe { slots.get_unchecked_mut(value_slot as usize) } = StackValue::Value(value.clone());
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                *unsafe { slots.get_unchecked_mut(self_slot as usize) } = StackValue::Value(value);
                Ok(true)
            }
            #[cfg(not(feature = "sync"))]
            Locals::Hybrid { .. } => {
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                let index = unsafe { self.get_unchecked(index_slot) };
                let StackValue::Value(RuntimeValue::Number(index)) = index else {
                    return Err("ForeachNext has invalid loop state");
                };
                let index_value = index.value();
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                let array = unsafe { self.get_unchecked(array_slot) };
                let StackValue::Value(RuntimeValue::Array(array)) = array else {
                    return Err("ForeachNext array slot is not an array");
                };
                if index_value >= array.len() as f64 {
                    return Ok(false);
                }
                let value = array.get(index_value as usize).cloned().unwrap_or(RuntimeValue::None);
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                unsafe {
                    self.set_unchecked(
                        index_slot,
                        StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0))),
                    );
                    self.set_unchecked(value_slot, StackValue::Value(value.clone()));
                    self.set_unchecked(self_slot, StackValue::Value(value));
                }
                Ok(true)
            }
            Locals::Boxed(slots) => {
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                let index = read_cell(unsafe { slots.get_unchecked(index_slot as usize) });
                let StackValue::Value(RuntimeValue::Number(index)) = index else {
                    return Err("ForeachNext has invalid loop state");
                };
                let index_value = index.value();
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                let array = read_cell(unsafe { slots.get_unchecked(array_slot as usize) });
                let StackValue::Value(RuntimeValue::Array(array)) = array else {
                    return Err("ForeachNext array slot is not an array");
                };
                if index_value >= array.len() as f64 {
                    return Ok(false);
                }
                let value = array.get(index_value as usize).cloned().unwrap_or(RuntimeValue::None);

                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                write_cell(
                    unsafe { slots.get_unchecked(index_slot as usize) },
                    StackValue::Value(RuntimeValue::Number(Number::new(index_value + 1.0))),
                );
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                write_cell(
                    unsafe { slots.get_unchecked(value_slot as usize) },
                    StackValue::Value(value.clone()),
                );
                // SAFETY: inherited from `Locals::advance_foreach`'s caller contract.
                write_cell(
                    unsafe { slots.get_unchecked(self_slot as usize) },
                    StackValue::Value(value),
                );
                Ok(true)
            }
        }
    }
}

/// Reads a VM cell.
pub(crate) fn read_cell(cell: &Cell) -> StackValue {
    #[cfg(not(feature = "sync"))]
    {
        cell.borrow().upgraded()
    }
    #[cfg(feature = "sync")]
    {
        cell.read().unwrap().upgraded()
    }
}

/// Peeks at `cell`'s raw (non-upgraded) content to check whether it already holds the
/// weak-and-upgradable form a self-reference is downgraded to, without `read_cell`'s upgrade
/// turning that back into a literal `Coroutine` for the check.
fn is_weak_coroutine(cell: &Cell) -> bool {
    #[cfg(not(feature = "sync"))]
    {
        matches!(&*cell.borrow(), StackValue::WeakCoroutine(_))
    }
    #[cfg(feature = "sync")]
    {
        matches!(&*cell.read().unwrap(), StackValue::WeakCoroutine(_))
    }
}

/// Checks whether `cell` is this coroutine's raw weak reference.
pub(crate) fn is_weak_self_reference(cell: &Cell, handle: &CoroutineHandle) -> bool {
    #[cfg(not(feature = "sync"))]
    {
        matches!(
            &*cell.borrow(),
            StackValue::WeakCoroutine(value) if upgrade_handle(value).is_some_and(|value| same_handle(&value, handle))
        )
    }
    #[cfg(feature = "sync")]
    {
        matches!(
            &*cell.read().unwrap(),
            StackValue::WeakCoroutine(value) if upgrade_handle(value).is_some_and(|value| same_handle(&value, handle))
        )
    }
}

pub(crate) fn weak_coroutine_cell(cell: &Cell, handle: &CoroutineHandle) -> Option<Cell> {
    #[cfg(not(feature = "sync"))]
    {
        let value = cell.borrow();
        matches!(&*value, StackValue::Value(RuntimeValue::Coroutine(value)) if same_handle(value, handle))
            .then(|| new_cell(StackValue::WeakCoroutine(downgrade_handle(handle))))
    }
    #[cfg(feature = "sync")]
    {
        let value = cell.read().unwrap();
        matches!(&*value, StackValue::Value(RuntimeValue::Coroutine(value)) if same_handle(value, handle))
            .then(|| new_cell(StackValue::WeakCoroutine(downgrade_handle(handle))))
    }
}

/// Writes a VM cell and cleans up nested coroutine cycles.
pub(crate) fn write_cell(cell: &Cell, value: StackValue) {
    let mut written_handles = Vec::new();
    if let StackValue::Value(value) = &value {
        collect_coroutine_handles(value, &mut written_handles);
    }
    #[cfg(not(feature = "sync"))]
    {
        *cell.borrow_mut() = value;
    }
    #[cfg(feature = "sync")]
    {
        *cell.write().unwrap() = value;
    }
    downgrade_coroutine_handles(written_handles);
}

/// Appends to an array cell.
pub(crate) fn append_to_array_cell(cell: &Cell, value: RuntimeValue) -> Result<(), &'static str> {
    let mut appended_handles = Vec::new();
    collect_coroutine_handles(&value, &mut appended_handles);
    #[cfg(not(feature = "sync"))]
    {
        let mut stored = cell.borrow_mut();
        let StackValue::Value(RuntimeValue::Array(array)) = &mut *stored else {
            return Err("ForeachCollect accumulator is not an array");
        };
        array_mut(array).push(value);
    }
    #[cfg(feature = "sync")]
    {
        let mut stored = cell.write().unwrap();
        let StackValue::Value(RuntimeValue::Array(array)) = &mut *stored else {
            return Err("ForeachCollect accumulator is not an array");
        };
        array_mut(array).push(value);
    }
    // Scan only the new value to keep `foreach` collection linear.
    downgrade_coroutine_handles(appended_handles);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advance_foreach_reports_progress_and_updates_loop_slots() {
        let mut locals = Locals::flat(4);
        assert_advance_foreach(&mut locals);
    }

    #[test]
    fn advance_foreach_preserves_captured_loop_slots() {
        // This is `Hybrid` in the default build and `Boxed` with the `sync` feature.
        let mut locals = Locals::for_captured_slots(4, &[0, 3]);
        assert_advance_foreach(&mut locals);
    }

    fn assert_advance_foreach(locals: &mut Locals) {
        let values = RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(10.into()),
            RuntimeValue::Number(20.into()),
        ]));
        locals.set(0, StackValue::Value(RuntimeValue::None));
        locals.set(1, StackValue::Value(values));
        locals.set(2, StackValue::Value(RuntimeValue::Number(0.into())));

        // SAFETY: all slots passed below are within this four-slot frame.
        assert!(unsafe { locals.advance_foreach(1, 2, 3, 0) }.unwrap());
        assert_eq!(
            into_value(locals.get(2)),
            RuntimeValue::Number(1.into()),
            "the loop index advances after loading an element"
        );
        assert_eq!(into_value(locals.get(3)), RuntimeValue::Number(10.into()));
        assert_eq!(into_value(locals.get(0)), RuntimeValue::Number(10.into()));

        // SAFETY: all slots passed below are within this four-slot frame.
        assert!(unsafe { locals.advance_foreach(1, 2, 3, 0) }.unwrap());
        // SAFETY: all slots passed below are within this four-slot frame.
        assert!(!unsafe { locals.advance_foreach(1, 2, 3, 0) }.unwrap());
        assert_eq!(into_value(locals.get(3)), RuntimeValue::Number(20.into()));
        assert_eq!(into_value(locals.get(0)), RuntimeValue::Number(20.into()));
    }

    fn into_value(value: StackValue) -> RuntimeValue {
        match value {
            StackValue::Value(value) => value,
            StackValue::Closure(_) => panic!("test locals only contain runtime values"),
            StackValue::WeakCoroutine(_) | StackValue::NestedWeakCoroutine(_) => {
                panic!("test locals only contain runtime values")
            }
        }
    }
}
