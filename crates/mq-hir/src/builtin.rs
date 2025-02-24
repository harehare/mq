use compact_str::CompactString;
use rustc_hash::FxHashMap;

use crate::{scope::ScopeId, source::SourceId};

#[derive(Debug, Default)]
pub struct Builtin {
    pub functions: FxHashMap<CompactString, mq_lang::BuiltinFunctionDoc>,
    pub selectors: FxHashMap<CompactString, mq_lang::BuiltinSelectorDoc>,
    pub source_id: SourceId,
    pub scope_id: ScopeId,
    pub loaded: bool,
}

impl Builtin {
    pub fn new(source_id: SourceId, scope_id: ScopeId) -> Self {
        Self {
            functions: mq_lang::BUILTIN_FUNCTION_DOC.clone(),
            selectors: mq_lang::BUILTIN_SELECTOR_DOC.clone(),
            source_id,
            scope_id,
            loaded: false,
        }
    }
}
