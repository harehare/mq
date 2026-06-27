pub mod always_true_condition;
pub mod duplicate_match_arm;
pub mod infinite_loop;
pub mod missing_else_in_expr;
pub mod shadow_variable;
pub mod unreachable_code;
pub mod unused_function;
pub mod unused_import;
pub mod unused_variable;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(unused_variable::UnusedVariable),
        Box::new(unused_function::UnusedFunction),
        Box::new(unused_import::UnusedImport),
        Box::new(unreachable_code::UnreachableCode),
        Box::new(infinite_loop::InfiniteLoop),
        Box::new(duplicate_match_arm::DuplicateMatchArm),
        Box::new(shadow_variable::ShadowVariable),
        Box::new(missing_else_in_expr::MissingElseInExpr),
        Box::new(always_true_condition::AlwaysTrueCondition),
    ]
}
