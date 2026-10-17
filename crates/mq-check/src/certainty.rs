//! How sure the checker is that a diagnostic is a real mistake.
//!
//! The checker infers types flow-insensitively, so a diagnostic that rests on one of those
//! approximations may be wrong for a program that runs fine. A diagnostic is
//! [`Certainty::Definite`] only when every operand it is about is *exact*: a value whose type is
//! known for certain, from the rules in [`Prover`]. Everything else is [`Certainty::Possible`], so
//! a case the rules do not cover can only make a diagnostic less certain, never more.

use mq_hir::{Hir, SymbolId, SymbolKind};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::infer::InferenceContext;
use crate::{TypeError, symbol_range};

/// How sure the checker is that a [`TypeError`] is a real mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Certainty {
    /// The operands have types that are known for certain, so the program fails at runtime if it
    /// gets there.
    Definite,
    /// The diagnostic relies on an approximation, or the program may handle the failure.
    Possible,
}

/// A [`TypeError`] together with how sure the checker is about it.
#[derive(Debug, Clone)]
pub struct Classified {
    /// The diagnostic.
    pub error: TypeError,
    /// How sure the checker is about it.
    pub certainty: Certainty,
}

/// Builtins whose signature the audit (`just audit-builtins`) found no narrower than the function,
/// sorted by name. A call to one has a type that is known for certain when its arguments are. The
/// audit fails when a listed builtin has a finding, and lists the builtins it would accept.
#[rustfmt::skip]
pub const EXACT_BUILTINS: &[&str] = &[
    "abs",
    "acos",
    "all",
    "and",
    "any",
    "arrays",
    "ascii_downcase",
    "ascii_upcase",
    "asin",
    "atan",
    "atan2",
    "band",
    "base64",
    "base64url",
    "basename",
    "between",
    "bnot",
    "booleans",
    "bor",
    "bytes",
    "casefold",
    "cbrt",
    "ceil",
    "center",
    "chars",
    "chomp",
    "chunk_by",
    "chunks",
    "clamp",
    "collect",
    "common_indent",
    "compact_map",
    "complement",
    "contains",
    "cos",
    "cosh",
    "count_by",
    "count_substr",
    "css",
    "css_attr",
    "css_text",
    "cumavg",
    "cumsum",
    "decrease_header_depth",
    "dedent",
    "del",
    "del_path",
    "del_paths",
    "demote_heading",
    "describe",
    "dict",
    "dicts",
    "difference",
    "dirname",
    "downcase",
    "each",
    "ends_with",
    "entries",
    "eq",
    "exp",
    "explode",
    "extname",
    "extract_urls",
    "fill",
    "find",
    "find_index",
    "first",
    "flatten",
    "floor",
    "frequencies_by",
    "from_entries",
    "from_hex",
    "from_html",
    "frontmatter",
    "gcd",
    "geomean",
    "get",
    "get_location",
    "get_or",
    "get_path",
    "get_title",
    "get_url",
    "glob_match",
    "gmtime",
    "group_by",
    "gsub",
    "hexdump",
    "histogram",
    "html_escape",
    "html_unescape",
    "human_bytes",
    "human_size",
    "hypot",
    "identity",
    "increase_header_depth",
    "indent",
    "index_by",
    "insert",
    "interleave",
    "interpose",
    "intersection",
    "invert",
    "is_alnum",
    "is_alpha",
    "is_array",
    "is_ascii",
    "is_blank",
    "is_bool",
    "is_bytes",
    "is_callout",
    "is_code",
    "is_coroutine",
    "is_dict",
    "is_digit",
    "is_em",
    "is_empty",
    "is_h",
    "is_h1",
    "is_h2",
    "is_h3",
    "is_h4",
    "is_h5",
    "is_h6",
    "is_h_level",
    "is_html",
    "is_integer",
    "is_list",
    "is_lower",
    "is_markdown",
    "is_mdx",
    "is_mdx_flow_expression",
    "is_mdx_js_esm",
    "is_mdx_jsx_flow_element",
    "is_mdx_jsx_text_element",
    "is_mdx_text_expression",
    "is_nan",
    "is_none",
    "is_number",
    "is_regex_match",
    "is_space",
    "is_string",
    "is_table_align",
    "is_table_cell",
    "is_text",
    "is_toml",
    "is_upper",
    "is_yaml",
    "iterables",
    "join",
    "keys",
    "last",
    "lcm",
    "leaf_entries",
    "len",
    "ln",
    "localtime",
    "log",
    "log10",
    "log2",
    "lpad",
    "ltrim",
    "ltrimstr",
    "map",
    "map_values",
    "map_while",
    "markdown_escape",
    "markdowns",
    "max",
    "max_by",
    "md5",
    "mean",
    "median",
    "merge_defaults",
    "min",
    "min_by",
    "mode",
    "ne",
    "negate",
    "ngram",
    "nones",
    "numbers",
    "omit",
    "or",
    "partition",
    "path_join",
    "paths",
    "pick",
    "pluck",
    "pow",
    "product",
    "promote_heading",
    "rand",
    "rand_int",
    "random_string",
    "range",
    "redact",
    "regex_escape",
    "reject",
    "remove_chars",
    "repeat",
    "replace",
    "replace_prose",
    "reverse",
    "rindex",
    "round",
    "rpad",
    "rsplit_once",
    "rtrim",
    "rtrimstr",
    "sample",
    "sanitize_html",
    "scalars",
    "second",
    "select",
    "set",
    "set_path",
    "sha256",
    "sha512",
    "shell_quote",
    "shuffle",
    "sin",
    "sinh",
    "skip",
    "skip_while",
    "slice",
    "sort",
    "split",
    "split_once",
    "split_records",
    "sqrt",
    "squeeze_whitespace",
    "starts_with",
    "stddev",
    "stem",
    "stream_range",
    "strftime",
    "strings",
    "strip_ansi",
    "strip_tags",
    "sum",
    "sum_by",
    "take",
    "take_while",
    "tally",
    "tan",
    "tanh",
    "tap",
    "to_array",
    "to_blockquote",
    "to_boolean",
    "to_bytes",
    "to_callout",
    "to_code",
    "to_code_inline",
    "to_coroutine",
    "to_date",
    "to_date_iso8601",
    "to_definition",
    "to_delete",
    "to_em",
    "to_footnote",
    "to_footnote_ref",
    "to_h",
    "to_hex",
    "to_image",
    "to_link",
    "to_markdown",
    "to_markdown_string",
    "to_math",
    "to_math_inline",
    "to_md_fragment",
    "to_md_html",
    "to_md_list",
    "to_md_name",
    "to_md_table_align",
    "to_md_table_cell",
    "to_md_table_row",
    "to_md_text",
    "to_mdx",
    "to_number",
    "to_number_or",
    "to_string",
    "to_strong",
    "to_text",
    "trim",
    "trunc",
    "truncate",
    "type",
    "unicode_normalize",
    "union",
    "uniq",
    "unique_by",
    "unzip",
    "upcase",
    "url_decode",
    "url_encode",
    "utf8",
    "values",
    "variance",
    "walk",
    "with_entries",
    "word_wrap",
    "words",
    "xor",
    "zfill",
    "zip",
];

