pub mod dangerous_capability_call;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![Box::new(dangerous_capability_call::DangerousCapabilityCall)]
}
