/// The `inefficient_selector` rule.
pub mod inefficient_selector;
/// The `missing_depth_guard` rule.
pub mod missing_depth_guard;
/// The `selector_always_empty` rule.
pub mod selector_always_empty;

use crate::LintRule;

/// Returns the rules of this category.
pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(inefficient_selector::InefficientSelector),
        Box::new(selector_always_empty::SelectorAlwaysEmpty),
        Box::new(missing_depth_guard::MissingDepthGuard),
    ]
}
