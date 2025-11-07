use url::Url;

slotmap::new_key_type! { pub struct SourceId; }

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Source {
    pub url: Option<Url>,
}

impl Source {
    pub fn new(url: Option<Url>) -> Self {
        Self { url }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    pub source_id: Option<SourceId>,
    pub text_range: Option<mq_lang::Range>,
}

impl SourceInfo {
    pub fn new(source_id: Option<SourceId>, text_range: Option<mq_lang::Range>) -> Self {
        Self { source_id, text_range }
    }
}
