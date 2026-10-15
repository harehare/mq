/// The `ambiguous_qualified_access` rule.
pub mod ambiguous_qualified_access;
/// The `missing_module_doc` rule.
pub mod missing_module_doc;

use crate::LintRule;

/// Returns the rules of this category.
pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(missing_module_doc::MissingModuleDoc),
        Box::new(ambiguous_qualified_access::AmbiguousQualifiedAccess),
    ]
}
