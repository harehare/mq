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
pub mod rules;

pub use config::LintConfig;

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
    /// The rule that produced this diagnostic.
    pub rule_id: &'static str,
    pub severity: Severity,
    pub message: String,
    /// Source location of the finding, if available.
    pub range: Option<mq_lang::Range>,
    /// Optional suggestion for how to fix the issue.
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn new(rule_id: &'static str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            rule_id,
            severity,
            message: message.into(),
            range: None,
            help: None,
        }
    }

    pub fn with_range(mut self, range: mq_lang::Range) -> Self {
        self.range = Some(range);
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
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
}

/// A single lint rule.
pub trait LintRule: Send + Sync {
    /// Unique identifier for this rule (e.g. `"unused_variable"`).
    fn id(&self) -> &'static str;

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