/// Errors whose reason is the types of their operands.
fn depends_on_operand_types(error: &TypeError) -> bool {
    matches!(
        error,
        TypeError::Mismatch { .. }
            | TypeError::UnificationError { .. }
            | TypeError::UndefinedField { .. }
            | TypeError::UndefinedAttribute { .. }
    )
}

/// Classifies every error against the program it was found in.
pub(crate) fn classify_all(hir: &Hir, ctx: &InferenceContext, errors: Vec<TypeError>) -> Vec<Classified> {
    let mut prover: Option<Prover> = None;
    errors
        .into_iter()
        .map(|error| {
            let certainty = match &error {
                TypeError::NullablePropagation { .. }
                | TypeError::UnreachableCode { .. }
                | TypeError::NonExhaustiveMatch { .. }
                | TypeError::HeterogeneousArray { .. }
                | TypeError::OccursCheck { .. } => Certainty::Possible,
                _ if !depends_on_operand_types(&error) => {
                    // Whatever the reason, a `try:` body is allowed to fail.
                    if error.location().is_some_and(|range| in_try_body_at(hir, range)) {
                        Certainty::Possible
                    } else {
                        Certainty::Definite
                    }
                }
                _ => match error.location() {
                    Some(range) => prover
                        .get_or_insert_with(|| Prover::new(hir))
                        .classify_at(hir, ctx, range),
                    None => Certainty::Possible,
                },
            };
            Classified { error, certainty }
        })
        .collect()
}

