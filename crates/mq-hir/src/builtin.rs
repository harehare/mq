use mq_help::DocTable;

use crate::{scope::ScopeId, source::SourceId};

#[derive(Debug, Default, Clone)]
pub struct Builtin {
    pub disabled: bool,
    /// The source being analyzed is `builtin.mq` itself: the native builtins are declared, but the
    /// module is not loaded a second time on top of it.
    pub self_hosted: bool,
    pub docs: DocTable,
    pub source_id: SourceId,
    pub scope_id: ScopeId,
    pub loaded: bool,
}

impl Builtin {
    pub fn new(source_id: SourceId, scope_id: ScopeId) -> Self {
        Self {
            docs: mq_help::BUILTIN_DOC,
            source_id,
            scope_id,
            disabled: false,
            self_hosted: false,
            loaded: false,
        }
    }
}
