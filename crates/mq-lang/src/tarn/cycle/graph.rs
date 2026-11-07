//! The reference graph a collection scans, and the trial deletion over it.
//!
//! Nodes are the heap objects that closures and containers share: cells, closures, upvalue
//! lists, arrays, dicts. A node whose strong count is fully explained by edges inside the graph
//! is referenced only by a cycle, unless a node that something outside holds reaches it.

use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::value::{Cell, Closure, ReferenceVisitor, StackValue, WeakCell};
use rustc_hash::{FxHashMap, FxHashSet};

/// Nesting beyond this is not scanned; the node counts as referenced from outside.
pub(super) const MAX_DEPTH: usize = 128;
/// Nodes scanned before the whole collection gives up and frees nothing.
pub(super) const MAX_WORK: usize = 1 << 20;

/// Identity of a shared object: its address.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct NodeId(usize);

#[cfg(not(feature = "sync"))]
type WeakShared<T> = std::rc::Weak<T>;
#[cfg(feature = "sync")]
type WeakShared<T> = std::sync::Weak<T>;

impl NodeId {
    fn of<T: ?Sized>(shared: &Shared<T>) -> Self {
        Self(Shared::as_ptr(shared) as *const () as usize)
    }

    /// The node a weak reference points at, which is the same object as its strong handles.
    fn of_weak<T>(weak: &WeakShared<T>) -> Self {
        Self(weak.as_ptr() as *const () as usize)
    }
}

/// Where a reference to a node comes from.
#[derive(Clone, Copy)]
enum Referrer {
    /// A handle the collection itself holds to start a scan.
    Root,
    Node(NodeId),
}

/// The reference counts of a shared object, read together when the scan first reaches it.
struct Counts {
    strong: usize,
    weak: usize,
}

impl Counts {
    fn of<T>(shared: &Shared<T>) -> Self {
        Self {
            strong: Shared::strong_count(shared),
            weak: Shared::weak_count(shared),
        }
    }
}

struct Node {
    /// Strong count when the node was first reached.
    strong: usize,
    /// References counted from nodes inside the graph.
    inside: usize,
    /// Weak references other than the collector's own. Those that nothing in the graph accounts
    /// for can be upgraded at any time, so the node could come back to life after it was judged
    /// garbage.
    foreign_weak: usize,
    /// Not scanned, so what it references is unknown and it counts as held from outside.
    opaque: bool,
    children: Vec<NodeId>,
    /// Nodes this one holds weak references to. Not owning them, it does not keep them alive,
    /// but while it is alive it can upgrade them.
    weak_children: Vec<NodeId>,
    /// Present for cells only, to clear them if they turn out to be garbage.
    cell: Option<WeakCell>,
}

impl Node {
    /// `weak_inside` is how many of its weak references were found held by scanned nodes. The
    /// others belong to someone the scan cannot see; a mismatch in either direction counts too.
    fn is_held_from_outside(&self, weak_inside: usize) -> bool {
        self.opaque || self.strong > self.inside || self.foreign_weak != weak_inside
    }
}

pub(super) struct Graph {
    nodes: FxHashMap<NodeId, Node>,
    /// Weak references found held by scanned nodes, by the node they point at. The target may
    /// not have been reached yet when the holder is scanned.
    weak_inside: FxHashMap<NodeId, usize>,
    work: usize,
    max_work: usize,
}

impl Graph {
    /// Scans everything reachable from `roots`, which the caller holds one strong and one weak
    /// handle each on. Those handles are not counted as references from outside.
    pub(super) fn scan(roots: &[Cell]) -> Self {
        Self::scan_within(roots, MAX_WORK)
    }

    /// Like [`Graph::scan`], giving up after `max_work` nodes.
    pub(super) fn scan_within(roots: &[Cell], max_work: usize) -> Self {
        let mut graph = Self {
            nodes: FxHashMap::default(),
            weak_inside: FxHashMap::default(),
            work: 0,
            max_work,
        };
        for root in roots {
            graph.visit_cell(Referrer::Root, root, 0);
        }
        for root in roots {
            if let Some(node) = graph.nodes.get_mut(&NodeId::of(root)) {
                node.strong -= 1;
                node.foreign_weak -= 1;
            }
        }
        graph
    }

    /// The cells that no node held from outside reaches. Empty when the scan hit its work limit.
    pub(super) fn garbage_cells(&self) -> Vec<WeakCell> {
        if self.work > self.max_work {
            return Vec::new();
        }

        let live = self.live_nodes();

        self.nodes
            .iter()
            .filter(|(id, _)| !live.contains(id))
            .filter_map(|(_, node)| node.cell.clone())
            .collect()
    }

