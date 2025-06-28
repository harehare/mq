#[cfg(feature = "html-to-markdown")]
#[derive(Debug, Clone, Copy)]
pub struct ConversionOptions {
    pub extract_scripts_as_code_blocks: bool,
    pub generate_front_matter: bool,
    // Add future options here
}

#[cfg(feature = "html-to-markdown")]
impl Default for ConversionOptions {
    fn default() -> Self {
        ConversionOptions {
            extract_scripts_as_code_blocks: false,
            generate_front_matter: false,
        }
    }
}
