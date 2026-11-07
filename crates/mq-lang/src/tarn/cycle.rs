//! Frees reference cycles formed by closures through captured variables.
//!
//! Cycles can only be formed by writing into shared cells (`var` assignments, `+=`).
//! [`register`] tracks these cells, and [`collect`] performs a trial deletion
//! over the reachable graph.
mod graph;

use super::value::{Cell, StackValue, WeakCell};
use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use graph::Graph;
#[cfg(not(feature = "sync"))]
use std::cell::RefCell;
use std::collections::HashSet;
#[cfg(feature = "sync")]
use std::sync::{Mutex, PoisonError};

/// Candidates that trigger a collection, until survivors raise it.
const INITIAL_THRESHOLD: usize = 1024;

#[cfg(test)]
thread_local! {
    static THRESHOLD_FLOOR: std::cell::Cell<usize> = const { std::cell::Cell::new(INITIAL_THRESHOLD) };
}

fn threshold_floor() -> usize {
    #[cfg(test)]
    {
        THRESHOLD_FLOOR.get()
    }
    #[cfg(not(test))]
    {
        INITIAL_THRESHOLD
    }
}

#[derive(Default)]
struct Candidates {
    cells: Vec<WeakCell>,
    threshold: usize,
}

#[must_use]
#[derive(PartialEq, Eq)]
enum Registration {
    Pending,
    CollectionDue,
}

impl Candidates {
    /// Adds `cell` unless it repeats the last one.
    fn add(&mut self, cell: WeakCell) -> Registration {
        if self.cells.last().is_some_and(|last| last.ptr_eq(&cell)) {
            return Registration::Pending;
        }
        self.cells.push(cell);
        if self.cells.len() >= self.threshold.max(threshold_floor()) {
            Registration::CollectionDue
        } else {
            Registration::Pending
        }
    }

    /// Puts the cells that survived a collection back and waits for as many more before the
    /// next one, so a program that keeps many live candidates is not rescanned on every write.
    fn restore(&mut self, survivors: Vec<WeakCell>) {
        self.threshold = (survivors.len() * 2).max(threshold_floor());
        self.cells.extend(survivors);
    }
}

// Without `sync` a closure never leaves its thread, so the candidates stay per thread. With it,
// a closure can be dropped on another thread after the one that registered its cell has exited,
// so the candidates are shared by the process and any later collection can pick them up.
#[cfg(not(feature = "sync"))]
thread_local! {
    static CANDIDATES: RefCell<Candidates> = const { RefCell::new(Candidates { cells: Vec::new(), threshold: 0 }) };
}

#[cfg(feature = "sync")]
static CANDIDATES: Mutex<Candidates> = Mutex::new(Candidates {
    cells: Vec::new(),
    threshold: 0,
});

fn with_candidates<T>(f: impl FnOnce(&mut Candidates) -> T) -> T {
    #[cfg(not(feature = "sync"))]
    {
        CANDIDATES.with(|candidates| f(&mut candidates.borrow_mut()))
    }
    #[cfg(feature = "sync")]
    {
        f(&mut CANDIDATES.lock().unwrap_or_else(PoisonError::into_inner))
    }
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

    if with_candidates(|candidates| candidates.add(Shared::downgrade(cell))) == Registration::CollectionDue {
        collect();
    }
}

/// Frees the registered cells that only a cycle keeps alive.
pub(super) fn collect() {
    let registered = with_candidates(|candidates| std::mem::take(&mut candidates.cells));
    // An empty list means the threshold was never raised, so there is nothing to reset.
    if registered.is_empty() {
        return;
    }
    let distinct = distinct_live(registered);
    if !distinct.is_empty() {
        free_garbage(&distinct);
    }
    // Restoring even an empty list lowers a threshold that expired candidates had raised.
    let survivors = distinct.into_iter().filter(|weak| weak.strong_count() > 0).collect();
    with_candidates(|candidates| candidates.restore(survivors));
}

/// `cells` without duplicates and without cells that are already gone. A loop that writes
/// several shared cells in turn registers each of them over and over.
fn distinct_live(cells: Vec<WeakCell>) -> Vec<WeakCell> {
    let mut seen = HashSet::new();
    cells
        .into_iter()
        .filter(|weak| weak.strong_count() > 0 && seen.insert(weak.as_ptr()))
        .collect()
}

