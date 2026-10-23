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

/// Builtins that `just audit-builtins` does not cover, sorted by name: they do I/O or never
/// return, take more than three arguments, or never ran with an accepted call. A call to any other
/// builtin has a known type when its arguments do. The audit fails when this list is out of date.
#[rustfmt::skip]
pub const UNAUDITED_BUILTINS: &[&str] = &[
    "array",
    "breakpoint",
    "close",
    "collection",
    "comp",
    "date_add",
    "date_diff",
    "date_relative",
    "debug",
    "decode",
    "embed_images",
    "encode",
    "env",
    "error",
    "file_exists",
    "file_info",
    "file_size",
    "fold_while",
    "halt",
    "halt_error",
    "http",
    "http_all",
    "http_lines",
    "infinite",
    "input",
    "inspect",
    "join_by",
    "juxt",
    "load_markdown",
    "merge_with",
    "mktime",
    "mock_fetch",
    "open_file",
    "open_http",
    "pack",
    "print",
    "read_bytes",
    "read_file",
    "read_file_bytes",
    "read_line",
    "send",
    "stats_by",
    "status",
    "stderr",
    "stream_bytes",
    "stream_chunks",
    "stream_lines",
    "system",
    "token_compress",
    "top_by",
    "unpack",
    "walk_files",
    "write_file",
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
        .map(|error| Classified {
            certainty: classifier.classify(&error),
            error,
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
        let at_range = self
            .symbols_at
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
            .map_or(&[][..], Vec::as_slice);
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
            .classify_at(hir, at_range, operand_mismatch, no_overload, inlined)
    }
}
