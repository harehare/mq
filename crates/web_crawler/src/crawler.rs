use crate::robots::RobotsTxt;
use html2md;
use mq_lang::{Engine as MqEngine, Value as MqValue}; // Removed Error as MqError
use reqwest::Client;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use url::Url;
use scraper::{Html, Selector};

// Helper function to sanitize filename components
fn sanitize_filename_component(component: &str, max_len: usize) -> String {
    let mut sanitized: String = component
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    if sanitized.is_empty() {
        sanitized.push_str("empty");
    }

    // Truncate if necessary
    if sanitized.len() > max_len {
        sanitized.truncate(max_len);
    }
    sanitized
}

// Replace the placeholder extract_links function with this:
fn extract_links(html_content: &str, base_url: &Url) -> Vec<Url> {
    let mut found_urls = Vec::new();
    if html_content.is_empty() {
        return found_urls;
    }

    let document = Html::parse_document(html_content);
    let link_selector = Selector::parse("a[href]").expect("Failed to parse 'a[href]' selector"); // Should not panic with valid selector

    for element in document.select(&link_selector) {
        if let Some(href_attr) = element.value().attr("href") {
            match base_url.join(href_attr) {
                Ok(mut new_url) => {
                    // Remove fragment, if any
                    new_url.set_fragment(None);
                    found_urls.push(new_url);
                }
                Err(e) => {
                    tracing::debug!("Failed to parse or join URL '{}' with base '{}': {}", href_attr, base_url, e);
                }
            }
        }
    }
    tracing::debug!("Extracted {} links from {}", found_urls.len(), base_url);
    found_urls
}

pub struct Crawler {
    client: Client,
    to_visit: VecDeque<Url>,
    visited: HashSet<Url>,
    robots_cache: HashMap<String, Arc<RobotsTxt>>,
    crawl_delay: Duration,
    mq_engine: Option<MqEngine>,
    mq_script_content: Option<String>,
    user_agent: String,
    output_path: Option<String>,
    initial_domain: String, // To keep crawls within the starting domain
    custom_robots_path: Option<String>, // Store custom robots path
}

impl Crawler {
    pub async fn new(
        start_url: Url,
        crawl_delay_secs: f64,
        custom_robots_path: Option<String>, // Accept from CLI args
        mq_script_path: Option<String>,
        output_path: Option<String>,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent(format!("MySimpleCrawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE")))
            .build()
            .map_err(|e| format!("Failed to build reqwest client: {}", e))?;