/// Decides which expressions have a type that is known for certain.
///
/// An expression is exact when it is
///
/// - a literal, or an interpolated string
/// - an array or dict literal of exact elements
/// - an operator on exact operands
/// - a call of an audited builtin with exact arguments that is not fed an implicit input
/// - an `if` whose branches are exact, or a block whose last expression is exact
/// - a reference to a `let` or `var` that is never assigned again, whose value is exact, and
///   that no condition narrows around the reference
///
/// Parameters, calls of user functions, loops, `match`, `try` and anything else are not.
struct Prover {
    children: FxHashMap<SymbolId, Vec<SymbolId>>,
    /// Variables that are the target of an assignment
    assigned: FxHashSet<SymbolId>,
    /// The stage of a pipe that feeds each stage, when there is one. Definitions and assignments
    /// pass their input on, so they are skipped.
    previous: FxHashMap<SymbolId, SymbolId>,
    memo: FxHashMap<SymbolId, bool>,
    /// Whether the parameters of the function being proven are taken to be exact, which holds when
    /// it is called with exact arguments
    params_exact: bool,
    /// Whether the body of a function is exact when its parameters are
    bodies: FxHashMap<SymbolId, bool>,
}

impl Prover {
    fn new(hir: &Hir) -> Self {
        let mut children: FxHashMap<SymbolId, Vec<SymbolId>> = FxHashMap::default();
        for (id, symbol) in hir.symbols() {
            if let Some(parent) = symbol.parent {
                children.entry(parent).or_default().push(id);
            }
        }
        for list in children.values_mut() {
            list.sort_by_key(|&id| hir.symbol_insertion_order(id));
        }

        let mut assigned = FxHashSet::default();
        for (id, symbol) in hir.symbols() {
            if symbol.kind != SymbolKind::Assign {
                continue;
            }
            let target = children.get(&id).and_then(|c| c.first()).copied();
            if let Some(def) = target.and_then(|t| hir.resolve_reference_symbol(t)) {
                assigned.insert(def);
            }
        }

        let mut previous = FxHashMap::default();
        let mut link = |stages: &[SymbolId]| {
            let mut last: Option<SymbolId> = None;
            for &stage in stages {
                let Some(symbol) = hir.symbol(stage) else {
                    continue;
                };
                if !is_stage_kind(&symbol.kind) {
                    continue;
                }
                if let Some(before) = last {
                    previous.insert(stage, before);
                }
                if !passes_input_on(&symbol.kind) {
                    last = Some(stage);
                }
            }
        };
        for (parent, list) in &children {
            if hir
                .symbol(*parent)
                .is_some_and(|s| matches!(s.kind, SymbolKind::Block | SymbolKind::Function(_)))
            {
                link(list);
            }
        }
        // The top-level stages of each source
        let mut roots: FxHashMap<Option<mq_hir::SourceId>, Vec<SymbolId>> = FxHashMap::default();
        for (id, symbol) in hir.symbols() {
            if symbol.parent.is_none() && !hir.is_builtin_symbol(symbol) {
                roots.entry(symbol.source.source_id).or_default().push(id);
            }
        }
        for list in roots.values_mut() {
            list.sort_by_key(|&id| hir.symbol_insertion_order(id));
            link(list);
        }

        Self {
            children,
            assigned,
            previous,
            memo: FxHashMap::default(),
            params_exact: false,
            bodies: FxHashMap::default(),
        }
    }

