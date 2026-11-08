//! Frees reference cycles formed by closures through captured variables.
//!
//! Cycles can only be formed by writing into shared cells (`var` assignments, `+=`).
//! [`register`] tracks these cells, and [`collect`] performs a trial deletion
//! over the reachable graph.
//!
//! Each engine owns its candidates ([`CycleCandidates`]) and lends them to the running thread
//! only while it evaluates. Dropping an engine collects its own candidates, so another engine's
//! drop never consumes them and they follow the engine across threads.
mod abandoned;
mod graph;

use super::value::{Cell, StackValue, WeakCell};
use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use graph::Graph;
use rustc_hash::FxHashSet;
use std::cell::RefCell;
use std::fmt;

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
#[derive(Default, PartialEq, Eq)]
enum Registration {
    #[default]
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

/// One thread's candidates. Each thread registers and collects its own, so a collection never
/// touches the cells another running thread is working with.
struct ThreadCandidates(RefCell<Candidates>);

thread_local! {
    static CANDIDATES: ThreadCandidates = const {
        ThreadCandidates(RefCell::new(Candidates { cells: Vec::new(), threshold: 0 }))
    };
}

#[cfg(feature = "sync")]
impl Drop for ThreadCandidates {
    fn drop(&mut self) {
        let cells = std::mem::take(&mut self.0.get_mut().cells)
            .into_iter()
            .filter(|weak| weak.strong_count() > 0)
            .collect();
        abandoned::push(cells);
    }
}

/// The candidates of one engine. An engine's clone starts with none of its own.
#[derive(Default)]
pub(crate) struct CycleCandidates(Candidates);

impl Clone for CycleCandidates {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl fmt::Debug for CycleCandidates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CycleCandidates").finish_non_exhaustive()
    }
}

/// Keeps an engine's candidates on the running thread; see [`CycleCandidates::enter`].
#[must_use]
pub(crate) struct Entered<'a>(&'a mut Candidates);

impl CycleCandidates {
    /// Makes these the running thread's candidates until the returned guard drops.
    pub(crate) fn enter(&mut self) -> Entered<'_> {
        with_candidates(|running| std::mem::swap(running, &mut self.0));
        Entered(&mut self.0)
    }
}

impl Drop for Entered<'_> {
    fn drop(&mut self) {
        with_candidates(|running| std::mem::swap(running, self.0));
    }
}

/// Frees the cycles only this engine's variables kept alive. What a returned value still owns
/// is left for the next collection, since only the value's own drop can release it.
impl Drop for CycleCandidates {
    fn drop(&mut self) {
        let mut registered = std::mem::take(&mut self.0.cells);
        registered.extend(abandoned::take());
        abandoned::push(sweep(registered));
    }
}

/// Runs `f` on this thread's candidates. During thread teardown they are gone, and `f` is skipped.
fn with_candidates<T: Default>(f: impl FnOnce(&mut Candidates) -> T) -> T {
    CANDIDATES
        .try_with(|candidates| f(&mut candidates.0.borrow_mut()))
        .unwrap_or_default()
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
    let mut registered = with_candidates(|candidates| std::mem::take(&mut candidates.cells));
    registered.extend(abandoned::take());
    // An empty list means the threshold was never raised, so there is nothing to reset.
    if registered.is_empty() {
        return;
    }
    let survivors = sweep(registered);
    // Restoring even an empty list lowers a threshold that expired candidates had raised.
    with_candidates(|candidates| candidates.restore(survivors));
}

/// Clears the garbage reachable from `registered` and returns the candidates still alive.
fn sweep(registered: Vec<WeakCell>) -> Vec<WeakCell> {
    let distinct = distinct_live(registered);
    if !distinct.is_empty() {
        free_garbage(&distinct);
    }
    distinct.into_iter().filter(|weak| weak.strong_count() > 0).collect()
}

