//! Per-function wall-clock profiling for the Tarn VM.
//!
//! Available only with the `profile` feature. Unlike `vm_profile`, which counts opcodes, this
//! measures elapsed time per mq function and builtin, with self/total time and call counts.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Write as _};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::Ident;

thread_local! {
    static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Number of live scopes on all threads. While it is zero every VM hook returns after one
/// relaxed load instead of touching the thread-local.
static SCOPES: AtomicUsize = AtomicUsize::new(0);
/// Number of live scopes that also record lines, for the per-instruction hook.
static LINE_SCOPES: AtomicUsize = AtomicUsize::new(0);

const ROOT: u32 = u32::MAX;

/// Unprofiled lines shown around each profiled line in the HTML heatmap.
#[cfg(feature = "profile-lines")]
const HTML_CONTEXT: usize = 3;

/// A node of the call tree: one function reached through one call path.
struct Node {
    parent: u32,
    name: Ident,
    calls: u64,
    /// Every enter, including coroutine frames re-entered on resume. Each one costs a timer read.
    entries: u64,
    self_time: Duration,
}

/// Per-call cost of the hooks themselves, measured when a scope starts.
#[derive(Default, Clone, Copy)]
struct Overhead {
    /// Lands in the callee's own self time.
    callee: Duration,
    /// Lands in the caller's self time.
    caller: Duration,
}

struct Entry {
    node: u32,
    start: Instant,
    child_time: Duration,
}

/// Line attribution state. Time goes to the line of the instruction running when an
/// interval starts, so callee and native builtin time is charged to the line that ran it.
struct LineState {
    token: Option<crate::ast::TokenId>,
    current: Option<(u32, u32)>,
    since: Instant,
    map: HashMap<(u32, u32), (u64, Duration)>,
}

impl LineState {
    fn flush(&mut self, now: Instant) {
        if let Some(key) = self.current.take() {
            self.map.entry(key).or_default().1 += now.saturating_duration_since(self.since);
        }
        self.token = None;
    }
}

#[derive(Default)]
struct State {
    lines: Option<LineState>,
    nodes: Vec<Node>,
    index: HashMap<(u32, Ident), u32>,
    stack: Vec<Entry>,
    overhead: Overhead,
    closure_names: HashMap<(usize, crate::ast::TokenId), Ident>,
}

impl State {
    fn enter(&mut self, name: Ident, count_call: bool) {
        let parent = self.stack.last().map_or(ROOT, |entry| entry.node);
        let node = *self.index.entry((parent, name)).or_insert_with(|| {
            self.nodes.push(Node {
                parent,
                name,
                calls: 0,
                entries: 0,
                self_time: Duration::ZERO,
            });
            (self.nodes.len() - 1) as u32
        });
        let node_ref = &mut self.nodes[node as usize];
        node_ref.entries += 1;
        if count_call {
            node_ref.calls += 1;
        }
        self.stack.push(Entry {
            node,
            start: Instant::now(),
            child_time: Duration::ZERO,
        });
    }

    fn exit(&mut self) {
        let Some(entry) = self.stack.pop() else {
            return;
        };
        let elapsed = entry.start.elapsed();
        let node = &mut self.nodes[entry.node as usize];
        node.self_time += elapsed.saturating_sub(entry.child_time);
        if let Some(parent) = self.stack.last_mut() {
            parent.child_time += elapsed;
        }
    }
}

/// Measures what one enter/exit pair adds to the callee's and the caller's self time, on a
/// scratch state. A warm-up lets the clock and caches settle, and the median of several
/// batches resists scheduler noise.
fn measure_overhead() -> Overhead {
    const WARMUP_PAIRS: u32 = 20_000;
    const BATCHES: usize = 15;
    const PAIRS: u32 = 2_000;
    let name = Ident::new("<calibrate>");
    let mut state = State::default();
    let mut batch = |count: u32| {
        state.nodes.clear();
        state.index.clear();
        state.stack.clear();
        let started = Instant::now();
        for _ in 0..count {
            state.enter(name, true);
            state.exit();
        }
        let wall = started.elapsed();
        let callee = state.nodes.first().map_or(Duration::ZERO, |n| n.self_time);
        Overhead {
            callee: callee / count,
            caller: wall.saturating_sub(callee) / count,
        }
    };
    batch(WARMUP_PAIRS);
    let mut samples: Vec<Overhead> = (0..BATCHES).map(|_| batch(PAIRS)).collect();
    samples.sort_by_key(|o| o.callee + o.caller);
    samples[samples.len() / 2]
}

/// Time attributed to one source line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineProfile {
    /// Raw id of the module the line belongs to. `0` is the top-level query.
    pub module_id: u32,
    /// One-based line number.
    pub line: u32,
    /// Times execution moved onto this line.
    pub hits: u64,
    /// Time spent running instructions of this line, including native builtins it calls.
    pub time: Duration,
}

