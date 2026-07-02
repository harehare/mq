//! `http_get`/`http_post` builtins.
//!
//! Gated at compile time by the `http-import-ureq` feature and at runtime by the
//! `--allow-net` CLI flag (see [`super::capability`]) — both must be satisfied before a
//! request is made. Requests go through the same SSRF-hardened agent used for HTTP module
//! imports (see [`crate::module::resolver::ssrf`]): HTTPS only, no automatic redirects, and
//! DNS resolution filtered to publicly routable addresses so a hostname can't be rebound to
//! an internal address after the initial check.

use super::Error;
use super::capability;
use crate::RuntimeValue;
use crate::module::resolver::ssrf::ssrf_safe_agent;

/// Maximum response body size read from `http_get`/`http_post` (10 MiB).
const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

fn ensure_net_allowed(fn_name: &str) -> Result<(), Error> {
    if capability::is_net_allowed() {
        Ok(())
    } else {
        Err(Error::Runtime(format!(
            "{fn_name}: network access is disabled; re-run mq with --allow-net to enable http_get/http_post"
        )))
    }
}

fn ensure_https(fn_name: &str, url: &str) -> Result<(), Error> {
    if url.starts_with("https://") {
        Ok(())
    } else {
        Err(Error::Runtime(format!(
            "{fn_name}: only https:// URLs are allowed, got {url:?}"
        )))
    }
}

fn read_body(fn_name: &str, mut response: ureq::http::Response<ureq::Body>) -> Result<RuntimeValue, Error> {
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Runtime(format!(
            "{fn_name}: request failed with status {status}"
        )));
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_SIZE)
        .read_to_string()
        .map(RuntimeValue::String)
        .map_err(|e| Error::Runtime(format!("{fn_name}: failed to read response body: {e}")))
}

/// Performs an HTTPS GET request and returns the response body as a string.
pub(super) fn get(url: &str) -> Result<RuntimeValue, Error> {
    ensure_net_allowed("http_get")?;
    ensure_https("http_get", url)?;

    let agent = ssrf_safe_agent(TIMEOUT, true);
    let response = agent
        .get(url)
        .call()
        .map_err(|e| Error::Runtime(format!("http_get: {e}")))?;
    read_body("http_get", response)
}

/// Performs an HTTPS POST request with `body` and returns the response body as a string.
pub(super) fn post(url: &str, body: &str) -> Result<RuntimeValue, Error> {
    ensure_net_allowed("http_post")?;
    ensure_https("http_post", url)?;

    let agent = ssrf_safe_agent(TIMEOUT, true);
    let response = agent
        .post(url)
        .send(body)
        .map_err(|e| Error::Runtime(format!("http_post: {e}")))?;
    read_body("http_post", response)
}

#[cfg(test)]
mod tests {
    use super::*;

    // `capability::NET_ALLOWED` is a single process-wide flag, so every case that flips it
    // must run in one #[test] function — cargo test runs tests in parallel by default, and
    // two tests toggling the same global independently would race and flake.
    #[test]
    fn test_net_capability_gate_and_https_enforcement() {
        capability::set_allow_net(false);
        assert!(
            get("https://example.com").is_err(),
            "http_get should be blocked when --allow-net is not set"
        );
        assert!(
            post("https://example.com", "{}").is_err(),
            "http_post should be blocked when --allow-net is not set"
        );

        capability::set_allow_net(true);
        assert!(
            get("http://example.com").is_err(),
            "http_get should reject non-https URLs"
        );
        assert!(
            post("http://example.com", "{}").is_err(),
            "http_post should reject non-https URLs"
        );
        assert!(
            get("https://this-domain-should-not-exist-mq-test.invalid").is_err(),
            "http_get should surface a request error for an unresolvable host"
        );

        capability::set_allow_net(false);
    }
}
