//! Frees reference cycles that closures form through captured variables.
//!
//! An edge of such a cycle is only ever added by writing into a cell that is already shared
//! (`var` assignment, `+=`), so [`register`] records just those cells. [`collect`] then runs a
//! trial deletion over the graph reachable from them: a node whose strong count is fully
//! explained by edges inside the graph is garbage unless something live reaches it. Whatever it
//! cannot see into (coroutines, a cell that is borrowed, a graph over the work budget) counts as
//! an outside reference, so the collector leaks rather than frees a live value.

mod graph;

use super::value::{Cell, StackValue, WeakCell};
use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use graph::Graph;
use std::cell::RefCell;
use std::collections::HashSet;

/// Candidates that trigger a collection, until survivors raise it.
const INITIAL_THRESHOLD: usize = 1024;

#[derive(Default)]
struct Candidates {
    cells: Vec<WeakCell>,
    threshold: usize,
}

impl Candidates {
    /// Adds `cell` unless it repeats the last one. Returns whether a collection is due.
    fn push(&mut self, cell: WeakCell) -> bool {
        if self.cells.last().is_some_and(|last| last.ptr_eq(&cell)) {
            return false;
        }
        self.cells.push(cell);
        self.cells.len() >= self.threshold.max(INITIAL_THRESHOLD)
    }

    /// Puts the cells that survived a collection back and waits for as many more before the
    /// next one, so a program that keeps many live candidates is not rescanned on every write.
    fn restore(&mut self, survivors: Vec<WeakCell>) {
        self.threshold = (survivors.len() * 2).max(INITIAL_THRESHOLD);
        self.cells.extend(survivors);
    }
}

thread_local! {
    static CANDIDATES: RefCell<Candidates> = const { RefCell::new(Candidates { cells: Vec::new(), threshold: 0 }) };
}

/// Whether writing `value` into a shared cell can close a cycle through a closure.
#[inline(always)]
pub(super) fn may_hold_closure(value: &StackValue) -> bool {
    match value {
        StackValue::Closure(_) => true,
        StackValue::Value(value) => runtime_value_may_hold_closure(value),
        _ => false,
    }
}

/// Whether `value` is, or can contain, a closure.
#[inline(always)]
pub(super) fn runtime_value_may_hold_closure(value: &RuntimeValue) -> bool {
    matches!(
        value,
        RuntimeValue::Closure(_) | RuntimeValue::Array(_) | RuntimeValue::Dict(_)
    )
}

/// Records `cell` as a place where a cycle may have just closed. Collects when enough pile up.
pub(super) fn register(cell: &Cell) {
    // A cell nothing else references cannot be part of a cycle.
    if Shared::strong_count(cell) < 2 {
        return;
    }

    let due = CANDIDATES.with(|candidates| candidates.borrow_mut().push(Shared::downgrade(cell)));

    if due {
        collect();
    }
}

/// Frees the registered cells that only a cycle keeps alive.
pub(super) fn collect() {
    let registered = CANDIDATES.with(|candidates| std::mem::take(&mut candidates.borrow_mut().cells));

    if registered.is_empty() {
        return;
    }

    free_garbage(&registered);

    let survivors = registered.into_iter().filter(|weak| weak.strong_count() > 0).collect();
    CANDIDATES.with(|candidates| candidates.borrow_mut().restore(survivors));
}

/// Scans the graph reachable from the live `registered` cells and clears the garbage in it.
fn free_garbage(registered: &[WeakCell]) {
    let mut seen = HashSet::new();
    let roots: Vec<Cell> = registered
        .iter()
        .filter(|weak| seen.insert(weak.as_ptr()))
        .filter_map(WeakCell::upgrade)
        .collect();
    let garbage = Graph::scan(&roots).garbage_cells();
    drop(roots);

    garbage
        .iter()
        .filter_map(WeakCell::upgrade)
        .for_each(|cell| clear(&cell));
}

/// Empties `cell`, which drops what the cycle through it kept alive.
fn clear(cell: &Cell) {
    #[cfg(not(feature = "sync"))]
    let guard = cell.try_borrow_mut().ok();
    #[cfg(feature = "sync")]
    let guard = cell.try_write().ok();

    if let Some(mut guard) = guard {
        let old = std::mem::replace(&mut *guard, StackValue::Value(RuntimeValue::None));
        // Release the cell first: dropping the old value can reach this cell again.
        drop(guard);
        drop(old);
    }
}

#[cfg(test)]
pub(super) fn pending_candidates() -> usize {
    CANDIDATES.with(|candidates| {
        candidates
            .borrow()
            .cells
            .iter()
            .filter(|weak| weak.strong_count() > 0)
            .count()
    })
}

#[cfg(test)]
pub(super) fn reset() {
    CANDIDATES.with(|candidates| candidates.borrow_mut().cells.clear());
}

#[cfg(test)]
mod tests {
    use super::super::value::{Closure, new_cell};
    use super::*;
    use rstest::rstest;

    fn eval(query: &str) -> Vec<String> {
        reset();
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let input = crate::parse_markdown_input("# a\n").unwrap();
        let values = engine.eval(query, input.into_iter()).expect("query runs");
        values.values().iter().map(ToString::to_string).collect()
    }

    fn closure_over(cells: Vec<Cell>) -> Shared<Closure> {
        Shared::new(Closure {
            chunk_index: 0,
            upvalues: Some(Shared::new(cells)),
        })
    }

    fn empty_cell() -> Cell {
        new_cell(StackValue::Value(RuntimeValue::None))
    }