    fn live_nodes(&self) -> FxHashSet<NodeId> {
        let mut live = FxHashSet::default();
        let mut pending: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(id, node)| node.is_held_from_outside(self.weak_inside.get(id).copied().unwrap_or(0)))
            .map(|(id, _)| *id)
            .collect();

        while let Some(id) = pending.pop() {
            // A weak reference can point at an object the scan never reached.
            if live.insert(id)
                && let Some(node) = self.nodes.get(&id)
            {
                pending.extend(node.children.iter().chain(&node.weak_children));
            }
        }
        live
    }

    /// Records one reference `from -> id`. Returns whether the children of `id` still need
    /// scanning: it is new and within the depth and work limits.
    fn reference(&mut self, from: Referrer, id: NodeId, counts: Counts, depth: usize, cell: Option<WeakCell>) -> bool {
        let inside = match from {
            Referrer::Root => 0,
            Referrer::Node(from) => {
                self.nodes
                    .get_mut(&from)
                    .expect("a referencing node is already recorded")
                    .children
                    .push(id);
                1
            }
        };

        if let Some(node) = self.nodes.get_mut(&id) {
            node.inside += inside;
            return false;
        }

        self.work += 1;

        let opaque = depth > MAX_DEPTH || self.work > self.max_work;
        self.nodes.insert(
            id,
            Node {
                strong: counts.strong,
                foreign_weak: counts.weak,
                inside,
                opaque,
                children: Vec::new(),
                weak_children: Vec::new(),
                cell,
            },
        );
        !opaque
    }

    /// Records that the node `from` holds a weak reference to `to`.
    fn weak_edge(&mut self, from: NodeId, to: NodeId) {
        *self.weak_inside.entry(to).or_default() += 1;
        self.nodes
            .get_mut(&from)
            .expect("a referencing node is already recorded")
            .weak_children
            .push(to);
    }

    fn mark_opaque(&mut self, id: NodeId) {
        self.nodes
            .get_mut(&id)
            .expect("an opaque node is already recorded")
            .opaque = true;
    }

    /// Records a reference to `shared` and, the first time, scans its children with `scan`.
    fn visit<T>(&mut self, from: NodeId, shared: &Shared<T>, depth: usize, scan: impl FnOnce(&mut Self, NodeId, &T)) {
        let id = NodeId::of(shared);
        if self.reference(Referrer::Node(from), id, Counts::of(shared), depth, None) {
            scan(self, id, shared);
        }
    }

    fn visit_cell(&mut self, from: Referrer, cell: &Cell, depth: usize) {
        let id = NodeId::of(cell);
        // Counted before the collector adds its own weak handle to the cell.
        let counts = Counts::of(cell);
        if !self.reference(from, id, counts, depth, Some(Shared::downgrade(cell))) {
            return;
        }

        #[cfg(not(feature = "sync"))]
        let guard = cell.try_borrow().ok();
        #[cfg(feature = "sync")]
        let guard = cell.try_read().ok();
        let Some(guard) = guard else {
            self.mark_opaque(id);
            return;
        };

        self.visit_stack_value(id, &guard, depth + 1);
    }

    fn visit_stack_value(&mut self, from: NodeId, value: &StackValue, depth: usize) {
        match value {
            StackValue::Closure(closure) => self.visit_closure(from, closure, depth),
            StackValue::Value(value) => self.visit_value(from, value, depth),
            StackValue::NestedWeakCoroutine(value) => self.visit_value(from, value, depth),
            StackValue::WeakCoroutine(weak) => self.weak_edge(from, NodeId::of_weak(weak)),
        }
    }

    fn visit_closure(&mut self, from: NodeId, closure: &Shared<Closure>, depth: usize) {
        self.visit(from, closure, depth, |graph, id, closure| {
            if let Some(upvalues) = &closure.upvalues {
                graph.visit_upvalues(id, upvalues, depth + 1);
            }
        });
    }

    fn visit_upvalues(&mut self, from: NodeId, upvalues: &Shared<Vec<Cell>>, depth: usize) {
        self.visit(from, upvalues, depth, |graph, id, cells| {
            for cell in cells {
                graph.visit_cell(Referrer::Node(id), cell, depth + 1);
            }
        });
    }

    /// Only containers and closures can lead back to a cell; every other value is a leaf.
    fn visit_value(&mut self, from: NodeId, value: &RuntimeValue, depth: usize) {
        match value {
            RuntimeValue::Array(array) => self.visit(from, array, depth, |graph, id, elements| {
                for element in elements {
                    graph.visit_value(id, element, depth + 1);
                }
            }),
            RuntimeValue::Dict(dict) => self.visit(from, dict, depth, |graph, id, dict| {
                for element in dict.values() {
                    graph.visit_value(id, element, depth + 1);
                }
            }),
            RuntimeValue::Closure(closure) => self.visit(from, closure, depth, |graph, id, closure| {
                if let Some(upvalues) = &closure.upvalues {
                    graph.visit_upvalues(id, upvalues, depth + 1);
                }
                for element in &closure.bound_args {
                    graph.visit_value(id, element, depth + 1);
                }
            }),
            RuntimeValue::WeakCoroutine(weak) => self.weak_edge(from, NodeId::of_weak(weak)),
            RuntimeValue::Coroutine(handle) => self.visit(from, handle, depth, |graph, id, state| {
                #[cfg(not(feature = "sync"))]
                let guard = state.try_borrow().ok();
                #[cfg(feature = "sync")]
                let guard = state.try_read().ok();
                match guard {
                    Some(state) => state.visit_references(&mut Visiting {
                        graph,
                        from: id,
                        depth: depth + 1,
                    }),
                    // Running: it is borrowed by whoever drives it.
                    None => graph.mark_opaque(id),
                }
            }),
            _ => {}
        }
    }
}

