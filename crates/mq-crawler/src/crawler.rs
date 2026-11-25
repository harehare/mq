use crate::http_client::{FetchErrorKind, FetchOptions, HttpClient};
use crate::robots::{RobotsTxt, origin_key};
use crossbeam::queue::SegQueue;
use dashmap::mapref::entry::Entry;
use dashmap::{DashMap, DashSet};
use miette::miette;
use mq_markdown::ConversionOptions;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
use std::{fs, io};
use tokio::sync::{Notify, RwLock, Semaphore};
use tokio::time::sleep;
use url::Url;

#[derive(Clone, Debug, Default)]
/// Format of the crawl statistics written to stderr.
pub enum OutputFormat {
    #[default]
    /// Human-readable text.
    Text,
    /// JSON.
    Json,
}

#[derive(Debug, Default)]
/// Counters and timing collected during a crawl.
pub struct CrawlResult {
    /// When the crawl started.
    pub start_time: Option<Instant>,
    /// When the crawl finished.
    pub end_time: Option<Instant>,
    /// Pages fetched and processed successfully.
    pub pages_crawled: usize,
    /// Pages skipped because robots.txt disallows them.
    pub pages_skipped_robots: usize,
    /// Pages skipped because the response was not HTML.
    pub pages_skipped_non_html: usize,
    /// URLs skipped without a request because of a non-HTML file extension.
    pub pages_skipped_extension: usize,
    /// Pages that failed to fetch or process.
    pub pages_failed: usize,
    /// Links found across all pages.
    pub links_discovered: usize,
    /// Total pages visited.
    pub total_pages_visited: usize,
    /// First error message that made a page fail, kept for the final diagnostic.
    pub first_failure: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
/// Serializable summary of a [`CrawlResult`].
pub struct CrawlResultStats {
    /// Pages fetched and processed successfully.
    pub pages_crawled: usize,
    /// Pages skipped because robots.txt disallows them.
    pub pages_skipped_robots: usize,
    /// Pages skipped because the response was not HTML.
    #[serde(default)]
    pub pages_skipped_non_html: usize,
    /// URLs skipped without a request because of a non-HTML file extension.
    #[serde(default)]
    pub pages_skipped_extension: usize,
    /// Pages that failed to fetch or process.
    pub pages_failed: usize,
    /// Links found across all pages.
    pub links_discovered: usize,
    /// Total pages visited.
    pub total_pages_visited: usize,
    /// Crawl duration in seconds.
    pub duration_secs: Option<f64>,
}

impl CrawlResult {
    /// Counts a failed page and keeps the first error message.
    fn record_failure(&mut self, message: String) {
        self.pages_failed += 1;
        self.first_failure.get_or_insert(message);
    }

    /// Elapsed time, if the crawl has started and finished.
    pub fn duration(&self) -> Option<Duration> {
        if let (Some(start), Some(end)) = (self.start_time, self.end_time) {
            Some(end.duration_since(start))
        } else {
            None
        }
    }

    /// Converts to the serializable statistics form.
    pub fn to_stats(&self) -> CrawlResultStats {
        CrawlResultStats {
            pages_crawled: self.pages_crawled,
            pages_skipped_robots: self.pages_skipped_robots,
            pages_skipped_non_html: self.pages_skipped_non_html,
            pages_skipped_extension: self.pages_skipped_extension,
            pages_failed: self.pages_failed,
            links_discovered: self.links_discovered,
            total_pages_visited: self.total_pages_visited,
            duration_secs: self.duration().map(|d| d.as_secs_f64()),
        }
    }

    /// Writes the statistics to stderr in `format`.
    pub fn write_stats_to_stderr(&self, format: &OutputFormat) {
        let stderr = io::stderr();
        let mut handle = stderr.lock();

        match format {
            OutputFormat::Text => {
                let _ = writeln!(handle, "\n=== Crawl Statistics ===");
                let _ = writeln!(handle, "Pages crawled successfully: {}", self.pages_crawled);
                let _ = writeln!(handle, "Pages skipped (robots.txt): {}", self.pages_skipped_robots);
                let _ = writeln!(handle, "Pages skipped (non-HTML): {}", self.pages_skipped_non_html);
                let _ = writeln!(handle, "URLs skipped (extension): {}", self.pages_skipped_extension);
                let _ = writeln!(handle, "Pages failed: {}", self.pages_failed);
                let _ = writeln!(handle, "Total pages visited: {}", self.total_pages_visited);
                let _ = writeln!(handle, "Links discovered: {}", self.links_discovered);

                if let Some(duration) = self.duration() {
                    let _ = writeln!(handle, "Total duration: {:.2}s", duration.as_secs_f64());
                }
                let _ = writeln!(handle, "========================\n");
            }
            OutputFormat::Json => {
                let stats = self.to_stats();
                if let Ok(json) = serde_json::to_string_pretty(&stats) {
                    let _ = writeln!(handle, "{}", json);
                }
            }
        }
    }
}

/// A snapshot of crawl progress that can be written to disk and later
/// reloaded to resume an interrupted crawl via `--resume-from`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CrawlCheckpoint {
    /// URLs that have already been visited (queued, in-flight, or completed).
    pub visited: Vec<Url>,
    /// URLs still pending, paired with their crawl depth.
    pub frontier: Vec<(Url, usize)>,
    /// Cumulative statistics from the crawl(s) that produced this checkpoint.
    pub stats: CrawlResultStats,
}

impl CrawlCheckpoint {
    /// Loads a checkpoint previously written by `Crawler::save_checkpoint`.
    pub fn load(path: &str) -> Result<Self, String> {
        let data = fs::read_to_string(path).map_err(|e| format!("Failed to read checkpoint file '{}': {}", path, e))?;
        serde_json::from_str(&data).map_err(|e| format!("Failed to parse checkpoint file '{}': {}", path, e))
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
/// Extensions that are never HTML, so the URL is not requested at all.
const NON_HTML_EXTENSIONS: &[&str] = &[
    "7z", "aac", "apk", "avi", "bin", "bmp", "bz2", "css", "csv", "dmg", "doc", "docx", "eot", "epub", "exe", "flac",
    "gif", "gz", "ico", "iso", "jpeg", "jpg", "js", "json", "m4a", "mkv", "mov", "mp3", "mp4", "ogg", "otf", "pdf",
    "png", "ppt", "pptx", "rar", "svg", "tar", "tgz", "ttf", "wav", "webm", "webp", "woff", "woff2", "xls", "xlsx",
    "xml", "xz", "zip",
];

fn has_non_html_extension(url: &Url) -> bool {
    let Some(name) = url.path().rsplit('/').next() else {
        return false;
    };
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| NON_HTML_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)))
}

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

