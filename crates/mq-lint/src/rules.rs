//! All lint rules, organized by category.

pub mod complexity;
pub mod correctness;
pub mod module;
pub mod selector;
pub mod style;

use crate::LintRule;

/// Returns all built-in lint rules.
pub fn all_rules() -> Vec<Box<dyn LintRule>> {
    let mut rules: Vec<Box<dyn LintRule>> = Vec::new();
    rules.extend(correctness::all());
    rules.extend(style::all());
    rules.extend(complexity::all());
    rules.extend(selector::all());
    rules.extend(module::all());
    rules
}
