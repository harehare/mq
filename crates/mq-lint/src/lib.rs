//! Linter for the mq language.
//!
//! This crate provides static analysis rules for mq programs, organized into
//! categories: correctness, style, complexity, selector, and module.
//!
//! ## Example
//!
//! ```rust
//! use mq_lint::{Linter, LintContext, LintConfig};
//! use mq_hir::Hir;
//!
//! let mut hir = Hir::default();
//! let (source_id, _) = hir.add_code(None, "let x = .h1;");
//!
//! let config = LintConfig::default();
//! let ctx = LintContext::new(&hir, source_id, &config);
//! let linter = Linter::with_default_rules();
//! let diagnostics = linter.run(&ctx);
//! ```

pub mod config;
pub mod fix;
pub mod message;
pub mod rules;

pub use config::LintConfig;
pub use fix::Fix;
pub use message::{LintMessage, RuleId};

use mq_hir::{Hir, SourceId};

/// Severity level for a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Code style suggestion — does not affect correctness.
    Style,
    /// Performance hint.
    Perf,
    /// Likely unintentional or suspicious code.
    Warn,
    /// Definite error that must be fixed.
    Error,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Style => write!(f, "style"),
            Severity::Perf => write!(f, "perf"),
            Severity::Warn => write!(f, "warn"),
            Severity::Error => write!(f, "error"),
        }
    }
}

/// A lint finding produced by a [`LintRule`].
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// The message data that identifies the rule and renders its text.
    pub kind: LintMessage,
    pub severity: Severity,
    /// Source location of the finding, if available.
    pub range: Option<mq_lang::Range>,
    /// A machine-applicable rewrite, if the rule can suggest one.
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(kind: LintMessage, severity: Severity) -> Self {
        Self {
            kind,
            severity,
            range: None,
            fix: None,
        }
    }

    pub fn with_range(mut self, range: mq_lang::Range) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    /// The rule that produced this diagnostic.
    pub fn rule_id(&self) -> RuleId {
        self.kind.rule_id()
    }

    /// Human-readable diagnostic text.
    pub fn message(&self) -> String {
        self.kind.to_string()
    }

    /// Suggested fix text, if the rule has one.
    pub fn help(&self) -> Option<String> {
        self.kind.help()
    }
}

/// Context passed to each [`LintRule`] during analysis.
pub struct LintContext<'a> {
    pub hir: &'a Hir,
    pub source_id: SourceId,
    pub config: &'a LintConfig,
}

impl<'a> LintContext<'a> {
    pub fn new(hir: &'a Hir, source_id: SourceId, config: &'a LintConfig) -> Self {
        Self { hir, source_id, config }
    }

    /// Returns all symbols that belong to this source, including those added
    /// via `insert_symbol` (e.g. Variable, Selector, Ref, Keyword).
    ///
    /// This is broader than `hir.symbols_for_source()`, which only returns
    /// symbols registered with `add_symbol` (structured constructs like
    /// Function, Match, Block, etc.).
    pub fn all_symbols(&self) -> impl Iterator<Item = (mq_hir::SymbolId, &mq_hir::Symbol)> + '_ {
        let source_id = self.source_id;
        self.hir
            .symbols()
            .filter(move |(_, s)| s.source.source_id == Some(source_id))
    }

    /// The full source range spanned by a symbol and all of its descendants.
    ///
    /// A symbol's own `source.text_range` often covers only its own token (e.g. a `BinaryOp`
    /// symbol's range is just the operator), not the whole subtree, so rules building a [`Fix`]
    /// should use this instead.
    pub fn full_range(&self, symbol_id: mq_hir::SymbolId) -> Option<mq_lang::Range> {
        let symbol = self.hir.symbol(symbol_id)?;
        let mut range = symbol.source.text_range;

        for (child_id, _) in self.all_symbols().filter(|(_, s)| s.parent == Some(symbol_id)) {
            if let Some(child_range) = self.full_range(child_id) {
                range = Some(match range {
                    Some(r) => fix::union(r, child_range),
                    None => child_range,
                });
            }
        }

        range
    }
}

/// A single lint rule.
pub trait LintRule: Send + Sync {
    /// Unique identifier for this rule.
    fn id(&self) -> RuleId;

    /// Default severity when the rule fires.
    fn severity(&self) -> Severity;

    /// Analyze the HIR and return any diagnostics.
    fn check(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic>;
}

/// Runs all registered [`LintRule`]s against a [`LintContext`].
#[derive(Default)]
pub struct Linter {
    rules: Vec<Box<dyn LintRule>>,
}

impl Linter {
    /// Create a linter with the full default rule set.
    pub fn with_default_rules() -> Self {
        Self {
            rules: rules::all_rules(),
        }
    }

    pub fn run(&self, ctx: &LintContext<'_>) -> Vec<Diagnostic> {
        self.rules
            .iter()
            .filter(|rule| ctx.config.is_rule_enabled(rule.id()))
            .flat_map(|rule| rule.check(ctx))
            .collect()
    }
}
