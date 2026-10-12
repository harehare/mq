#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct ConversionOptions {
    pub extract_scripts_as_code_blocks: bool,
    pub generate_front_matter: bool,
    pub use_title_as_h1: bool,
    /// Base URL for resolving relative `href`/`src` values; falls back to `<base href>`.
    pub base_url: Option<String>,
}

impl ConversionOptions {
    /// Sets `extract_scripts_as_code_blocks`.
    pub fn with_extract_scripts_as_code_blocks(mut self, extract_scripts_as_code_blocks: bool) -> Self {
        self.extract_scripts_as_code_blocks = extract_scripts_as_code_blocks;
        self
    }

    /// Sets `generate_front_matter`.
    pub fn with_generate_front_matter(mut self, generate_front_matter: bool) -> Self {
        self.generate_front_matter = generate_front_matter;
        self
    }

    /// Sets `use_title_as_h1`.
    pub fn with_use_title_as_h1(mut self, use_title_as_h1: bool) -> Self {
        self.use_title_as_h1 = use_title_as_h1;
        self
    }

    /// Sets `base_url`.
    pub fn with_base_url(mut self, base_url: Option<String>) -> Self {
        self.base_url = base_url;
        self
    }
}
