use crate::http_client::HttpClient;
use crate::robots::RobotsTxt;
use crossbeam::queue::SegQueue;
use dashmap::{DashMap, DashSet};
use futures::stream::StreamExt;
use miette::miette;
use mq_markdown::ConversionOptions;
use scraper::{Html, Selector};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::{fs, io};
use tokio::sync::RwLock;
use tokio::time::sleep;
use url::Url;

#[derive(Debug, Default)]
pub struct CrawlResult {
    pub start_time: Option<Instant>,
    pub end_time: Option<Instant>,
    pub pages_crawled: usize,
    pub pages_skipped_robots: usize,
    pub pages_failed: usize,
    pub links_discovered: usize,
    pub total_pages_visited: usize,
}

impl CrawlResult {
    pub fn duration(&self) -> Option<Duration> {
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            Some(end.duration_since(start))
        } else {
            None
        }
    }

    pub fn write_stats_to_stderr(&self) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();

        let _ = writeln!(handle, "\n=== Crawl Statistics ===");
        let _ = writeln!(handle, "Pages crawled successfully: {}", self.pages_crawled);
        let _ = writeln!(
            handle,
            "Pages skipped (robots.txt): {}",
            self.pages_skipped_robots
        );
        let _ = writeln!(handle, "Pages failed: {}", self.pages_failed);
        let _ = writeln!(handle, "Total pages visited: {}", self.total_pages_visited);
        let _ = writeln!(handle, "Links discovered: {}", self.links_discovered);

        if let Some(duration) = self.duration() {
            let _ = writeln!(handle, "Total duration: {:.2}s", duration.as_secs_f64());
        }
        let _ = writeln!(handle, "========================\n");
    }
}

// Helper function to sanitize filename components
fn sanitize_filename_component(component: &str, max_len: usize) -> String {
    let mut sanitized: String = component
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();

    if sanitized.is_empty() {
        sanitized.push_str("empty");
    }

    if sanitized.chars().count() > max_len {
        sanitized = sanitized.chars().take(max_len).collect();
    }
    sanitized
}

// Replace the placeholder extract_links function with this:
fn extract_links(html_content: &str, base_url: &Url) -> Vec<Url> {
    if html_content.is_empty() {
        return Vec::new();
    }

    let mut found_urls = Vec::new();
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
                    tracing::debug!(
                        "Failed to parse or join URL '{}' with base '{}': {}",
                        href_attr,
                        base_url,
                        e
                    );
                }
            }
        }
    }
    tracing::debug!("Extracted {} links from {}", found_urls.len(), base_url);
    found_urls
}

#[derive(Debug, Clone)]
pub struct Crawler {
    conversion_options: mq_markdown::ConversionOptions,
    crawl_delay: Duration,
    custom_robots_path: Option<String>, // Store custom robots path
    concurrency: usize,
    http_client: HttpClient,
    initial_domain: String, // To keep crawls within the starting domain
    mq_query: String,
    output_path: Option<String>,
    result: Arc<RwLock<CrawlResult>>,
    robots_cache: Arc<DashMap<String, Arc<RobotsTxt>>>,
    to_visit: Arc<SegQueue<Url>>,
    user_agent: String,
    visited: Arc<DashSet<Url>>,
}

