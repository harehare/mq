/// The `complex_interpolation` rule.
pub mod complex_interpolation;
/// The `deeply_nested` rule.
pub mod deeply_nested;
/// The `function_too_long` rule.
pub mod function_too_long;
/// The `too_many_match_arms` rule.
pub mod too_many_match_arms;
/// The `too_many_params` rule.
pub mod too_many_params;

use crate::LintRule;

/// Returns the rules of this category.
pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(function_too_long::FunctionTooLong),
        Box::new(too_many_params::TooManyParams),
        Box::new(deeply_nested::DeeplyNested),
        Box::new(too_many_match_arms::TooManyMatchArms),
        Box::new(complex_interpolation::ComplexInterpolation),
    ]
}
