use crate::http_client::HttpClient;
use robots_txt::{Robots, matcher::SimpleMatcher};
use std::sync::OnceLock;
use url::Url;

// A simple wrapper for robots.txt handling.
// It stores the text content and caches the parsed Robots structure.
#[derive(Debug)]
pub struct RobotsTxt {
    robots_text: Option<String>,
    domain: String,
    parsed_robots: OnceLock<bool>,
}

impl RobotsTxt {
    /// Fetches robots.txt for a given domain.
    /// If a custom_robots_path is provided and is a valid URL, it fetches from there.
    /// Otherwise, it constructs the default robots.txt URL (e.g., http://example.com/robots.txt).
    pub async fn fetch(
        http_client: &HttpClient,
        target_url: &Url,
        custom_robots_path: Option<&str>,
    ) -> Result<Self, String> {
        let domain = target_url
            .domain()
            .map(|domain| domain.to_string())
            .or_else(|| {
                target_url.host_str().map(|host| {
                    if let Some(port) = target_url.port() {
                        format!("{}:{}", host, port)
                    } else {
                        host.to_string()
                    }
                })
            })
            .as_deref()
            .ok_or_else(|| {
                format!(
                    "Target URL {} does not have a valid domain or host.",
                    target_url
                )
            })?
            .to_string();

        let robots_url_str = if let Some(path) = custom_robots_path {
            if let Ok(custom_url) = Url::parse(path) {
                custom_url.to_string()
            } else {
                return Err(format!(
                    "Custom robots_path '{}' is not a valid URL. Local file paths are not yet supported for robots.txt.",
                    path
                ));
            }
        } else {
            let base_url_str = format!("{}://{}", target_url.scheme(), domain);
            let base_url = Url::parse(&base_url_str)
                .map_err(|e| format!("Failed to parse base URL: {}", e))?;
            base_url
                .join("/robots.txt")
                .map_err(|e| format!("Failed to join /robots.txt: {}", e))?
                .to_string()
        };

        let robots_url = Url::parse(&robots_url_str)
            .map_err(|e| format!("Invalid robots.txt URL {}: {}", robots_url_str, e))?;

        tracing::info!("Fetching robots.txt from: {}", robots_url);

        match http_client.fetch(robots_url).await {
            Ok(response) => {
                tracing::debug!("robots.txt content for {}:\n{}", domain, response);
                Ok(RobotsTxt {
                    robots_text: Some(response),
                    domain,
                    parsed_robots: OnceLock::new(),
                })
            }
            Err(e) => {
                tracing::warn!("Error fetching robots.txt for {}: {}", domain, e);
                Ok(RobotsTxt {
                    robots_text: None,
                    domain,
                    parsed_robots: OnceLock::new(),
                })
            }
        }
    }

