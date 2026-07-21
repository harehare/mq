//! Web crawler for collecting markdown content from websites.
//!
//! This crate provides functionality to crawl websites and extract markdown content.
//! It respects robots.txt, handles concurrent requests, and converts HTML to markdown
//! for batch processing with mq.
//!
//! # Features
//!
//! - Asynchronous web crawling with configurable concurrency
//! - robots.txt compliance
//! - HTML to markdown conversion
//! - Link discovery and following
//! - sitemap.xml ingestion as a seed-URL source
//! - Crawl statistics and result tracking
//! - Support for custom HTTP headers, cookies, and user agents
//! - Basic and bearer authentication for protected sites
//! - Automatic retry with exponential backoff for failed requests
//! - Rate limiting and politeness delays
//!
//! # Usage
//!
//! ```rust,ignore
//! use mq_crawler::crawler::Crawler;
//! use url::Url;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let start_url = Url::parse("https://your-target-site.com")?;
//!     let crawler = Crawler::new(start_url, None, 10);
//!     let result = crawler.crawl().await?;
//!     println!("Crawled {} pages", result.pages_crawled);
//!     Ok(())
//! }
//! ```
//!
//! # Crawling Behavior
//!
//! The crawler:
//! - Starts from a specified URL
//! - Follows links found on each page
//! - Respects robots.txt directives
//! - Limits depth and breadth of crawling
//! - Converts HTML pages to markdown
//! - Tracks statistics about the crawl
//!
pub mod crawler;
pub mod http_client;
pub mod robots;
pub mod sitemap;
