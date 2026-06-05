pub mod deprecated_selector;
pub mod env_var_in_selector;
pub mod inefficient_selector;
pub mod missing_depth_guard;
pub mod selector_always_empty;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(inefficient_selector::InefficientSelector),
        Box::new(selector_always_empty::SelectorAlwaysEmpty),
        Box::new(env_var_in_selector::EnvVarInSelector),
        Box::new(missing_depth_guard::MissingDepthGuard),
        Box::new(deprecated_selector::DeprecatedSelector),
    ]
}