        let initial_domain = start_url.domain().ok_or_else(|| "Start URL has no domain".to_string())?.to_string();
        let user_agent = format!("MySimpleCrawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE"));

        let mut to_visit = VecDeque::new();
        to_visit.push_back(start_url.clone());

        let mq_engine;
        let mq_script_content;

        if let Some(script_path_str) = mq_script_path { // Changed from mq_script_path to mq_script_path_str for clarity
            let script_content_str = fs::read_to_string(&script_path_str) // Renamed script to script_content_str
                .map_err(|e| format!("Failed to read mq_lang script from {}: {}", script_path_str, e))?;
            tracing::info!("Successfully loaded mq_lang script from: {}", script_path_str); // Added log
            let engine = MqEngine::default();
            mq_engine = Some(engine);
            mq_script_content = Some(script_content_str);
        } else {
            tracing::info!("No mq_lang script provided."); // Added log
            mq_engine = None;
            mq_script_content = None;
        }

        Ok(Self {
            client,
            to_visit,
            visited: HashSet::new(),
            robots_cache: HashMap::new(),
            crawl_delay: Duration::from_secs_f64(crawl_delay_secs),
            mq_engine,
            mq_script_content,
            user_agent,
            output_path,
            initial_domain,
            custom_robots_path, // Store it
        })
    }

    async fn get_or_fetch_robots(&mut self, url_to_check: &Url) -> Result<Arc<RobotsTxt>, String> {
        let domain = url_to_check.domain().ok_or_else(|| "URL has no domain".to_string())?.to_string();
        if let Some(robots) = self.robots_cache.get(&domain) {
            return Ok(robots.clone());
        }

        // Use the stored custom_robots_path
        let robots = RobotsTxt::fetch(&self.client, url_to_check, self.custom_robots_path.as_deref()).await?;
        let arc_robots = Arc::new(robots);
        self.robots_cache.insert(domain.clone(), arc_robots.clone());
        Ok(arc_robots)
    }

    pub async fn run(&mut self) -> Result<(), String> {
        let mut startup_info = format!(
            "Crawler run initiated. User-Agent: '{}'. Initial domain: '{}'. Crawl delay: {:?}.",
            self.user_agent, self.initial_domain, self.crawl_delay
        );
        if let Some(ref path) = self.custom_robots_path {
            startup_info.push_str(&format!(" Custom robots.txt path: '{}'.", path));
        }
        if self.mq_script_content.is_some() {
            startup_info.push_str(&format!(" Using mq_lang script ({} bytes).", self.mq_script_content.as_ref().unwrap().len()));
        }
        if let Some(ref path) = self.output_path {
            startup_info.push_str(&format!(" Outputting to directory: '{}'.", path));
        } else {
            startup_info.push_str(" Outputting to stdout.");
        }
        tracing::info!("{}", startup_info);

        while let Some(current_url) = self.to_visit.pop_front() {
            if self.visited.contains(&current_url) {
                tracing::debug!("Skipping already visited URL: {}", current_url);
                continue;
            }

            if current_url.domain().map_or(true, |d| d != self.initial_domain) {
                tracing::info!("Skipping URL from different domain: {}", current_url);
                continue;
            }

            tracing::info!("Processing URL: {}", current_url);

            let robots_rules = self.get_or_fetch_robots(&current_url).await?;

            if !robots_rules.is_allowed(&current_url, &self.user_agent) {
                tracing::warn!("Skipping URL disallowed by robots.txt: {}", current_url);
                self.visited.insert(current_url);
                continue;
            }

            match self.client.get(current_url.clone()).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let html_content = response.text().await.map_err(|e| format!("Failed to read response text: {}", e))?;

                        let mut markdown = html2md::parse_html(&html_content);
                        tracing::debug!("Converted HTML to Markdown (first 100 chars): {:.100}", markdown.chars().take(100).collect::<String>());

                        if let (Some(engine), Some(script_content_str)) = (&mut self.mq_engine, &self.mq_script_content) { // Renamed script to script_content_str
                            tracing::info!("Applying mq_lang script to content from {}", current_url); // Enhanced log

                            let eval_input = markdown.clone(); // Prepare input separately for clarity
                            match engine.eval(script_content_str, std::iter::once(MqValue::from(eval_input))) {
                                Ok(processed_values) => { // processed_values is mq_lang::Values
                                    let mut iter = processed_values.into_iter();
                                    if let Some(first_value) = iter.next() {
                                        markdown = first_value.to_string();
                                        tracing::info!("Successfully applied mq_lang script. Markdown (first 100 chars) from {}: {:.100}", current_url, markdown.chars().take(100).collect::<String>());
                                        if iter.next().is_some() { // Check if there were more values
                                            tracing::warn!("mq_lang script produced multiple output values. Only the first one was used for markdown content from {}.", current_url);
                                        }
                                    } else {
                                        tracing::warn!("mq_lang script produced no output values. Original markdown from {} will be used.", current_url);
                                    }
                                }
                                Err(e) => {
                                    let error_string = format!("{:?}", e);
                                    tracing::error!("Error running mq_lang script on content from {}: {}. Original markdown will be used.", current_url, error_string.chars().take(200).collect::<String>());
                                }
                            }
                        }

                        self.output_markdown(&current_url, &markdown)?;

                        let new_links = extract_links(&html_content, &current_url);
                        for link in new_links {
                            if !self.visited.contains(&link) && !self.to_visit.contains(&link) {
                                 if link.domain().map_or(false, |d| d == self.initial_domain) {
                                    self.to_visit.push_back(link);
                                 }
                            }
                        }

                    } else {
                        tracing::warn!("Request to {} failed with status: {}", current_url, response.status());
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to fetch URL {}: {}", current_url, e);
                }
            }

            self.visited.insert(current_url);
            tokio::time::sleep(self.crawl_delay).await;
        }
        Ok(())
    }

    fn output_markdown(&self, url: &Url, markdown: &str) -> Result<(), String> {
        tracing::debug!("Preparing to output markdown for {}", url); // Changed from info to debug for this line
        if let Some(ref output_dir_str) = self.output_path { // Renamed path_str to output_dir_str
            let domain_str = url.domain().unwrap_or("unknown_domain");
            let path_str = url.path();

            // Sanitize domain and path components
            let sane_domain = sanitize_filename_component(domain_str, 50); // Max 50 chars for domain part

            let sane_path = if path_str == "/" || path_str.is_empty() {
                "index".to_string()
            } else {
                sanitize_filename_component(path_str.trim_matches('/'), 100) // Max 100 chars for path part
            };

            let filename = format!("{}_{}.md", sane_domain, sane_path);

            let output_dir = Path::new(output_dir_str);
            if !output_dir.exists() {
                fs::create_dir_all(&output_dir)
                    .map_err(|e| format!("Failed to create output directory '{}': {}", output_dir_str, e))?;
                tracing::info!("Created output directory: {:?}", output_dir);
            } else if !output_dir.is_dir() {
                return Err(format!("Specified output path '{}' exists but is not a directory.", output_dir_str));
            }

            let output_file_path = output_dir.join(&filename);
            tracing::info!("Saving markdown for {} to: {:?}", url, output_file_path);

            let mut file = fs::File::create(&output_file_path)
                .map_err(|e| format!("Failed to create output file '{:?}': {}", output_file_path, e))?;
            file.write_all(markdown.as_bytes())
                .map_err(|e| format!("Failed to write markdown to file '{:?}': {}", output_file_path, e))?;
            tracing::debug!("Successfully wrote {} bytes to {:?}", markdown.len(), output_file_path);

        } else {
            // Print to stdout
            println!("\n\n--- Markdown for: {} ---\n", url);
            println!("{}", markdown);
            println!("--- End of Markdown for: {} ---\n\n", url);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn test_extract_no_links() {
        let html = "<html><body><p>No links here.</p></body></html>";
        let base = Url::parse("http://example.com").unwrap();
        let links = extract_links(html, &base);
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_simple_link() {
        let html = r#"<html><body><a href="http://example.com/page1">Page 1</a></body></html>"#;
        let base = Url::parse("http://example.com").unwrap();
        let links = extract_links(html, &base);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].as_str(), "http://example.com/page1");
    }

    #[test]
    fn test_extract_relative_link() {
        let html = r#"<html><body><a href="/page2">Page 2</a></body></html>"#;
        let base = Url::parse("http://example.com/path/").unwrap();
        let links = extract_links(html, &base);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].as_str(), "http://example.com/page2");
    }

    #[test]
    fn test_extract_link_with_fragment() {
        let html = r#"<html><body><a href="page3#section">Page 3</a></body></html>"#;
        let base = Url::parse("http://example.com/").unwrap();
        let links = extract_links(html, &base);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].as_str(), "http://example.com/page3"); // Fragment should be removed
    }

    #[test]
    fn test_extract_multiple_links() {
        let html = r##"
            <html><body>
                <a href="https://othersite.com/abs">Absolute</a>
                <a href="relative/link">Relative</a>
                <a href="../another">Another Relative</a>
                <a href="#fragmentonly">Fragment Only</a>
                <a href="page?query=val">With Query</a>
            </body></html>
        "##;
        let base = Url::parse("http://example.com/folder1/folder2/current.html").unwrap();
        let links = extract_links(html, &base);

        let expected_urls = vec![
            "https://othersite.com/abs",
            "http://example.com/folder1/folder2/relative/link",
            "http://example.com/folder1/another",
            "http://example.com/folder1/folder2/current.html", // #fragmentonly resolves to base
            "http://example.com/folder1/folder2/page?query=val",
        ];

        assert_eq!(links.len(), expected_urls.len());
        for expected_url_str in expected_urls {
            let expected_url = Url::parse(expected_url_str).unwrap();
            assert!(links.contains(&expected_url), "Expected link {} not found in {:?}", expected_url_str, links);
        }
    }

     #[test]
    fn test_extract_empty_href() {
        let html = r#"<html><body><a href="">Empty Href</a></body></html>"#;
        let base = Url::parse("http://example.com/page.html").unwrap();
        let links = extract_links(html, &base);
        assert_eq!(links.len(), 1);
        // Empty href resolves to the base URL itself
        assert_eq!(links[0].as_str(), "http://example.com/page.html");
    }

    #[test]
    fn test_malformed_url() {
        let html = r#"<html><body><a href="http://[::1]:namedport">Malformed</a></body></html>"#;
        let base = Url::parse("http://example.com").unwrap();
        let links = extract_links(html, &base);
        // url::Url::join will fail for "http://[::1]:namedport" as it's not a valid relative path part if base is http
        // and `Url::parse("http://[::1]:namedport")` itself fails.
        // If `base_url.join(href_attr)` returns Err, it's logged and skipped.
        assert!(links.is_empty());
    }

    #[test]
    fn test_empty_html_input() {
        let html = "";
        let base = Url::parse("http://example.com").unwrap();
        let links = extract_links(html, &base);
        assert!(links.is_empty());
    }
}
