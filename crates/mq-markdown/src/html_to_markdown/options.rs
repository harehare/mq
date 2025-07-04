#[derive(Debug, Clone, Copy, Default)]
pub struct ConversionOptions {
    pub extract_scripts_as_code_blocks: bool,
    pub generate_front_matter: bool,
    pub use_title_as_h1: bool,
}
