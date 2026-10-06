//! Runtime conformance: do the values `let` bindings hold at runtime fit the types the checker
//! inferred for them?
//!
//! The `.mq` test files are run with a handler that, before every statement, looks at the
//! bindings visible to the running frame. A binding is matched to the `let` of the same name that
//! is in scope at that point (in the test file, `builtin.mq` or a standard module), and its value
//! is compared with the inferred type. A value outside the type is a sign of an unsound inference
//! (a false negative of the checker, or a type that is too narrow and would give false positives).
//!
//! Slow (every statement is stepped), so it is ignored by default:
//! `just test-conformance`. The result is pinned in `tests/runtime_conformance.snap`;
//! regenerate it with `UPDATE_CORPUS=1`.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mq_check::{TypeChecker, types::Type};
use mq_hir::{Hir, SymbolId, SymbolKind};
use mq_lang::{DebugContext, DebuggerAction, DebuggerHandler, Position, RuntimeValue};
use mq_markdown::NodeKind;
use url::Url;

/// The shape of a runtime value, as much as is needed to compare it with a type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Shape {
    Number,
    Bool,
    String,
    Symbol,
    Bytes,
    None,
    Node(NodeKind),
    /// The distinct shapes of (up to a sample of) the elements
    Array(Vec<Shape>),
    Dict(Vec<(String, Shape)>),
    Function,
    Generator,
    Other,
}

const SAMPLE: usize = 16;

fn shape_of(value: &RuntimeValue) -> Shape {
    match value {
        RuntimeValue::Number(_) => Shape::Number,
        RuntimeValue::Boolean(_) => Shape::Bool,
        RuntimeValue::String(_) => Shape::String,
        RuntimeValue::Symbol(_) => Shape::Symbol,
        RuntimeValue::Bytes(_) => Shape::Bytes,
        RuntimeValue::None => Shape::None,
        RuntimeValue::Markdown(node, _) => Shape::Node(node.kind()),
        RuntimeValue::Array(items) => {
            let shapes: BTreeSet<Shape> = items.iter().take(SAMPLE).map(shape_of).collect();
            Shape::Array(shapes.into_iter().collect())
        }
        RuntimeValue::Dict(entries) => {
            let mut fields: Vec<(String, Shape)> = entries.iter().map(|(k, v)| (k.to_string(), shape_of(v))).collect();
            fields.sort();
            Shape::Dict(fields)
        }
        // Closures and coroutines have private representations; go by their type name.
        other => match other.name() {
            "coroutine" => Shape::Generator,
            "function" | "native_function" => Shape::Function,
            _ => Shape::Other,
        },
    }
}

/// Whether a value of `shape` can have the type `ty`. Types the checker leaves open (type
/// variables, `dynamic`) accept everything.
fn fits(shape: &Shape, ty: &Type) -> bool {
    match (ty, shape) {
        (Type::Var(_) | Type::Dynamic | Type::Never | Type::RowEmpty, _) => true,
        (Type::Union(members), _) => members.iter().any(|member| fits(shape, member)),
        (Type::Int | Type::Float | Type::Number, Shape::Number)
        | (Type::String, Shape::String)
        | (Type::Bool, Shape::Bool)
        | (Type::Symbol, Shape::Symbol)
        | (Type::Bytes, Shape::Bytes)
        | (Type::None, Shape::None)
        | (Type::Function(..), Shape::Function)
        | (Type::Generator(_), Shape::Generator)
        | (Type::Tuple(_), Shape::Array(_)) => true,
        (Type::Node(kinds), Shape::Node(kind)) => kinds.contains(*kind),
        (Type::Array(elem), Shape::Array(items)) => items.iter().all(|item| fits(item, elem)),
        (Type::Dict(_, value), Shape::Dict(fields)) => fields.iter().all(|(_, field)| fits(field, value)),
        (Type::Record(known, _), Shape::Dict(fields)) => known.iter().all(|(name, field_ty)| {
            fields
                .iter()
                .find(|(field_name, _)| field_name == name)
                .is_none_or(|(_, field)| fits(field, field_ty))
        }),
        _ => false,
    }
}

/// A `let` binding of one source and where it can be observed.
struct Binding {
    id: SymbolId,
    line: u32,
    /// Observations are valid after the initializer has been evaluated
    init_end: Position,
    /// The extent of the enclosing function, if any
    scope: Option<(Position, Position)>,
    ty: Type,
}

/// Everything known about one source: its `let` bindings by name and their inferred types.
struct SourceModel {
    bindings: BTreeMap<String, Vec<Binding>>,
}

fn position_of(range: &mq_lang::Range) -> (Position, Position) {
    (range.start, range.end)
}

