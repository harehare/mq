use mq_help::DocTable;

use crate::{scope::ScopeId, source::SourceId};

#[derive(Debug, Default, Clone)]
pub struct Builtin {
    pub disabled: bool,
    pub functions: DocTable,
    pub internal_functions: DocTable,
    pub selectors: DocTable,
    pub source_id: SourceId,
    pub scope_id: ScopeId,
    pub loaded: bool,
}

impl Builtin {
    pub fn new(source_id: SourceId, scope_id: ScopeId) -> Self {
        Self {
            functions: mq_help::BUILTIN_FUNCTION_DOC,
            internal_functions: mq_help::INTERNAL_FUNCTION_DOC,
            selectors: mq_help::BUILTIN_SELECTOR_DOC,
            source_id,
            scope_id,
            disabled: false,
            loaded: false,
        }
    }
}