/// Returns whether `url`'s path and query pass the include/exclude globs.
/// An empty include list admits every URL; exclude always wins.
fn matches_path_filters(include: &[glob::Pattern], exclude: &[glob::Pattern], url: &Url) -> bool {
    let target = match url.query() {
        Some(q) => format!("{}?{}", url.path(), q),
        None => url.path().to_string(),
    };
    (include.is_empty() || include.iter().any(|p| p.matches(&target))) && !exclude.iter().any(|p| p.matches(&target))
}

#[derive(Debug, Clone)]
/// Crawls pages from a start URL and applies an mq query to each page.
pub struct Crawler {
    allowed_domains: Option<Vec<String>>,
    checkpoint_interval_pages: usize,
    checkpoint_path: Option<String>,
    conversion_options: mq_markdown::ConversionOptions,
    crawl_delay: Duration,
    custom_robots_path: Option<String>,
    concurrency: usize,
    depth_limit: Option<usize>,
    domain_last_request: Arc<DashMap<String, Instant>>,
    exclude_patterns: Vec<glob::Pattern>,
    format: OutputFormat,
    suppress_stats: bool,
    http_client: HttpClient,
    include_patterns: Vec<glob::Pattern>,
    initial_domain: String,
    map: bool,
    max_pages: Option<usize>,
    max_response_bytes: Option<usize>,
    skip_extensions: bool,
    start_url: Url,
    mq_query: String,
    output_path: Option<String>,
    result: Arc<RwLock<CrawlResult>>,
    robots_cache: Arc<DashMap<String, Arc<RobotsTxt>>>,
    to_visit: Arc<SegQueue<(Url, usize)>>,
    notify: Arc<Notify>,
    user_agent: String,
    visited: Arc<DashSet<Url>>,
}

/// Settings for [`Crawler::new`].
#[derive(Debug, Default)]
pub struct CrawlerOptions {
    /// Delay between requests to the same domain, in seconds.
    pub crawl_delay_secs: f64,
    /// Path of a robots.txt file to use instead of fetching one.
    pub custom_robots_path: Option<String>,
    /// mq query applied to each page. Defaults to `identity()`.
    pub mq_query: Option<String>,
    /// Directory to write converted pages to. Pages go to stdout when unset.
    pub output_path: Option<String>,
    /// Maximum number of concurrent requests. Values below 1 are treated as 1.
    pub concurrency: usize,
    /// Format of the crawl statistics written to stderr.
    pub format: OutputFormat,
    /// HTML to Markdown conversion options.
    pub conversion_options: mq_markdown::ConversionOptions,
    /// Maximum link depth from the start URL. Unlimited when unset.
    pub depth_limit: Option<usize>,
    /// Domains the crawl may visit: the start domain only when unset, any domain when empty.
    pub allowed_domains: Option<Vec<String>>,
    /// Glob patterns matched against each URL's path and query. When non-empty, only matching
    /// URLs are queued. The start URL is always crawled.
    pub include: Vec<String>,
    /// Glob patterns for URLs that are never queued. Takes precedence over `include`.
    pub exclude: Vec<String>,
    /// Extra URLs queued at depth 0 next to the start URL.
    pub additional_seed_urls: Vec<Url>,
    /// Only list the URLs reached, one per line, without converting pages or running the query.
    /// Written to `urls.txt` in `output_path` when set, otherwise to stdout.
    pub map: bool,
    /// Maximum number of pages to crawl. Unlimited when unset.
    pub max_pages: Option<usize>,
    /// Largest page body accepted, in bytes. Unlimited when unset.
    pub max_response_bytes: Option<usize>,
    /// Skip URLs with a known non-HTML extension without requesting them.
    pub skip_extensions: bool,
    /// File the crawl progress is periodically saved to.
    pub checkpoint_path: Option<String>,
    /// Pages crawled between checkpoint saves. Values below 1 are treated as 1.
    pub checkpoint_interval_pages: usize,
    /// Checkpoint to continue from instead of starting at `start_url`.
    pub resume_checkpoint: Option<CrawlCheckpoint>,
    /// Do not write the crawl statistics to stderr.
    pub suppress_stats: bool,
}