/// Scans the graph reachable from the live `candidates` and clears the garbage in it.
fn free_garbage(candidates: &[WeakCell]) {
    let roots: Vec<Cell> = candidates.iter().filter_map(WeakCell::upgrade).collect();
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
fn threshold() -> usize {
    with_candidates(|candidates| candidates.threshold)
}

#[cfg(test)]
fn registered_len() -> usize {
    with_candidates(|candidates| candidates.cells.len())
}

/// Empties the candidates and returns a guard that keeps other tests off them, since with
/// `sync` they are shared by every test thread.
#[cfg(test)]
fn reset() -> std::sync::MutexGuard<'static, ()> {
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    with_candidates(|candidates| *candidates = Candidates::default());
    guard
}

#[cfg(test)]
mod tests {
    use super::super::value::{Closure, new_cell};
    use super::*;
    use rstest::rstest;

    fn eval(query: &str) -> Vec<String> {
        let _serial = reset();
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let input = crate::parse_markdown_input("# a\n").unwrap();
        let values = engine.eval(query, input.into_iter()).expect("query runs");
        values.values().iter().map(ToString::to_string).collect()
    }

    /// Runs `query` with a collection after every `floor` candidates.
    fn eval_with_floor(query: &str, floor: usize) -> Result<Vec<String>, String> {
        THRESHOLD_FLOOR.set(floor);
        let _serial = reset();
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let input = crate::parse_markdown_input("# a\n").unwrap();
        let result = engine.eval(query, input.into_iter());
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        result
            .map(|values| values.values().iter().map(ToString::to_string).collect())
            .map_err(|error| error.to_string())
    }

    mod sound {
        use super::*;
        use proptest::prelude::*;

        fn statement() -> impl Strategy<Value = String> {
            let var = prop::sample::select(vec!["a", "b", "c"]);
            let closure = prop::sample::select(vec!["g0", "g1", "g2"]);
            prop_oneof![
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = [{g}]")),
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} += [{g}]")),
                (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = {y}")),
                var.clone().prop_map(|x| format!("{x} += [1]")),
                (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = [fn(v): len({y});]")),
                (closure, var).prop_map(|(g, y)| format!("let {g} = fn(v): len({y}) + v;")),
            ]
        }

        fn program(statements: &[String], in_loop: bool) -> String {
            let setup = "var a = [] | var b = [] | var c = [] | \
                         let g0 = fn(v): len(a) + v; | let g1 = fn(v): len(b) + v; | let g2 = fn(v): len(c) + v;";
            let body = statements.join(" | ");
            let result = "[len(a), len(b), len(c), g0(1), g1(2), g2(3)]";
            if in_loop {
                format!(
                    "def run(): {setup} | {body} | {result}; | var i = 0 | var r = 0 | while (i < 3): r = run() | i += 1; | r"
                )
            } else {
                format!("{setup} | {body} | {result}")
            }
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(512))]

            /// Collecting after every candidate gives the same result as collecting only at the
            /// end, so no live cell is ever cleared.
            #[test]
            fn collecting_early_never_changes_the_result(
                statements in prop::collection::vec(statement(), 1..10),
                in_loop in any::<bool>(),
            ) {
                let code = program(&statements, in_loop);
                prop_assert_eq!(
                    eval_with_floor(&code, 1),
                    eval_with_floor(&code, usize::MAX),
                    "diverged evaluating {}", code
                );
            }
        }
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
    fn cells_written_in_turn_are_registered_once_after_a_collection() {
        let _serial = reset();
        THRESHOLD_FLOOR.set(usize::MAX);
        let (x, y) = (empty_cell(), empty_cell());
        let (_x_user, _y_user) = (Shared::clone(&x), Shared::clone(&y));
        for _ in 0..100 {
            write_for_test(&x, StackValue::Value(RuntimeValue::Array(Default::default())));
            write_for_test(&y, StackValue::Value(RuntimeValue::Array(Default::default())));
        }
        assert_eq!(registered_len(), 200);
        collect();
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        assert_eq!(registered_len(), 2);
    }