impl Crawler {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        http_client: HttpClient,
        start_url: Url,
        crawl_delay_secs: f64,
        custom_robots_path: Option<String>, // Accept from CLI args
        mq_query: Option<String>,
        output_path: Option<String>,
        concurrency: usize,
        conversion_options: mq_markdown::ConversionOptions,
    ) -> Result<Self, String> {
        let initial_domain = start_url
            .domain()
            .ok_or_else(|| "Start URL has no domain".to_string())?
            .to_string();
        let user_agent = format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE"));

        let to_visit = SegQueue::new();
        to_visit.push(start_url.clone());

        Ok(Self {
            http_client,
            to_visit: Arc::new(to_visit),
            visited: Arc::new(DashSet::new()),
            robots_cache: Arc::new(DashMap::new()),
            crawl_delay: Duration::from_secs_f64(crawl_delay_secs),
            mq_query: mq_query.unwrap_or("identity()".to_string()),
            user_agent,
            output_path,
            initial_domain,
            custom_robots_path,
            result: Arc::new(RwLock::new(CrawlResult::default())),
            concurrency: concurrency.max(1),
            conversion_options,
        })
    }

    async fn get_or_fetch_robots(&self, url_to_check: &Url) -> Result<Arc<RobotsTxt>, String> {
        let domain = url_to_check
            .domain()
            .ok_or_else(|| "URL has no domain".to_string())?
            .to_string();

        // Check if we already have this domain's robots cached
        {
            if let Some(robots) = self.robots_cache.get(&domain) {
                return Ok(robots.clone());
            }
        }

        // Use the stored custom_robots_path
        let robots = RobotsTxt::fetch(
            &self.http_client,
            url_to_check,
            self.custom_robots_path.as_deref(),
        )
        .await?;
        let arc_robots = Arc::new(robots);

        // Cache the result
        self.robots_cache.insert(domain.clone(), arc_robots.clone());
        Ok(arc_robots)
    }

    pub async fn run(&mut self) -> Result<(), String> {
        // Record start time
        {
            let mut result = self.result.write().await;
            result.start_time = Some(Instant::now());
        }

        let mut startup_info = format!(
            "Crawler run initiated. User-Agent: '{}'. Initial domain: '{}'. Crawl delay: {:?}. Concurrency: {}.",
            self.user_agent, self.initial_domain, self.crawl_delay, self.concurrency
        );
        if let Some(ref path) = self.custom_robots_path {
            startup_info.push_str(&format!(" Custom robots.txt path: '{}'.", path));
        }
        if let Some(ref path) = self.output_path {
            startup_info.push_str(&format!(" Outputting to directory: '{}'.", path));
        } else {
            startup_info.push_str(" Outputting to stdout.");
        }
        tracing::info!("{}", startup_info);

        if let Some(ref output_dir_str) = self.output_path {
            let output_dir = Path::new(output_dir_str);
            if !output_dir.exists() {
                fs::create_dir_all(output_dir).map_err(|e| {
                    format!(
                        "Failed to create output directory '{}': {}",
                        output_dir_str, e
                    )
                })?;
                tracing::info!("Created output directory: {:?}", output_dir);
            } else if !output_dir.is_dir() {
                return Err(format!(
                    "Specified output path '{}' exists but is not a directory.",
                    output_dir_str
                ));
            }
        }

        self.run_parallel().await
    }

    async fn run_parallel(&mut self) -> Result<(), String> {
        while !self.to_visit.is_empty() {
            // Collect up to `concurrency` URLs for processing
            let mut urls_to_process = Vec::new();
            {
                while let Some(url) = self.to_visit.pop() {
                    urls_to_process.push(url);
                }
            }

            if urls_to_process.is_empty() {
                break;
            }

            // Filter out URLs that should be skipped and mark as visited atomically
            let mut valid_urls = Vec::with_capacity(self.concurrency);
            {
                for url in urls_to_process {
                    if !self.should_skip_url_without_visited_check(&url)
                        && self.visited.insert(url.clone())
                    {
                        valid_urls.push(url);
                    }
                }
            }

            let futures = valid_urls
                .into_iter()
                .map(|url| {
                    let crawler = self.clone();
                    tokio::task::spawn(async move {
                        crawler.process_url(url).await;
                    })
                })
                .collect::<Vec<_>>();

            futures::stream::iter(futures)
                .buffer_unordered(self.concurrency)
                .collect::<Vec<_>>()
                .await;

            // Apply crawl delay after processing batch
            sleep(self.crawl_delay).await;
        }

        self.finalize_crawl().await;
        Ok(())
    }

    fn should_skip_url_without_visited_check(&self, url: &Url) -> bool {
        // Check domain
        if url.domain().is_none_or(|d| d != self.initial_domain) {
            tracing::info!("Skipping URL from different domain: {}", url);
            return true;
        }

        false
    }

    async fn process_url(&self, current_url: Url) {
        tracing::info!("Processing URL: {}", current_url);

        // URL is already marked as visited in run_parallel, no need to check again
        let robots_rules = match self.get_or_fetch_robots(&current_url).await {
            Ok(rules) => rules,
            Err(e) => {
                tracing::error!("Failed to fetch robots.txt for {}: {}", current_url, e);
                return;
            }
        };

        if !robots_rules.is_allowed(&current_url, &self.user_agent) {
            tracing::warn!("Skipping URL disallowed by robots.txt: {}", current_url);
            {
                let mut result = self.result.write().await;
                result.pages_skipped_robots += 1;
            }
            return;
        }

        match self.http_client.fetch(current_url.clone()).await {
            Ok(html_content) => {
                tracing::info!("Applying mq query to content from {}", current_url);

                let query = self.mq_query.clone();
                let html_content_clone = html_content.clone();
                let conversion_options = self.conversion_options;
                let current_url_clone = current_url.clone();
                let self_clone = self.clone();
                let new_links = tokio::task::spawn_blocking(move || {
                    if let Err(e) =
                        Self::execute_query(&query, &html_content_clone, conversion_options).map(
                            |md| {
                                if let Err(e) =
                                    self_clone.output_markdown(&current_url_clone, md.as_str())
                                {
                                    tracing::error!(
                                        "Failed to output markdown for {}: {}",
                                        current_url_clone,
                                        e
                                    );
                                }
                            },
                        )
                    {
                        tracing::error!(
                            "Failed to execute mq query on content from {}: {}",
                            current_url_clone,
                            e
                        );
                    }

                    extract_links(&html_content, &current_url_clone)
                })
                .await;

                match new_links {
                    Ok(new_links) => {
                        {
                            let mut result = self.result.write().await;
                            result.links_discovered += new_links.len();
                            result.pages_crawled += 1;
                        }

                        {
                            for link in new_links {
                                if !self.visited.contains(&link)
                                    && link.domain().is_some_and(|d| d == self.initial_domain)
                                {
                                    self.to_visit.push(link);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!(
                            "Failed to execute mq query on content from {}: {}",
                            current_url,
                            e
                        );
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to fetch URL {}: {}", current_url, e);
                {
                    let mut result = self.result.write().await;
                    result.pages_failed += 1;
                }
            }
        }
    }

    fn execute_query(
        query: &str,
        input: &str,
        conversion_options: ConversionOptions,
    ) -> miette::Result<String> {
        let input = mq_lang::parse_html_input_with_options(input, conversion_options)?;
        let mut mq_engine = mq_lang::Engine::default();
        mq_engine.load_builtin_module();

        match mq_engine
            .eval(query, input.into_iter())
            .map_err(|e| miette!(format!("Error evaluating mq query: {}", e)))
        {
            Ok(values) => Ok(mq_markdown::Markdown::new(
                values
                    .into_iter()
                    .map(|value| match value {
                        mq_lang::Value::Markdown(node) => node.clone(),
                        _ => value.to_string().into(),
                    })
                    .collect(),
            )
            .to_string()),
            Err(e) => Err(miette!(e)),
        }
    }

    async fn finalize_crawl(&self) {
        // Record end time and final statistics
        {
            let mut result = self.result.write().await;
            result.end_time = Some(Instant::now());
            let visited_len = self.visited.len();
            result.total_pages_visited = visited_len;
        }

        // Write statistics to stderr
        {
            let result = self.result.read().await;
            result.write_stats_to_stderr();
        }
    }

    fn output_markdown(&self, url: &Url, markdown: &str) -> Result<(), String> {
        tracing::debug!("Preparing to output markdown for {}", url);
        if let Some(ref output_dir_str) = self.output_path {
            // Renamed path_str to output_dir_str
            let domain_str = url.domain().unwrap_or("unknown_domain");
            let path_str = url.path();

            // Sanitize domain and path components
            let sane_domain = sanitize_filename_component(domain_str, 50);

            let sane_path = if path_str == "/" || path_str.is_empty() {
                "index".to_string()
            } else {
                sanitize_filename_component(path_str.trim_matches('/'), 100)
            };

            let filename = format!("{}_{}.md", sane_domain, sane_path);
            let output_dir = Path::new(output_dir_str);
            let output_file_path = output_dir.join(&filename);
            tracing::info!("Saving markdown for {} to: {:?}", url, output_file_path);

            let mut file = fs::File::create(&output_file_path).map_err(|e| {
                format!(
                    "Failed to create output file '{:?}': {}",
                    output_file_path, e
                )
            })?;
            file.write_all(markdown.as_bytes()).map_err(|e| {
                format!(
                    "Failed to write markdown to file '{:?}': {}",
                    output_file_path, e
                )
            })?;
            tracing::debug!(
                "Successfully wrote {} bytes to {:?}",
                markdown.len(),
                output_file_path
            );
        } else {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            writeln!(
                handle,
                "Filename: {}_{}.md --",
                sanitize_filename_component(url.domain().unwrap_or("unknown_domain"), 50),
                if url.path() == "/" || url.path().is_empty() {
                    "index".to_string()
                } else {
                    sanitize_filename_component(url.path().trim_matches('/'), 100)
                }
            )
            .map_err(|e| format!("Failed to write to stdout: {}", e))?;
            handle
                .write_all(markdown.as_bytes())
                .map_err(|e| format!("Failed to write markdown to stdout: {}", e))?;
            writeln!(handle).map_err(|e| format!("Failed to write newline to stdout: {}", e))?;
            handle
                .flush()
                .map_err(|e| format!("Failed to flush stdout after writing markdown: {}", e))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;
    use url::Url;

    #[rstest]
    #[case(
        "<html><body><p>No links here.</p></body></html>",
        "http://example.com",
        vec![]
    )]
    #[case(
        r#"<html><body><a href="http://example.com/page1">Page 1</a></body></html>"#,
        "http://example.com",
        vec!["http://example.com/page1"]
    )]
    #[case(
        r#"<html><body><a href="/page2">Page 2</a></body></html>"#,
        "http://example.com/path/",
        vec!["http://example.com/page2"]
    )]
    #[case(
        r#"<html><body><a href="page3#section">Page 3</a></body></html>"#,
        "http://example.com/",
        vec!["http://example.com/page3"]
    )]
    #[case(
        r##"
            <html><body>
                <a href="https://othersite.com/abs">Absolute</a>
                <a href="relative/link">Relative</a>
                <a href="../another">Another Relative</a>
                <a href="#fragmentonly">Fragment Only</a>
                <a href="page?query=val">With Query</a>
            </body></html>
        "##,
        "http://example.com/folder1/folder2/current.html",
        vec![
            "https://othersite.com/abs",
            "http://example.com/folder1/folder2/relative/link",
            "http://example.com/folder1/another",
            "http://example.com/folder1/folder2/current.html",
            "http://example.com/folder1/folder2/page?query=val"
        ]
    )]
    #[case(
        r#"<html><body><a href="">Empty Href</a></body></html>"#,
        "http://example.com/page.html",
        vec!["http://example.com/page.html"]
    )]
    #[case(
        r#"<html><body><a href="http://[::1]:namedport">Malformed</a></body></html>"#,
        "http://example.com",
        vec![]
    )]
    #[case(
        "",
        "http://example.com",
        vec![]
    )]
    fn test_extract_links(
        #[case] html: &str,
        #[case] base_url: &str,
        #[case] expected_urls: Vec<&str>,
    ) {
        let base = Url::parse(base_url).unwrap();
        let links = extract_links(html, &base);

        assert_eq!(
            links.len(),
            expected_urls.len(),
            "Link count mismatch for base_url: {}",
            base_url
        );

        for expected_url_str in expected_urls {
            let expected_url = Url::parse(expected_url_str).unwrap();
            assert!(
                links.contains(&expected_url),
                "Expected link {} not found in {:?}",
                expected_url_str,
                links
            );
        }
    }

    #[rstest]
    #[case("abcDEF-123_foo", 20, "abcDEF-123_foo")]
    #[case("!@#abc/\\:*?\"<>|", 20, "abc")]
    #[case("", 10, "empty")]
    #[case("valid_name", 5, "valid")]
    #[case("___", 10, "___")]
    #[case("foo bar-baz_123", 20, "foobar-baz_123")]
    #[case("日本語テスト", 10, "日本語テスト")]
    fn test_sanitize_filename_component(
        #[case] input: &str,
        #[case] max_len: usize,
        #[case] expected: &str,
    ) {
        let result = sanitize_filename_component(input, max_len);
        assert_eq!(result, expected, "input: {}", input);
    }

    #[test]
    fn test_crawl_result_duration_some() {
        let start = Instant::now();
        let end = start + Duration::from_secs(5);
        let result = CrawlResult {
            start_time: Some(start),
            end_time: Some(end),
            ..Default::default()
        };
        assert_eq!(result.duration(), Some(Duration::from_secs(5)));
    }

    #[test]
    fn test_crawl_result_duration_none() {
        let result = CrawlResult {
            start_time: None,
            end_time: None,
            ..Default::default()
        };
        assert_eq!(result.duration(), None);

        let result = CrawlResult {
            start_time: Some(Instant::now()),
            end_time: None,
            ..Default::default()
        };
        assert_eq!(result.duration(), None);

        let result = CrawlResult {
            start_time: None,
            end_time: Some(Instant::now()),
            ..Default::default()
        };
        assert_eq!(result.duration(), None);
    }
}
