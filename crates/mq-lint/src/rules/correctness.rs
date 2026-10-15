/// The `always_true_condition` rule.
pub mod always_true_condition;
/// The `deprecated_function_call` rule.
pub mod deprecated_function_call;
/// The `duplicate_import` rule.
pub mod duplicate_import;
/// The `duplicate_match_arm` rule.
pub mod duplicate_match_arm;
/// The `infinite_loop` rule.
pub mod infinite_loop;
/// The `missing_else_in_expr` rule.
pub mod missing_else_in_expr;
/// The `shadow_variable` rule.
pub mod shadow_variable;
/// The `unreachable_code` rule.
pub mod unreachable_code;
/// The `unused_function` rule.
pub mod unused_function;
/// The `unused_import` rule.
pub mod unused_import;
/// The `unused_parameter` rule.
pub mod unused_parameter;
/// The `unused_variable` rule.
pub mod unused_variable;

use crate::LintRule;

/// Returns the rules of this category.
pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(unused_variable::UnusedVariable),
        Box::new(unused_function::UnusedFunction),
        Box::new(unused_import::UnusedImport),
        Box::new(unused_parameter::UnusedParameter),
        Box::new(unreachable_code::UnreachableCode),
        Box::new(infinite_loop::InfiniteLoop),
        Box::new(deprecated_function_call::DeprecatedFunctionCall),
        Box::new(duplicate_match_arm::DuplicateMatchArm),
        Box::new(duplicate_import::DuplicateImport),
        Box::new(shadow_variable::ShadowVariable),
        Box::new(missing_else_in_expr::MissingElseInExpr),
        Box::new(always_true_condition::AlwaysTrueCondition),
    ]
}