/// Adapts the graph to what a suspended coroutine reports holding.
struct Visiting<'a> {
    graph: &'a mut Graph,
    from: NodeId,
    depth: usize,
}

impl ReferenceVisitor for Visiting<'_> {
    fn cell(&mut self, cell: &Cell) {
        self.graph.visit_cell(Referrer::Node(self.from), cell, self.depth);
    }

    fn upvalues(&mut self, upvalues: &Shared<Vec<Cell>>) {
        self.graph.visit_upvalues(self.from, upvalues, self.depth);
    }

    fn weak_cell(&mut self, cell: &WeakCell) {
        self.graph.weak_edge(self.from, NodeId::of_weak(cell));
    }

    fn stack_value(&mut self, value: &StackValue) {
        self.graph.visit_stack_value(self.from, value, self.depth);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(strong: usize, inside: usize, foreign_weak: usize) -> Node {
        Node {
            strong,
            inside,
            foreign_weak,
            opaque: false,
            children: Vec::new(),
            weak_children: Vec::new(),
            cell: Some(WeakCell::new()),
        }
    }

    fn graph(nodes: Vec<(usize, Node)>, weak_inside: Vec<(usize, usize)>) -> Graph {
        Graph {
            nodes: nodes.into_iter().map(|(id, node)| (NodeId(id), node)).collect(),
            weak_inside: weak_inside.into_iter().map(|(id, count)| (NodeId(id), count)).collect(),
            work: 0,
            max_work: MAX_WORK,
        }
    }

    /// Cells 1 and 2 refer to each other; cell 2 also has one weak reference.
    fn cycle_with_a_weak_reference() -> Vec<(usize, Node)> {
        let mut first = node(1, 1, 0);
        first.children = vec![NodeId(2)];
        let mut second = node(1, 1, 1);
        second.children = vec![NodeId(1)];
        vec![(1, first), (2, second)]
    }

    #[test]
    fn a_weak_reference_nothing_in_the_graph_holds_keeps_the_cycle() {
        let graph = graph(cycle_with_a_weak_reference(), vec![]);
        assert!(graph.garbage_cells().is_empty());
    }

    #[test]
    fn a_weak_reference_held_by_garbage_does_not_keep_the_cycle() {
        // Cell 3 is garbage too (held only by itself) and holds the weak reference to cell 2.
        let mut nodes = cycle_with_a_weak_reference();
        let mut holder = node(1, 1, 0);
        holder.children = vec![NodeId(3)];
        holder.weak_children = vec![NodeId(2)];
        nodes.push((3, holder));
        let graph = graph(nodes, vec![(2, 1)]);
        assert_eq!(graph.garbage_cells().len(), 3);
    }

    #[test]
    fn a_weak_reference_held_by_a_live_node_keeps_the_cycle() {
        // Cell 3 is held from outside and holds the weak reference to cell 2, so it can upgrade it.
        let mut nodes = cycle_with_a_weak_reference();
        let mut holder = node(2, 1, 0);
        holder.children = vec![NodeId(3)];
        holder.weak_children = vec![NodeId(2)];
        nodes.push((3, holder));
        let graph = graph(nodes, vec![(2, 1)]);
        assert!(graph.garbage_cells().is_empty());
    }

    #[test]
    fn more_weak_references_found_than_counted_are_treated_as_outside() {
        let graph = graph(cycle_with_a_weak_reference(), vec![(2, 2)]);
        assert!(graph.garbage_cells().is_empty());
    }
}