impl Crawler {
    /// Creates a crawler for `start_url`.
    pub async fn new(http_client: HttpClient, start_url: Url, options: CrawlerOptions) -> Result<Self, String> {
        let CrawlerOptions {
            crawl_delay_secs,
            custom_robots_path,
            mq_query,
            output_path,
            concurrency,
            format,
            conversion_options,
            depth_limit,
            allowed_domains,
            include,
            exclude,
            additional_seed_urls,
            map,
            max_pages,
            max_response_bytes,
            skip_extensions,
            checkpoint_path,
            checkpoint_interval_pages,
            resume_checkpoint,
            suppress_stats,
        } = options;
        let parse_patterns = |patterns: Vec<String>| {
            patterns
                .iter()
                .map(|p| glob::Pattern::new(p).map_err(|e| format!("Invalid glob pattern '{}': {}", p, e)))
                .collect::<Result<Vec<_>, _>>()
        };
        let include_patterns = parse_patterns(include)?;
        let exclude_patterns = parse_patterns(exclude)?;
        let initial_domain = start_url
            .host_str()
            .ok_or_else(|| "Start URL has no host".to_string())?
            .to_string();
        let user_agent = format!("mq crawler/0.1 ({})", env!("CARGO_PKG_HOMEPAGE"));

        let to_visit = SegQueue::new();
        let visited = DashSet::new();
        let mut initial_result = CrawlResult::default();

        if let Some(checkpoint) = resume_checkpoint {
            tracing::info!(
                "Resuming crawl from checkpoint: {} visited, {} queued.",
                checkpoint.visited.len(),
                checkpoint.frontier.len()
            );
            for url in checkpoint.visited {
                visited.insert(url);
            }
            for (url, depth) in checkpoint.frontier {
                if url != start_url && !matches_path_filters(&include_patterns, &exclude_patterns, &url) {
                    continue;
                }
                to_visit.push((url, depth));
            }
            initial_result.pages_crawled = checkpoint.stats.pages_crawled;
            initial_result.pages_skipped_robots = checkpoint.stats.pages_skipped_robots;
            initial_result.pages_skipped_non_html = checkpoint.stats.pages_skipped_non_html;
            initial_result.pages_skipped_extension = checkpoint.stats.pages_skipped_extension;
            initial_result.pages_failed = checkpoint.stats.pages_failed;
            initial_result.links_discovered = checkpoint.stats.links_discovered;
        } else {
            to_visit.push((start_url.clone(), 0));
            for seed_url in additional_seed_urls {
                if !matches_path_filters(&include_patterns, &exclude_patterns, &seed_url) {
                    continue;
                }
                to_visit.push((seed_url, 0));
            }
        }

        Ok(Self {
            allowed_domains,
            exclude_patterns,
            include_patterns,
            checkpoint_interval_pages: checkpoint_interval_pages.max(1),
            checkpoint_path,
            http_client,
            to_visit: Arc::new(to_visit),
            visited: Arc::new(visited),
            robots_cache: Arc::new(DashMap::new()),
            crawl_delay: Duration::from_secs_f64(crawl_delay_secs),
            domain_last_request: Arc::new(DashMap::new()),
            map,
            max_pages,
            max_response_bytes,
            skip_extensions,
            start_url,
            mq_query: mq_query.unwrap_or("identity()".to_string()),
            user_agent,
            output_path,
            initial_domain,
            custom_robots_path,
            result: Arc::new(RwLock::new(initial_result)),
            notify: Arc::new(Notify::new()),
            concurrency: concurrency.max(1),
            format,
            suppress_stats,
            conversion_options,
            depth_limit,
        })
    }

    async fn get_or_fetch_robots(&self, url_to_check: &Url) -> Result<Arc<RobotsTxt>, String> {
        let origin = origin_key(url_to_check).ok_or_else(|| "URL has no host".to_string())?;

        // Check if we already have this origin's robots cached
        {
            if let Some(robots) = self.robots_cache.get(&origin) {
                return Ok(robots.clone());
            }
        }

        // Use the stored custom_robots_path
        let robots = RobotsTxt::fetch(&self.http_client, url_to_check, self.custom_robots_path.as_deref()).await?;
        let arc_robots = Arc::new(robots);

        // Cache the result
        self.robots_cache.insert(origin, arc_robots.clone());
        Ok(arc_robots)
    }

    /// Statistics for the crawl so far. Call after [`Crawler::run`] for the final counts.
    pub async fn stats(&self) -> CrawlResultStats {
        self.result.read().await.to_stats()
    }

    /// The first error that made a page fail, if any.
    pub async fn first_failure(&self) -> Option<String> {
        self.result.read().await.first_failure.clone()
    }

    /// Runs the crawl to completion.
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
                fs::create_dir_all(output_dir)
                    .map_err(|e| format!("Failed to create output directory '{}': {}", output_dir_str, e))?;
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
        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let active_tasks = Arc::new(AtomicUsize::new(0));
        let mut pages_at_last_checkpoint = self.result.read().await.pages_crawled;