/// Aggregated timing of one function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionProfile {
    /// Function or builtin name. Unnamed closures are `<closure>`.
    pub name: String,
    /// Number of calls.
    pub calls: u64,
    /// Time spent in the function itself, excluding callees.
    pub self_time: Duration,
    /// Time spent in the function including callees. Recursive calls are counted once.
    pub total_time: Duration,
}

/// One call path in a finished profile. Parents precede their children.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Frame {
    parent: u32,
    /// Index into `Profile::labels`.
    label: u32,
    self_time: Duration,
}

/// The result of one profiled scope.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    functions: Vec<FunctionProfile>,
    /// Distinct display labels, shared by all frames of the same function.
    labels: Vec<String>,
    frames: Vec<Frame>,
    lines: Vec<LineProfile>,
    overhead: Duration,
}

impl Profile {
    /// Returns functions ordered by descending self time, then name.
    pub fn functions(&self) -> &[FunctionProfile] {
        &self.functions
    }

    /// Returns the measurement cost per call that was subtracted from the function times.
    pub fn overhead_per_call(&self) -> Duration {
        self.overhead
    }

    /// Returns the sum of self time over all functions.
    pub fn total_time(&self) -> Duration {
        self.functions.iter().map(|f| f.self_time).sum()
    }

    /// Returns lines ordered by descending time. Empty unless line profiling was enabled.
    pub fn lines(&self) -> &[LineProfile] {
        &self.lines
    }

    /// Renders the `top` hottest lines. `query` is the top-level query text, shown beside
    /// module `0` lines, and `module_name` resolves other module ids.
    pub fn format_lines(&self, top: usize, query: Option<&str>, module_name: impl Fn(u32) -> String) -> String {
        let all: f64 = self.lines.iter().map(|l| l.time.as_secs_f64()).sum();
        let mut out = format!("{:<28} {:>10} {:>12} {:>6}  source\n", "line", "hits", "time", "%");
        for l in self.lines.iter().take(top) {
            let pct = if all > 0.0 {
                l.time.as_secs_f64() / all * 100.0
            } else {
                0.0
            };
            let (place, text) = if l.module_id == 0 {
                let text = query
                    .and_then(|q| q.lines().nth(l.line.saturating_sub(1) as usize))
                    .map_or("", str::trim);
                (format!("query:{}", l.line), text)
            } else {
                (format!("{}:{}", module_name(l.module_id), l.line), "")
            };
            let _ = writeln!(out, "{place:<28} {:>10} {:>12.3?} {pct:>5.1}%  {text}", l.hits, l.time);
        }
        out
    }

    /// Replaces module ids in `<closure@ID:LINE>` labels with names from `module_name`, e.g.
    /// `<closure@query:3>`. Module `0` is the top-level query.
    #[must_use]
    pub fn with_module_names(mut self, module_name: impl Fn(u32) -> String) -> Self {
        let rename = |label: &str| -> Option<String> {
            let (id, line) = label.strip_prefix("<closure@")?.strip_suffix('>')?.split_once(':')?;
            let id: u32 = id.parse().ok()?;
            let name = if id == 0 { "query".to_string() } else { module_name(id) };
            Some(format!("<closure@{name}:{line}>"))
        };
        for f in &mut self.functions {
            if let Some(name) = rename(&f.name) {
                f.name = name;
            }
        }
        for label in &mut self.labels {
            if let Some(name) = rename(label) {
                *label = name;
            }
        }
        self
    }

