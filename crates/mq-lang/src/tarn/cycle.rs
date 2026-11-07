//! Frees reference cycles that closures form through captured variables.
//!
//! An edge of such a cycle is only ever added by writing into a cell that is already shared
//! (`var` assignment, `+=`), so [`register`] records just those cells. [`collect`] then runs a
//! trial deletion over the graph reachable from them: a node whose strong count is fully
//! explained by edges inside the graph is garbage unless something live reaches it. Whatever it
//! cannot see into (coroutines, a cell that is borrowed, a graph over the work budget) counts as
//! an outside reference, so the collector leaks rather than frees a live value.

use super::value::{Cell, StackValue, WeakCell};
use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

const INITIAL_THRESHOLD: usize = 1024;
const MAX_WORK: usize = 1 << 20;
const MAX_DEPTH: usize = 128;

#[derive(Default)]
struct Candidates {
    cells: Vec<WeakCell>,
    threshold: usize,
}

thread_local! {
    static CANDIDATES: RefCell<Candidates> = const { RefCell::new(Candidates { cells: Vec::new(), threshold: 0 }) };
}

/// Whether writing `value` into a shared cell can close a cycle through a closure.
#[inline(always)]
pub(super) fn may_hold_closure(value: &StackValue) -> bool {
    matches!(
        value,
        StackValue::Closure(_)
            | StackValue::Value(RuntimeValue::Closure(_) | RuntimeValue::Array(_) | RuntimeValue::Dict(_))
    )
}

/// Whether `value` can add a closure to a container already stored in a cell.
#[inline(always)]
pub(super) fn runtime_value_may_hold_closure(value: &RuntimeValue) -> bool {
    matches!(
        value,
        RuntimeValue::Closure(_) | RuntimeValue::Array(_) | RuntimeValue::Dict(_)
    )
}

/// Records `cell` as a place where a cycle may have just closed. Collects when enough pile up.
pub(super) fn register(cell: &Cell) {
    if Shared::strong_count(cell) < 2 {
        return;
    }
    let weak = Shared::downgrade(cell);
    let due = CANDIDATES.with(|candidates| {
        let mut candidates = candidates.borrow_mut();
        if candidates.cells.last().is_some_and(|last| last.ptr_eq(&weak)) {
            return false;
        }
        candidates.cells.push(weak);
        candidates.cells.len() >= candidates.threshold.max(INITIAL_THRESHOLD)
    });
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
    let mut seen = HashSet::new();
    let roots: Vec<Cell> = registered
        .iter()
        .filter(|weak| seen.insert(weak.as_ptr() as usize))
        .filter_map(WeakCell::upgrade)
        .collect();

    let mut graph = Graph::default();
    for root in &roots {
        graph.visit_cell(None, root, 0);
    }
    for root in &roots {
        // `roots` itself holds one strong reference to each.
        if let Some(node) = graph.nodes.get_mut(&addr(root)) {
            node.strong -= 1;
        }
    }
    let garbage = graph.garbage();
    drop(roots);
    for cell in garbage.iter().filter_map(WeakCell::upgrade) {
        clear(&cell);
    }

    let survivors: Vec<WeakCell> = registered.into_iter().filter(|weak| weak.strong_count() > 0).collect();
    CANDIDATES.with(|candidates| {
        let mut candidates = candidates.borrow_mut();
        let threshold = (survivors.len() * 2).max(INITIAL_THRESHOLD);
        survivors.into_iter().for_each(|weak| candidates.cells.push(weak));
        candidates.threshold = threshold;
    });
}

fn clear(cell: &Cell) {
    #[cfg(not(feature = "sync"))]
    let guard = cell.try_borrow_mut().ok();
    #[cfg(feature = "sync")]
    let guard = cell.try_write().ok();
    if let Some(mut guard) = guard {
        let old = std::mem::replace(&mut *guard, StackValue::Value(RuntimeValue::None));
        drop(guard);
        drop(old);
    }
}

fn addr<T: ?Sized>(shared: &Shared<T>) -> usize {
    Shared::as_ptr(shared) as *const () as usize
}

/// Hashes an address with one multiply; the graph is rebuilt on every collection, so the
/// default hasher's per-key cost dominates.
#[derive(Default)]
struct AddrHasher(u64);

impl Hasher for AddrHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        bytes.iter().for_each(|byte| self.write_u64(u64::from(*byte)));
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(26);
    }
}

type AddrMap<V> = HashMap<usize, V, BuildHasherDefault<AddrHasher>>;
type AddrSet = HashSet<usize, BuildHasherDefault<AddrHasher>>;

struct Node {
    strong: usize,
    internal: usize,
    opaque: bool,
    children: Vec<usize>,
    cell: Option<WeakCell>,
}

#[derive(Default)]
struct Graph {
    nodes: AddrMap<Node>,
    work: usize,
}

