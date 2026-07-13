//! Support for reading `sitemap.xml` files as a seed-URL source.
//!
//! Sitemaps come in two flavors, both handled here:
//! - A `<urlset>` document, whose `<url><loc>` entries are crawl targets.
//! - A `<sitemapindex>` document, whose `<sitemap><loc>` entries point at
//!   further sitemaps that are fetched and parsed recursively.

use crate::http_client::HttpClient;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashSet;
use url::Url;

/// Maximum depth of `<sitemapindex>` nesting to follow, guarding against
/// pathological or maliciously deep sitemap chains.
const MAX_SITEMAP_INDEX_DEPTH: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SitemapKind {
    UrlSet,
    SitemapIndex,
}

/// Fetches a sitemap at `sitemap_url` and returns the flat list of page URLs
/// it describes. `<sitemapindex>` documents are followed recursively until
/// only `<urlset>` (page) entries remain.
pub async fn fetch_sitemap_urls(http_client: &HttpClient, sitemap_url: &Url) -> Result<Vec<Url>, String> {
    let mut visited_sitemaps = HashSet::new();
    let mut urls = Vec::new();
    fetch_sitemap_urls_recursive(http_client, sitemap_url, 0, &mut visited_sitemaps, &mut urls).await?;
    Ok(urls)
}

fn fetch_sitemap_urls_recursive<'a>(
    http_client: &'a HttpClient,
    sitemap_url: &'a Url,
    depth: usize,
    visited_sitemaps: &'a mut HashSet<Url>,
    urls: &'a mut Vec<Url>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
    Box::pin(async move {
        if depth > MAX_SITEMAP_INDEX_DEPTH {
            return Err(format!(
                "Sitemap index nesting exceeded max depth of {} at {}",
                MAX_SITEMAP_INDEX_DEPTH, sitemap_url
            ));
        }

        if !visited_sitemaps.insert(sitemap_url.clone()) {
            tracing::warn!("Skipping already-visited sitemap URL (cycle detected): {}", sitemap_url);
            return Ok(());
        }

        tracing::info!("Fetching sitemap: {}", sitemap_url);
        let body = http_client
            .fetch(sitemap_url.clone())
            .await
            .map_err(|e| format!("Failed to fetch sitemap {}: {}", sitemap_url, e))?;

        let (kind, locs) =
            parse_sitemap_xml(&body).map_err(|e| format!("Failed to parse sitemap {}: {}", sitemap_url, e))?;

        for loc in locs {
            match Url::parse(&loc) {
                Ok(url) => match kind {
                    SitemapKind::UrlSet => urls.push(url),
                    SitemapKind::SitemapIndex => {
                        fetch_sitemap_urls_recursive(http_client, &url, depth + 1, visited_sitemaps, urls).await?;
                    }
                },
                Err(e) => tracing::warn!("Skipping invalid <loc> URL '{}' in sitemap {}: {}", loc, sitemap_url, e),
            }
        }

        Ok(())
    })
}

