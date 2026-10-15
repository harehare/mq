/// The `boolean_comparison` rule.
pub mod boolean_comparison;
/// The `constant_string_concat` rule.
pub mod constant_string_concat;
/// The `naming_convention` rule.
pub mod naming_convention;
/// The `negated_condition` rule.
pub mod negated_condition;
/// The `prefer_coalesce` rule.
pub mod prefer_coalesce;
/// The `prefer_let_over_var` rule.
pub mod prefer_let_over_var;
/// The `prefer_pipe_style` rule.
pub mod prefer_pipe_style;
/// The `prefer_specific_heading` rule.
pub mod prefer_specific_heading;
/// The `redundant_boolean_literal` rule.
pub mod redundant_boolean_literal;
/// The `redundant_try` rule.
pub mod redundant_try;
/// The `unnecessary_interpolation` rule.
pub mod unnecessary_interpolation;

use crate::LintRule;

/// Returns the rules of this category.
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
        Box::new(constant_string_concat::ConstantStringConcat),
        Box::new(negated_condition::NegatedCondition),
    ]
}
