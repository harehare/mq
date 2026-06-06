pub mod inefficient_selector;
pub mod missing_depth_guard;
pub mod selector_always_empty;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(inefficient_selector::InefficientSelector),
        Box::new(selector_always_empty::SelectorAlwaysEmpty),
        Box::new(missing_depth_guard::MissingDepthGuard),
    ]
}
