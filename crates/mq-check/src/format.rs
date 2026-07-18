//! Machine-readable diagnostic output formats for the `mq-check` CLI.

mod json;
mod sarif;

use std::io::{self, Write};

use mq_check::TypeError;
use mq_hir::{Hir, HirError, HirWarning};

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
    /// SARIF 2.1.0 JSON, for GitHub code scanning and other SARIF consumers
    Sarif,
}

/// Returns the syntax errors and warnings on `hir` as [`CheckDiagnostic`]s.
pub(crate) fn syntax_diagnostics(hir: &Hir) -> Vec<CheckDiagnostic> {
    let mut diagnostics: Vec<CheckDiagnostic> = hir
        .errors()
        .iter()
        .map(|error| CheckDiagnostic {
            severity: Severity::Error,
            code: hir_error_code(error),
            message: error.to_string(),
            range: Some(hir_error_range(error)),
        })
        .collect();

    diagnostics.extend(hir.warnings().iter().map(|warning| CheckDiagnostic {
        severity: Severity::Warning,
        code: hir_warning_code(warning),
        message: warning.to_string(),
        range: Some(hir_warning_range(warning)),
    }));

    diagnostics
}

/// Returns a stable rule code for a [`HirError`] variant.
fn hir_error_code(error: &HirError) -> &'static str {
    match error {
        HirError::UnresolvedSymbol { .. } => "hir::unresolved_symbol",
        HirError::ModuleNotFound { .. } => "hir::module_not_found",
    }
}

fn hir_error_range(error: &HirError) -> mq_lang::Range {
    match error {
        HirError::UnresolvedSymbol { symbol, .. } => symbol.source.text_range.unwrap_or_default(),
        HirError::ModuleNotFound { symbol, .. } => symbol.source.text_range.unwrap_or_default(),
    }
}

/// Returns a stable rule code for a [`HirWarning`] variant.
fn hir_warning_code(warning: &HirWarning) -> &'static str {
    match warning {
        HirWarning::UnreachableCode { .. } => "hir::unreachable_code",
    }
}

fn hir_warning_range(warning: &HirWarning) -> mq_lang::Range {
    match warning {
        HirWarning::UnreachableCode { symbol } => symbol.source.text_range.unwrap_or_default(),
    }
}

/// Converts type-check errors into [`CheckDiagnostic`]s.
pub(crate) fn type_diagnostics(errors: &[TypeError]) -> Vec<CheckDiagnostic> {
    errors
        .iter()
        .map(|error| CheckDiagnostic {
            severity: Severity::Error,
            code: type_error_code(error),
            message: error.to_string(),
            range: error.location(),
        })
        .collect()
}

/// Returns the `miette` diagnostic code for a [`TypeError`] variant.
fn type_error_code(error: &TypeError) -> &'static str {
    match error {
        TypeError::Mismatch { .. } => "typechecker::type_mismatch",
        TypeError::UnificationError { .. } => "typechecker::unification_error",
        TypeError::OccursCheck { .. } => "typechecker::occurs_check",
        TypeError::UndefinedSymbol { .. } => "typechecker::undefined_symbol",
        TypeError::WrongArity { .. } => "typechecker::wrong_arity",
        TypeError::UndefinedField { .. } => "typechecker::undefined_field",
        TypeError::HeterogeneousArray { .. } => "typechecker::heterogeneous_array",
        TypeError::TypeVarNotFound(_) => "typechecker::type_var_not_found",
        TypeError::Internal(_) => "typechecker::internal_error",
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
        OutputFormat::Sarif => sarif::write_sarif_report(w, results),
    }
}
