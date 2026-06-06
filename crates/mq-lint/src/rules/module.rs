pub mod ambiguous_qualified_access;
pub mod missing_module_doc;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(missing_module_doc::MissingModuleDoc),
        Box::new(ambiguous_qualified_access::AmbiguousQualifiedAccess),
    ]
}
