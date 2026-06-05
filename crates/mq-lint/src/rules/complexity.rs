pub mod complex_interpolation;
pub mod deeply_nested;
pub mod function_too_long;
pub mod too_many_match_arms;
pub mod too_many_params;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(function_too_long::FunctionTooLong),
        Box::new(too_many_params::TooManyParams),
        Box::new(deeply_nested::DeeplyNested),
        Box::new(too_many_match_arms::TooManyMatchArms),
        Box::new(complex_interpolation::ComplexInterpolation),
    ]
}
