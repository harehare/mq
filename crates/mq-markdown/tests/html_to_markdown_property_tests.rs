//! Property-based regression coverage for the html-to-markdown noise-removal and
//! entry-point-selection pipeline, including multibyte/emoji content.
//!
//! Each generated text is tagged with a role-specific prefix (`AD-`, `HIDDEN-`,
//! `CONTENT-A-`, `CONTENT-B-`) so substring checks can't cross-match between roles,
//! letting the random suffix vary freely (ASCII words, Japanese, emoji) without risking
//! a spurious pass/fail from accidental overlap between generated strings.

#![cfg(feature = "html-to-markdown")]

use mq_markdown::{ConversionOptions, convert_html_to_markdown};
use proptest::prelude::*;

fn word() -> impl Strategy<Value = &'static str> {
    prop_oneof![
        Just("lorem"),
        Just("ipsum"),
        Just("dolor"),
        Just("日本語"),
        Just("文章"),
        Just("絵文字🎉"),
        Just("café"),
    ]
}

fn tagged_text(tag: &'static str) -> impl Strategy<Value = String> {
    prop::collection::vec(word(), 1..6).prop_map(move |words| format!("{tag}-{}", words.join(" ")))
}

// Like `tagged_text`, but guaranteed >=25 trimmed chars (mq-markdown's scoring
// candidate threshold) even in the worst case of drawing the shortest (2-char) word
// every time: a 10-char tag prefix + 6 words of 2 chars + 5 spaces = 27 chars.
fn long_tagged_text(tag: &'static str) -> impl Strategy<Value = String> {
    prop::collection::vec(word(), 6..12).prop_map(move |words| format!("{tag}-{}", words.join(" ")))
}

proptest! {
    #[test]
    fn prop_noise_and_hidden_stripped_content_preserved(
        ad_text in tagged_text("AD"),
        hidden_text in tagged_text("HIDDEN"),
        content_a in tagged_text("CONTENT-A"),
        content_b in tagged_text("CONTENT-B"),
    ) {
        let html = format!(
            r#"<html><body>
                <div class="ad-container">{ad_text}</div>
                <p hidden>{hidden_text}</p>
                <div id="content"><p>{content_a}</p><p>{content_b}</p></div>
            </body></html>"#
        );
        let md = convert_html_to_markdown(&html, ConversionOptions::default()).unwrap();

        prop_assert!(!md.contains(&ad_text), "ad content leaked into output:\n{md}");
        prop_assert!(!md.contains(&hidden_text), "hidden content leaked into output:\n{md}");
        prop_assert!(md.contains(&content_a), "real content missing from output:\n{md}");
        prop_assert!(md.contains(&content_b), "real content missing from output:\n{md}");
    }

    // No entry-point selector matches here, so this exercises the scoring fallback.
    #[test]
    fn prop_scoring_fallback_prefers_prose_over_link_only_sibling(
        ad_text in tagged_text("AD"),
        content_a in long_tagged_text("CONTENT-A"),
        content_b in long_tagged_text("CONTENT-B"),
    ) {
        let html = format!(
            r#"<html><body>
                <div class="promo-links"><a href="/a">{ad_text}</a></div>
                <div><p>{content_a}</p><p>{content_b}</p></div>
            </body></html>"#
        );
        let md = convert_html_to_markdown(&html, ConversionOptions::default()).unwrap();

        prop_assert!(!md.contains(&ad_text), "link-only sibling leaked into output:\n{md}");
        prop_assert!(md.contains(&content_a), "real content missing from output:\n{md}");
        prop_assert!(md.contains(&content_b), "real content missing from output:\n{md}");
    }

    #[test]
    fn prop_front_matter_description_never_panics_and_is_preserved(desc in tagged_text("DESC")) {
        let html = format!(
            r#"<html><head><meta name="description" content="{desc}"></head><body><p>Body</p></body></html>"#
        );
        let options = ConversionOptions {
            generate_front_matter: true,
            ..Default::default()
        };
        let md = convert_html_to_markdown(&html, options).unwrap();
        prop_assert!(md.contains(&desc), "description missing from front matter:\n{md}");
    }
}