    fn classify_at(&mut self, hir: &Hir, ctx: &InferenceContext, range: mq_lang::Range) -> Certainty {
        let at_range: Vec<SymbolId> = hir
            .symbols()
            .filter(|(_, symbol)| symbol_range(hir, symbol) == Some(range))
            .map(|(id, _)| id)
            .collect();
        if at_range.is_empty() {
            return Certainty::Possible;
        }

        for &root in &at_range {
            if inside_try_body(hir, root) || self.has_unsettled_operand(hir, ctx, root) {
                return Certainty::Possible;
            }
            // The expression at the location and, when it is an operand, the operation around it
            let mut subjects = vec![root];
            if let Some(parent) = hir.symbol(root).and_then(|s| s.parent)
                && hir.symbol(parent).is_some_and(|s| is_operation(&s.kind))
            {
                subjects.push(parent);
            }
            for subject in subjects {
                if !self.operands_are_exact(hir, subject) {
                    return Certainty::Possible;
                }
            }
        }
        Certainty::Definite
    }

    /// Whether the expression is exact, and for an operation, so are its operands.
    fn operands_are_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(symbol) = hir.symbol(id) else {
            return false;
        };
        if is_operation(&symbol.kind) {
            // A call that takes the piped value as an argument has an operand that is not a child.
            if symbol.kind == SymbolKind::Call
                && !((is_audited_builtin(hir, id, symbol) || self.is_exact_user_call(hir, id))
                    && self.input_is_exact(hir, id))
            {
                return false;
            }
            let operands = self.children.get(&id).cloned().unwrap_or_default();
            operands
                .into_iter()
                .filter(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .all(|operand| self.is_exact(hir, operand))
        } else {
            self.is_exact(hir, id)
        }
    }

