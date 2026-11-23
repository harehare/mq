//! HTML to Markdown conversion binding.

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
#[serde(rename_all = "camelCase", default)]
/// Options for HTML to Markdown conversion, passed from JS as a plain object.
pub struct ConversionOptions {
    /// Extract script tags as code blocks.
    pub extract_scripts_as_code_blocks: bool,
    /// Generate front matter from the HTML metadata.
    pub generate_front_matter: bool,
    /// Use the `<title>` as the top-level heading.
    pub use_title_as_h1: bool,
    /// Base URL for resolving relative `href`/`src` values; empty falls back to `<base href>`.
    pub base_url: String,
}

impl From<ConversionOptions> for mq_markdown::ConversionOptions {
    fn from(options: ConversionOptions) -> Self {
        Self {
            extract_scripts_as_code_blocks: options.extract_scripts_as_code_blocks,
            generate_front_matter: options.generate_front_matter,
            use_title_as_h1: options.use_title_as_h1,
            base_url: Some(options.base_url).filter(|s| !s.is_empty()),
        }
    }
}

#[wasm_bindgen(js_name=htmlToMarkdown)]
/// Converts HTML to Markdown.
pub async fn html_to_markdown(html_input: &str, options: JsValue) -> Result<String, JsValue> {
    let options: ConversionOptions = if options.is_null() || options.is_undefined() {
        ConversionOptions::default()
    } else {
        serde_wasm_bindgen::from_value(options).map_err(|e| JsValue::from_str(&format!("Invalid options: {}", e)))?
    };
    mq_markdown::convert_html_to_markdown(html_input, options.into())
        .map_err(|e| JsValue::from_str(&format!("Failed to convert HTML to Markdown: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;
    #[cfg(feature = "test-in-worker")]
    wasm_bindgen_test_configure!(run_in_dedicated_worker);
    #[cfg(not(feature = "test-in-worker"))]
    wasm_bindgen_test_configure!(run_in_browser);

    #[allow(unused)]
    #[wasm_bindgen_test]
    async fn test_html_to_markdown() {
        let html = r#"<h1>Hello World</h1><p>This is a <strong>test</strong>.</p>"#;
        let result = html_to_markdown(html, JsValue::UNDEFINED).await;
        assert!(result.is_ok());
        let markdown = result.unwrap();
        assert!(markdown.contains("# Hello World"));
        assert!(markdown.contains("**test**"));
    }
}
