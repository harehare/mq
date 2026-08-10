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

    #[test]
    fn test_finds_content_div_among_boilerplate_siblings() {
        let html = Html::parse_document(
            r#"<html><body>
                <div class="sidebar"><a href="/a">Link one</a><a href="/b">Link two</a><a href="/c">Link three</a></div>
                <div id="content">
                    <p>This is the first real paragraph of the article, with plenty of words to score well.</p>
                    <p>And here is a second paragraph, continuing the article with even more prose content.</p>
                </div>
            </body></html>"#,
        );
        let best = find_best_candidate(&html).expect("should find a candidate");
        assert_eq!(best.value().attr("id"), Some("content"));
    }

    #[test]
    fn test_returns_none_when_heading_sibling_would_be_lost() {
        let html = Html::parse_document(
            r#"<html><body>
                <h1>Page Title</h1>
                <div>
                    <p>This is a long enough paragraph to be considered a real scoring candidate here.</p>
                </div>
            </body></html>"#,
        );
        assert!(find_best_candidate(&html).is_none());
    }

    #[test]
    fn test_returns_none_when_no_candidate_qualifies() {
        let html = Html::parse_document("<html><body><div><p>Too short</p></div></body></html>");
        assert!(find_best_candidate(&html).is_none());
    }
}
