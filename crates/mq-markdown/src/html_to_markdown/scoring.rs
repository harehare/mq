//! Arc90/Readability-style content scoring, used as a last-resort fallback when no
//! semantic entry-point selector (`main`, `article`, `#content`, ...) matched.
//!
//! The candidate is only used when no heading/paragraph exists outside its subtree
//! (see [`has_heading_or_prose_outside`]) so it can never discard real content.

use rustc_hash::FxHashMap;
use scraper::{ElementRef, Html, Selector};

const MIN_CANDIDATE_TEXT_LEN: usize = 25;

const POSITIVE_TOKENS: &[&str] = &[
    "article", "body", "content", "entry", "hentry", "main", "page", "post", "text", "blog", "story",
];

const NEGATIVE_TOKENS: &[&str] = &[
    "comment", "combx", "contact", "foot", "footer", "footnote", "masthead", "media", "meta", "outbrain", "promo",
    "related", "scroll", "shoutbox", "sidebar", "sponsor", "shopping", "tags", "tool", "widget", "popup", "disqus",
    "extra", "share", "nav",
];

fn class_id_tokens(el: &ElementRef) -> impl Iterator<Item = String> {
    let class = el.value().attr("class").unwrap_or("").to_string();
    let id = el.value().attr("id").unwrap_or("").to_string();
    format!("{class} {id}")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .into_iter()
}

fn class_weight(el: &ElementRef) -> f64 {
    let tokens: Vec<String> = class_id_tokens(el).collect();
    let mut weight = 0.0;
    if tokens.iter().any(|t| POSITIVE_TOKENS.contains(&t.as_str())) {
        weight += 25.0;
    }
    if tokens.iter().any(|t| NEGATIVE_TOKENS.contains(&t.as_str())) {
        weight -= 25.0;
    }
    weight
}

fn tag_score(el: &ElementRef) -> f64 {
    match el.value().name() {
        "div" | "article" | "section" => 5.0,
        "blockquote" | "pre" | "td" => 3.0,
        "address" | "ol" | "ul" | "form" | "dl" | "dt" | "dd" | "li" => -3.0,
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "th" => -5.0,
        _ => 0.0,
    }
}

fn link_density(el: &ElementRef) -> f64 {
    let total_len = el.text().collect::<String>().chars().count();
    if total_len == 0 {
        return 0.0;
    }
    let Ok(link_selector) = Selector::parse("a") else {
        return 0.0;
    };
    let link_len: usize = el
        .select(&link_selector)
        .map(|a| a.text().collect::<String>().chars().count())
        .sum();
    link_len as f64 / total_len as f64
}