    /// Renders Brendan Gregg's folded stacks (`a;b;c <microseconds>`), accepted by `inferno`
    /// and speedscope. Paths are built while walking the call tree, never stored.
    pub fn to_folded(&self) -> String {
        let mut first_child = vec![ROOT; self.frames.len()];
        let mut next_sibling = vec![ROOT; self.frames.len()];
        let mut roots = Vec::new();
        for (i, frame) in self.frames.iter().enumerate().rev() {
            if frame.parent == ROOT {
                roots.push(i as u32);
            } else {
                next_sibling[i] = first_child[frame.parent as usize];
                first_child[frame.parent as usize] = i as u32;
            }
        }
        let by_label = |a: &u32, b: &u32| {
            let (a, b) = (&self.frames[*a as usize], &self.frames[*b as usize]);
            self.labels[a.label as usize]
                .cmp(&self.labels[b.label as usize])
                .then(a.self_time.cmp(&b.self_time))
        };
        let mut out = String::new();
        // Siblings are visited in label order, which sorts the output by path.
        let mut todo: Vec<(u32, usize)> = Vec::new();
        let push_sorted = |todo: &mut Vec<(u32, usize)>, mut nodes: Vec<u32>, depth: usize| {
            nodes.sort_by(|a, b| by_label(b, a));
            todo.extend(nodes.into_iter().map(|n| (n, depth)));
        };
        push_sorted(&mut todo, roots, 0);
        let mut path: Vec<u32> = Vec::new();
        while let Some((node, depth)) = todo.pop() {
            let frame = &self.frames[node as usize];
            path.truncate(depth);
            path.push(frame.label);
            let micros = frame.self_time.as_micros();
            if micros > 0 {
                for (i, label) in path.iter().enumerate() {
                    if i > 0 {
                        out.push(';');
                    }
                    out.push_str(&self.labels[*label as usize]);
                }
                let _ = writeln!(out, " {micros}");
            }
            let mut children = Vec::new();
            let mut child = first_child[node as usize];
            while child != ROOT {
                children.push(child);
                child = next_sibling[child as usize];
            }
            push_sorted(&mut todo, children, depth + 1);
        }
        out
    }

    /// Renders the per-function table as JSON, with times in nanoseconds.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\"total_ns\":");
        let _ = write!(
            out,
            "{},\"overhead_per_call_ns\":{},\"functions\":[",
            self.total_time().as_nanos(),
            self.overhead.as_nanos()
        );
        for (i, f) in self.functions.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let _ = write!(
                out,
                "{{\"name\":\"{}\",\"calls\":{},\"self_ns\":{},\"total_ns\":{}}}",
                f.name.replace('\\', "\\\\").replace('"', "\\\""),
                f.calls,
                f.self_time.as_nanos(),
                f.total_time.as_nanos()
            );
        }
        out.push_str("]}");
        out
    }
}

/// One module's accumulated lines in a [`LineReport`].
#[cfg(feature = "profile-lines")]
#[derive(Debug)]
struct ReportModule {
    name: String,
    source: Option<String>,
    lines: HashMap<u32, (u64, Duration)>,
}

/// Line profiles from several evaluations merged into one HTML heatmap. Modules are matched
/// by name and source, so engines that number the same module differently still combine.
#[cfg(feature = "profile-lines")]
#[derive(Debug, Default)]
pub struct LineReport {
    modules: Vec<ReportModule>,
}

#[cfg(feature = "profile-lines")]
impl LineReport {
    /// Adds the lines of `profile`. `module` returns a module's display name and source for
    /// a raw id.
    pub fn add(&mut self, profile: &Profile, module: impl Fn(u32) -> (String, Option<String>)) {
        let mut ids: HashMap<u32, usize> = HashMap::new();
        for l in &profile.lines {
            let index = *ids.entry(l.module_id).or_insert_with(|| {
                let (name, source) = module(l.module_id);
                self.modules
                    .iter()
                    .position(|m| m.name == name && m.source == source)
                    .unwrap_or_else(|| {
                        self.modules.push(ReportModule {
                            name,
                            source,
                            lines: HashMap::new(),
                        });
                        self.modules.len() - 1
                    })
            });
            let entry = self.modules[index].lines.entry(l.line).or_default();
            entry.0 += l.hits;
            entry.1 += l.time;
        }
    }

