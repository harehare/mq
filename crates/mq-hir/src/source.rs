use url::Url;

slotmap::new_key_type! {
    /// Identifier of a [`Source`].
    pub struct SourceId;
}

/// A source file registered in the HIR.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    /// Location of the source, if it has one.
    pub url: Option<Url>,
}

impl Source {
    /// Creates a source with the given URL.
    pub fn new(url: Option<Url>) -> Self {
        Self { url }
    }
}

/// Location of a symbol or scope.
#[derive(Debug, Clone, PartialEq, Hash)]
pub struct SourceInfo {
    /// The source the item belongs to.
    pub source_id: Option<SourceId>,
    /// Range of the item within the source.
    pub text_range: Option<mq_lang::Range>,
}

impl SourceInfo {
    /// Creates a source location.
    pub fn new(source_id: Option<SourceId>, text_range: Option<mq_lang::Range>) -> Self {
        Self { source_id, text_range }
    }
}