    /// Checks if a URL is allowed to be crawled by a specific user-agent.
    /// Uses cached parsed robots.txt content for performance.
    pub fn is_allowed(&self, url_to_check: &Url, user_agent: &str) -> bool {
        // Ensure the URL is for the same domain this robots.txt is for.
        if url_to_check.domain().is_none() {
            tracing::warn!(
                "Checking URL {} against robots.txt for a different domain {}",
                url_to_check,
                self.domain
            );
            return true;
        } else if url_to_check.domain().unwrap_or_default() != self.domain {
            tracing::warn!(
                "Checking URL {} against robots.txt for a different domain {}",
                url_to_check,
                self.domain
            );
            return false;
        }

        // Simple optimization - just avoid reparsing if we have no robots.txt
        let has_robots = *self
            .parsed_robots
            .get_or_init(|| self.robots_text.is_some());

        if !has_robots {
            return true;
        }

        let robots = match &self.robots_text {
            Some(text) => Robots::from_str_lossy(text),
            None => return true,
        };

        let section = robots.choose_section(user_agent);
        let matcher = SimpleMatcher::new(&section.rules);
        matcher.check_path(url_to_check.path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::{Method::GET, MockServer};
    use std::sync::Once;
    use tokio::runtime::Runtime;

    static INIT: Once = Once::new();
    fn init_tracing() {
        INIT.call_once(|| {
            let _ = tracing_subscriber::fmt::try_init();
        });
    }

    #[test]
    fn test_fetch_successful() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            let robots_body = "User-agent: *\nDisallow: /private/";
            let mock = server
                .mock_async(|when, then| {
                    when.method(GET).path("/robots.txt");
                    then.status(200).body(robots_body);
                })
                .await;

            let http_client = HttpClient::new_reqwest(30.0).unwrap();
            let url = Url::parse(&format!("http://{}", server.address())).unwrap();
            let robots = RobotsTxt::fetch(&http_client, &url, None).await.unwrap();

            assert_eq!(robots.domain, server.address().to_string());
            assert_eq!(robots.robots_text.as_deref(), Some(robots_body));
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_fetch_not_found() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            let mock = server
                .mock_async(|when, then| {
                    when.method(GET).path("/robots.txt");
                    then.status(404);
                })
                .await;

            let http_client = HttpClient::new_reqwest(30.0).unwrap();
            let url = Url::parse(&format!("http://{}", server.address())).unwrap();
            let robots = RobotsTxt::fetch(&http_client, &url, None).await.unwrap();

            assert_eq!(robots.domain, server.address().to_string());
            assert!(robots.robots_text.is_none());
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_fetch_custom_robots_path() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            let robots_body = "User-agent: *\nDisallow: /foo/";
            let mock = server
                .mock_async(|when, then| {
                    when.method(GET).path("/custom-robots.txt");
                    then.status(200).body(robots_body);
                })
                .await;

            let http_client = HttpClient::new_reqwest(30.0).unwrap();
            let custom_url = format!("http://{}/custom-robots.txt", server.address());
            let url = Url::parse(&format!("http://{}", server.address())).unwrap();
            let robots = RobotsTxt::fetch(&http_client, &url, Some(&custom_url))
                .await
                .unwrap();

            assert_eq!(robots.domain, server.address().to_string());
            assert_eq!(robots.robots_text.as_deref(), Some(robots_body));
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_fetch_invalid_custom_path() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let http_client = HttpClient::new_reqwest(30.0).unwrap();
            let url = Url::parse("http://example.com").unwrap();
            let result = RobotsTxt::fetch(&http_client, &url, Some("/not/a/url")).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().contains("not a valid URL"));
        });
    }

    #[test]
    fn test_robots_txt_parsing_basic_direct() {
        // This test now verifies the full logic via is_allowed for a specific text
        let rules_text = "User-agent: *\nDisallow: /private/\nAllow: /public/";
        let rt = RobotsTxt {
            robots_text: Some(rules_text.to_string()),
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };

        let private_url = Url::parse("http://example.com/private/something").unwrap();
        let public_url = Url::parse("http://example.com/public/page").unwrap();
        let other_url = Url::parse("http://example.com/anywhere_else").unwrap();

        assert!(!rt.is_allowed(&private_url, "*"));
        assert!(rt.is_allowed(&public_url, "*"));
        assert!(rt.is_allowed(&other_url, "*"));
    }

    #[test]
    fn test_is_allowed_with_no_robots_text() {
        // Updated test name
        let rt = RobotsTxt {
            robots_text: None,
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };
        let url = Url::parse("http://example.com/some/path").unwrap();
        assert!(rt.is_allowed(&url, "test-agent"));
    }

    #[test]
    fn test_is_allowed_with_simple_disallow_text() {
        // Updated test name
        let rules = "User-agent: test-agent
Disallow: /forbidden/";
        let rt = RobotsTxt {
            robots_text: Some(rules.to_string()),
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };

        let allowed_url = Url::parse("http://example.com/allowed/path").unwrap();
        let disallowed_url = Url::parse("http://example.com/forbidden/path").unwrap();

        assert!(rt.is_allowed(&allowed_url, "test-agent"));
        assert!(!rt.is_allowed(&disallowed_url, "test-agent"));
        assert!(rt.is_allowed(&disallowed_url, "another-agent"));
    }

    #[test]
    fn test_is_allowed_different_domain() {
        // Should return false if the URL is for a different domain
        let rules = "User-agent: *\nDisallow: /private/";
        let rt = RobotsTxt {
            robots_text: Some(rules.to_string()),
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };

        let other_domain_url = Url::parse("http://other.com/private/").unwrap();
        assert!(!rt.is_allowed(&other_domain_url, "*"));
    }

    #[test]
    fn test_is_allowed_url_without_domain() {
        // Should return true if the URL has no domain (cannot check)
        let rules = "User-agent: *\nDisallow: /private/";
        let rt = RobotsTxt {
            robots_text: Some(rules.to_string()),
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };

        // This is a relative URL, which has no domain
        let url = Url::parse("file:///private/").unwrap();
        assert!(rt.is_allowed(&url, "*"));
    }

    #[test]
    fn test_is_allowed_specific_user_agent() {
        // Only disallow for a specific user-agent
        let rules = "User-agent: special-bot\nDisallow: /blocked/\nUser-agent: *\nAllow: /";
        let rt = RobotsTxt {
            robots_text: Some(rules.to_string()),
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };

        let blocked_url = Url::parse("http://example.com/blocked/page").unwrap();
        let allowed_url = Url::parse("http://example.com/other/page").unwrap();

        assert!(!rt.is_allowed(&blocked_url, "special-bot"));
        assert!(rt.is_allowed(&blocked_url, "other-bot"));
        assert!(rt.is_allowed(&allowed_url, "special-bot"));
    }

    #[test]
    fn test_is_allowed_empty_robots_txt() {
        // Empty robots.txt should allow everything
        let rt = RobotsTxt {
            robots_text: Some("".to_string()),
            domain: "example.com".to_string(),
            parsed_robots: OnceLock::new(),
        };
        let url = Url::parse("http://example.com/any/path").unwrap();
        assert!(rt.is_allowed(&url, "*"));
    }
}
