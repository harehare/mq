use std::sync::Arc;

use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::{scope::ScopeId, source::SourceId};

#[derive(Debug, Default, Clone)]
pub struct Builtin {
    pub disabled: bool,
    pub functions: Arc<FxHashMap<SmolStr, mq_lang::BuiltinFunctionDoc>>,
    pub internal_functions: Arc<FxHashMap<SmolStr, mq_lang::BuiltinFunctionDoc>>,
    pub selectors: Arc<FxHashMap<SmolStr, mq_lang::BuiltinSelectorDoc>>,
    pub source_id: SourceId,
    pub scope_id: ScopeId,
    pub loaded: bool,
}

impl Builtin {
    pub fn new(source_id: SourceId, scope_id: ScopeId) -> Self {
        Self {
            functions: Arc::new(mq_lang::BUILTIN_FUNCTION_DOC.clone()),
            internal_functions: Arc::new(mq_lang::INTERNAL_FUNCTION_DOC.clone()),
            selectors: Arc::new(mq_lang::BUILTIN_SELECTOR_DOC.clone()),
            source_id,
            scope_id,
            disabled: false,
            loaded: false,
        }
    }
}