impl SourceModel {
    fn build(name: &str, code: &str) -> Self {
        let is_builtin = name == "builtin";
        let mut hir = Hir::default();
        // builtin.mq is checked like user code, on top of the builtin signatures.
        hir.builtin.disabled = false;
        for global in mq_check::TEST_RUNNER_GLOBALS {
            hir.declare_global(global);
        }
        let url = Url::parse(&format!("file:///{name}.mq")).unwrap();
        let (source_id, _) = hir.add_code(Some(url), code);
        let mut checker = TypeChecker::new();
        let _ = checker.check(&hir);

        // The extent of every function, from the ranges of its descendants.
        let mut extents: BTreeMap<SymbolId, (Position, Position)> = BTreeMap::new();
        let mut subtree_end: BTreeMap<SymbolId, Position> = BTreeMap::new();
        for (id, symbol) in hir.symbols() {
            if symbol.source.source_id != Some(source_id) {
                continue;
            }
            let Some(range) = symbol.source.text_range.as_ref() else {
                continue;
            };
            let (start, end) = position_of(range);
            for (ancestor, ancestor_symbol) in ancestors(&hir, id) {
                let key = ancestor;
                if matches!(ancestor_symbol.kind, SymbolKind::Function(_)) {
                    let extent = extents.entry(key).or_insert((start, end));
                    extent.0 = extent.0.min(start);
                    extent.1 = extent.1.max(end);
                }
                let latest = subtree_end.entry(key).or_insert(end);
                *latest = (*latest).max(end);
            }
        }

        let mut bindings: BTreeMap<String, Vec<Binding>> = BTreeMap::new();
        for (id, symbol) in hir.symbols() {
            if symbol.source.source_id != Some(source_id)
                || symbol.kind != SymbolKind::Variable
                || hir.is_builtin_symbol(symbol) && !is_builtin
            {
                continue;
            }
            // A `var` changes type as it is assigned; its references are typed per point, so the
            // type of the declaration cannot be compared with every value it holds.
            if is_mutable(&hir, id) {
                continue;
            }
            let (Some(name), Some(range), Some(scheme)) = (
                symbol.value.as_ref(),
                symbol.source.text_range.as_ref(),
                checker.type_of(id),
            ) else {
                continue;
            };
            let init_end = subtree_end.get(&id).copied().unwrap_or(range.end);
            let scope = ancestors(&hir, id)
                .find(|(_, ancestor)| matches!(ancestor.kind, SymbolKind::Function(_)))
                .and_then(|(function, _)| extents.get(&function).copied());
            bindings.entry(name.to_string()).or_default().push(Binding {
                id,
                line: range.start.line,
                init_end,
                scope,
                ty: scheme.ty.clone(),
            });
        }
        Self { bindings }
    }

    /// The binding named `name` that is in scope at `at` and declared latest before it.
    fn binding_at(&self, name: &str, at: Position) -> Option<&Binding> {
        self.bindings
            .get(name)?
            .iter()
            .filter(|binding| binding.init_end < at)
            .filter(|binding| binding.scope.is_none_or(|(start, end)| start <= at && at <= end))
            .max_by_key(|binding| binding.init_end)
    }
}

/// Whether the binding was declared with `var`.
fn is_mutable(hir: &Hir, id: SymbolId) -> bool {
    let Some(symbol) = hir.symbol(id) else {
        return false;
    };
    hir.symbols().any(|(_, sibling)| {
        sibling.parent == symbol.parent
            && sibling.kind == SymbolKind::Keyword
            && sibling.value.as_deref() == Some("var")
            && sibling
                .source
                .text_range
                .zip(symbol.source.text_range)
                .is_some_and(|(keyword, name)| {
                    keyword.start.line == name.start.line && keyword.start.column < name.start.column
                })
    })
}

fn ancestors(hir: &Hir, id: SymbolId) -> impl Iterator<Item = (SymbolId, &mq_hir::Symbol)> {
    let mut current = hir.symbol(id).and_then(|symbol| symbol.parent);
    std::iter::from_fn(move || {
        let ancestor = current?;
        let symbol = hir.symbol(ancestor)?;
        current = symbol.parent;
        Some((ancestor, symbol))
    })
}

#[derive(Default)]
struct Observations {
    models: BTreeMap<String, Arc<SourceModel>>,
    /// (source, binding line, name) -> inferred type
    inferred: BTreeMap<(String, u32, String), String>,
    /// (source, binding line, name) -> distinct shapes that did not fit
    mismatches: BTreeMap<(String, u32, String), BTreeSet<Shape>>,
    /// Bindings seen as `none` where the type excludes it. A `let` in a branch that did not run
    /// is `none` too, so these are only counted, not reported one by one.
    none_only: BTreeSet<(String, u32, String)>,
    /// Checked (symbol key, shape) pairs, to check each once
    seen: BTreeSet<(String, SymbolId, Shape)>,
    checks: usize,
    steps: usize,
}