        loop {
            let max_pages_reached = self.max_pages.is_some_and(|max| self.visited.len() >= max);

            // Termination condition: no tasks in flight, and either the queue is
            // empty or we have hit the --max-pages limit.
            if (self.to_visit.is_empty() || max_pages_reached) && active_tasks.load(Ordering::SeqCst) == 0 {
                break;
            }

            // Register interest in the next notification *before* draining the
            // queue so we cannot miss a wakeup that arrives while we are spawning.
            let notified = self.notify.notified();

            // Drain the queue, spawning tasks up to the semaphore limit
            if !max_pages_reached {
                while let Some((url, depth)) = self.to_visit.pop() {
                    if self.should_skip_url_without_visited_check(&url) || !self.visited.insert(url.clone()) {
                        continue;
                    }

                    let permit = semaphore.clone().acquire_owned().await.expect("Semaphore closed");
                    active_tasks.fetch_add(1, Ordering::SeqCst);

                    let crawler = self.clone();
                    let active_tasks_clone = active_tasks.clone();
                    let notify_clone = self.notify.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        crawler.process_url_with_rate_limit(url, depth).await;
                        active_tasks_clone.fetch_sub(1, Ordering::SeqCst);
                        // Wake the main loop so it can either spawn new tasks or
                        // detect the termination condition.
                        notify_clone.notify_one();
                    });

                    if self.max_pages.is_some_and(|max| self.visited.len() >= max) {
                        tracing::info!(
                            "Reached --max-pages limit ({}); no further URLs will be queued.",
                            self.max_pages.expect("checked by is_some_and above")
                        );
                        break;
                    }
                }
            }

            self.maybe_save_checkpoint(&mut pages_at_last_checkpoint).await;

            // The drain may have discarded every remaining entry (already visited
            // or skipped) without spawning, so no worker is left to notify us.
            if active_tasks.load(Ordering::SeqCst) == 0
                && (self.to_visit.is_empty() || self.max_pages.is_some_and(|max| self.visited.len() >= max))
            {
                break;
            }

            // Block until a task finishes or a new URL is enqueued.
            // If notify_one() was already called since we created `notified`
            // above, this returns immediately without spinning.
            notified.await;
        }

        let mut checkpoint_error = None;
        if self.checkpoint_path.is_some()
            && let Err(e) = self.save_checkpoint().await
        {
            tracing::error!("Failed to save final checkpoint: {}", e);
            checkpoint_error = Some(format!("Failed to save final checkpoint: {}", e));
        }

        self.finalize_crawl().await;
        checkpoint_error.map_or(Ok(()), Err)
    }

    /// Saves a checkpoint if `--checkpoint-path` is set and at least
    /// `checkpoint_interval_pages` pages have been crawled since the last save.
    async fn maybe_save_checkpoint(&self, pages_at_last_checkpoint: &mut usize) {
        if self.checkpoint_path.is_none() {
            return;
        }

        let pages_crawled = self.result.read().await.pages_crawled;
        if pages_crawled < pages_at_last_checkpoint.saturating_add(self.checkpoint_interval_pages) {
            return;
        }

        *pages_at_last_checkpoint = pages_crawled;
        if let Err(e) = self.save_checkpoint().await {
            tracing::error!("Failed to save checkpoint: {}", e);
        }
    }

    /// Snapshots the current visited set and pending frontier to
    /// `checkpoint_path`, so the crawl can later be resumed with `--resume-from`.
    /// The write is atomic (temp file + rename) so a crash mid-write cannot
    /// leave behind a corrupt checkpoint.
    async fn save_checkpoint(&self) -> Result<(), String> {
        let Some(path) = self.checkpoint_path.clone() else {
            return Ok(());
        };

        let visited: Vec<Url> = self.visited.iter().map(|entry| entry.clone()).collect();
        let frontier = self.snapshot_frontier();
        let stats = self.result.read().await.to_stats();
        let visited_len = visited.len();
        let frontier_len = frontier.len();

        let checkpoint = CrawlCheckpoint {
            visited,
            frontier,
            stats,
        };
        let json =
            serde_json::to_string_pretty(&checkpoint).map_err(|e| format!("Failed to serialize checkpoint: {}", e))?;

        let tmp_path = format!("{}.tmp", path);
        fs::write(&tmp_path, json).map_err(|e| format!("Failed to write checkpoint to '{}': {}", tmp_path, e))?;
        fs::rename(&tmp_path, &path).map_err(|e| format!("Failed to finalize checkpoint file '{}': {}", path, e))?;

        tracing::info!(
            "Checkpoint saved to '{}' ({} visited, {} queued).",
            path,
            visited_len,
            frontier_len
        );
        Ok(())
    }

    /// Drains `to_visit` and pushes every item back, returning a snapshot of
    /// its contents. Safe to call concurrently with producers pushing new
    /// items; items pushed during the drain may be missing from the snapshot
    /// but are never lost from the queue itself.
    fn snapshot_frontier(&self) -> Vec<(Url, usize)> {
        let mut items = Vec::new();
        while let Some(item) = self.to_visit.pop() {
            items.push(item);
        }
        for item in &items {
            self.to_visit.push(item.clone());
        }
        items
    }

    fn should_skip_url_without_visited_check(&self, url: &Url) -> bool {
        let Some(domain) = url.host_str() else {
            tracing::info!("Skipping URL with no host: {}", url);
            return true;
        };

        let allowed = match &self.allowed_domains {
            None => domain == self.initial_domain,
            Some(list) if list.is_empty() => true,
            Some(list) => list.iter().any(|d| d == domain),
        };

        if !allowed {
            tracing::info!("Skipping URL from disallowed domain: {}", url);
            return true;
        }

        false
    }

    async fn wait_for_domain_rate_limit(&self, domain: &str) {
        if self.crawl_delay.is_zero() {
            return;
        }
        loop {
            let now = Instant::now();
            // Hold the shard lock for the entire check-and-update so concurrent
            // tasks cannot both observe the same (or absent) timestamp and
            // simultaneously bypass the intended per-domain delay.
            let wait = match self.domain_last_request.entry(domain.to_string()) {
                Entry::Occupied(mut e) => {
                    let elapsed = now.duration_since(*e.get());
                    if elapsed >= self.crawl_delay {
                        e.insert(now);
                        None
                    } else {
                        Some(self.crawl_delay - elapsed)
                    }
                }
                Entry::Vacant(e) => {
                    e.insert(now);
                    None
                }
            };
            // The entry (and its shard lock) is dropped before any await point.
            match wait {
                None => return,
                Some(dur) => sleep(dur).await,
            }
        }
    }

    async fn process_url_with_rate_limit(&self, url: Url, depth: usize) {
        if let Some(domain) = url.host_str() {
            self.wait_for_domain_rate_limit(domain).await;
        }
        self.process_url(url, depth).await;
    }

    async fn process_url(&self, current_url: Url, current_depth: usize) {
        tracing::info!("Processing URL: {} (depth: {})", current_url, current_depth);

        if self.skip_extensions && has_non_html_extension(&current_url) {
            tracing::info!("Skipping {} without a request: non-HTML extension", current_url);
            self.result.write().await.pages_skipped_extension += 1;
            return;
        }

        // URL is already marked as visited in run_parallel, no need to check again
        let robots_rules = match self.get_or_fetch_robots(&current_url).await {
            Ok(rules) => rules,
            Err(e) => {
                tracing::error!("Failed to fetch robots.txt for {}: {}", current_url, e);
                self.result
                    .write()
                    .await
                    .record_failure(format!("Failed to fetch robots.txt for {}: {}", current_url, e));
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

        let fetch_options = FetchOptions {
            html_only: true,
            max_bytes: self.max_response_bytes,
        };
        let requested_url = current_url.clone();
        match self.http_client.fetch_with(current_url.clone(), fetch_options).await {
            Ok(fetched) => {
                let html_content = fetched.content;
                // Resolve links and name output after the URL actually served, not the requested one.
                let current_url = fetched.final_url;
                if current_url != requested_url
                    && requested_url != self.start_url
                    && !matches_path_filters(&self.include_patterns, &self.exclude_patterns, &current_url)
                {
                    tracing::info!(
                        "Skipping {}: redirected to {} which is excluded by path filters",
                        requested_url,
                        current_url
                    );
                    return;
                }
                if current_url != requested_url && !self.visited.insert(current_url.clone()) {
                    tracing::info!(
                        "Skipping {}: redirected to already visited {}",
                        requested_url,
                        current_url
                    );
                    return;
                }
                tracing::info!("Applying mq query to content from {}", current_url);

                let query = self.mq_query.clone();
                let html_content_clone = html_content.clone();
                // Resolve relative links against the page actually being crawled, not one static base.
                let mut conversion_options = self.conversion_options.clone();
                conversion_options.base_url = Some(current_url.to_string());
                let current_url_clone = current_url.clone();
                let self_clone = self.clone();
                let new_links = tokio::task::spawn_blocking(move || {
                    let failure = if self_clone.map {
                        self_clone
                            .output_map_url(&current_url_clone)
                            .err()
                            .map(|e| format!("Failed to output URL {}: {}", current_url_clone, e))
                    } else {
                        match Self::execute_query(&query, &html_content_clone, conversion_options) {
                            Ok(md) => self_clone
                                .output_markdown(&current_url_clone, md.as_str())
                                .err()
                                .map(|e| format!("Failed to output markdown for {}: {}", current_url_clone, e)),
                            Err(e) => Some(format!(
                                "Failed to execute mq query on content from {}: {}",
                                current_url_clone, e
                            )),
                        }
                    };
                    if let Some(message) = &failure {
                        tracing::error!("{}", message);
                    }

                    (extract_links(&html_content, &current_url_clone), failure)
                })
                .await;

                match new_links {
                    Ok((new_links, failure)) => {
                        {
                            let mut result = self.result.write().await;
                            result.links_discovered += new_links.len();
                            match failure {
                                Some(message) => result.record_failure(message),
                                None => result.pages_crawled += 1,
                            }
                        }

                        {
                            let next_depth = current_depth + 1;
                            for link in new_links {
                                if !self.visited.contains(&link)
                                    && !self.should_skip_url_without_visited_check(&link)
                                    && matches_path_filters(&self.include_patterns, &self.exclude_patterns, &link)
                                    && self.depth_limit.is_none_or(|limit| next_depth <= limit)
                                {
                                    self.to_visit.push((link, next_depth));
                                    // Wake the main loop to pick up the new URL.
                                    self.notify.notify_one();
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to execute mq query on content from {}: {}", current_url, e);
                    }
                }
            }
            Err(e) if e.kind == FetchErrorKind::NotHtml => {
                tracing::warn!("Skipping {}: {}", current_url, e);
                let mut result = self.result.write().await;
                result.pages_skipped_non_html += 1;
            }
            Err(e) => {
                tracing::error!("Failed to fetch URL {}: {}", current_url, e);
                self.result
                    .write()
                    .await
                    .record_failure(format!("Failed to fetch URL {}: {}", current_url, e));
            }
        }
    }

    fn execute_query(query: &str, input: &str, conversion_options: ConversionOptions) -> miette::Result<String> {
        let input = mq_lang::parse_html_input_with_options(input, conversion_options)?;
        let mut mq_engine = mq_lang::DefaultEngine::default();
        mq_engine.load_builtin_module();

        match mq_engine
            .eval(query, input.into_iter())
            .map_err(|e| miette!(format!("Error evaluating mq query: {}", e)))
        {
            Ok(values) => Ok(mq_markdown::Markdown::new(
                values
                    .into_iter()
                    .map(|value| match value {
                        mq_lang::RuntimeValue::Markdown(node, _) => mq_syntax::Shared::unwrap_or_clone(node),
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

        if !self.suppress_stats {
            let result = self.result.read().await;
            result.write_stats_to_stderr(&self.format);
        }
    }

    fn output_map_url(&self, url: &Url) -> Result<(), String> {
        let Some(ref output_dir) = self.output_path else {
            println!("{}", url);
            return Ok(());
        };
        std::fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create {}: {}", output_dir, e))?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(Path::new(output_dir).join("urls.txt"))
            .map_err(|e| format!("Failed to open urls.txt: {}", e))?;
        writeln!(file, "{}", url).map_err(|e| format!("Failed to write urls.txt: {}", e))
    }

    fn output_markdown(&self, url: &Url, markdown: &str) -> Result<(), String> {
        tracing::debug!("Preparing to output markdown for {}", url);
        if let Some(ref output_dir_str) = self.output_path {
            // Renamed path_str to output_dir_str
            let domain_str = url.host_str().unwrap_or("unknown_domain");
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

            let mut file = fs::File::create(&output_file_path)
                .map_err(|e| format!("Failed to create output file '{:?}': {}", output_file_path, e))?;
            file.write_all(markdown.as_bytes())
                .map_err(|e| format!("Failed to write markdown to file '{:?}': {}", output_file_path, e))?;
            tracing::debug!("Successfully wrote {} bytes to {:?}", markdown.len(), output_file_path);
        } else {
            let filename = format!(
                "{}_{}.md",
                sanitize_filename_component(url.host_str().unwrap_or("unknown_domain"), 50),
                if url.path() == "/" || url.path().is_empty() {
                    "index".to_string()
                } else {
                    sanitize_filename_component(url.path().trim_matches('/'), 100)
                }
            );
            writeln!(io::stderr().lock(), "Filename: {} --", filename)
                .map_err(|e| format!("Failed to write to stderr: {}", e))?;
            let stdout = io::stdout();
            let mut handle = stdout.lock();
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
    use httpmock::{Method::GET, MockServer};
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
    fn test_extract_links(#[case] html: &str, #[case] base_url: &str, #[case] expected_urls: Vec<&str>) {
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
    fn test_sanitize_filename_component(#[case] input: &str, #[case] max_len: usize, #[case] expected: &str) {
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

    /// Helper to build a Crawler for unit tests without sending any HTTP requests.
    /// Uses the `.invalid` TLD (RFC 2606 reserved) to make clear no real domain is intended.
    async fn make_test_crawler(
        start_domain: &str,
        allowed_domains: Option<Vec<String>>,
        depth_limit: Option<usize>,
    ) -> Crawler {
        let start_url = Url::parse(&format!("http://{}/", start_domain)).unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        Crawler::new(
            http_client,
            start_url,
            CrawlerOptions {
                concurrency: 1,
                depth_limit,
                allowed_domains,
                checkpoint_interval_pages: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_should_skip_url_no_domain() {
        let crawler = make_test_crawler("start.invalid", None, None).await;

        // URL without domain (e.g. data: URI) should be skipped
        let no_domain = Url::parse("data:text/plain,hello").unwrap();
        assert!(crawler.should_skip_url_without_visited_check(&no_domain));
    }

    #[tokio::test]
    async fn test_should_skip_url_same_domain_allowed() {
        let crawler = make_test_crawler("start.invalid", None, None).await;
        let same = Url::parse("http://start.invalid/page").unwrap();
        assert!(!crawler.should_skip_url_without_visited_check(&same));
    }

    #[tokio::test]
    async fn test_should_skip_url_different_domain_denied() {
        let crawler = make_test_crawler("start.invalid", None, None).await;
        let other = Url::parse("http://other.invalid/page").unwrap();
        assert!(crawler.should_skip_url_without_visited_check(&other));
    }

    #[tokio::test]
    async fn test_should_skip_url_empty_allowed_list_permits_all() {
        let crawler = make_test_crawler("start.invalid", Some(vec![]), None).await;
        let other = Url::parse("http://other.invalid/page").unwrap();
        assert!(!crawler.should_skip_url_without_visited_check(&other));
    }

    #[tokio::test]
    async fn test_should_skip_url_explicit_allowed_list() {
        let allowed = vec!["start.invalid".to_string(), "extra.invalid".to_string()];
        let crawler = make_test_crawler("start.invalid", Some(allowed), None).await;

        let allowed_url = Url::parse("http://extra.invalid/page").unwrap();
        let denied_url = Url::parse("http://denied.invalid/page").unwrap();

        assert!(!crawler.should_skip_url_without_visited_check(&allowed_url));
        assert!(crawler.should_skip_url_without_visited_check(&denied_url));
    }

    #[rstest]
    #[case(&[], &[], "/any", true)]
    #[case(&["/docs/*"], &[], "/docs/a/b", true)]
    #[case(&["/docs/*"], &[], "/blog/a", false)]
    #[case(&[], &["/blog/*"], "/blog/a", false)]
    #[case(&[], &["/blog/*"], "/docs/a", true)]
    #[case(&["/docs/*"], &["/docs/private/*"], "/docs/private/x", false)]
    #[case(&["/search?q=*"], &[], "/search?q=rust", true)]
    fn test_matches_path_filters(
        #[case] include: &[&str],
        #[case] exclude: &[&str],
        #[case] path: &str,
        #[case] expected: bool,
    ) {
        let compile = |v: &[&str]| v.iter().map(|p| glob::Pattern::new(p).unwrap()).collect::<Vec<_>>();
        let url = Url::parse(&format!("http://start.invalid{}", path)).unwrap();
        assert_eq!(
            matches_path_filters(&compile(include), &compile(exclude), &url),
            expected
        );
    }

    #[tokio::test]
    async fn test_crawler_new_rejects_invalid_glob() {
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let result = Crawler::new(
            http_client,
            Url::parse("http://start.invalid/").unwrap(),
            CrawlerOptions {
                include: vec!["[".to_string()],
                ..Default::default()
            },
        )
        .await;
        assert!(result.unwrap_err().contains("Invalid glob pattern"));
    }

    #[tokio::test]
    async fn test_crawler_new_filters_seed_urls() {
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let crawler = Crawler::new(
            http_client,
            Url::parse("http://start.invalid/").unwrap(),
            CrawlerOptions {
                include: vec!["/docs/*".to_string()],
                additional_seed_urls: vec![
                    Url::parse("http://start.invalid/docs/a").unwrap(),
                    Url::parse("http://start.invalid/blog/b").unwrap(),
                ],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        // Start URL is exempt; only the matching seed is queued.
        assert_eq!(crawler.to_visit.len(), 2);
    }

    #[tokio::test]
    async fn test_crawler_new_with_depth_limit() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let crawler = Crawler::new(
            http_client,
            start_url,
            CrawlerOptions {
                crawl_delay_secs: 1.0,
                concurrency: 1,
                depth_limit: Some(2),
                checkpoint_interval_pages: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(crawler.depth_limit, Some(2));
    }

    #[tokio::test]
    async fn test_crawler_new_with_unlimited_depth() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let crawler = Crawler::new(
            http_client,
            start_url,
            CrawlerOptions {
                crawl_delay_secs: 1.0,
                concurrency: 1,
                checkpoint_interval_pages: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        assert_eq!(crawler.depth_limit, None);
    }

    #[tokio::test]
    async fn test_crawler_new_with_additional_seed_urls() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let seed_urls = vec![
            Url::parse("http://start.invalid/from-sitemap-1").unwrap(),
            Url::parse("http://start.invalid/from-sitemap-2").unwrap(),
        ];
        let crawler = Crawler::new(
            http_client,
            start_url.clone(),
            CrawlerOptions {
                concurrency: 1,
                additional_seed_urls: seed_urls.clone(),
                checkpoint_interval_pages: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let mut queued = Vec::new();
        while let Some((url, depth)) = crawler.to_visit.pop() {
            queued.push((url, depth));
        }

        assert_eq!(queued.len(), 3);
        assert!(queued.contains(&(start_url, 0)));
        for seed_url in seed_urls {
            assert!(queued.contains(&(seed_url, 0)));
        }
    }

    #[tokio::test]
    async fn test_max_pages_limits_total_visited() {
        let server = MockServer::start_async().await;
        let port = server.address().port();

        let index_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200).body(
                    r#"<html><body>
                        <a href="/page1">1</a>
                        <a href="/page2">2</a>
                        <a href="/page3">3</a>
                        <a href="/page4">4</a>
                        <a href="/page5">5</a>
                    </body></html>"#,
                );
            })
            .await;
        let page1_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/page1");
                then.status(200).body("<html><body>Page 1</body></html>");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();

        let mut crawler = Crawler::new(
            http_client,
            start_url,
            CrawlerOptions {
                concurrency: 1,
                max_pages: Some(2),
                checkpoint_interval_pages: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        crawler.run().await.unwrap();

        assert_eq!(crawler.visited.len(), 2);
        assert!(!crawler.to_visit.is_empty(), "unvisited URLs should remain queued");
        index_mock.assert_async().await;
        page1_mock.assert_async().await;
    }

    #[rstest]
    #[case("http://127.0.0.1:8080/", "127.0.0.1")]
    #[case("http://[::1]:8080/", "[::1]")]
    #[tokio::test]
    async fn test_crawler_new_accepts_ip_start_url(#[case] start: &str, #[case] host: &str) {
        let start_url = Url::parse(start).unwrap();
        let crawler = Crawler::new(HttpClient::default(), start_url, CrawlerOptions::default())
            .await
            .unwrap();

        assert_eq!(crawler.initial_domain, host);
        let same_host = Url::parse(&format!("http://{}/page", host)).unwrap();
        assert!(!crawler.should_skip_url_without_visited_check(&same_host));
        let other_host = Url::parse("http://192.0.2.1/page").unwrap();
        assert!(crawler.should_skip_url_without_visited_check(&other_host));
    }

    #[tokio::test]
    async fn test_crawl_honours_robots_txt_on_non_default_port() {
        let server = MockServer::start_async().await;
        let port = server.address().port();

        server
            .mock_async(|when, then| {
                when.method(GET).path("/robots.txt");
                then.status(200).body("User-agent: *\nDisallow: /private");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body(r#"<html><body><a href="/private/x">x</a></body></html>"#);
            })
            .await;
        let private_mock = server
            .mock_async(|when, then| {
                when.method(GET).path("/private/x");
                then.status(200)
                    .header("content-type", "text/html")
                    .body("<html><body>secret</body></html>");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions::default(),
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        assert_eq!(private_mock.calls_async().await, 0);
        let result = crawler.result.read().await;
        assert_eq!(result.pages_crawled, 1);
        assert_eq!(result.pages_skipped_robots, 1);
    }

    #[tokio::test]
    async fn test_map_lists_urls_without_running_query() {
        let server = MockServer::start_async().await;
        let port = server.address().port();
        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body(r#"<html><body><a href="/a">a</a><a href="/b">b</a></body></html>"#);
            })
            .await;
        for path in ["/a", "/b"] {
            server
                .mock_async(move |when, then| {
                    when.method(GET).path(path);
                    then.status(200)
                        .header("content-type", "text/html")
                        .body("<html><body><h1>page</h1></body></html>");
                })
                .await;
        }

        let out = tempfile::tempdir().unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            Url::parse(&format!("http://localhost:{}/", port)).unwrap(),
            CrawlerOptions {
                map: true,
                // A query that would fail on every page if it were run.
                mq_query: Some("this_is_not_defined()".to_string()),
                output_path: Some(out.path().to_string_lossy().into_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        let mut urls: Vec<String> = std::fs::read_to_string(out.path().join("urls.txt"))
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect();
        urls.sort();
        let base = format!("http://localhost:{}", port);
        assert_eq!(urls, vec![format!("{base}/"), format!("{base}/a"), format!("{base}/b")]);
        assert!(!out.path().join("localhost_index.md").exists());
    }

    #[rstest]
    #[case("http://a.com/doc.PDF", true)]
    #[case("http://a.com/x/img.png?v=1", true)]
    #[case("http://a.com/page.html", false)]
    #[case("http://a.com/a.b/page", false)]
    #[case("http://a.com/", false)]
    fn test_has_non_html_extension(#[case] url: &str, #[case] expected: bool) {
        assert_eq!(has_non_html_extension(&Url::parse(url).unwrap()), expected);
    }

    #[tokio::test]
    async fn test_crawl_skips_extension_without_request() {
        let server = MockServer::start_async().await;
        let port = server.address().port();
        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body(r#"<html><body><a href="/doc.pdf">pdf</a></body></html>"#);
            })
            .await;
        let pdf = server
            .mock_async(|when, then| {
                when.method(GET).path("/doc.pdf");
                then.status(200).header("content-type", "application/pdf").body("x");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions {
                skip_extensions: true,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        pdf.assert_calls_async(0).await;
        let result = crawler.result.read().await;
        assert_eq!(result.pages_skipped_extension, 1);
        assert_eq!(result.pages_skipped_non_html, 0);
    }

    #[tokio::test]
    async fn test_crawl_skips_non_html_responses() {
        let server = MockServer::start_async().await;
        let port = server.address().port();

        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html; charset=utf-8")
                    .body(r#"<html><body><a href="/doc.pdf">pdf</a></body></html>"#);
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/doc.pdf");
                then.status(200)
                    .header("content-type", "application/pdf")
                    .body("%PDF-1.4 binary");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions::default(),
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        let result = crawler.result.read().await;
        assert_eq!(result.pages_crawled, 1);
        assert_eq!(result.pages_skipped_non_html, 1);
        assert_eq!(result.pages_failed, 0);
    }

    #[tokio::test]
    async fn test_crawl_resolves_links_against_redirect_target() {
        let server = MockServer::start_async().await;
        let port = server.address().port();

        server
            .mock_async(|when, then| {
                when.method(GET).path("/old");
                then.status(301).header("location", "/new/index.html");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/new/index.html");
                then.status(200)
                    .header("content-type", "text/html")
                    .body(r#"<html><body><a href="page2.html">next</a></body></html>"#);
            })
            .await;
        let target = server
            .mock_async(|when, then| {
                when.method(GET).path("/new/page2.html");
                then.status(200)
                    .header("content-type", "text/html")
                    .body("<html><body>ok</body></html>");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/old", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions::default(),
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        target.assert_async().await;
        let final_url = Url::parse(&format!("http://localhost:{}/new/index.html", port)).unwrap();
        assert!(crawler.visited.contains(&final_url));
    }

    #[tokio::test]
    async fn test_crawl_rejects_response_over_max_bytes() {
        let server = MockServer::start_async().await;
        let port = server.address().port();

        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body(format!("<html><body>{}</body></html>", "x".repeat(1000)));
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions {
                max_response_bytes: Some(100),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        let stats = crawler.stats().await;
        assert_eq!(stats.pages_crawled, 0);
        assert_eq!(stats.pages_failed, 1);
    }

    #[tokio::test]
    async fn test_crawl_counts_invalid_query_as_failed() {
        let server = MockServer::start_async().await;
        let port = server.address().port();
        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body("<html><body>ok</body></html>");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions {
                mq_query: Some("select(".to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        let stats = crawler.stats().await;
        assert_eq!(stats.pages_crawled, 0);
        assert_eq!(stats.pages_failed, 1);
        assert!(crawler.first_failure().await.unwrap().contains("mq query"));
    }

    #[tokio::test]
    async fn test_crawl_fails_when_final_checkpoint_cannot_be_saved() {
        let server = MockServer::start_async().await;
        let port = server.address().port();
        server
            .mock_async(|when, then| {
                when.method(GET).path("/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body("<html><body>ok</body></html>");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/", port)).unwrap();
        let missing = std::env::temp_dir().join(format!("mq-crawl-missing-{}/crawl.json", std::process::id()));
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions {
                checkpoint_path: Some(missing.to_string_lossy().into_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let err = crawler.run().await.unwrap_err();
        assert!(err.contains("final checkpoint"), "{err}");
    }

    #[tokio::test]
    async fn test_save_checkpoint_captures_visited_and_frontier() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();
        let checkpoint_file = tempfile::NamedTempFile::new().unwrap();
        let checkpoint_path = checkpoint_file.path().to_string_lossy().to_string();

        let crawler = Crawler::new(
            http_client,
            start_url.clone(),
            CrawlerOptions {
                concurrency: 1,
                checkpoint_path: Some(checkpoint_path.clone()),
                checkpoint_interval_pages: 1,
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // Drain the default seeded start URL so the frontier reflects only
        // the mid-crawl state set up below.
        while crawler.to_visit.pop().is_some() {}

        // Simulate mid-crawl state: one page visited, one still queued.
        crawler.visited.insert(start_url.clone());
        let queued_url = Url::parse("http://start.invalid/page1").unwrap();
        crawler.to_visit.push((queued_url.clone(), 1));

        crawler.save_checkpoint().await.unwrap();

        let loaded = CrawlCheckpoint::load(&checkpoint_path).unwrap();
        assert_eq!(loaded.visited, vec![start_url]);
        assert_eq!(loaded.frontier, vec![(queued_url, 1)]);

        // The frontier snapshot must not have drained the live queue.
        assert!(!crawler.to_visit.is_empty());
    }

    #[tokio::test]
    async fn test_resume_from_checkpoint_restores_state() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let http_client = HttpClient::new_reqwest(30.0).unwrap();

        let visited_url = Url::parse("http://start.invalid/already-visited").unwrap();
        let queued_url = Url::parse("http://start.invalid/still-queued").unwrap();
        let checkpoint = CrawlCheckpoint {
            visited: vec![visited_url.clone()],
            frontier: vec![(queued_url.clone(), 3)],
            stats: CrawlResultStats {
                pages_crawled: 5,
                pages_skipped_robots: 1,
                pages_skipped_non_html: 4,
                pages_skipped_extension: 2,
                pages_failed: 2,
                links_discovered: 7,
                total_pages_visited: 1,
                duration_secs: None,
            },
        };

        let crawler = Crawler::new(
            http_client,
            start_url.clone(),
            CrawlerOptions {
                concurrency: 1,
                checkpoint_interval_pages: 1,
                resume_checkpoint: Some(checkpoint),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        // The checkpoint's visited set is restored, and the start URL is not
        // re-queued alongside it.
        assert!(crawler.visited.contains(&visited_url));
        assert_eq!(crawler.visited.len(), 1);

        let mut queued = Vec::new();
        while let Some(item) = crawler.to_visit.pop() {
            queued.push(item);
        }
        assert_eq!(queued, vec![(queued_url, 3)]);

        let result = crawler.result.read().await;
        assert_eq!(result.pages_crawled, 5);
        assert_eq!(result.pages_skipped_robots, 1);
        assert_eq!(result.pages_skipped_non_html, 4);
        assert_eq!(result.pages_failed, 2);
        assert_eq!(result.links_discovered, 7);
    }

    #[tokio::test]
    async fn test_resume_applies_path_filters_to_frontier() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let kept = Url::parse("http://start.invalid/docs/new").unwrap();
        let dropped = Url::parse("http://start.invalid/docs/old").unwrap();
        let checkpoint = CrawlCheckpoint {
            visited: vec![],
            frontier: vec![(dropped, 1), (kept.clone(), 2), (start_url.clone(), 0)],
            stats: CrawlResultStats::default(),
        };

        let crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url.clone(),
            CrawlerOptions {
                concurrency: 1,
                checkpoint_interval_pages: 1,
                include: vec!["/docs/*".to_string()],
                exclude: vec!["/docs/old".to_string()],
                resume_checkpoint: Some(checkpoint),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        let mut queued = Vec::new();
        while let Some(item) = crawler.to_visit.pop() {
            queued.push(item);
        }
        assert_eq!(queued, vec![(kept, 2), (start_url, 0)]);
    }

    #[tokio::test]
    async fn test_resume_with_visited_duplicate_in_frontier_completes() {
        let start_url = Url::parse("http://start.invalid/").unwrap();
        let guide = Url::parse("http://start.invalid/docs/guide").unwrap();
        let news = Url::parse("http://start.invalid/news").unwrap();
        let checkpoint = CrawlCheckpoint {
            visited: vec![guide.clone()],
            frontier: vec![(guide, 1), (news, 1)],
            stats: CrawlResultStats::default(),
        };

        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions {
                concurrency: 1,
                include: vec!["/docs/*".to_string()],
                resume_checkpoint: Some(checkpoint),
                ..Default::default()
            },
        )
        .await
        .unwrap();

        tokio::time::timeout(Duration::from_secs(10), crawler.run())
            .await
            .expect("resumed crawl must not hang")
            .unwrap();
    }

    #[tokio::test]
    async fn test_crawl_skips_redirect_to_excluded_path() {
        let server = MockServer::start_async().await;
        let port = server.address().port();

        server
            .mock_async(|when, then| {
                when.method(GET).path("/docs/");
                then.status(200)
                    .header("content-type", "text/html")
                    .body(r#"<html><body><a href="/docs/old-guide">x</a></body></html>"#);
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/docs/old-guide");
                then.status(301).header("location", "/docs/archive/old-guide");
            })
            .await;
        server
            .mock_async(|when, then| {
                when.method(GET).path("/docs/archive/old-guide");
                then.status(200)
                    .header("content-type", "text/html")
                    .body("<html><body>archived</body></html>");
            })
            .await;

        let start_url = Url::parse(&format!("http://localhost:{}/docs/", port)).unwrap();
        let mut crawler = Crawler::new(
            HttpClient::new_reqwest(30.0).unwrap(),
            start_url,
            CrawlerOptions {
                include: vec!["/docs/*".to_string()],
                exclude: vec!["/docs/archive/*".to_string()],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        crawler.run().await.unwrap();

        let archived = Url::parse(&format!("http://localhost:{}/docs/archive/old-guide", port)).unwrap();
        assert!(!crawler.visited.contains(&archived));
        assert_eq!(crawler.result.read().await.pages_crawled, 1);
    }
}