    /// Whether no line was recorded.
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Renders a self-contained HTML heatmap, one section per module, hottest first. Line
    /// background darkens with the line's share of the hottest line's time.
    pub fn format_html(&self) -> String {
        let module_time = |id: usize| -> Duration { self.modules[id].lines.values().map(|l| l.1).sum() };
        let mut modules: Vec<usize> = (0..self.modules.len()).collect();
        modules.sort_by_key(|&id| (std::cmp::Reverse(module_time(id)), id));
        let hottest = self
            .modules
            .iter()
            .flat_map(|m| m.lines.values().map(|l| l.1))
            .max()
            .unwrap_or_default()
            .as_secs_f64();

        let mut out = String::from(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>mq line profile</title><style>\
:root{--bg:#2a3444;--card:#1e293b;--fg:#e2e8f0;--muted:#94a3b8;--line:#4a5568;--accent:#4fc3f7;--accent-fg:#85d4ff}\
body{margin:0;padding:2rem;background:var(--bg);color:var(--fg);font-family:Inter,system-ui,sans-serif}\
h1{font-family:'Space Grotesk',Inter,sans-serif;font-size:1.6rem;color:var(--accent-fg);margin:0 0 .25rem}\
p.sub{color:var(--muted);margin:0 0 1.5rem;font-size:.85rem}\
section{background:var(--card);border:1px solid var(--line);border-radius:.75rem;margin:0 0 1.25rem;overflow:hidden}\
h2{font-size:.9rem;font-weight:600;margin:0;padding:.7rem 1rem;border-bottom:1px solid var(--line);color:var(--accent-fg)}\
h2 span{color:var(--muted);font-weight:400;margin-left:.6rem}\
table{border-collapse:collapse;width:100%;font:12px/1.6 ui-monospace,SFMono-Regular,Menlo,monospace}\
td{padding:0 .75rem;white-space:pre;vertical-align:top}\
td.n{text-align:right;color:var(--muted);width:1%}\
td.g{color:var(--muted);text-align:center;padding:.15rem 0}\
tr:hover td{background:rgba(255,255,255,.05)}\
</style></head><body><h1>mq line profile</h1><p class=sub>Darker lines took more time, relative to the hottest line.</p>",
        );
        for &id in &modules {
            let ReportModule {
                name,
                source,
                lines: times,
            } = &self.modules[id];
            let _ = write!(
                out,
                "<section><h2>{}<span>{:.3?}</span></h2><table>",
                html_escape(name),
                module_time(id)
            );
            let source_lines: Vec<&str> = source.as_deref().map(|s| s.lines().collect()).unwrap_or_default();
            let last = times.keys().copied().max().unwrap_or(0) as usize;
            let total = source_lines.len().max(last);
            // Show profiled lines with a few lines of context; collapse the rest.
            let visible = |number: usize| {
                (number.saturating_sub(HTML_CONTEXT)..=number + HTML_CONTEXT).any(|n| times.contains_key(&(n as u32)))
            };
            let mut skipped = false;
            for number in 1..=total {
                if !visible(number) {
                    if !skipped {
                        out.push_str("<tr><td class=g colspan=4>&#8942;</td></tr>");
                        skipped = true;
                    }
                    continue;
                }
                skipped = false;
                let text = source_lines.get(number - 1).copied().unwrap_or("");
                let (hits, time, style) = match times.get(&(number as u32)) {
                    Some(&(hits, time)) => {
                        // Square root keeps lines much cooler than the hottest one visible.
                        let alpha = if hottest > 0.0 {
                            (time.as_secs_f64() / hottest).sqrt() * 0.6
                        } else {
                            0.0
                        };
                        (
                            hits.to_string(),
                            format!("{time:.3?}"),
                            format!(" style=\"background:rgba(79,195,247,{alpha:.2})\""),
                        )
                    }
                    None => (String::new(), String::new(), String::new()),
                };
                let _ = write!(
                    out,
                    "<tr{style}><td class=n>{number}</td><td class=n>{hits}</td><td class=n>{time}</td><td>{}</td></tr>",
                    html_escape(text)
                );
            }
            out.push_str("</table></section>");
        }
        out.push_str("</body></html>");
        out
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let all = self.total_time().as_secs_f64();
        writeln!(
            f,
            "{:<32} {:>10} {:>12} {:>12} {:>6}",
            "function", "calls", "self", "total", "self%"
        )?;
        for func in &self.functions {
            let pct = if all > 0.0 {
                func.self_time.as_secs_f64() / all * 100.0
            } else {
                0.0
            };
            writeln!(
                f,
                "{:<32} {:>10} {:>12.3?} {:>12.3?} {:>5.1}%",
                func.name, func.calls, func.self_time, func.total_time, pct
            )?;
        }
        writeln!(f, "(measurement cost of {:.0?} per call subtracted)", self.overhead)?;
        Ok(())
    }
}

/// Activates function profiling on the current thread until [`Self::finish`] is called.
///
/// Scopes may be nested; an inner scope temporarily replaces its parent's data.
pub struct ProfileScope {
    previous: Option<State>,
    finished: bool,
    lines: bool,
}

impl ProfileScope {
    /// Starts profiling on the current thread.
    pub fn start() -> Self {
        SCOPES.fetch_add(1, Ordering::Relaxed);
        let previous = ACTIVE.with(|active| active.replace(Some(State::default())));
        let overhead = measure_overhead();
        with_state(|state| state.overhead = overhead);
        Self {
            previous,
            finished: false,
            lines: false,
        }
    }

