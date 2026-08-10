#![cfg(feature = "html-to-markdown")]
pub mod converter;
mod iframe;
pub mod node;
mod noise;
pub mod options;
pub mod parser;
mod scoring;

use miette::miette;
pub use options::ConversionOptions;
use scraper::Html;
use scraper::Selector;
use std::collections::BTreeMap;

fn find_element<'a>(html: &'a Html, selector_str: &str) -> Option<scraper::ElementRef<'a>> {
    Selector::parse(selector_str)
        .ok()
        .and_then(|sel| html.select(&sel).next())
}

/// Resolves the base URL: `options.base_url` takes priority over a `<base href>` in `<head>`.
fn resolve_base_url(html: &Html, options_base: Option<&str>) -> Option<url::Url> {
    if let Some(explicit) = options_base {
        return url::Url::parse(explicit).ok();
    }
    let base_href = find_element(html, "base").and_then(|el| el.value().attr("href"))?;
    url::Url::parse(base_href).ok()
}

/// Rewrites relative `href`/`src` on link/image/embed elements into absolute URLs against `base`.
fn resolve_relative_urls(nodes: &mut [node::HtmlNode], base: &url::Url) {
    for n in nodes.iter_mut() {
        if let node::HtmlNode::Element(el) = n {
            let attr_name = match el.tag_name.as_str() {
                "a" => Some("href"),
                "img" | "source" | "iframe" | "embed" | "video" | "audio" | "object" => Some("src"),
                _ => None,
            };
            if let Some(attr_name) = attr_name
                && let Some(Some(value)) = el.attributes.get(attr_name)
                && !value.is_empty()
                && let Ok(resolved) = base.join(value)
            {
                el.attributes.insert(attr_name.to_string(), Some(resolved.to_string()));
            }
            resolve_relative_urls(&mut el.children, base);
        }
    }
}

/// Recursively drops unrendered and boilerplate (ads/share/comments/cookie banners/...) nodes.
fn strip_noise_nodes(nodes: &mut Vec<node::HtmlNode>) {
    nodes.retain(|n| match n {
        node::HtmlNode::Element(el) => !(noise::is_hidden_element(el) || noise::is_noise_by_class_id(el)),
        _ => true,
    });
    for n in nodes.iter_mut() {
        if let node::HtmlNode::Element(el) = n {
            strip_noise_nodes(&mut el.children);
        }
    }
}

fn extract_title_text(html: &Html) -> Option<String> {
    let head = find_element(html, "head")?;
    let title_sel = Selector::parse("title").ok()?;
    let title_text = head.select(&title_sel).next()?.text().collect::<String>();
    let trimmed = title_text.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

/// Collects `<meta name=... content=...>` and `<meta property=... content=...>` pairs
/// within `head`, keyed by the lowercased `name`/`property` value (OGP and Twitter Card
/// tags use `property`/`name` inconsistently across sites, so both are checked).
fn collect_meta_content(head: &scraper::ElementRef) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let Ok(selector) = Selector::parse("meta") else {
        return map;
    };
    for meta_node in head.select(&selector) {
        let Some(content) = meta_node
            .value()
            .attr("content")
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        for attr in ["name", "property"] {
            if let Some(key) = meta_node.value().attr(attr) {
                map.entry(key.to_lowercase()).or_insert_with(|| content.to_string());
            }
        }
    }
    map
}

/// Recursively unwraps a JSON-LD document (arrays and `@graph` wrappers) into individual
/// schema.org node objects.
fn flatten_json_ld(value: serde_json::Value, out: &mut Vec<serde_json::Value>) {
    match value {
        serde_json::Value::Array(items) => items.into_iter().for_each(|item| flatten_json_ld(item, out)),
        serde_json::Value::Object(ref map) => {
            if let Some(graph) = map.get("@graph").cloned() {
                flatten_json_ld(graph, out);
            }
            out.push(value);
        }
        _ => {}
    }
}

const SCHEMA_ARTICLE_TYPES: &[&str] = &["Article", "NewsArticle", "BlogPosting", "TechArticle", "Report"];

/// Finds the first `<script type="application/ld+json">` node whose `@type` is an
/// article-like schema.org type (anywhere in the document, not just `<head>`, since
/// sites commonly emit JSON-LD just before `</body>`).
fn find_article_schema(html: &Html) -> Option<serde_json::Value> {
    let selector = Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;
    let mut docs = Vec::new();
    for script in html.select(&selector) {
        let text: String = script.text().collect();
        if let Ok(value) = serde_json::from_str(text.trim()) {
            flatten_json_ld(value, &mut docs);
        }
    }
    docs.into_iter().find(|doc| match doc.get("@type") {
        Some(serde_json::Value::String(t)) => SCHEMA_ARTICLE_TYPES.iter().any(|a| t.contains(a)),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str())
            .any(|t| SCHEMA_ARTICLE_TYPES.iter().any(|a| t.contains(a))),
        _ => false,
    })
}

/// Extracts a display string from a schema.org value: a plain string, an object's
/// `name` (falling back to `url`), or the comma-joined result over an array.
fn schema_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        serde_json::Value::Object(map) => map
            .get("name")
            .and_then(schema_text)
            .or_else(|| map.get("url").and_then(schema_text)),
        serde_json::Value::Array(items) => {
            let names: Vec<String> = items.iter().filter_map(schema_text).collect();
            (!names.is_empty()).then(|| names.join(", "))
        }
        _ => None,
    }
}

