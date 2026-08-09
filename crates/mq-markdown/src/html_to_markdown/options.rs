#[derive(Debug, Clone, Default)]
pub struct ConversionOptions {
    pub extract_scripts_as_code_blocks: bool,
    pub generate_front_matter: bool,
    pub use_title_as_h1: bool,
    /// Base URL used to resolve relative `href`/`src` values (e.g. `/path`, `../img.png`)
    /// into absolute URLs. Falls back to a `<base href>` found in the document's `<head>`
    /// when unset. Relative URLs are left unresolved if neither is available.
    pub base_url: Option<String>,
}
