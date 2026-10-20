//! Machine-readable diagnostic output formats for the `mq-check` CLI.

mod json;
mod markdown;
mod sarif;

use std::io::{self, Write};

use mq_check::{Classified, TypeCheckerOptions, TypeError};
use mq_hir::{Hir, HirError, HirWarning, SourceId, Symbol};

/// Severity of a check diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Severity {
    Error,
    Warning,
}

impl Severity {
    /// Returns the wire representation shared by the JSON and SARIF output formats.
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// A single syntax or type-check diagnostic, in a form suitable for machine consumption.
#[derive(Clone, Debug)]
pub(crate) struct CheckDiagnostic {
    pub(crate) severity: Severity,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    pub(crate) range: Option<mq_lang::Range>,
}

/// Machine-readable diagnostic output format.
#[derive(Clone, Copy, Debug, Default, PartialEq, clap::ValueEnum)]
pub(crate) enum OutputFormat {
    /// Human-readable colored report (default)
    #[default]
    Text,
    /// A single JSON array of diagnostics across every checked file
    Json,
    /// GitHub-flavored Markdown table, suitable for a PR description or comment
    Markdown,
    /// SARIF 2.1.0 JSON, for GitHub code scanning and other SARIF consumers
    Sarif,
}

/// Returns the parse errors of the source and the syntax errors and warnings that `hir` reports
/// for `source_id` as [`CheckDiagnostic`]s.
///
/// `hir` also holds the modules the source includes or imports. Their diagnostics are
/// positions in other files, so reporting them against `source_id` would be misleading.
/// Circular module dependencies are the exception, since they fail the source anyway.
pub(crate) fn syntax_diagnostics(
    hir: &Hir,
    source_id: SourceId,
    parse_errors: &[(String, mq_lang::Range)],
) -> Vec<CheckDiagnostic> {
    let mut diagnostics: Vec<CheckDiagnostic> = parse_errors
        .iter()
        .map(|(message, range)| CheckDiagnostic {
            severity: Severity::Error,
            code: "mq::syntax",
            message: message.clone(),
            range: Some(*range),
        })
        .collect();

    diagnostics.extend(hir.errors().iter().filter_map(|error| {
        let symbol = hir_error_symbol(error);
        if symbol.source.source_id == Some(source_id) {
            return Some(CheckDiagnostic {
                severity: Severity::Error,
                code: hir_error_code(error),
                message: error.to_string(),
                range: Some(hir_error_range(error)),
            });
        }
        // A cycle closed inside a module still fails the whole source, so it is reported here
        // without a range, which would point into the other file.
        if !matches!(error, HirError::CircularModule { .. }) {
            return None;
        }
        let module = symbol
            .source
            .source_id
            .and_then(|id| hir.url_by_source(&id))
            .map_or("unknown", |url| url.path());
        Some(CheckDiagnostic {
            severity: Severity::Error,
            code: hir_error_code(error),
            message: format!("{error} (in module `{module}`)"),
            range: None,
        })
    }));

    diagnostics.extend(
        hir.warnings()
            .iter()
            .filter(|warning| hir_warning_symbol(warning).source.source_id == Some(source_id))
            .map(|warning| CheckDiagnostic {
                severity: Severity::Warning,
                code: hir_warning_code(warning),
                message: warning.to_string(),
                range: Some(hir_warning_range(warning)),
            }),
    );

    diagnostics
}

/// Returns a stable rule code for a [`HirError`] variant.
fn hir_error_code(error: &HirError) -> &'static str {
    match error {
        HirError::UnresolvedSymbol { .. } => "hir::unresolved_symbol",
        HirError::ModuleNotFound { .. } => "hir::module_not_found",
        HirError::CircularModule { .. } => "hir::circular_module",
        HirError::YieldOutsideFunction { .. } => "hir::yield_outside_function",
    }
}

fn hir_error_symbol(error: &HirError) -> &Symbol {
    match error {
        HirError::UnresolvedSymbol { symbol, .. }
        | HirError::ModuleNotFound { symbol, .. }
        | HirError::CircularModule { symbol, .. }
        | HirError::YieldOutsideFunction { symbol } => symbol,
    }
}

fn hir_error_range(error: &HirError) -> mq_lang::Range {
    hir_error_symbol(error).source.text_range.unwrap_or_default()
}

/// Returns a stable rule code for a [`HirWarning`] variant.
fn hir_warning_code(warning: &HirWarning) -> &'static str {
    match warning {
        HirWarning::UnreachableCode { .. } => "hir::unreachable_code",
    }
}

fn hir_warning_symbol(warning: &HirWarning) -> &Symbol {
    match warning {
        HirWarning::UnreachableCode { symbol } => symbol,
    }
}

fn hir_warning_range(warning: &HirWarning) -> mq_lang::Range {
    hir_warning_symbol(warning).source.text_range.unwrap_or_default()
}

/// Converts type-check errors into [`CheckDiagnostic`]s.
///
/// An uncertain error is left out unless the options show it, as a warning or, when `strict`, as
/// an error.
pub(crate) fn type_diagnostics(errors: &[Classified], options: &TypeCheckerOptions) -> Vec<CheckDiagnostic> {
    errors
        .iter()
        .filter_map(|c| {
            let severity = match c.severity(options)? {
                mq_check::Severity::Error => Severity::Error,
                mq_check::Severity::Warning => Severity::Warning,
            };
            Some(CheckDiagnostic {
                severity,
                code: type_error_code(&c.error),
                message: c.error.to_string(),
                range: c.error.location(),
            })
        })
        .collect()
}

/// Returns the `miette` diagnostic code for a [`TypeError`] variant.
fn type_error_code(error: &TypeError) -> &'static str {
    match error {
        TypeError::Mismatch { .. } => "typechecker::type_mismatch",
        TypeError::UnificationError { .. } => "typechecker::unification_error",
        TypeError::UndefinedSymbol { .. } => "typechecker::undefined_symbol",
        TypeError::WrongArity { .. } => "typechecker::wrong_arity",
        TypeError::UndefinedField { .. } => "typechecker::undefined_field",
        TypeError::UndefinedAttribute { .. } => "typechecker::undefined_attribute",
        TypeError::HeterogeneousArray { .. } => "typechecker::heterogeneous_array",
        TypeError::NullablePropagation { .. } => "typechecker::nullable_propagation",
        TypeError::UnreachableCode { .. } => "typechecker::unreachable_code",
        TypeError::NonExhaustiveMatch { .. } => "typechecker::non_exhaustive_patterns",
    }
}

/// Dispatches to the writer for the requested machine-readable output format.
///
/// Must not be called with [`OutputFormat::Text`], which is rendered separately.
pub(crate) fn write_report(
    w: &mut impl Write,
    format: OutputFormat,
    results: &[(String, Vec<CheckDiagnostic>)],
) -> io::Result<()> {
    match format {
        OutputFormat::Text => unreachable!("text output is handled separately"),
        OutputFormat::Json => json::write_json_report(w, results),
        OutputFormat::Markdown => markdown::write_markdown_report(w, results),
        OutputFormat::Sarif => sarif::write_sarif_report(w, results),
    }
}