    #[rstest]
    #[case::stored_in_its_own_variable("var a = 0 | a = fn(x): a; | 1")]
    #[case::through_another_local("var a = 0 | let g = fn(x): a; | a = g | 1")]
    #[case::through_a_chain("var a = 0 | let g = fn(x): a; | let h = fn(x): g(x); | a = h | 1")]
    #[case::two_variables("var a = 0 | var b = 0 | a = fn(x): b; | b = fn(x): a; | 1")]
    #[case::inside_an_array("var a = [] | let g = fn(x): len(a); | a = [g] | 1")]
    #[case::inside_a_dict("var a = {} | let g = fn(x): len(a); | a = {\"f\": g} | 1")]
    #[case::appended_to_an_array("var a = [] | let g = fn(x): len(a); | a += [g] | 1")]
    #[case::inside_a_function("def mk(): var a = 0 | let g = fn(x): a; | a = g | 1; | mk()")]
    #[case::returned_from_a_function(
        "def mk(): var a = [] | let g = fn(x): len(a); | a = [g] | [g]; | let r = mk() | 1"
    )]
    #[case::next_to_a_coroutine(
        "def gen(): yield: 1 | yield: 2; | var c = gen() | let f = fn(x): next(c); | c = [c, f] | 1"
    )]
    #[case::repeated_in_a_loop(
        "var i = 0 | while (i < 3000): var a = [] | let g = fn(x): len(a); | a = [g] | i += 1; | 1"
    )]
    fn cycles_are_freed_by_the_end_of_the_run(#[case] query: &str) {
        assert_eq!(eval(query).last().map(String::as_str), Some("1"));
        assert_eq!(pending_candidates(), 0);
    }

    #[rstest]
    #[case::escaped_closure_still_sees_its_array(
        "def mk(): var a = [] | let g = fn(x): len(a); | a = [g, 5] | [g]; | let f = first(mk()) | f(0)",
        "2"
    )]
    #[case::dict_cycle_stays_usable_while_referenced(
        "var d = {} | let f = fn(x): len(d); | d = {\"f\": f, \"n\": 3} | f(0)",
        "2"
    )]
    #[case::live_cycle_survives_collections_that_free_others(
        "var keep = [] | let kg = fn(x): len(keep); | keep = [kg, 1, 2] | var i = 0 | \
         while (i < 3000): var a = [] | let g = fn(x): len(a); | a = [g] | i += 1; | kg(0)",
        "3"
    )]
    #[case::many_escaped_closures_stay_usable(
        "def mk(): var a = [] | let g = fn(x): len(a); | a = [g, 5] | [g]; | \
         let fs = map(range(3000), fn(i): first(mk());) | let f = last(fs) | f(0)",
        "2"
    )]
    fn collection_keeps_values_that_are_still_referenced(#[case] query: &str, #[case] expected: &str) {
        assert_eq!(eval(query).last().map(String::as_str), Some(expected));
    }

    #[test]
    fn a_cell_and_closure_that_only_refer_to_each_other_are_freed() {
        reset();
        let cell = empty_cell();
        let closure = closure_over(vec![Shared::clone(&cell)]);
        write_for_test(&cell, StackValue::Closure(closure));
        let weak = Shared::downgrade(&cell);
        drop(cell);
        assert!(weak.upgrade().is_some());
        collect();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn a_cell_kept_by_an_outside_reference_is_freed_once_that_reference_is_gone() {
        reset();
        let cell = empty_cell();
        let closure = closure_over(vec![Shared::clone(&cell)]);
        write_for_test(&cell, StackValue::Closure(Shared::clone(&closure)));
        let weak = Shared::downgrade(&cell);
        drop(cell);
        collect();
        assert!(weak.upgrade().is_some());
        drop(closure);
        collect();
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn a_cell_referenced_from_outside_is_not_freed() {
        reset();
        let cell = empty_cell();
        let closure = closure_over(vec![Shared::clone(&cell)]);
        write_for_test(&cell, StackValue::Closure(closure));
        collect();
        assert!(matches!(read(&cell), StackValue::Closure(_)));
    }

    #[test]
    fn a_borrowed_cell_is_treated_as_referenced() {
        reset();
        let cell = empty_cell();
        let closure = closure_over(vec![Shared::clone(&cell)]);
        write_for_test(&cell, StackValue::Closure(closure));
        let weak = Shared::downgrade(&cell);
        #[cfg(not(feature = "sync"))]
        let guard = cell.borrow_mut();
        #[cfg(feature = "sync")]
        let guard = cell.write().unwrap();
        collect();
        drop(guard);
        drop(cell);
        assert!(weak.upgrade().is_some());
    }

    #[test]
    fn a_graph_deeper_than_the_limit_is_kept_not_overflowed() {
        reset();
        let head = empty_cell();
        let mut tail = Shared::clone(&head);
        for _ in 0..(graph::MAX_DEPTH * 4) {
            let next = empty_cell();
            write_for_test(&tail, StackValue::Closure(closure_over(vec![Shared::clone(&next)])));
            tail = next;
        }
        write_for_test(&tail, StackValue::Closure(closure_over(vec![Shared::clone(&head)])));
        let weak = Shared::downgrade(&head);
        drop((head, tail));
        collect();
        assert!(weak.upgrade().is_some());
    }

    fn write_for_test(cell: &Cell, value: StackValue) {
        super::super::value::write_cell(cell, value);
    }

    fn read(cell: &Cell) -> StackValue {
        super::super::value::read_cell(cell)
    }
}