/// Parses sitemap XML, returning the document kind and the raw text of each
/// `<loc>` element found.
fn parse_sitemap_xml(xml: &str) -> Result<(SitemapKind, Vec<String>), String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut kind = None;
    let mut locs = Vec::new();
    let mut in_loc = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.local_name().as_ref() {
                b"urlset" => kind = Some(SitemapKind::UrlSet),
                b"sitemapindex" => kind = Some(SitemapKind::SitemapIndex),
                b"loc" => in_loc = true,
                _ => {}
            },
            Ok(Event::End(e)) => {
                if e.local_name().as_ref() == b"loc" {
                    in_loc = false;
                }
            }
            Ok(Event::Text(t)) => {
                if in_loc {
                    let decoded = t.decode().map_err(|e| format!("Failed to decode <loc> text: {}", e))?;
                    let text = quick_xml::escape::unescape(&decoded)
                        .map_err(|e| format!("Failed to unescape <loc> text: {}", e))?;
                    let text = text.trim();
                    if !text.is_empty() {
                        locs.push(text.to_string());
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Invalid sitemap XML: {}", e)),
            _ => {}
        }
        buf.clear();
    }

    let kind = kind.ok_or_else(|| "Sitemap XML is missing a <urlset> or <sitemapindex> root element".to_string())?;
    Ok((kind, locs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{Method::GET, MockServer};

    #[test]
    fn test_parse_urlset() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://example.com/page1</loc>
    <lastmod>2024-01-01</lastmod>
  </url>
  <url>
    <loc>https://example.com/page2</loc>
  </url>
</urlset>"#;

        let (kind, locs) = parse_sitemap_xml(xml).unwrap();
        assert_eq!(kind, SitemapKind::UrlSet);
        assert_eq!(locs, vec!["https://example.com/page1", "https://example.com/page2"]);
    }

    #[test]
    fn test_parse_sitemapindex() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap>
    <loc>https://example.com/sitemap-a.xml</loc>
  </sitemap>
  <sitemap>
    <loc>https://example.com/sitemap-b.xml</loc>
  </sitemap>
</sitemapindex>"#;

        let (kind, locs) = parse_sitemap_xml(xml).unwrap();
        assert_eq!(kind, SitemapKind::SitemapIndex);
        assert_eq!(
            locs,
            vec!["https://example.com/sitemap-a.xml", "https://example.com/sitemap-b.xml"]
        );
    }

    #[test]
    fn test_parse_missing_root_element() {
        let xml = r#"<?xml version="1.0"?><foo><bar/></foo>"#;
        let result = parse_sitemap_xml(xml);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing a <urlset>"));
    }

    #[test]
    fn test_parse_invalid_xml() {
        let xml = "<urlset><url><loc>https://example.com/a</wrong></url></urlset>";
        let result = parse_sitemap_xml(xml);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_fetch_sitemap_urls_urlset() {
        let server = MockServer::start_async().await;
        let body = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/a</loc></url>
  <url><loc>https://example.com/b</loc></url>
</urlset>"#;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sitemap.xml");
                then.status(200).body(body);
            })
            .await;

        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let sitemap_url = Url::parse(&format!("http://{}/sitemap.xml", server.address())).unwrap();
        let urls = fetch_sitemap_urls(&http_client, &sitemap_url).await.unwrap();

        assert_eq!(
            urls,
            vec![
                Url::parse("https://example.com/a").unwrap(),
                Url::parse("https://example.com/b").unwrap(),
            ]
        );
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_sitemap_urls_follows_index() {
        let server = MockServer::start_async().await;
        let address = server.address().to_string();

        let index_body = format!(
            r#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>http://{}/sitemap-a.xml</loc></sitemap>
</sitemapindex>"#,
            address
        );
        let leaf_body = r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://example.com/leaf</loc></url>
</urlset>"#;

        let index_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sitemap.xml");
                then.status(200).body(index_body.clone());
            })
            .await;
        let leaf_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sitemap-a.xml");
                then.status(200).body(leaf_body);
            })
            .await;

        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let sitemap_url = Url::parse(&format!("http://{}/sitemap.xml", address)).unwrap();
        let urls = fetch_sitemap_urls(&http_client, &sitemap_url).await.unwrap();

        assert_eq!(urls, vec![Url::parse("https://example.com/leaf").unwrap()]);
        index_mock.assert_async().await;
        leaf_mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_sitemap_urls_detects_cycle() {
        let server = MockServer::start_async().await;
        let address = server.address().to_string();

        let cyclic_body = format!(
            r#"<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>http://{}/sitemap.xml</loc></sitemap>
</sitemapindex>"#,
            address
        );

        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sitemap.xml");
                then.status(200).body(cyclic_body.clone());
            })
            .await;

        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let sitemap_url = Url::parse(&format!("http://{}/sitemap.xml", address)).unwrap();
        let urls = fetch_sitemap_urls(&http_client, &sitemap_url).await.unwrap();

        assert!(urls.is_empty());
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn test_fetch_sitemap_urls_fetch_error() {
        let server = MockServer::start_async().await;
        let mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/sitemap.xml");
                then.status(404);
            })
            .await;

        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let sitemap_url = Url::parse(&format!("http://{}/sitemap.xml", server.address())).unwrap();
        let result = fetch_sitemap_urls(&http_client, &sitemap_url).await;

        assert!(result.is_err());
        mock.assert_async().await;
    }
}
