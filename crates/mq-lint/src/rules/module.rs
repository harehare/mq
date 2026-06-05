pub mod ambiguous_qualified_access;
pub mod circular_import;
pub mod missing_module_doc;
pub mod reexport_private;

use crate::LintRule;

pub fn all() -> Vec<Box<dyn LintRule>> {
    vec![
        Box::new(circular_import::CircularImport),
        Box::new(missing_module_doc::MissingModuleDoc),
        Box::new(reexport_private::ReexportPrivate),
        Box::new(ambiguous_qualified_access::AmbiguousQualifiedAccess),
    ]
}