impl Graph {
    /// Adds the edge `parent -> id`. Returns whether `id` is new and still needs scanning.
    fn enter(&mut self, parent: Option<usize>, id: usize, strong: usize, cell: Option<WeakCell>) -> bool {
        if let Some(parent) = parent
            && let Some(node) = self.nodes.get_mut(&parent)
        {
            node.children.push(id);
        }
        let edge = usize::from(parent.is_some());
        match self.nodes.get_mut(&id) {
            Some(node) => {
                node.internal += edge;
                false
            }
            None => {
                self.nodes.insert(
                    id,
                    Node {
                        strong,
                        internal: edge,
                        opaque: false,
                        children: Vec::new(),
                        cell,
                    },
                );
                true
            }
        }
    }

    /// Whether the scan of node `id` must stop, treating it as referenced from outside.
    fn exhausted(&mut self, id: usize, depth: usize) -> bool {
        self.work += 1;
        if depth > MAX_DEPTH || self.work > MAX_WORK {
            self.nodes.get_mut(&id).expect("entered").opaque = true;
            return true;
        }
        false
    }

    fn visit_cell(&mut self, parent: Option<usize>, cell: &Cell, depth: usize) {
        let id = addr(cell);
        if !self.enter(parent, id, Shared::strong_count(cell), Some(Shared::downgrade(cell))) {
            return;
        }
        if self.exhausted(id, depth) {
            return;
        }
        #[cfg(not(feature = "sync"))]
        let guard = cell.try_borrow().ok();
        #[cfg(feature = "sync")]
        let guard = cell.try_read().ok();
        let Some(guard) = guard else {
            self.nodes.get_mut(&id).expect("entered").opaque = true;
            return;
        };
        match &*guard {
            StackValue::Closure(closure) => self.visit_closure(id, closure, depth + 1),
            StackValue::Value(value) => self.visit_value(id, value, depth + 1),
            StackValue::NestedWeakCoroutine(value) => self.visit_value(id, value, depth + 1),
            StackValue::WeakCoroutine(_) => {}
        }
    }

    fn visit_closure(&mut self, parent: usize, closure: &Shared<super::value::Closure>, depth: usize) {
        let id = addr(closure);
        if !self.enter(Some(parent), id, Shared::strong_count(closure), None) || self.exhausted(id, depth) {
            return;
        }
        if let Some(upvalues) = &closure.upvalues {
            self.visit_upvalues(id, upvalues, depth + 1);
        }
    }

    fn visit_upvalues(&mut self, parent: usize, upvalues: &Shared<Vec<Cell>>, depth: usize) {
        let id = addr(upvalues);
        if !self.enter(Some(parent), id, Shared::strong_count(upvalues), None) || self.exhausted(id, depth) {
            return;
        }
        for cell in upvalues.iter() {
            self.visit_cell(Some(id), cell, depth + 1);
        }
    }

    fn visit_value(&mut self, parent: usize, value: &RuntimeValue, depth: usize) {
        match value {
            RuntimeValue::Array(array) => {
                let id = addr(array);
                if !self.enter(Some(parent), id, Shared::strong_count(array), None) || self.exhausted(id, depth) {
                    return;
                }
                for element in array.iter() {
                    self.visit_value(id, element, depth + 1);
                }
            }
            RuntimeValue::Dict(dict) => {
                let id = addr(dict);
                if !self.enter(Some(parent), id, Shared::strong_count(dict), None) || self.exhausted(id, depth) {
                    return;
                }
                for element in dict.values() {
                    self.visit_value(id, element, depth + 1);
                }
            }
            RuntimeValue::Closure(closure) => {
                let id = addr(closure);
                if !self.enter(Some(parent), id, Shared::strong_count(closure), None) || self.exhausted(id, depth) {
                    return;
                }
                if let Some(upvalues) = &closure.upvalues {
                    self.visit_upvalues(id, upvalues, depth + 1);
                }
                for element in &closure.bound_args {
                    self.visit_value(id, element, depth + 1);
                }
            }
            _ => {}
        }
    }

    /// The cells no live node reaches. Empty when the scan hit its work limit.
    fn garbage(&self) -> Vec<WeakCell> {
        if self.work > MAX_WORK {
            return Vec::new();
        }
        let mut live = AddrSet::default();
        let mut pending: Vec<usize> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.opaque || node.strong > node.internal)
            .map(|(id, _)| *id)
            .collect();
        while let Some(id) = pending.pop() {
            if live.insert(id) {
                pending.extend(self.nodes[&id].children.iter().copied());
            }
        }
        self.nodes
            .iter()
            .filter(|(id, _)| !live.contains(id))
            .filter_map(|(_, node)| node.cell.clone())
            .collect()
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
        for _ in 0..(MAX_DEPTH * 4) {
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
