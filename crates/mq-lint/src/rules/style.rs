pub mod boolean_comparison;
pub mod naming_convention;
pub mod prefer_coalesce;
pub mod prefer_let_over_var;
pub mod prefer_pipe_style;
pub mod prefer_specific_heading;
pub mod redundant_boolean_literal;
pub mod redundant_try;
pub mod unnecessary_interpolation;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(prefer_let_over_var::PreferLetOverVar),
        Box::new(prefer_pipe_style::PreferPipeStyle),
        Box::new(prefer_coalesce::PreferCoalesce),
        Box::new(prefer_specific_heading::PreferSpecificHeading),
        Box::new(redundant_try::RedundantTry),
        Box::new(naming_convention::NamingConvention),
        Box::new(boolean_comparison::BooleanComparison),
        Box::new(redundant_boolean_literal::RedundantBooleanLiteral),
        Box::new(unnecessary_interpolation::UnnecessaryInterpolation),
    ]
}
