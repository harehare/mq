use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};

/// Syntax highlighter supporting various programming languages and HTML.
///
/// This struct uses tree-sitter to provide syntax highlighting with ANSI color codes
/// for various programming languages and markup formats.
///
/// # Examples
///
/// ```rust
/// use mq_viewer::SyntaxHighlighter;
///
/// let mut highlighter = SyntaxHighlighter::new();
/// let code = "fn main() { println!(\"Hello\"); }";
/// let highlighted = highlighter.highlight(code, Some("rust"));
/// println!("{}", highlighted);
/// ```
pub struct SyntaxHighlighter {
    highlighter: Highlighter,
}

impl SyntaxHighlighter {
    pub fn new() -> Self {
        Self {
            highlighter: Highlighter::new(),
        }
    }

    /// Get the appropriate tree-sitter language and highlight configuration for a given language
    fn get_highlight_config(lang: &str) -> Option<HighlightConfiguration> {
        let (language, query) = match lang.to_lowercase().as_str() {
            "rust" | "rs" => (
                tree_sitter_rust::LANGUAGE.into(),
                tree_sitter_rust::HIGHLIGHTS_QUERY,
            ),
            "javascript" | "js" => (
                tree_sitter_javascript::LANGUAGE.into(),
                tree_sitter_javascript::HIGHLIGHT_QUERY,
            ),
            "typescript" | "ts" => (
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
            "tsx" => (
                tree_sitter_typescript::LANGUAGE_TSX.into(),
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ),
            "python" | "py" => (
                tree_sitter_python::LANGUAGE.into(),
                tree_sitter_python::HIGHLIGHTS_QUERY,
            ),
            "go" => (
                tree_sitter_go::LANGUAGE.into(),
                tree_sitter_go::HIGHLIGHTS_QUERY,
            ),
            "html" => (
                tree_sitter_html::LANGUAGE.into(),
                tree_sitter_html::HIGHLIGHTS_QUERY,
            ),
            "css" => (
                tree_sitter_css::LANGUAGE.into(),
                tree_sitter_css::HIGHLIGHTS_QUERY,
            ),
            "json" => (
                tree_sitter_json::LANGUAGE.into(),
                tree_sitter_json::HIGHLIGHTS_QUERY,
            ),
            "bash" | "sh" => (
                tree_sitter_bash::LANGUAGE.into(),
                tree_sitter_bash::HIGHLIGHT_QUERY,
            ),
            "c" => (
                tree_sitter_c::LANGUAGE.into(),
                tree_sitter_c::HIGHLIGHT_QUERY,
            ),
            "cpp" | "c++" | "cxx" => (
                tree_sitter_cpp::LANGUAGE.into(),
                tree_sitter_cpp::HIGHLIGHT_QUERY,
            ),
            "java" => (
                tree_sitter_java::LANGUAGE.into(),
                tree_sitter_java::HIGHLIGHTS_QUERY,
            ),
            "hs" | "haskell" => (
                tree_sitter_haskell::LANGUAGE.into(),
                tree_sitter_haskell::HIGHLIGHTS_QUERY,
            ),
            "elm" => (
                tree_sitter_elm::LANGUAGE.into(),
                tree_sitter_elm::HIGHLIGHTS_QUERY,
            ),
            "mq" => (
                tree_sitter_mq::LANGUAGE.into(),
                tree_sitter_mq::HIGHLIGHTS_QUERY,
            ),
            _ => return None,
        };

        let mut config = HighlightConfiguration::new(language, "", query, "", "").ok()?;

        config.configure(&[
            "attribute",
            "constant",
            "function.builtin",
            "function",
            "keyword",
            "operator",
            "property",
            "punctuation",
            "punctuation.bracket",
            "punctuation.delimiter",
            "string",
            "string.special",
            "tag",
            "type",
            "type.builtin",
            "variable",
            "variable.builtin",
            "variable.parameter",
            "comment",
            "number",
            "boolean",
            "escape",
            "label",
            "namespace",
            "constructor",
            "embedded",
        ]);

        Some(config)
    }

    /// Highlight code and return colored output
    pub fn highlight(&mut self, code: &str, lang: Option<&str>) -> String {
        // If no language specified or config not available, return plain text
        let Some(lang) = lang else {
            return code.to_string();
        };

        let Some(config) = Self::get_highlight_config(lang) else {
            return code.to_string();
        };

        let highlights = match self
            .highlighter
            .highlight(&config, code.as_bytes(), None, |_| None)
        {
            Ok(h) => h,
            Err(_) => return code.to_string(),
        };

        let mut result = String::new();
        let mut current_pos = 0;

        for event in highlights {
            match event {
                Ok(HighlightEvent::Source { start, end }) => {
                    if start > current_pos {
                        // Add unhighlighted text
                        result.push_str(&code[current_pos..start]);
                    }
                    result.push_str(&code[start..end]);
                    current_pos = end;
                }
                Ok(HighlightEvent::HighlightStart(Highlight(idx))) => {
                    // Apply color based on highlight type
                    let color_code = Self::get_color_for_highlight(idx);
                    result.push_str(color_code);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    // Reset color
                    result.push_str("\x1b[0m");
                }
                Err(_) => {}
            }
        }

        // Add any remaining text
        if current_pos < code.len() {
            result.push_str(&code[current_pos..]);
        }

        result
    }

    /// Map highlight index to ANSI color codes
    fn get_color_for_highlight(idx: usize) -> &'static str {
        match idx {
            0 => "\x1b[36m",  // attribute - cyan
            1 => "\x1b[35m",  // constant - magenta
            2 => "\x1b[33m",  // function.builtin - yellow
            3 => "\x1b[34m",  // function - blue
            4 => "\x1b[95m",  // keyword - bright magenta
            5 => "\x1b[37m",  // operator - white
            6 => "\x1b[36m",  // property - cyan
            7 => "\x1b[90m",  // punctuation - bright black
            8 => "\x1b[90m",  // punctuation.bracket - bright black
            9 => "\x1b[90m",  // punctuation.delimiter - bright black
            10 => "\x1b[32m", // string - green
            11 => "\x1b[92m", // string.special - bright green
            12 => "\x1b[34m", // tag - blue
            13 => "\x1b[33m", // type - yellow
            14 => "\x1b[93m", // type.builtin - bright yellow
            15 => "\x1b[37m", // variable - white
            16 => "\x1b[35m", // variable.builtin - magenta
            17 => "\x1b[36m", // variable.parameter - cyan
            18 => "\x1b[90m", // comment - bright black (gray)
            19 => "\x1b[35m", // number - magenta
            20 => "\x1b[35m", // boolean - magenta
            21 => "\x1b[36m", // escape - cyan
            22 => "\x1b[33m", // label - yellow
            23 => "\x1b[36m", // namespace - cyan
            24 => "\x1b[33m", // constructor - yellow
            25 => "\x1b[37m", // embedded - white
            _ => "\x1b[0m",   // default - reset
        }
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::rust("rust", r#"fn main() { println!("Hello, world!"); }"#)]
    #[case::python("python", r#"def main(): print("Hello, world!")"#)]
    #[case::js("javascript", r#"function main() { console.log('Hello, world!'); }"#)]
    #[case::ts(
        "typescript",
        r#"function main(): void { console.log('Hello, world!'); }"#
    )]
    #[case::go("go", r#"func main() { fmt.Println("Hello, world!") }"#)]
    #[case::html("html", r#"<h1>Hello</h1>"#)]
    #[case::css("css", r#"body { color: red; }"#)]
    #[case::json("json", r#"{ "hello": "world" }"#)]
    #[case::bash("bash", r#"echo 'Hello, world!'"#)]
    #[case::c("c", r#"int main() { printf("Hello, world!"); }"#)]
    #[case::java("java", r#"public class Main { public static void main(String[] args) { System.out.println("Hello, world!"); } }"#)]
    #[case::haskell("haskell", r#"main = putStrLn "Hello, world!""#)]
    #[case::elm("elm", r#"main = text "Hello, world!""#)]
    #[case::mq("mq", r#"fn(): "Hello, world!""#)]
    #[case::bool("mq", r#"fn(): true"#)]
    #[case::number("mq", r#"fn(): 42"#)]
    fn test_highlighting_for_supported_languages(#[case] lang: &str, #[case] code: &str) {
        let mut highlighter = SyntaxHighlighter::new();
        let result = highlighter.highlight(code, Some(lang));
        dbg!(&result);
        assert!(
            result.contains("\x1b["),
            "Expected ANSI escape codes for language: {}",
            lang
        );
    }

    #[rstest]
    #[case("unknown", "some code")]
    #[case("unsupported", "another code")]
    fn test_highlighting_for_unsupported_languages(#[case] lang: &str, #[case] code: &str) {
        let mut highlighter = SyntaxHighlighter::new();
        let result = highlighter.highlight(code, Some(lang));
        assert_eq!(
            result, code,
            "Should return original code for unsupported language: {}",
            lang
        );
    }

    #[test]
    fn test_highlighting_empty_code() {
        let mut highlighter = SyntaxHighlighter::new();
        let result = highlighter.highlight("", Some("rust"));
        assert_eq!(result, "");
    }

    #[test]
    fn test_highlighting_with_invalid_code() {
        let mut highlighter = SyntaxHighlighter::new();
        // Intentionally malformed code for rust
        let code = "fn {";
        let result = highlighter.highlight(code, Some("rust"));
        // Should not panic, may or may not contain ANSI codes
        assert!(!result.is_empty());
    }
}