#[derive(Debug)]
struct Handler {
    /// The test files, to name the main query of each by its content
    files: Vec<(String, String)>,
    state: Mutex<Observations>,
}

impl std::fmt::Debug for Observations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Observations").finish_non_exhaustive()
    }
}

impl Handler {
    fn source_name(&self, context: &DebugContext) -> String {
        match &context.source.name {
            Some(name) => name.strip_suffix(".mq").unwrap_or(name).to_string(),
            None => self
                .files
                .iter()
                .find(|(_, content)| context.source.code.starts_with(content.as_str()))
                .map_or_else(|| "<main>".to_string(), |(name, _)| name.clone()),
        }
    }
}

impl DebuggerHandler for Handler {
    fn on_step(&self, context: &DebugContext) -> DebuggerAction {
        let name = self.source_name(context);
        let at = context.token.range.start;
        let mut state = self.state.lock().unwrap();
        state.steps += 1;
        let model = match state.models.get(&name) {
            Some(model) => Arc::clone(model),
            None => {
                let model = Arc::new(SourceModel::build(&name, &context.source.code));
                state.models.insert(name.clone(), Arc::clone(&model));
                model
            }
        };

        for (binding_name, value) in context.vm_bindings() {
            let Some(binding) = model.binding_at(&binding_name.to_string(), at) else {
                continue;
            };
            let shape = shape_of(&value);
            let key = (name.clone(), binding.id, shape.clone());
            if !state.seen.insert(key) {
                continue;
            }
            state.checks += 1;
            let report_key = (name.clone(), binding.line, binding_name.to_string());
            state
                .inferred
                .entry(report_key.clone())
                .or_insert_with(|| binding.ty.display_renumbered());
            if !fits(&shape, &binding.ty) {
                if shape == Shape::None {
                    state.none_only.insert(report_key);
                } else {
                    state.mismatches.entry(report_key).or_default().insert(shape);
                }
            }
        }
        DebuggerAction::StepInto
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn test_files(root: &Path) -> Vec<PathBuf> {
    let mut files = vec![root.join("crates/mq-lang/builtin_tests.mq")];
    let modules = root.join("crates/mq-lang/modules");
    let mut module_tests: Vec<PathBuf> = std::fs::read_dir(modules)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| {
            path.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("_test.mq"))
        })
        .collect();
    module_tests.sort();
    files.extend(module_tests);
    files
}

fn render_shape(shape: &Shape) -> String {
    match shape {
        Shape::Array(items) if items.is_empty() => "[]".to_string(),
        Shape::Array(items) => format!("[{}]", items.iter().map(render_shape).collect::<Vec<_>>().join(" | ")),
        Shape::Dict(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, shape)| format!("{name}: {}", render_shape(shape)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Shape::Node(kind) => kind.name().to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

#[test]
#[ignore = "steps every statement of the .mq test suite; run with `just test-conformance`"]
fn runtime_values_fit_the_inferred_types() {
    let root = workspace_root();
    let files = test_files(&root);
    let named: Vec<(String, String)> = files
        .iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            (name, std::fs::read_to_string(path).unwrap())
        })
        .collect();
    let handler = Arc::new(Handler {
        files: named,
        state: Mutex::new(Observations::default()),
    });

    let runner = mq_test::TestRunner::new(files).with_step_handler(handler.clone());
    let _ = runner.run();

    let state = handler.state.lock().unwrap();
    let mut report = String::new();
    for ((source, line, name), shapes) in &state.mismatches {
        let inferred = &state.inferred[&(source.clone(), *line, name.clone())];
        let observed = shapes.iter().map(render_shape).collect::<Vec<_>>().join(" ; ");
        writeln!(
            report,
            "{source}:{line} {name}: inferred {inferred}, observed {observed}"
        )
        .unwrap();
    }
    let none_only = state
        .none_only
        .iter()
        .filter(|key| !state.mismatches.contains_key(*key))
        .count();
    writeln!(
        report,
        "TOTAL: sources={} bindings_observed={} mismatching={} seen_as_none_only={none_only}",
        state.models.len(),
        state.inferred.len(),
        state.mismatches.len()
    )
    .unwrap();
    eprintln!("steps={} checks={}", state.steps, state.checks);

    let snap = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/runtime_conformance.snap");
    if std::env::var_os("UPDATE_CORPUS").is_some() {
        std::fs::write(&snap, &report).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&snap).unwrap_or_default();
    assert_eq!(
        report, expected,
        "runtime conformance changed; regenerate with UPDATE_CORPUS=1"
    );
}
