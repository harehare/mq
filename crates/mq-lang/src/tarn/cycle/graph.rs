//! The reference graph a collection scans, and the trial deletion over it.
//!
//! Nodes are the heap objects that closures and containers share: cells, closures, upvalue
//! lists, arrays, dicts. A node whose strong count is fully explained by edges inside the graph
//! is referenced only by a cycle, unless a node that something outside holds reaches it.

use crate::Shared;
use crate::runtime::runtime_value::RuntimeValue;
use crate::tarn::value::{Cell, Closure, StackValue, WeakCell};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// Nesting beyond this is not scanned; the node counts as referenced from outside.
pub(super) const MAX_DEPTH: usize = 128;
/// Nodes scanned before the whole collection gives up and frees nothing.
const MAX_WORK: usize = 1 << 20;

/// Hashes an address with one multiply. The graph is rebuilt on every collection, so the
/// default hasher's per-key cost would dominate.
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

type AddrMap<V> = HashMap<NodeId, V, BuildHasherDefault<AddrHasher>>;
type AddrSet = HashSet<NodeId, BuildHasherDefault<AddrHasher>>;

/// Identity of a shared object: its address.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct NodeId(usize);

impl NodeId {
    fn of<T: ?Sized>(shared: &Shared<T>) -> Self {
        Self(Shared::as_ptr(shared) as *const () as usize)
    }
}

/// Where a reference to a node comes from.
#[derive(Clone, Copy)]
enum Referrer {
    /// A handle the collection itself holds to start a scan.
    Root,
    Node(NodeId),
}

struct Node {
    /// Strong count when the node was first reached.
    strong: usize,
    /// References counted from nodes inside the graph.
    inside: usize,
    /// Not scanned, so what it references is unknown and it counts as held from outside.
    opaque: bool,
    children: Vec<NodeId>,
    /// Kept for cells only, to clear them if they turn out to be garbage.
    cell: Option<WeakCell>,
}

impl Node {
    fn is_held_from_outside(&self) -> bool {
        self.opaque || self.strong > self.inside
    }
}

#[derive(Default)]
pub(super) struct Graph {
    nodes: AddrMap<Node>,
    work: usize,
}

impl Graph {
    /// Scans everything reachable from `roots`, which the caller holds one handle each on.
    /// Those handles are not counted as references from outside.
    pub(super) fn scan(roots: &[Cell]) -> Self {
        let mut graph = Self::default();
        for root in roots {
            graph.visit_cell(Referrer::Root, root, 0);
        }
        for root in roots {
            if let Some(node) = graph.nodes.get_mut(&NodeId::of(root)) {
                node.strong -= 1;
            }
        }
        graph
    }

    /// The cells that no node held from outside reaches. Empty when the scan hit its work limit.
    pub(super) fn garbage_cells(&self) -> Vec<WeakCell> {
        if self.work > MAX_WORK {
            return Vec::new();
        }

        let live = self.live_nodes();

        self.nodes
            .iter()
            .filter(|(id, _)| !live.contains(id))
            .filter_map(|(_, node)| node.cell.clone())
            .collect()
    }

    fn live_nodes(&self) -> AddrSet {
        let mut live = AddrSet::default();
        let mut pending: Vec<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, node)| node.is_held_from_outside())
            .map(|(id, _)| *id)
            .collect();

        while let Some(id) = pending.pop() {
            if live.insert(id) {
                pending.extend(&self.nodes[&id].children);
            }
        }
        live
    }

    /// Records one reference `from -> id`. Returns whether the children of `id` still need
    /// scanning: it is new and within the depth and work limits.
    fn reference(&mut self, from: Referrer, id: NodeId, strong: usize, depth: usize, cell: Option<WeakCell>) -> bool {
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

        let opaque = depth > MAX_DEPTH || self.work > MAX_WORK;
        self.nodes.insert(
            id,
            Node {
                strong,
                inside,
                opaque,
                children: Vec::new(),
                cell,
            },
        );
        !opaque
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
        if self.reference(Referrer::Node(from), id, Shared::strong_count(shared), depth, None) {
            scan(self, id, shared);
        }
    }

    fn visit_cell(&mut self, from: Referrer, cell: &Cell, depth: usize) {
        let id = NodeId::of(cell);
        let strong = Shared::strong_count(cell);

        if !self.reference(from, id, strong, depth, Some(Shared::downgrade(cell))) {
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

        match &*guard {
            StackValue::Closure(closure) => self.visit_closure(id, closure, depth + 1),
            StackValue::Value(value) => self.visit_value(id, value, depth + 1),
            StackValue::NestedWeakCoroutine(value) => self.visit_value(id, value, depth + 1),
            StackValue::WeakCoroutine(_) => {}
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
            _ => {}
        }
    }
}
