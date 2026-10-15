/// The `dangerous_capability_call` rule.
pub mod dangerous_capability_call;

use crate::LintRule;

/// Returns the rules of this category.
pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![Box::new(dangerous_capability_call::DangerousCapabilityCall)]
}
