//! How sure the checker is that a diagnostic is a real mistake.
//!
//! The checker infers types flow-insensitively, so a diagnostic that rests on one of those
//! approximations may be wrong for a program that runs fine. A diagnostic is
//! [`Certainty::Definite`] only when every operand it is about is *exact*: a value whose type is
//! known for certain, from the rules in `Prover`. Everything else is [`Certainty::Uncertain`], so
//! a case the rules do not cover can only make a diagnostic less certain, never more.

use mq_hir::{Hir, SymbolId};
use rustc_hash::FxHashMap;

use crate::infer::InferenceContext;
use crate::related::{Related, variable_arguments};
use crate::{TypeError, symbol_range};

mod prover;

use prover::{Prover, in_try_body};

/// How sure the checker is that a [`TypeError`] is a real mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Certainty {
    /// The operands have types that are known for certain, so the program fails at runtime if it
    /// gets there.
    Definite,
    /// The diagnostic relies on an approximation, or the program may handle the failure.
    Uncertain,
}

/// How a diagnostic is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Severity {
    /// Fails the check.
    Error,
    /// Shown, but does not fail the check.
    Warning,
}

/// A [`TypeError`] together with how sure the checker is about it.
#[derive(Debug, Clone)]
pub struct Classified {
    /// The diagnostic.
    pub error: TypeError,
    /// Other places that explain the diagnostic, e.g. where an argument got its type.
    pub related: Vec<Related>,
    /// How sure the checker is about it.
    pub certainty: Certainty,
}

impl Classified {
    /// How the diagnostic is shown under `options`, or `None` when it is left out.
    ///
    /// A definite error is an error. An uncertain one is left out unless
    /// [`show_uncertain`](crate::TypeCheckerOptions::show_uncertain) asks for it as a warning, or
    /// [`strict`](crate::TypeCheckerOptions::strict) makes it an error. Advisory diagnostics
    /// ([`TypeError::is_advisory`]) are warnings.
    pub fn severity(&self, options: &crate::TypeCheckerOptions) -> Option<Severity> {
        if self.error.is_advisory() {
            return Some(Severity::Warning);
        }
        match self.certainty {
            Certainty::Definite => Some(Severity::Error),
            Certainty::Uncertain if options.strict => Some(Severity::Error),
            Certainty::Uncertain if options.show_uncertain => Some(Severity::Warning),
            Certainty::Uncertain => None,
        }
    }
}

/// Builtins that `just audit-builtins` does not cover, sorted by name. A call to any other builtin
/// has a known type when its arguments do. The audit fails when this list is out of date, so a new
/// builtin has to be called by the audit, or listed here until it is.
pub const UNAUDITED_BUILTINS: &[&str] = &[];

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
pub(crate) fn classify_all(
    hir: &Hir,
    ctx: &InferenceContext,
    errors: Vec<TypeError>,
    declared_input: bool,
) -> Vec<Classified> {
    let mut classifier = Classifier {
        hir,
        ctx,
        declared_input,
        symbols_at: None,
        prover: None,
    };
    errors
        .into_iter()
        .map(|error| {
            let certainty = classifier.classify(&error);
            let related = classifier.related(&error);
            Classified {
                certainty,
                related,
                error,
            }
        })
        .collect()
}

struct Classifier<'a> {
    hir: &'a Hir,
    ctx: &'a InferenceContext,
    declared_input: bool,
    /// The symbols at each range, built when the first error needs it
    symbols_at: Option<FxHashMap<mq_lang::Range, Vec<SymbolId>>>,
    /// Built when the first error needs it
    prover: Option<Prover<'a>>,
}

impl Classifier<'_> {
    /// The symbols whose range is `range`.
    fn symbols_at(&mut self, range: mq_lang::Range) -> &[SymbolId] {
        let hir = self.hir;
        self.symbols_at
            .get_or_insert_with(|| {
                let mut index: FxHashMap<mq_lang::Range, Vec<SymbolId>> = FxHashMap::default();
                for (id, symbol) in hir.symbols() {
                    if let Some(range) = symbol_range(hir, symbol) {
                        index.entry(range).or_default().push(id);
                    }
                }
                index
            })
            .get(&range)
            .map_or(&[][..], Vec::as_slice)
    }

    /// Where the variables passed to the call of a type error got their type.
    fn related(&mut self, error: &TypeError) -> Vec<Related> {
        if !matches!(error, TypeError::Mismatch { .. } | TypeError::UnificationError { .. }) {
            return Vec::new();
        }
        let Some(range) = error.location() else {
            return Vec::new();
        };
        let (hir, ctx) = (self.hir, self.ctx);
        variable_arguments(hir, ctx, self.symbols_at(range))
    }

    fn classify(&mut self, error: &TypeError) -> Certainty {
        if error.is_advisory() {
            return Certainty::Uncertain;
        }
        let about_operands = depends_on_operand_types(error);
        let Some(range) = error.location() else {
            return if about_operands {
                Certainty::Uncertain
            } else {
                Certainty::Definite
            };
        };
        let (hir, ctx, declared_input) = (self.hir, self.ctx, self.declared_input);
        let at_range = self.symbols_at(range).to_vec();
        // Whatever the reason, a `try:` body is allowed to fail.
        if at_range.iter().any(|&id| in_try_body(hir, id)) {
            return Certainty::Uncertain;
        }
        if !about_operands {
            return Certainty::Definite;
        }
        let operand_mismatch = matches!(error, TypeError::Mismatch { .. });
        let no_overload = match error {
            TypeError::UnificationError { left, right, .. } if right == "no matching overload" => {
                left.split(" with arguments").next()
            }
            _ => None,
        };
        let inlined = ctx.inlined_error(error);
        self.prover
            .get_or_insert_with(|| Prover::new(hir, ctx, declared_input))
            .classify_at(hir, &at_range, operand_mismatch, no_overload, inlined)
    }
}