    fn is_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        if let Some(&known) = self.memo.get(&id) {
            return known;
        }
        // A cycle is not exact
        self.memo.insert(id, false);
        let exact = self.prove(hir, id);
        self.memo.insert(id, exact);
        exact
    }

    fn prove(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(symbol) = hir.symbol(id) else {
            return false;
        };
        let children: Vec<SymbolId> = self.children.get(&id).cloned().unwrap_or_default();
        match &symbol.kind {
            SymbolKind::Number
            | SymbolKind::String
            | SymbolKind::Boolean
            | SymbolKind::None
            | SymbolKind::Symbol
            | SymbolKind::Bytes
            | SymbolKind::InterpolatedString => true,
            SymbolKind::Array | SymbolKind::Dict => children
                .iter()
                .filter(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .all(|&c| self.is_exact(hir, c)),
            SymbolKind::UnaryOp | SymbolKind::BinaryOp => {
                !children.is_empty() && children.iter().all(|&c| self.is_exact(hir, c))
            }
            SymbolKind::Call => self.call_is_exact(hir, id, symbol, &children),
            SymbolKind::If => {
                let branches: Vec<SymbolId> = children.iter().skip(1).copied().collect();
                !branches.is_empty()
                    && branches
                        .iter()
                        .all(|&branch| match hir.symbol(branch).map(|s| &s.kind) {
                            Some(SymbolKind::Elif) | Some(SymbolKind::Else) => self
                                .children
                                .get(&branch)
                                .and_then(|c| c.last())
                                .copied()
                                .is_some_and(|body| self.is_exact(hir, body)),
                            _ => self.is_exact(hir, branch),
                        })
            }
            SymbolKind::Block => children
                .iter()
                .rev()
                .find(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .copied()
                .is_some_and(|last| self.is_exact(hir, last)),
            SymbolKind::Ref => self.ref_is_exact(hir, id),
            _ => false,
        }
    }

    fn call_is_exact(&mut self, hir: &Hir, id: SymbolId, symbol: &mq_hir::Symbol, children: &[SymbolId]) -> bool {
        (is_audited_builtin(hir, id, symbol) || self.is_exact_user_call(hir, id))
            && self.input_is_exact(hir, id)
            && children
                .iter()
                .filter(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .all(|&c| self.is_exact(hir, c))
    }

    /// Whether the call is of a function defined in the program, given every parameter and whose
    /// body is exact when its parameters are, so that exact arguments give a result of known type.
    fn is_exact_user_call(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(def_id) = hir.resolve_reference_symbol(id) else {
            return false;
        };
        let Some(def) = hir.symbol(def_id) else {
            return false;
        };
        let SymbolKind::Function(params) = &def.kind else {
            return false;
        };
        if hir.is_builtin_symbol(def) || params.iter().any(|p| p.is_variadic || p.has_default) {
            return false;
        }
        let given = self.children.get(&id).map_or(0, |c| {
            c.iter()
                .filter(|&&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
                .count()
        });
        given == params.len() && self.body_is_exact(hir, def_id)
    }

    /// Whether every expression in the body of the function is exact if its parameters are.
    fn body_is_exact(&mut self, hir: &Hir, def_id: SymbolId) -> bool {
        if let Some(&known) = self.bodies.get(&def_id) {
            return known;
        }
        // A recursive function is not exact
        self.bodies.insert(def_id, false);
        let outer_memo = std::mem::take(&mut self.memo);
        let outer_params = std::mem::replace(&mut self.params_exact, true);

        let stages: Vec<SymbolId> = self
            .children
            .get(&def_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|&c| hir.symbol(c).is_some_and(|s| is_expression(&s.kind)))
            .collect();
        let exact = !stages.is_empty() && stages.iter().all(|&stage| self.stage_is_exact(hir, stage));

        self.memo = outer_memo;
        self.params_exact = outer_params;
        self.bodies.insert(def_id, exact);
        exact
    }

    /// A stage of a body: a definition is exact when its value is.
    fn stage_is_exact(&mut self, hir: &Hir, stage: SymbolId) -> bool {
        match hir.symbol(stage).map(|s| &s.kind) {
            Some(SymbolKind::Variable | SymbolKind::DestructuringBinding) => self
                .children
                .get(&stage)
                .and_then(|c| c.last())
                .copied()
                .is_some_and(|init| self.is_exact(hir, init)),
            _ => self.is_exact(hir, stage),
        }
    }

    /// Whether what the stage is piped is exact, or it has no stage before it.
    fn input_is_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        match self.previous.get(&id).copied() {
            Some(before) => self.is_exact(hir, before),
            None => true,
        }
    }

    fn ref_is_exact(&mut self, hir: &Hir, id: SymbolId) -> bool {
        let Some(def) = hir.resolve_reference_symbol(id) else {
            return false;
        };
        let Some(def_symbol) = hir.symbol(def) else {
            return false;
        };
        if self.assigned.contains(&def) || self.is_narrowed_around(hir, id, def) {
            return false;
        }
        if def_symbol.kind == SymbolKind::Parameter {
            return self.params_exact;
        }
        if def_symbol.kind != SymbolKind::Variable {
            return false;
        }
        let Some(&init) = self.children.get(&def).and_then(|c| c.last()) else {
            return false;
        };
        self.is_exact(hir, init)
    }

    /// Whether a condition above the reference tests the same variable, which narrows its type.
    fn is_narrowed_around(&self, hir: &Hir, id: SymbolId, def: SymbolId) -> bool {
        let mut current = hir.symbol(id).and_then(|s| s.parent);
        while let Some(ancestor_id) = current {
            let Some(ancestor) = hir.symbol(ancestor_id) else {
                return false;
            };
            if matches!(
                ancestor.kind,
                SymbolKind::If
                    | SymbolKind::Elif
                    | SymbolKind::Unless
                    | SymbolKind::While
                    | SymbolKind::Until
                    | SymbolKind::Match
            ) {
                let tested = self.children.get(&ancestor_id).and_then(|c| c.first()).copied();
                if tested.is_some_and(|t| self.reads(hir, t, def)) {
                    return true;
                }
            }
            current = ancestor.parent;
        }
        false
    }

    fn reads(&self, hir: &Hir, root: SymbolId, def: SymbolId) -> bool {
        let mut stack = vec![root];
        while let Some(id) = stack.pop() {
            if hir.symbol(id).is_some_and(|s| s.kind == SymbolKind::Ref)
                && hir.resolve_reference_symbol(id) == Some(def)
            {
                return true;
            }
            if let Some(children) = self.children.get(&id) {
                stack.extend(children.iter().copied());
            }
        }
        false
    }

    /// An operand whose type the checker could not settle.
    fn has_unsettled_operand(&self, hir: &Hir, ctx: &InferenceContext, root: SymbolId) -> bool {
        let unsettled = |id: SymbolId| {
            ctx.get_symbol_type(id)
                .map(|ty| ctx.resolve_type(ty))
                .is_some_and(|ty| ty.has_pending_var() || ty.is_dynamic())
        };
        // A function passed as an argument has the type variables of its own parameters.
        self.children.get(&root).is_some_and(|c| {
            c.iter()
                .any(|&id| hir.symbol(id).is_some_and(|s| is_expression(&s.kind)) && unsettled(id))
        })
    }
}

/// Whether the call is of a builtin whose signature has been audited.
fn is_audited_builtin(hir: &Hir, id: SymbolId, symbol: &mq_hir::Symbol) -> bool {
    let Some(name) = symbol.value.as_deref() else {
        return false;
    };
    EXACT_BUILTINS.binary_search(&name).is_ok()
        && hir
            .resolve_reference_symbol(id)
            .and_then(|def| hir.symbol(def))
            .is_some_and(|def| hir.is_builtin_symbol(def))
}

/// Whether the symbol applies something to operands.
fn is_operation(kind: &SymbolKind) -> bool {
    matches!(kind, SymbolKind::Call | SymbolKind::BinaryOp | SymbolKind::UnaryOp)
}

/// Whether the symbol is an expression with a value (not a keyword or a parameter).
fn is_expression(kind: &SymbolKind) -> bool {
    !matches!(
        kind,
        SymbolKind::Keyword | SymbolKind::Parameter | SymbolKind::Selector(_) | SymbolKind::Function(_)
    )
}

fn is_stage_kind(kind: &SymbolKind) -> bool {
    !matches!(kind, SymbolKind::Keyword | SymbolKind::Parameter)
}

/// Whether a stage hands its own input to the next stage instead of its value.
fn passes_input_on(kind: &SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::Variable
            | SymbolKind::DestructuringBinding
            | SymbolKind::Function(_)
            | SymbolKind::Assign
            | SymbolKind::Include(_)
            | SymbolKind::Import(_)
            | SymbolKind::Module(_)
    )
}

/// Whether the expression at `range` is inside a `try:` body.
fn in_try_body_at(hir: &Hir, range: mq_lang::Range) -> bool {
    hir.symbols()
        .filter(|(_, symbol)| symbol_range(hir, symbol) == Some(range))
        .any(|(id, _)| inside_try_body(hir, id))
}

/// The failure of a `try:` body is handled by its `catch`.
fn inside_try_body(hir: &Hir, id: SymbolId) -> bool {
    let mut child = id;
    let mut current = hir.symbol(id).and_then(|s| s.parent);
    while let Some(parent_id) = current {
        let Some(parent) = hir.symbol(parent_id) else {
            return false;
        };
        if parent.kind == SymbolKind::Try {
            let is_handler = hir.symbol(child).is_some_and(|s| s.kind == SymbolKind::Catch);
            return !is_handler;
        }
        child = parent_id;
        current = parent.parent;
    }
    false
}
