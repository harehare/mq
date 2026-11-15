use crate::http_client::{FetchErrorKind, HttpClient};
use robots_txt::{Robots, matcher::SimpleMatcher};
use std::path::Path;
use url::Url;

/// What a site's robots.txt permits.
#[derive(Debug)]
enum Policy {
    Rules(String),
    AllowAll,
    DisallowAll,
}

/// Wrapper for robots.txt handling, scoped to one origin (scheme, host and port).
#[derive(Debug)]
pub struct RobotsTxt {
    policy: Policy,
    origin: String,
}

/// Cache key identifying the robots.txt that governs `url`.
pub(crate) fn origin_key(url: &Url) -> Option<String> {
    let origin = url.origin();
    origin.is_tuple().then(|| origin.ascii_serialization())
}

impl RobotsTxt {
    /// Fetches robots.txt for the origin of `target_url`.
    /// If a custom_robots_path is an http(s) URL it fetches from there; a `file://` URL or
    /// plain path is read from disk.
    /// Otherwise, it uses `/robots.txt` on the same scheme, host and port.
    ///
    /// Follows RFC 9309: a `4xx` response (other than `429`) means no rules apply,
    /// while an unreachable file or a `5xx` response disallows everything.
    pub async fn fetch(
        http_client: &HttpClient,
        target_url: &Url,
        custom_robots_path: Option<&str>,
    ) -> Result<Self, String> {
        let origin =
            origin_key(target_url).ok_or_else(|| format!("Target URL {} does not have a valid host.", target_url))?;

        let robots_url_str = match custom_robots_path {
            Some(path) => match Url::parse(path) {
                Ok(url) if matches!(url.scheme(), "http" | "https") => url.to_string(),
                Ok(url) if url.scheme() == "file" => {
                    let file = url
                        .to_file_path()
                        .map_err(|_| format!("Invalid file URL for robots_path '{}'.", path))?;
                    return Self::from_file(&file, origin);
                }
                _ => return Self::from_file(Path::new(path), origin),
            },
            None => format!("{}/robots.txt", origin),
        };

        let robots_url =
            Url::parse(&robots_url_str).map_err(|e| format!("Invalid robots.txt URL {}: {}", robots_url_str, e))?;

        tracing::info!("Fetching robots.txt from: {}", robots_url);

        let policy = match http_client.fetch_with(robots_url, Default::default()).await {
            Ok(response) => {
                tracing::debug!("robots.txt content for {}:\n{}", origin, response);
                Policy::Rules(response)
            }
            Err(e) => match e.kind {
                FetchErrorKind::Status(code) if (400..500).contains(&code) && code != 429 => {
                    tracing::info!("No robots.txt for {} (status {}), allowing all.", origin, code);
                    Policy::AllowAll
                }
                _ => {
                    tracing::warn!("robots.txt for {} is unreachable ({}), disallowing all.", origin, e);
                    Policy::DisallowAll
                }
            },
        };

        Ok(RobotsTxt { policy, origin })
    }

