use reqwest::Client;
use url::Url;
use robots_txt::{Robots, SimpleMatcher}; // Added SimpleMatcher

// A simple wrapper for robots.txt handling.
// It stores the text content and parses it on demand.
#[derive(Debug)]
pub struct RobotsTxt {
    robots_text: Option<String>, // Stores the content of robots.txt
    domain: String,
}

impl RobotsTxt {
    /// Fetches robots.txt for a given domain.
    /// If a custom_robots_path is provided and is a valid URL, it fetches from there.
    /// Otherwise, it constructs the default robots.txt URL (e.g., http://example.com/robots.txt).
    pub async fn fetch(client: &Client, target_url: &Url, custom_robots_path: Option<&str>) -> Result<Self, String> {
        let domain = target_url.domain().ok_or_else(|| "Failed to get domain from URL".to_string())?.to_string();

        let robots_url_str = if let Some(path) = custom_robots_path {
            if let Ok(custom_url) = Url::parse(path) {
                custom_url.to_string()
            } else {
                return Err(format!("Custom robots_path '{}' is not a valid URL. Local file paths are not yet supported for robots.txt.", path));
            }
        } else {
            let base_url_str = format!("{}://{}", target_url.scheme(), domain);
            let base_url = Url::parse(&base_url_str).map_err(|e| format!("Failed to parse base URL: {}", e))?;
            base_url.join("/robots.txt").map_err(|e| format!("Failed to join /robots.txt: {}", e))?.to_string()
        };

        let robots_url = Url::parse(&robots_url_str).map_err(|e| format!("Invalid robots.txt URL {}: {}", robots_url_str, e))?;

        tracing::info!("Fetching robots.txt from: {}", robots_url);

        match client.get(robots_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let text = response.text().await.map_err(|e| format!("Failed to read robots.txt response body: {}", e))?;
                    tracing::debug!("robots.txt content for {}:\n{}", domain, text);
                    Ok(RobotsTxt { robots_text: Some(text), domain })
                } else {
                    tracing::warn!("Failed to fetch robots.txt for {}: HTTP {}", domain, response.status());
                    Ok(RobotsTxt { robots_text: None, domain }) // No robots.txt or error, assume crawl is allowed
                }
            }
            Err(e) => {
                tracing::warn!("Error fetching robots.txt for {}: {}", domain, e);
                Ok(RobotsTxt { robots_text: None, domain }) // Network error, assume crawl is allowed
            }
        }
    }

    /// Checks if a URL is allowed to be crawled by a specific user-agent.
    /// Parses the stored robots.txt content on each call.
    pub fn is_allowed(&self, url_to_check: &Url, user_agent: &str) -> bool {
        let parsed_robots = match &self.robots_text {
            Some(text) => Robots::from_str(text),
            None => return true, // No robots.txt content, so allow by default.
        };

        // Ensure the URL is for the same domain this robots.txt is for.
        if url_to_check.domain().map_or(true, |d| d != self.domain) {
            tracing::warn!("Checking URL {} against robots.txt for a different domain {}", url_to_check, self.domain);
            // This case is debatable: should it allow or deny?
            // For now, allow, assuming the caller is responsible for calling with correct robots.txt.
            return true;
        }

        let section = parsed_robots.choose_section(user_agent);
        // SimpleMatcher::new expects a slice of rules.
        // section.rules is Vec<Rule<'a>>, so &section.rules or section.rules.as_slice() works.
        let matcher = SimpleMatcher::new(&section.rules);

        matcher.check_path(url_to_check.path())
    }
}

// TODO: Consider a RobotsCache to store RobotsTxt per domain if crawling multiple domains,
// but for a single-site crawler, this might be overkill if parsing on demand is acceptable.

#[cfg(test)]
mod tests {
    use super::*;
    // reqwest::Client is not needed here anymore as tests are synchronous for `is_allowed`
    // and `fetch` is not directly unit-tested without a mock.

    #[test]
    fn test_robots_txt_parsing_basic_direct() {
        // This test now verifies the full logic via is_allowed for a specific text
        let rules_text = "User-agent: *\nDisallow: /private/\nAllow: /public/";
        let rt = RobotsTxt {
            robots_text: Some(rules_text.to_string()),
            domain: "example.com".to_string(),
        };

        let private_url = Url::parse("http://example.com/private/something").unwrap();
        let public_url = Url::parse("http://example.com/public/page").unwrap();
        let other_url = Url::parse("http://example.com/anywhere_else").unwrap();

        assert!(!rt.is_allowed(&private_url, "*"));
        assert!(rt.is_allowed(&public_url, "*"));
        assert!(rt.is_allowed(&other_url, "*"));
    }

    #[test]
    fn test_is_allowed_with_no_robots_text() { // Updated test name
        let rt = RobotsTxt { robots_text: None, domain: "example.com".to_string() };
        let url = Url::parse("http://example.com/some/path").unwrap();
        assert!(rt.is_allowed(&url, "test-agent"));
    }

    #[test]
    fn test_is_allowed_with_simple_disallow_text() { // Updated test name
        let rules = "User-agent: test-agent
Disallow: /forbidden/";
        let rt = RobotsTxt { robots_text: Some(rules.to_string()), domain: "example.com".to_string() };

        let allowed_url = Url::parse("http://example.com/allowed/path").unwrap();
        let disallowed_url = Url::parse("http://example.com/forbidden/path").unwrap();

        assert!(rt.is_allowed(&allowed_url, "test-agent"));
        assert!(!rt.is_allowed(&disallowed_url, "test-agent"));
        // Different agent should be allowed as per typical robots.txt behavior (rules are specific)
        assert!(rt.is_allowed(&disallowed_url, "another-agent"));
    }

    // A proper test for `fetch` would require a mock HTTP server or actual network calls.
    // Example (requires internet and a site with robots.txt, keep ignored):
    /*
    use reqwest::Client; // Keep Client for this specific ignored test
    #[tokio::test]
    #[ignore] // Ignored because it makes a network request
    async fn test_fetch_real_robots_txt() {
        let client = Client::new();
        let url = Url::parse("https://www.google.com/").unwrap(); // Test with a known URL

        match RobotsTxt::fetch(&client, &url, None).await {
            Ok(robots_rules) => {
                assert!(robots_rules.robots_text.is_some(), "Expected to fetch some robots.txt data from Google");
                // Check if "search" is disallowed for a common bot (example)
                // This is illustrative and might change.
                // assert!(!robots_rules.is_allowed(&Url::parse("https://www.google.com/search").unwrap(), "Googlebot"));
            }
            Err(e) => {
                panic!("Failed to fetch robots.txt from google.com: {}", e);
            }
        }
    }
    */
}