    /// Also attributes time to source lines. Requires the `profile-lines` feature to have an effect.
    #[must_use]
    pub fn with_lines(mut self) -> Self {
        with_state(|state| {
            state.lines = Some(LineState {
                token: None,
                current: None,
                since: Instant::now(),
                map: HashMap::new(),
            });
        });
        if !self.lines {
            self.lines = true;
            LINE_SCOPES.fetch_add(1, Ordering::Relaxed);
        }
        self
    }

    /// Stops this scope, restores a possible parent scope, and returns the result.
    pub fn finish(mut self) -> Profile {
        let state = ACTIVE.with(|active| active.replace(self.previous.take()));
        self.finished = true;
        state.map(build).unwrap_or_default()
    }
}

impl Drop for ProfileScope {
    fn drop(&mut self) {
        if !self.finished {
            ACTIVE.with(|active| active.replace(self.previous.take()));
        }
        SCOPES.fetch_sub(1, Ordering::Relaxed);
        if self.lines {
            LINE_SCOPES.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(feature = "profile-lines")]
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn label(name: Ident) -> String {
    name.resolve_with(str::to_string)
}

fn build(mut state: State) -> Profile {
    while !state.stack.is_empty() {
        state.exit();
    }
    let mut lines: Vec<LineProfile> = state
        .lines
        .take()
        .map(|mut l| {
            l.flush(Instant::now());
            l.map
                .into_iter()
                .map(|((module_id, line), (hits, time))| LineProfile {
                    module_id,
                    line,
                    hits,
                    time,
                })
                .collect()
        })
        .unwrap_or_default();
    lines.sort_by(|a, b| {
        b.time
            .cmp(&a.time)
            .then(a.module_id.cmp(&b.module_id))
            .then(a.line.cmp(&b.line))
    });
    let nodes = &state.nodes;
    // Charge each call's measurement cost back out: the callee's own time holds the hooks'
    // callee share, and the caller's time holds the caller share of every call it made.
    let mut child_entries = vec![0u64; nodes.len()];
    for node in nodes {
        if node.parent != ROOT {
            child_entries[node.parent as usize] += node.entries;
        }
    }
    // Calibrated at both ends; the smaller is less likely to be inflated by a noisy moment.
    let end = measure_overhead();
    let overhead = if end.callee + end.caller < state.overhead.callee + state.overhead.caller {
        end
    } else {
        state.overhead
    };
    let corrected: Vec<Duration> = nodes
        .iter()
        .zip(&child_entries)
        .map(|(node, &children)| {
            let cost = overhead
                .callee
                .saturating_mul(node.entries.try_into().unwrap_or(u32::MAX))
                + overhead.caller.saturating_mul(children.try_into().unwrap_or(u32::MAX));
            node.self_time.saturating_sub(cost)
        })
        .collect();

    // Parents are always created before their children, so one reverse pass sums subtrees.
    let mut subtree = corrected.clone();
    for i in (0..nodes.len()).rev() {
        if nodes[i].parent != ROOT {
            let time = subtree[i];
            subtree[nodes[i].parent as usize] += time;
        }
    }
    // Walk the call tree once, tracking which functions are active, so a recursive function's
    // total counts only its outermost activation.
    let mut first_child = vec![ROOT; nodes.len()];
    let mut next_sibling = vec![ROOT; nodes.len()];
    let mut roots = Vec::new();
    for (i, node) in nodes.iter().enumerate().rev() {
        if node.parent == ROOT {
            roots.push(i as u32);
        } else {
            next_sibling[i] = first_child[node.parent as usize];
            first_child[node.parent as usize] = i as u32;
        }
    }
    let mut outermost = vec![false; nodes.len()];
    let mut active: HashMap<Ident, u32> = HashMap::new();
    // The flag marks the exit of a node whose children are done.
    let mut todo: Vec<(u32, bool)> = roots.into_iter().map(|n| (n, false)).collect();
    while let Some((index, leaving)) = todo.pop() {
        let name = nodes[index as usize].name;
        if leaving {
            if let Some(count) = active.get_mut(&name) {
                *count -= 1;
            }
            continue;
        }
        let count = active.entry(name).or_insert(0);
        outermost[index as usize] = *count == 0;
        *count += 1;
        todo.push((index, true));
        let mut child = first_child[index as usize];
        while child != ROOT {
            todo.push((child, false));
            child = next_sibling[child as usize];
        }
    }

    let mut label_ids: HashMap<Ident, u32> = HashMap::new();
    let mut labels: Vec<String> = Vec::new();
    let mut by_name: HashMap<Ident, FunctionProfile> = HashMap::new();
    let mut frames = Vec::with_capacity(nodes.len());
    for (i, node) in nodes.iter().enumerate() {
        let label_id = *label_ids.entry(node.name).or_insert_with(|| {
            labels.push(label(node.name));
            (labels.len() - 1) as u32
        });
        let entry = by_name.entry(node.name).or_insert_with(|| FunctionProfile {
            name: labels[label_id as usize].clone(),
            calls: 0,
            self_time: Duration::ZERO,
            total_time: Duration::ZERO,
        });
        entry.calls += node.calls;
        entry.self_time += corrected[i];
        if outermost[i] {
            entry.total_time += subtree[i];
        }
        frames.push(Frame {
            parent: node.parent,
            label: label_id,
            self_time: corrected[i],
        });
    }
    let mut functions: Vec<_> = by_name.into_values().collect();
    functions.sort_by(|a, b| b.self_time.cmp(&a.self_time).then_with(|| a.name.cmp(&b.name)));

    Profile {
        functions,
        labels,
        frames,
        lines,
        overhead: overhead.callee + overhead.caller,
    }
}

#[inline(always)]
fn with_state(f: impl FnOnce(&mut State)) {
    if SCOPES.load(Ordering::Relaxed) == 0 {
        return;
    }
    ACTIVE.with(|active| {
        if let Some(state) = active.borrow_mut().as_mut() {
            f(state);
        }
    });
}

/// Starts timing builtin `name`.
#[inline]
pub(crate) fn enter(name: Ident) {
    with_state(|state| state.enter(name, true));
}

/// Starts timing the function a frame runs. Unnamed chunks are labelled by the line of their
/// first instruction (`<closure@MODULE:LINE>`), or `<main>` for the top-level program.
pub(crate) fn enter_frame(
    name: Option<Ident>,
    token: Option<crate::ast::TokenId>,
    arena: &crate::TokenArena,
    count_call: bool,
    top_level: bool,
) {
    with_state(|state| {
        let name = match name {
            Some(name) => name,
            None if top_level => Ident::new("<main>"),
            None => match token {
                Some(id) => {
                    let key = (crate::Shared::as_ptr(arena) as *const () as usize, id);
                    *state.closure_names.entry(key).or_insert_with(|| {
                        let (module, line) = crate::with_token(arena, id, |t| (t.module_id.raw(), t.range.start.line));
                        Ident::new(&format!("<closure@{module}:{line}>"))
                    })
                }
                None => Ident::new("<closure>"),
            },
        };
        state.enter(name, count_call);
    });
}

/// Stops timing the innermost entry.
#[inline]
pub(crate) fn exit() {
    with_state(State::exit);
}

/// Whether line attribution is on. Checked before computing a token, so the per-instruction
/// cost with no scope is one relaxed load.
#[cfg(feature = "profile-lines")]
#[inline(always)]
pub(crate) fn lines_enabled() -> bool {
    LINE_SCOPES.load(Ordering::Relaxed) > 0
}

/// Moves line attribution to `token`'s line. Called before each instruction.
#[cfg(feature = "profile-lines")]
#[inline]
pub(crate) fn line_tick(token: Option<crate::ast::TokenId>, arena: &crate::TokenArena) {
    with_state(|state| {
        let Some(lines) = state.lines.as_mut() else {
            return;
        };
        if token == lines.token && lines.current.is_some() {
            return;
        }
        lines.token = token;
        let key = token.map(|id| crate::with_token(arena, id, |t| (t.module_id.raw(), t.range.start.line)));
        if key == lines.current {
            return;
        }
        let now = Instant::now();
        if let Some(previous) = lines.current {
            lines.map.entry(previous).or_default().1 += now.saturating_duration_since(lines.since);
        }
        if let Some(key) = key {
            lines.map.entry(key).or_default().0 += 1;
        }
        lines.current = key;
        lines.since = now;
    });
}

/// Closes the open line interval so time outside the VM is not charged to a line.
#[cfg(feature = "profile-lines")]
#[inline]
pub(crate) fn line_flush() {
    with_state(|state| {
        if let Some(lines) = state.lines.as_mut() {
            lines.flush(Instant::now());
        }
    });
}

/// Returns the current stack depth, or 0 when inactive.
#[inline]
pub(crate) fn depth() -> usize {
    if SCOPES.load(Ordering::Relaxed) == 0 {
        return 0;
    }
    ACTIVE.with(|active| active.borrow().as_ref().map_or(0, |s| s.stack.len()))
}

/// Exits entries until the stack is `mark` deep. Frames the VM drops without a pop
/// (a drive's bottom frame, a suspended coroutine) are closed here.
#[inline]
pub(crate) fn truncate(mark: usize) {
    with_state(|state| {
        while state.stack.len() > mark {
            state.exit();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_and_total_exclude_double_counted_recursion() {
        let scope = ProfileScope::start();
        let f = Ident::new("f");
        let g = Ident::new("g");
        enter(f);
        enter(f);
        enter(g);
        exit();
        exit();
        exit();
        let profile = scope.finish();
        let f = profile.functions().iter().find(|x| x.name == "f").unwrap();
        assert_eq!(f.calls, 2);
        assert!(f.total_time >= f.self_time);
        assert!(profile.to_folded().is_empty() || profile.to_folded().contains("f;f"));
    }

    #[test]
    fn truncate_closes_open_entries() {
        let scope = ProfileScope::start();
        enter(Ident::new("a"));
        enter(Ident::new("b"));
        truncate(0);
        assert_eq!(depth(), 0);
        assert_eq!(scope.finish().functions().len(), 2);
    }

    fn profile_of(code: &str) -> (Profile, bool) {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let scope = ProfileScope::start();
        let ok = engine.eval(code, std::iter::once(crate::RuntimeValue::None)).is_ok();
        assert_eq!(depth(), 0, "profile stack must be balanced after evaluation");
        (scope.finish(), ok)
    }

    fn function<'a>(profile: &'a Profile, name: &str) -> &'a FunctionProfile {
        profile
            .functions()
            .iter()
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("{name} not profiled: {:?}", profile.functions()))
    }

    #[test]
    fn recursion_counts_each_call_and_total_is_not_double_counted() {
        let (profile, ok) = profile_of("def fib(n): if (n < 2): n else: fib(n - 1) + fib(n - 2); fib(10)");
        assert!(ok);
        let fib = function(&profile, "fib");
        assert_eq!(fib.calls, 177);
        assert!(fib.total_time >= fib.self_time);
        assert!(fib.total_time <= profile.total_time());
    }

    #[test]
    fn tail_calls_are_counted_once_per_iteration() {
        let (profile, ok) =
            profile_of("def count(n, acc): if (n == 0): acc else: count(n - 1, acc + 1); count(100, 0)");
        assert!(ok);
        assert_eq!(function(&profile, "count").calls, 101);
    }

    #[test]
    fn generator_resume_keeps_the_stack_balanced() {
        let code = "def h(): let c = (fn(): yield: 1 | yield: 2 | yield: 3;)() | next(c) | next(c) | next(c); h()";
        let (profile, ok) = profile_of(code);
        assert!(ok);
        assert_eq!(function(&profile, "h").calls, 1);
        let generator = profile
            .functions()
            .iter()
            .find(|f| f.name.starts_with("<closure@"))
            .expect("generator frame is profiled");
        assert_eq!(generator.calls, 1);
    }

    #[test]
    fn caught_errors_keep_the_stack_balanced() {
        let code = "def boom(n): if (n == 0): error(\"x\") else: boom(n - 1); try: boom(3) catch: 1";
        let (profile, ok) = profile_of(code);
        assert!(ok);
        assert_eq!(function(&profile, "boom").calls, 4);
    }

    #[test]
    fn uncaught_errors_still_produce_a_profile() {
        let (profile, ok) = profile_of("def inf(n): inf(n + 1) + 1; inf(0)");
        assert!(!ok);
        assert!(function(&profile, "inf").calls > 1);
    }

    #[test]
    fn unnamed_functions_are_labelled_by_location() {
        let (profile, ok) = profile_of("def f(): [1, 2] | map(fn(x): x + 1;); f()");
        assert!(ok);
        let profile = profile.with_module_names(|id| format!("m{id}"));
        assert!(function(&profile, "<main>").calls == 1);
        assert!(
            profile
                .functions()
                .iter()
                .any(|f| f.name.starts_with("<closure@query:1")),
            "{:?}",
            profile.functions()
        );
    }

    #[test]
    fn measurement_cost_is_subtracted_without_exceeding_wall_time() {
        let mut engine = crate::DefaultEngine::default();
        engine.load_builtin_module();
        let scope = ProfileScope::start();
        let started = Instant::now();
        let result = engine.eval(
            "def fib(n): if (n < 2): n else: fib(n - 1) + fib(n - 2); fib(15)",
            std::iter::once(crate::RuntimeValue::None),
        );
        let wall = started.elapsed();
        assert!(result.is_ok());
        let profile = scope.finish();
        assert!(profile.overhead_per_call() > Duration::ZERO);
        // Subtracting the hooks' cost leaves less than the elapsed time, never more.
        assert!(profile.total_time() < wall, "{:?} vs {wall:?}", profile.total_time());
        assert!(profile.to_json().contains("\"overhead_per_call_ns\""));
    }

    #[test]
    fn correction_never_makes_times_negative_or_total_smaller_than_self() {
        let (profile, _) = profile_of("def f(n): if (n == 0): 0 else: f(n - 1); f(50)");
        for f in profile.functions() {
            assert!(f.total_time >= f.self_time, "{f:?}");
        }
    }

    #[test]
    fn builtins_called_as_values_are_profiled() {
        let (profile, ok) = profile_of("[[1], [2, 3]] | map(len)");
        assert!(ok);
        assert_eq!(function(&profile, "len").calls, 2);
        let (profile, ok) = profile_of("len([1, 2])");
        assert!(ok);
        assert_eq!(function(&profile, "len").calls, 1);
    }

    #[test]
    fn deep_recursion_profiles_without_materializing_paths() {
        let (profile, ok) = profile_of("def f(n): if (n <= 0): 0 else: 1 + f(n - 1); | f(100)");
        assert!(ok);
        let f = function(&profile, "f");
        assert_eq!(f.calls, 101);
        assert!(f.total_time >= f.self_time);
        assert!(profile.to_json().contains("\"name\":\"f\""));
    }

    #[test]
    fn folded_stacks_are_sorted_by_path() {
        let scope = ProfileScope::start();
        let (a, b) = (Ident::new("a"), Ident::new("b"));
        for name in [b, a] {
            enter(name);
            std::thread::sleep(Duration::from_millis(2));
            enter(Ident::new("c"));
            std::thread::sleep(Duration::from_millis(2));
            exit();
            exit();
        }
        let folded = scope.finish().to_folded();
        let paths: Vec<&str> = folded.lines().filter_map(|l| l.rsplit_once(' ').map(|p| p.0)).collect();
        assert_eq!(paths, ["a", "a;c", "b", "b;c"]);
    }
}