    fn from_file(path: &Path, origin: String) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read robots.txt file '{}': {}", path.display(), e))?;
        tracing::info!("Using robots.txt from local file: {}", path.display());
        Ok(RobotsTxt {
            policy: Policy::Rules(text),
            origin,
        })
    }

    /// Checks if a URL is allowed to be crawled by a specific user-agent.
    /// A URL from another origin is never allowed.
    pub fn is_allowed(&self, url_to_check: &Url, user_agent: &str) -> bool {
        if origin_key(url_to_check).as_deref() != Some(self.origin.as_str()) {
            tracing::warn!(
                "Checking URL {} against robots.txt for a different origin {}",
                url_to_check,
                self.origin
            );
            return false;
        }

        match &self.policy {
            Policy::AllowAll => true,
            Policy::DisallowAll => false,
            Policy::Rules(text) => {
                let robots = Robots::from_str_lossy(text);
                let section = robots.choose_section(user_agent);
                SimpleMatcher::new(&section.rules).check_path(url_to_check.path())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_client::RetryConfig;
    use httpmock::{Method::GET, MockServer};
    use reqwest::header::HeaderMap;
    use std::sync::Once;
    use tokio::runtime::Runtime;

    static INIT: Once = Once::new();
    fn init_tracing() {
        INIT.call_once(|| {
            let _ = tracing_subscriber::fmt::try_init();
        });
    }

    fn robots_with(policy: Policy) -> RobotsTxt {
        RobotsTxt {
            policy,
            origin: "http://example.com".to_string(),
        }
    }

    fn robots_with_rules(rules: &str) -> RobotsTxt {
        robots_with(Policy::Rules(rules.to_string()))
    }

    fn no_retry_client() -> HttpClient {
        HttpClient::new_reqwest_with_options(30.0, 3, RetryConfig::disabled(), HeaderMap::new(), None).unwrap()
    }

    #[test]
    fn test_fetch_local_file_path_and_file_url() {
        let dir = std::env::temp_dir().join(format!("mq_robots_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("robots.txt");
        std::fs::write(&file, "User-agent: *\nDisallow: /private/").unwrap();
        let url_str = Url::from_file_path(&file).unwrap().to_string();
        let target = Url::parse("http://example.com/private/x").unwrap();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            for p in [file.to_str().unwrap(), url_str.as_str()] {
                let r = RobotsTxt::fetch(&no_retry_client(), &target, Some(p)).await.unwrap();
                assert!(!r.is_allowed(&target, "bot"));
                assert!(r.is_allowed(&Url::parse("http://example.com/ok").unwrap(), "bot"));
            }
            let missing = dir.join("nope.txt");
            assert!(
                RobotsTxt::fetch(&no_retry_client(), &target, missing.to_str())
                    .await
                    .is_err()
            );
        });
        let _ = std::fs::remove_dir_all(&dir);
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

            assert_eq!(robots.origin, format!("http://{}", server.address()));
            assert!(matches!(&robots.policy, Policy::Rules(text) if text == robots_body));
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_fetch_keeps_port_for_host_name() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            let mock = server
                .mock_async(|when, then| {
                    when.method(GET).path("/robots.txt");
                    then.status(200).body("User-agent: *\nDisallow: /private");
                })
                .await;

            let http_client = HttpClient::new_reqwest(30.0).unwrap();
            let url = Url::parse(&format!("http://localhost:{}/", server.address().port())).unwrap();
            let robots = RobotsTxt::fetch(&http_client, &url, None).await.unwrap();

            mock.assert_async().await;
            let private = Url::parse(&format!("http://localhost:{}/private/x", server.address().port())).unwrap();
            assert!(!robots.is_allowed(&private, "*"));
        });
    }

    #[test]
    fn test_fetch_not_found_allows_all() {
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

            assert!(matches!(robots.policy, Policy::AllowAll));
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_fetch_server_error_disallows_all() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            let mock = server
                .mock_async(|when, then| {
                    when.method(GET).path("/robots.txt");
                    then.status(503);
                })
                .await;

            let url = Url::parse(&format!("http://{}", server.address())).unwrap();
            let robots = RobotsTxt::fetch(&no_retry_client(), &url, None).await.unwrap();

            assert!(matches!(robots.policy, Policy::DisallowAll));
            assert!(!robots.is_allowed(&url, "*"));
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_fetch_too_many_requests_disallows_all() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let server = MockServer::start_async().await;
            server
                .mock_async(|when, then| {
                    when.method(GET).path("/robots.txt");
                    then.status(429);
                })
                .await;

            let url = Url::parse(&format!("http://{}", server.address())).unwrap();
            let robots = RobotsTxt::fetch(&no_retry_client(), &url, None).await.unwrap();

            assert!(matches!(robots.policy, Policy::DisallowAll));
        });
    }

    #[test]
    fn test_fetch_unreachable_disallows_all() {
        init_tracing();
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            // Bind then drop to get a port nothing listens on.
            let port = std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port();
            let url = Url::parse(&format!("http://127.0.0.1:{}/", port)).unwrap();
            let robots = RobotsTxt::fetch(&no_retry_client(), &url, None).await.unwrap();

            assert!(matches!(robots.policy, Policy::DisallowAll));
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
            let robots = RobotsTxt::fetch(&http_client, &url, Some(&custom_url)).await.unwrap();

            assert!(matches!(&robots.policy, Policy::Rules(text) if text == robots_body));
            mock.assert_async().await;
        });
    }

    #[test]
    fn test_origin_key_includes_scheme_and_port() {
        let key = |s: &str| origin_key(&Url::parse(s).unwrap());
        assert_eq!(key("http://localhost:8080/a").as_deref(), Some("http://localhost:8080"));
        assert_eq!(key("https://example.com:443/a").as_deref(), Some("https://example.com"));
        assert_eq!(key("http://[::1]:80/a").as_deref(), Some("http://[::1]"));
        assert_ne!(key("http://localhost:1/"), key("http://localhost:2/"));
        assert_eq!(key("data:text/plain,hello"), None);
    }

    #[test]
    fn test_robots_txt_parsing_basic_direct() {
        let rt = robots_with_rules("User-agent: *\nDisallow: /private/\nAllow: /public/");

        let private_url = Url::parse("http://example.com/private/something").unwrap();
        let public_url = Url::parse("http://example.com/public/page").unwrap();
        let other_url = Url::parse("http://example.com/anywhere_else").unwrap();

        assert!(!rt.is_allowed(&private_url, "*"));
        assert!(rt.is_allowed(&public_url, "*"));
        assert!(rt.is_allowed(&other_url, "*"));
    }

    #[test]
    fn test_is_allowed_with_allow_all_policy() {
        let rt = robots_with(Policy::AllowAll);
        let url = Url::parse("http://example.com/some/path").unwrap();
        assert!(rt.is_allowed(&url, "test-agent"));
    }

    #[test]
    fn test_is_allowed_with_simple_disallow_text() {
        let rt = robots_with_rules("User-agent: test-agent\nDisallow: /forbidden/");

        let allowed_url = Url::parse("http://example.com/allowed/path").unwrap();
        let disallowed_url = Url::parse("http://example.com/forbidden/path").unwrap();

        assert!(rt.is_allowed(&allowed_url, "test-agent"));
        assert!(!rt.is_allowed(&disallowed_url, "test-agent"));
        assert!(rt.is_allowed(&disallowed_url, "another-agent"));
    }

    #[test]
    fn test_is_allowed_different_origin() {
        let rt = robots_with(Policy::AllowAll);

        assert!(!rt.is_allowed(&Url::parse("http://other.com/private/").unwrap(), "*"));
        assert!(!rt.is_allowed(&Url::parse("http://example.com:8080/").unwrap(), "*"));
        assert!(!rt.is_allowed(&Url::parse("https://example.com/").unwrap(), "*"));
    }

    #[test]
    fn test_is_allowed_url_without_host() {
        let rt = robots_with(Policy::AllowAll);
        assert!(!rt.is_allowed(&Url::parse("file:///private/").unwrap(), "*"));
    }

    #[test]
    fn test_is_allowed_specific_user_agent() {
        let rt = robots_with_rules("User-agent: special-bot\nDisallow: /blocked/\nUser-agent: *\nAllow: /");

        let blocked_url = Url::parse("http://example.com/blocked/page").unwrap();
        let allowed_url = Url::parse("http://example.com/other/page").unwrap();

        assert!(!rt.is_allowed(&blocked_url, "special-bot"));
        assert!(rt.is_allowed(&blocked_url, "other-bot"));
        assert!(rt.is_allowed(&allowed_url, "special-bot"));
    }

    #[test]
    fn test_is_allowed_empty_robots_txt() {
        let rt = robots_with_rules("");
        let url = Url::parse("http://example.com/any/path").unwrap();
        assert!(rt.is_allowed(&url, "*"));
    }
}