    #[test]
    fn the_threshold_falls_back_when_the_candidates_that_raised_it_are_gone() {
        let _serial = reset();
        THRESHOLD_FLOOR.set(4);
        let cells: Vec<Cell> = (0..10).map(|_| empty_cell()).collect();
        let holders: Vec<Cell> = cells.iter().map(Shared::clone).collect();
        for cell in &cells {
            write_for_test(cell, StackValue::Value(RuntimeValue::Array(Default::default())));
        }
        collect();
        assert!(threshold() > 4, "ten live candidates raise the threshold");
        drop((cells, holders));
        collect();
        let lowered = threshold();
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        assert_eq!(lowered, 4);
    }

    /// A cell holding a closure that captures the cell itself and a sentinel. The sentinel is
    /// freed exactly when the cycle is.
    fn self_capturing_cell() -> (Cell, Shared<Closure>, Shared<String>) {
        let sentinel = Shared::new(String::from("sentinel"));
        let sentinel_cell = new_cell(StackValue::Value(RuntimeValue::String(Shared::clone(&sentinel))));
        let cell = empty_cell();
        let closure = closure_over(vec![Shared::clone(&cell), sentinel_cell]);
        write_for_test(&cell, StackValue::Closure(Shared::clone(&closure)));
        (cell, closure, sentinel)
    }

    fn is_freed(sentinel: &Shared<String>) -> bool {
        Shared::strong_count(sentinel) == 1
    }

    #[test]
    fn a_cell_and_closure_that_only_refer_to_each_other_are_freed() {
        let _serial = reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop((cell, closure));
        assert!(!is_freed(&sentinel));
        collect();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_closure_referenced_from_outside_keeps_its_cell_until_it_is_dropped() {
        let _serial = reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop(cell);
        collect();
        assert!(!is_freed(&sentinel));
        drop(closure);
        collect();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_cell_referenced_from_outside_is_not_freed() {
        let _serial = reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop(closure);
        collect();
        assert!(!is_freed(&sentinel));
        assert!(matches!(read(&cell), StackValue::Closure(_)));
    }

    #[cfg(feature = "sync")]
    #[test]
    fn a_cycle_left_by_an_exited_thread_is_collected_by_a_later_collection() {
        let _serial = reset();
        let (closure, sentinel) = std::thread::spawn(|| {
            let (cell, closure, sentinel) = self_capturing_cell();
            drop(cell);
            collect();
            (closure, sentinel)
        })
        .join()
        .unwrap();
        assert!(!is_freed(&sentinel));
        drop(closure);
        collect();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_cell_with_a_foreign_weak_reference_is_not_freed() {
        let _serial = reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        let foreign = Shared::downgrade(&cell);
        drop((cell, closure));
        collect();
        // Another holder could upgrade `foreign` at any moment, so the cell must stay intact.
        let revived = foreign.upgrade().expect("still alive");
        assert!(matches!(read(&revived), StackValue::Closure(_)));
        assert!(!is_freed(&sentinel));
        drop((foreign, revived));
        collect();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_borrowed_cell_is_treated_as_referenced() {
        let _serial = reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop(closure);
        #[cfg(not(feature = "sync"))]
        let guard = cell.borrow_mut();
        #[cfg(feature = "sync")]
        let guard = cell.write().unwrap();
        collect();
        drop(guard);
        assert!(!is_freed(&sentinel));
    }

    #[test]
    fn a_graph_deeper_than_the_limit_is_kept_not_overflowed() {
        let _serial = reset();
        let sentinel = Shared::new(String::from("sentinel"));
        let sentinel_cell = new_cell(StackValue::Value(RuntimeValue::String(Shared::clone(&sentinel))));
        // A ring of cells, each holding a closure over the next one; the first also holds the sentinel.
        let ring: Vec<Cell> = (0..graph::MAX_DEPTH * 4).map(|_| empty_cell()).collect();
        for (index, cell) in ring.iter().enumerate() {
            let mut captures = vec![Shared::clone(&ring[(index + 1) % ring.len()])];
            if index == 0 {
                captures.push(Shared::clone(&sentinel_cell));
            }
            write_for_test(cell, StackValue::Closure(closure_over(captures)));
        }
        drop((ring, sentinel_cell));
        collect();
        assert!(!is_freed(&sentinel));
    }

    fn write_for_test(cell: &Cell, value: StackValue) {
        super::super::value::write_cell(cell, value);
    }

    fn read(cell: &Cell) -> StackValue {
        super::super::value::read_cell(cell)
    }
}