/// True if a heading/substantial paragraph exists in `el`'s subtree outside `best_id`.
fn has_heading_or_prose_outside(el: ElementRef, best_id: ego_tree::NodeId) -> bool {
    if el.id() == best_id {
        return false;
    }
    let is_heading = matches!(el.value().name(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
    let is_prose = matches!(el.value().name(), "p" | "pre" | "td" | "blockquote")
        && el.text().collect::<String>().trim().chars().count() >= MIN_CANDIDATE_TEXT_LEN;
    if is_heading || is_prose {
        return true;
    }
    el.child_elements()
        .any(|child| has_heading_or_prose_outside(child, best_id))
}

/// Finds the best "main content" candidate via content scoring, or `None` if no
/// candidate qualifies, or a candidate was found but discarding everything else would
/// lose a heading or paragraph that lives outside it.
pub(super) fn find_best_candidate(html: &Html) -> Option<ElementRef<'_>> {
    let selector = Selector::parse("p, pre, td").ok()?;
    let mut scores: FxHashMap<ego_tree::NodeId, f64> = FxHashMap::default();

    for candidate in html.select(&selector) {
        let text = candidate.text().collect::<String>();
        let trimmed_len = text.trim().chars().count();
        if trimmed_len < MIN_CANDIDATE_TEXT_LEN {
            continue;
        }
        let comma_bonus = text.matches(',').count() as f64;
        let length_bonus = (trimmed_len / 100).min(3) as f64;
        let content_score = 1.0 + comma_bonus + length_bonus;

        let Some(parent) = candidate.parent().and_then(ElementRef::wrap) else {
            continue;
        };
        let entry = scores
            .entry(parent.id())
            .or_insert_with(|| tag_score(&parent) + class_weight(&parent));
        *entry += content_score;

        if let Some(grandparent) = parent.parent().and_then(ElementRef::wrap) {
            let entry = scores
                .entry(grandparent.id())
                .or_insert_with(|| tag_score(&grandparent) + class_weight(&grandparent));
            *entry += content_score / 2.0;
        }
    }

    let best = scores
        .into_iter()
        .filter_map(|(id, score)| {
            let el = ElementRef::wrap(html.tree.get(id)?)?;
            if matches!(
                el.value().name(),
                "nav" | "header" | "footer" | "aside" | "body" | "html"
            ) {
                return None;
            }
            let density = link_density(&el).min(0.9);
            let final_score = score * (1.0 - density);
            (final_score > 0.0).then_some((el, final_score))
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(el, _)| el)?;

    let body = super::find_element(html, "body")?;
    if has_heading_or_prose_outside(body, best.id()) {
        return None;
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rstest::rstest;

    #[rstest]
    #[case::picks_content_over_link_only_sidebar(
        r#"<html><body>
            <div class="sidebar"><a href="/a">Link one</a><a href="/b">Link two</a><a href="/c">Link three</a></div>
            <div id="content">
                <p>This is the first real paragraph of the article, with plenty of words to score well.</p>
                <p>And here is a second paragraph, continuing the article with even more prose content.</p>
            </div>
        </body></html>"#,
        Some("content")
    )]
    #[case::picks_content_over_link_only_sidebar_multibyte(
        r#"<html><body>
            <div class="sidebar"><a href="/a">リンク一</a><a href="/b">リンク二</a><a href="/c">リンク三</a></div>
            <div id="content">
                <p>これは記事の最初の段落です。スコアリングされるのに十分な長さの、意味のある日本語の文章が含まれています。</p>
                <p>これは記事の2番目の段落で、絵文字🎉も含む十分な長さの文章がここに続きます、はい。</p>
            </div>
        </body></html>"#,
        Some("content")
    )]
    #[case::bails_when_heading_sibling_would_be_lost(
        r#"<html><body>
            <h1>Page Title</h1>
            <div>
                <p>This is a long enough paragraph to be considered a real scoring candidate here.</p>
            </div>
        </body></html>"#,
        None
    )]
    #[case::returns_none_when_no_candidate_qualifies("<html><body><div><p>Too short</p></div></body></html>", None)]
    fn test_find_best_candidate(#[case] html: &str, #[case] expected_content_id: Option<&str>) {
        let doc = Html::parse_document(html);
        let result_id = find_best_candidate(&doc).and_then(|el| el.value().attr("id").map(str::to_string));
        assert_eq!(result_id.as_deref(), expected_content_id);
    }

    // Multibyte-inclusive vocabulary; shortest word is 2 chars, so >=12 words
    // guarantees each paragraph clears MIN_CANDIDATE_TEXT_LEN (25) even in the
    // (astronomically unlikely) worst case of drawing the same short word every time.
    fn prose_word() -> impl Strategy<Value = &'static str> {
        prop_oneof![
            Just("lorem"),
            Just("ipsum"),
            Just("dolor"),
            Just("sit"),
            Just("amet"),
            Just("日本語"),
            Just("文章"),
            Just("絵文字🎉"),
            Just("café"),
            Just("naïve"),
        ]
    }

    fn prose_text() -> impl Strategy<Value = String> {
        prop::collection::vec(prose_word(), 12..20).prop_map(|words| words.join(" "))
    }

    proptest! {
        // Must never panic on multibyte/emoji input.
        #[test]
        fn prop_content_div_always_wins_over_link_only_sidebar(para_a in prose_text(), para_b in prose_text()) {
            let html = format!(
                r#"<html><body>
                    <div class="sidebar"><a href="/a">Link</a><a href="/b">Link</a><a href="/c">Link</a></div>
                    <div id="content"><p>{para_a}</p><p>{para_b}</p></div>
                </body></html>"#
            );
            let doc = Html::parse_document(&html);
            let result_id = find_best_candidate(&doc).and_then(|el| el.value().attr("id").map(str::to_string));
            prop_assert_eq!(result_id.as_deref(), Some("content"));
        }
    }
}
