//! Content-quality filters inspired by [defuddle](https://github.com/kepano/defuddle):
//! drop unrendered elements and elements whose `class`/`id` mark them as boilerplate.

use super::node::HtmlElement;

/// Returns true if `element` is never rendered: the `hidden` attribute,
/// `aria-hidden="true"`, or an inline `display: none` / `visibility: hidden|collapse`.
pub(super) fn is_hidden_element(element: &HtmlElement) -> bool {
    if element.attributes.contains_key("hidden") {
        return true;
    }
    if element.attributes.get("aria-hidden").and_then(|v| v.as_deref()) == Some("true") {
        return true;
    }
    if let Some(Some(style)) = element.attributes.get("style") {
        for declaration in style.split(';') {
            let mut parts = declaration.splitn(2, ':');
            let (Some(property), Some(value)) = (parts.next(), parts.next()) else {
                continue;
            };
            let property = property.trim().to_ascii_lowercase();
            let value = value.trim().to_ascii_lowercase();
            if property == "display" && value == "none" {
                return true;
            }
            if property == "visibility" && matches!(value.as_str(), "hidden" | "collapse") {
                return true;
            }
        }
    }
    false
}

/// Boilerplate class/id tokens. Excludes author/byline/date/tags on purpose: front
/// matter generation is opt-in, so stripping those from the body would lose information.
const NOISE_TOKENS: &[&str] = &[
    // ads
    "ad",
    "ads",
    "advert",
    "advertisement",
    "adsense",
    "sponsor",
    "sponsored",
    "promo",
    "promoted",
    "promotion",
    // share buttons
    "share",
    "shares",
    "sharing",
    "addtoany",
    // related/recommended content widgets
    "related",
    "relatedposts",
    "recommended",
    "morestories",
    "morenews",
    // comments
    "comments",
    "commentform",
    "commentbox",
    "disqus",
    // cookie/consent banners
    "cookie",
    "cookies",
    "consent",
    "gdpr",
    // newsletter/subscribe forms
    "newsletter",
    "subscribe",
    "subscription",
    "signup",
    // popups/modals
    "popup",
    "modal",
    "overlay",
    "lightbox",
    // breadcrumbs/pagination
    "breadcrumb",
    "breadcrumbs",
    "pagination",
    "pager",
    // sidebar/widget wrappers
    "sidebar",
    "widget",
    "widgets",
    // misc chrome
    "noprint",
    "backtotop",
    "skiplink",
];

fn class_id_tokens(element: &HtmlElement) -> impl Iterator<Item = String> + '_ {
    ["class", "id"]
        .into_iter()
        .filter_map(|attr| element.attributes.get(attr).and_then(|v| v.as_deref()))
        .flat_map(|value| value.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
}

/// Returns true if `element`'s `class`/`id` tokens identify it as boilerplate noise.
pub(super) fn is_noise_by_class_id(element: &HtmlElement) -> bool {
    class_id_tokens(element).any(|token| NOISE_TOKENS.contains(&token.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use rustc_hash::FxHashMap;

    fn element_with_attrs(attrs: &[(&str, &str)]) -> HtmlElement {
        let mut attributes = FxHashMap::default();
        for (k, v) in attrs {
            attributes.insert(k.to_string(), Some(v.to_string()));
        }
        HtmlElement {
            tag_name: "div".to_string(),
            attributes,
            children: vec![],
        }
    }

    #[rstest]
    #[case(&[("hidden", "")], true)]
    #[case(&[("aria-hidden", "true")], true)]
    #[case(&[("aria-hidden", "false")], false)]
    #[case(&[("style", "display: none;")], true)]
    #[case(&[("style", "color: red; display:none")], true)]
    #[case(&[("style", "visibility: hidden")], true)]
    #[case(&[("style", "visibility: collapse")], true)]
    #[case(&[("style", "--footer-display: none")], false)]
    #[case(&[("style", "color: red")], false)]
    #[case(&[], false)]
    fn test_is_hidden_element(#[case] attrs: &[(&str, &str)], #[case] expected: bool) {
        assert_eq!(is_hidden_element(&element_with_attrs(attrs)), expected);
    }

    #[rstest]
    #[case(&[("class", "ad-slot")], true)]
    #[case(&[("class", "site-header")], false)]
    #[case(&[("id", "comments")], true)]
    #[case(&[("class", "social-share-buttons")], true)]
    #[case(&[("class", "related-articles")], true)]
    #[case(&[("class", "newsletter-signup")], true)]
    #[case(&[("class", "cookie-banner")], true)]
    #[case(&[("class", "breadcrumb-nav")], true)]
    #[case(&[("class", "sidebar-widget")], true)]
    #[case(&[("class", "already-read")], false)]
    #[case(&[("class", "gradient-bg")], false)]
    #[case(&[("class", "article-content")], false)]
    fn test_is_noise_by_class_id(#[case] attrs: &[(&str, &str)], #[case] expected: bool) {
        assert_eq!(is_noise_by_class_id(&element_with_attrs(attrs)), expected);
    }
}