fn first_non_empty<const N: usize>(candidates: [Option<String>; N]) -> Option<String> {
    candidates.into_iter().flatten().find(|s| !s.trim().is_empty())
}

fn extract_front_matter_from_head_ref(html: &Html) -> Option<BTreeMap<String, serde_yaml::Value>> {
    let head_element = find_element(html, "head")?;
    let mut fm_map = BTreeMap::new();

    if let Ok(title_selector) = Selector::parse("title")
        && let Some(title_node) = head_element.select(&title_selector).next()
    {
        let title_str = title_node.text().collect::<String>().trim().to_string();
        if !title_str.is_empty() {
            fm_map.insert("title".to_string(), serde_yaml::Value::String(title_str));
        }
    }

    let meta = collect_meta_content(&head_element);
    let schema = find_article_schema(html);
    let schema_field = |key: &str| schema.as_ref().and_then(|s| s.get(key)).and_then(schema_text);

    if let Some(keywords_content) = meta.get("keywords") {
        let keywords: Vec<serde_yaml::Value> = keywords_content
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|k| serde_yaml::Value::String(k.to_string()))
            .collect();
        if !keywords.is_empty() {
            fm_map.insert("keywords".to_string(), serde_yaml::Value::Sequence(keywords));
        }
    }

    let fields: [(&str, [Option<String>; 4]); 5] = [
        (
            "description",
            [
                meta.get("description").cloned(),
                meta.get("og:description").cloned(),
                meta.get("twitter:description").cloned(),
                schema_field("description"),
            ],
        ),
        (
            // twitter:creator is an @handle, not a display name, so it's the last resort.
            "author",
            [
                meta.get("author").cloned(),
                meta.get("article:author").cloned(),
                schema_field("author"),
                meta.get("twitter:creator").cloned(),
            ],
        ),
        (
            "published",
            [
                meta.get("article:published_time").cloned(),
                meta.get("date").cloned(),
                meta.get("pubdate").cloned(),
                schema_field("datePublished"),
            ],
        ),
        (
            "image",
            [
                meta.get("og:image").cloned(),
                meta.get("twitter:image").cloned(),
                schema_field("image"),
                None,
            ],
        ),
        (
            "site",
            [meta.get("og:site_name").cloned(), schema_field("publisher"), None, None],
        ),
    ];
    for (key, candidates) in fields {
        if let Some(value) = first_non_empty(candidates) {
            fm_map.insert(key.to_string(), serde_yaml::Value::String(value));
        }
    }

    if fm_map.is_empty() { None } else { Some(fm_map) }
}

pub fn convert_html_to_markdown(html_input: &str, options: ConversionOptions) -> miette::Result<String> {
    if html_input.trim().is_empty() {
        return Ok("".to_string());
    }

    let html = Html::parse_document(html_input);

    let mut front_matter_str = String::new();

    if options.generate_front_matter
        && let Some(fm_data) = extract_front_matter_from_head_ref(&html)
        && !fm_data.is_empty()
    {
        // Convert BTreeMap<String, Value> to serde_yaml::Mapping (which is BTreeMap<Value, Value>)
        let mut yaml_map = serde_yaml::Mapping::new();
        for (k, v) in fm_data {
            yaml_map.insert(serde_yaml::Value::String(k), v);
        }
        let yaml_value = serde_yaml::Value::Mapping(yaml_map);

        match serde_yaml::to_string(&yaml_value) {
            Ok(yaml) => {
                // serde_yaml::to_string might add its own "---" if it's a single doc,
                // or not if it's just a mapping. We want to ensure our format.
                // It typically does not add --- for a Value::Mapping.
                let content = yaml
                    .trim_start_matches("---\n")
                    .trim_end_matches('\n')
                    .trim_end_matches("...");
                front_matter_str = format!("---\n{}\n---\n\n", content.trim());
            }
            Err(_) => {
                return Err(miette!("YAML serialization failed"));
            }
        }
    }

    // Smart extraction: prefer semantic/entry-point containers, then content-scoring
    // (for non-semantic markup), then fall back to the full document.
    const ENTRY_POINT_SELECTORS: &[&str] = &[
        "main",
        "[role=\"main\"]",
        "article",
        "[role=\"article\"]",
        "#content",
        ".post-content",
        ".entry-content",
        ".article-content",
        ".article-body",
        ".markdown-body",
    ];
    let doc_children = ENTRY_POINT_SELECTORS
        .iter()
        .find_map(|sel| find_element(&html, sel).map(|el| el.children().collect::<Vec<_>>()))
        .or_else(|| scoring::find_best_candidate(&html).map(|el| el.children().collect::<Vec<_>>()))
        .unwrap_or_else(|| html.root_element().children().collect());

    let mut nodes_for_markdown_conversion = parser::map_nodes_to_html_nodes(doc_children)?;
    strip_noise_nodes(&mut nodes_for_markdown_conversion);
    if let Some(base) = resolve_base_url(&html, options.base_url.as_deref()) {
        resolve_relative_urls(&mut nodes_for_markdown_conversion, &base);
    }
    let body_markdown = converter::convert_nodes_to_markdown(&nodes_for_markdown_conversion, &options)?;

    // Extract <title> from <head> separately so it is available even when smart extraction
    // selected only the children of <main>/<article> (which do not include <head>).
    let title_prefix = if options.use_title_as_h1 {
        extract_title_text(&html)
            .map(|t| format!("# {}\n\n", t))
            .unwrap_or_default()
    } else {
        String::new()
    };

    Ok(format!("{}{}{}", front_matter_str, title_prefix, body_markdown))
}