/// `cells` without duplicates and without cells that are already gone. A loop that writes
/// several shared cells in turn registers each of them over and over.
fn distinct_live(cells: Vec<WeakCell>) -> Vec<WeakCell> {
    let mut seen = FxHashSet::default();
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

/// How many registered entries point at one of `cells`. Counting only these keeps a test
/// independent of whatever other tests register in the shared candidates under `sync`.
#[cfg(test)]
fn registered_for(cells: &[&Cell]) -> usize {
    with_candidates(|candidates| {
        candidates
            .cells
            .iter()
            .filter(|weak| cells.iter().any(|cell| weak.as_ptr() == Shared::as_ptr(cell)))
            .count()
    })
}

/// Empties this thread's candidates, so a test starts from none.
#[cfg(test)]
fn reset() {
    with_candidates(|candidates| *candidates = Candidates::default());
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

    /// Evaluates `query` on a fresh engine.
    fn eval_on_new_engine(query: &str) -> Result<Vec<String>, String> {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let input = crate::parse_markdown_input("# a\n").unwrap();
        engine
            .eval(query, input.into_iter())
            .map(|values| values.values().iter().map(ToString::to_string).collect())
            .map_err(|error| error.to_string())
    }

    /// Runs `query` with a collection after every `floor` candidates.
    fn eval_with_floor(query: &str, floor: usize) -> Result<Vec<String>, String> {
        reset();
        THRESHOLD_FLOOR.set(floor);
        let result = eval_on_new_engine(query);
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        result
    }

    #[rstest]
    #[case::stored_in_a_captured_variable(
        "var a = [] | def gen(): yield: len(a) | yield: len(a) | yield: len(a); | let k = gen() | next(k) | a = [k] | [next(k), next(k)]"
    )]
    #[case::variable_written_while_suspended(
        "var a = [] | def gen(): yield: len(a) | yield: len(a) | yield: len(a); | let k = gen() | next(k) | a += [1] | a += [2] | [next(k), len(a)]"
    )]
    #[case::stored_with_a_closure(
        "var a = [] | def gen(): yield: len(a) | yield: len(a); | let k = gen() | let g = fn(v): len(a) + v; | a = [k, g] | [next(k), g(1), next(k)]"
    )]
    #[case::kept_by_two_variables(
        "var a = [] | var b = [] | def gen(): yield: len(a) + len(b) | yield: len(a) + len(b); | let k = gen() | a = [k] | b = [k] | [next(k), next(k)]"
    )]
    fn generators_keep_working_after_collections(#[case] query: &str) {
        let early = eval_with_floor(query, 1);
        assert!(early.is_ok(), "{early:?}");
        assert_eq!(early, eval_with_floor(query, usize::MAX));
    }

    mod sound {
        use super::*;
        use proptest::prelude::*;

        /// A statement over array variables, closures that read them, generators that yield
        /// their lengths, and containers holding any of these.
        fn statement() -> impl Strategy<Value = String> {
            let var = prop::sample::select(vec!["a", "b", "c"]);
            let closure = prop::sample::select(vec!["g0", "g1", "g2"]);
            let generator = prop::sample::select(vec!["k0", "k1"]);
            prop_oneof![
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = [{g}]")),
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} += [{g}]")),
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x}[0] = {g}")),
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = foreach(v, [1, 2]): [{g}];")),
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = partial(h, {g})")),
                (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} += [partial(h, {g})]")),
                (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = {y}")),
                var.clone().prop_map(|x| format!("{x} += [1]")),
                (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = [fn(v): len({y});]")),
                (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = [fn(v): [fn(w): len({y});];]")),
                (var.clone(), generator.clone()).prop_map(|(x, k)| format!("{x} = [{k}]")),
                (var.clone(), generator.clone()).prop_map(|(x, k)| format!("{x} += [{k}]")),
                generator.clone().prop_map(|k| format!("next({k})")),
                generator.prop_map(|k| format!("let {k} = gen()")),
                (closure, var).prop_map(|(g, y)| format!("let {g} = fn(v): len({y}) + v;")),
            ]
        }

        fn program(statements: &[String], in_loop: bool) -> String {
            let setup = [
                "var a = []",
                "var b = []",
                "var c = []",
                "let g0 = fn(v): len(a) + v;",
                "let g1 = fn(v): len(b) + v;",
                "let g2 = fn(v): len(c) + v;",
                "let h = fn(p, q): len(a) + len(q);",
                "def gen(): yield: len(a) | yield: len(b) | yield: len(c);",
                "let k0 = gen()",
                "let k1 = gen()",
            ]
            .join(" | ");
            let body = statements.join(" | ");
            // Closures and generators still work after any collection that ran meanwhile.
            let result = "[len(a), len(b), len(c), g0(1), g1(2), g2(3), next(k0), next(k1)]";
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
        reset();
        THRESHOLD_FLOOR.set(usize::MAX);
        let (x, y) = (empty_cell(), empty_cell());
        let (_x_user, _y_user) = (Shared::clone(&x), Shared::clone(&y));
        for _ in 0..100 {
            write_for_test(&x, StackValue::Value(RuntimeValue::Array(Default::default())));
            write_for_test(&y, StackValue::Value(RuntimeValue::Array(Default::default())));
        }
        assert_eq!(registered_for(&[&x, &y]), 200);
        collect();
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        assert_eq!(registered_for(&[&x, &y]), 2);
    }

    #[test]
    fn restoring_no_survivors_lowers_the_threshold_to_the_floor() {
        THRESHOLD_FLOOR.set(4);
        let mut candidates = Candidates::default();
        let live: Vec<Cell> = (0..10).map(|_| empty_cell()).collect();
        candidates.restore(live.iter().map(Shared::downgrade).collect());
        let raised = candidates.threshold;
        candidates.restore(Vec::new());
        let lowered = candidates.threshold;
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        assert_eq!((raised, lowered), (20, 4));
    }

    #[test]
    fn the_threshold_falls_back_when_the_candidates_that_raised_it_are_gone() {
        reset();
        THRESHOLD_FLOOR.set(4);
        let cells: Vec<Cell> = (0..10).map(|_| empty_cell()).collect();
        let holders: Vec<Cell> = cells.iter().map(Shared::clone).collect();
        for cell in &cells {
            write_for_test(cell, StackValue::Value(RuntimeValue::Array(Default::default())));
        }
        collect();
        let raised = threshold();
        assert!(raised > 4, "ten live candidates raise the threshold");
        drop((cells, holders));
        collect();
        let lowered = threshold();
        THRESHOLD_FLOOR.set(INITIAL_THRESHOLD);
        assert!(lowered < raised, "{lowered} should be below {raised}");
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

    const RETURNED_CYCLE: &str = "var a = [] | let g = fn(x): len(a); | a = [g, \"sentinel\"] | a";

    /// Evaluates a query on `engine` that returns an array holding a cycle, with the sentinel
    /// the cycle keeps alive.
    fn eval_returned_cycle(engine: &mut crate::DefaultEngine) -> (crate::RuntimeValues, Shared<String>) {
        let values = engine
            .eval(RETURNED_CYCLE, std::iter::once(RuntimeValue::None))
            .expect("query runs");
        let sentinel = match &values[0] {
            RuntimeValue::Array(items) => match &items[1] {
                RuntimeValue::String(text) => Shared::clone(text),
                other => panic!("unexpected {other:?}"),
            },
            other => panic!("unexpected {other:?}"),
        };
        (values, sentinel)
    }

    fn engine() -> crate::DefaultEngine {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        engine
    }

    /// Collects until `sentinel` is freed. Under `sync` another test's thread may be holding the
    /// abandoned candidates, so this retries until they are handed back.
    fn freed_by_later_collections(sentinel: &Shared<String>) -> bool {
        (0..2000).any(|_| {
            collect();
            is_freed(sentinel) || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                false
            }
        })
    }

    #[test]
    fn a_cycle_held_only_by_returned_values_is_freed_when_the_engine_is_dropped() {
        reset();
        let mut engine = engine();
        let (values, sentinel) = eval_returned_cycle(&mut engine);
        drop(values);
        assert!(!is_freed(&sentinel), "the cycle outlives the result until a collection");
        drop(engine);
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn dropping_another_engine_does_not_consume_the_cycles_of_a_live_result() {
        reset();
        let (mut owner, mut other) = (engine(), engine());
        let (values, sentinel) = eval_returned_cycle(&mut owner);
        let (other_values, other_sentinel) = eval_returned_cycle(&mut other);
        drop(other_values);
        drop(other);
        assert!(is_freed(&other_sentinel));
        assert!(!is_freed(&sentinel), "the result still owns the cycle");
        drop(values);
        drop(owner);
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_cycle_whose_result_outlives_the_engine_is_freed_by_a_later_collection() {
        reset();
        let mut owner = engine();
        let (values, sentinel) = eval_returned_cycle(&mut owner);
        drop(owner);
        assert!(!is_freed(&sentinel));
        drop(values);
        assert!(freed_by_later_collections(&sentinel));
    }

    #[cfg(feature = "sync")]
    #[test]
    fn an_engine_moved_to_another_thread_collects_the_cycles_of_its_result_there() {
        reset();
        let mut owner = engine();
        let (values, sentinel) = eval_returned_cycle(&mut owner);
        std::thread::spawn(move || {
            drop(values);
            drop(owner);
        })
        .join()
        .unwrap();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_cell_and_closure_that_only_refer_to_each_other_are_freed() {
        reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop((cell, closure));
        assert!(!is_freed(&sentinel));
        collect();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_closure_referenced_from_outside_keeps_its_cell_until_it_is_dropped() {
        reset();
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
        reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop(closure);
        collect();
        assert!(!is_freed(&sentinel));
        assert!(matches!(read(&cell), StackValue::Closure(_)));
    }

    /// Threads evaluate programs full of cycles, generators and closures at once, collecting
    /// after every candidate against one shared list, and must all get the single-threaded result.
    #[cfg(feature = "sync")]
    #[test]
    fn concurrent_evaluations_keep_their_results() {
        const PROGRAM: &str = "var a = [] | var b = [] | def gen(): yield: len(a) | yield: len(b) | yield: 7; | \
            let g0 = fn(v): len(a) + v; | let g1 = fn(v): len(b) + v; | let k = gen() | \
            var i = 0 | while (i < 20): a = [g0, k] | b += [g1] | i += 1; | \
            [len(a), len(b), g0(1), g1(2), next(k)]";
        reset();
        let expected = eval_on_new_engine(PROGRAM);
        assert!(expected.is_ok(), "{expected:?}");
        let threads: Vec<_> = (0..4)
            .map(|_| {
                std::thread::spawn(|| {
                    THRESHOLD_FLOOR.set(1);
                    (0..100).map(|_| eval_on_new_engine(PROGRAM)).collect::<Vec<_>>()
                })
            })
            .collect();
        for thread in threads {
            for result in thread.join().expect("no thread panics") {
                assert_eq!(result, expected);
            }
        }
        collect();
    }

    #[cfg(feature = "sync")]
    #[test]
    fn a_cycle_left_by_an_exited_thread_is_collected_by_a_later_collection() {
        reset();
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
        // A collection on another test's thread may take the orphan first. It frees the cycle on
        // its next collection, so this one waits for that.
        let freed = (0..2000).any(|_| {
            collect();
            is_freed(&sentinel) || {
                std::thread::sleep(std::time::Duration::from_millis(1));
                false
            }
        });
        assert!(freed);
    }

    #[test]
    fn a_cell_with_a_foreign_weak_reference_is_not_freed() {
        reset();
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
        reset();
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
    fn a_scan_over_the_work_limit_frees_nothing() {
        reset();
        let (cell, closure, sentinel) = self_capturing_cell();
        drop(closure);
        // The registry holds the weak handle `write_cell` registered; the roots hold the strong one.
        let roots = vec![cell];
        assert!(Graph::scan_within(&roots, 1).garbage_cells().is_empty());
        assert!(!Graph::scan_within(&roots, graph::MAX_WORK).garbage_cells().is_empty());
        drop(roots);
        collect();
        assert!(is_freed(&sentinel));
    }

    #[test]
    fn a_graph_deeper_than_the_limit_is_kept_not_overflowed() {
        reset();
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
