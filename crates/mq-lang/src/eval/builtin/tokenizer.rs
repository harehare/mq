//! `token_count` builtin. Two-tier: a dependency-free chars-per-token heuristic by default, or
//! exact counts via `tiktoken-rs` behind the opt-in `tiktoken` feature. The exact tier pulls in
//! several MB of vendored vocabulary data once `model` is resolved at runtime (every encoding
//! becomes reachable, so none can be dead-code-eliminated), hence the feature gate.

use super::Error;

/// Kept in its own module (instead of `#[cfg(not(feature = "tiktoken"))]`-ing it out) so it's
/// still compiled and tested under `--all-features`, even though [`token_count`] only calls it
/// when `tiktoken` is off.
#[cfg_attr(feature = "tiktoken", allow(dead_code))]
mod heuristic {
    // Rough chars-per-token by script; CJK tokenizes much denser than Latin under typical BPE.
    const CJK_CHARS_PER_TOKEN: f64 = 1.6;
    const LATIN_CHARS_PER_TOKEN: f64 = 4.0;
    const OTHER_CHARS_PER_TOKEN: f64 = 2.5;

    pub(super) fn is_cjk(c: char) -> bool {
        matches!(c as u32,
            0x3040..=0x30FF   // Hiragana, Katakana
            | 0x3130..=0x318F // Hangul Compatibility Jamo
            | 0x3400..=0x4DBF // CJK Unified Ideographs Extension A
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0xAC00..=0xD7A3 // Hangul Syllables
            | 0xF900..=0xFAFF // CJK Compatibility Ideographs
            | 0xFF00..=0xFFEF // Halfwidth and Fullwidth Forms
        )
    }

    pub(super) fn is_latin(c: char) -> bool {
        (c as u32) < 0x0250 // Basic Latin, Latin-1 Supplement, Latin Extended-A/B
    }

    pub(super) fn token_count(text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }

        let estimate: f64 = text
            .chars()
            .map(|c| {
                if is_cjk(c) {
                    1.0 / CJK_CHARS_PER_TOKEN
                } else if is_latin(c) {
                    1.0 / LATIN_CHARS_PER_TOKEN
                } else {
                    1.0 / OTHER_CHARS_PER_TOKEN
                }
            })
            .sum();

        estimate.ceil() as usize
    }
}

/// `model` must be one of tiktoken-rs's recognized names (e.g. `"gpt-5"`, `"gpt-4"`,
/// `"text-embedding-3-small"`); unrecognized names error rather than falling back silently.
/// Uses `encode_ordinary` so special-token syntax in `text` (e.g. `<|endoftext|>`) is counted
/// as plain text, since `text` is arbitrary document content, not a trusted prompt.
#[cfg(feature = "tiktoken")]
fn exact_token_count(text: &str, model: &str) -> Result<usize, Error> {
    tiktoken_rs::bpe_for_model(model)
        .map(|bpe| bpe.encode_ordinary(text).len())
        .map_err(|e| Error::Runtime(format!("token_count: {e}")))
}

pub(super) fn token_count(text: &str, model: &str) -> Result<usize, Error> {
    #[cfg(feature = "tiktoken")]
    {
        exact_token_count(text, model)
    }
    #[cfg(not(feature = "tiktoken"))]
    {
        let _ = model;
        Ok(heuristic::token_count(text))
    }
}

#[cfg(test)]
mod tests {
    use super::heuristic::{is_cjk, is_latin, token_count as heuristic_token_count};
    #[cfg(feature = "tiktoken")]
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::empty("", 0)]
    #[case::single_ascii_char("a", 1)]
    #[case::short_english("Hello, world!", 4)]
    fn test_heuristic_token_count(#[case] text: &str, #[case] expected: usize) {
        assert_eq!(heuristic_token_count(text), expected);
    }

    #[test]
    fn test_heuristic_token_count_cjk_denser_than_latin() {
        let english = "hello world";
        let japanese = "こんにちは世界";
        assert!(english.chars().count() >= japanese.chars().count());
        assert!(heuristic_token_count(japanese) > heuristic_token_count(english) / 2);
    }

    #[test]
    fn test_is_cjk() {
        assert!(is_cjk('あ'));
        assert!(is_cjk('ア'));
        assert!(is_cjk('漢'));
        assert!(is_cjk('한'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
    }

    #[test]
    fn test_is_latin() {
        assert!(is_latin('a'));
        assert!(is_latin('Z'));
        assert!(is_latin('é'));
        assert!(!is_latin('漢'));
        assert!(!is_latin('Ж')); // Cyrillic
    }

    #[cfg(feature = "tiktoken")]
    #[rstest]
    #[case::empty("", "gpt-4", 0)]
    #[case::simple("Hello, world!", "gpt-4", 4)]
    fn test_exact_token_count(#[case] text: &str, #[case] model: &str, #[case] expected: usize) {
        assert_eq!(exact_token_count(text, model).unwrap(), expected);
    }

    #[cfg(feature = "tiktoken")]
    #[test]
    fn test_exact_token_count_unknown_model() {
        assert!(exact_token_count("hello", "not-a-real-model").is_err());
    }

    #[cfg(feature = "tiktoken")]
    #[test]
    fn test_exact_token_count_ignores_special_tokens_in_text() {
        assert!(exact_token_count("<|endoftext|>", "gpt-4").unwrap() > 0);
    }
}
