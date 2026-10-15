/// Options for converting HTML to Markdown.
#[derive(Debug, Clone, Default)]
pub struct ConversionOptions {
    /// Emit `<script>` contents as code blocks.
    pub extract_scripts_as_code_blocks: bool,
    /// Emit YAML front matter built from `<head>`.
    pub generate_front_matter: bool,
    /// Use `<title>` as a level-1 heading.
    pub use_title_as_h1: bool,
    /// Base URL for resolving relative `href`/`src` values; falls back to `<base href>`.
    pub base_url: Option<String>,
}
