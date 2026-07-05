//! `http` builtin: issues an HTTPS request using any method (`get`, `post`, `put`, `delete`,
//! `patch`, `head`, ...) and returns the response body as a string.
//!
//! Gated at compile time by the `http-import-ureq` feature and at runtime by the
//! `--allow-net` CLI flag (see [`super::capability`]) — both must be satisfied before a
//! request is made. Requests go through the same SSRF-hardened agent used for HTTP module
//! imports (see [`crate::module::resolver::ssrf`]): HTTPS only, no automatic redirects, and
//! DNS resolution filtered to publicly routable addresses so a hostname can't be rebound to
//! an internal address after the initial check.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use ureq::http;

use super::Error;
use super::capability;
use crate::module::resolver::ssrf::{is_https, ssrf_safe_agent};
use crate::{Ident, RuntimeValue};

/// Maximum response body size read from `http` (10 MiB).
const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Built once and reused so repeated calls share connection pooling.
static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| ssrf_safe_agent(TIMEOUT, true));

fn ensure_net_allowed() -> Result<(), Error> {
    if capability::is_net_allowed() {
        Ok(())
    } else {
        Err(Error::Runtime(
            "http: network access is disabled; re-run mq with --allow-net to enable http".into(),
        ))
    }
}

fn ensure_https(url: &str) -> Result<(), Error> {
    if is_https(url) {
        Ok(())
    } else {
        Err(Error::Runtime(format!(
            "http: only https:// URLs are allowed, got {url:?}"
        )))
    }
}

/// Accepts either a string (`"post"`) or a symbol (`:post`) method name, case-insensitively.
fn parse_method(value: &RuntimeValue) -> Result<http::Method, Error> {
    let name = match value {
        RuntimeValue::Symbol(name) => name.as_str(),
        RuntimeValue::String(name) => name.clone(),
        other => {
            return Err(Error::Runtime(format!(
                "http: method must be a string or symbol, got {other}"
            )));
        }
    };
    name.to_ascii_uppercase()
        .parse::<http::Method>()
        .map_err(|_| Error::Runtime(format!("http: invalid HTTP method {name:?}")))
}

fn read_body(mut response: http::Response<ureq::Body>) -> Result<RuntimeValue, Error> {
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Runtime(format!("http: request failed with status {status}")));
    }
    response
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_SIZE)
        .read_to_string()
        .map(RuntimeValue::String)
        .map_err(|e| Error::Runtime(format!("http: failed to read response body: {e}")))
}

/// Applies `headers` to `builder`, requiring every value to be a string. Invalid header
/// names/values (e.g. containing CR/LF) are caught later when the request is built.
fn apply_headers(
    mut builder: http::request::Builder,
    headers: Option<&BTreeMap<Ident, RuntimeValue>>,
) -> Result<http::request::Builder, Error> {
    let Some(headers) = headers else {
        return Ok(builder);
    };
    for (name, value) in headers {
        let value = match value {
            RuntimeValue::String(value) => value.as_str(),
            other => {
                return Err(Error::Runtime(format!(
                    "http: header {name:?} must be a string, got {other}"
                )));
            }
        };
        builder = builder.header(name.as_str(), value);
    }
    Ok(builder)
}

/// Performs an HTTPS request with the given `method` and returns the response body as a string.
/// `body`, when present, is sent as the request body regardless of method. `headers`, when
/// present, are applied to the request; every header value must be a string.
pub(super) fn request(
    method: &RuntimeValue,
    url: &str,
    body: Option<&str>,
    headers: Option<&BTreeMap<Ident, RuntimeValue>>,
) -> Result<RuntimeValue, Error> {
    ensure_net_allowed()?;
    ensure_https(url)?;
    let method = parse_method(method)?;
    let builder = apply_headers(http::Request::builder().method(method).uri(url), headers)?;

    let response = match body {
        Some(body) => {
            let request = builder
                .body(body.to_string())
                .map_err(|e| Error::Runtime(format!("http: {e}")))?;
            AGENT.run(request)
        }
        None => {
            let request = builder.body(()).map_err(|e| Error::Runtime(format!("http: {e}")))?;
            AGENT.run(request)
        }
    }
    .map_err(|e| Error::Runtime(format!("http: {e}")))?;

    read_body(response)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::Ident;

    const ALL_METHODS: &[&str] = &[
        "get", "head", "post", "put", "delete", "connect", "options", "trace", "patch",
    ];

    fn symbol(name: &str) -> RuntimeValue {
        RuntimeValue::Symbol(Ident::new(name))
    }

    #[rstest]
    #[case::get("get", "GET")]
    #[case::head("head", "HEAD")]
    #[case::post("post", "POST")]
    #[case::put("put", "PUT")]
    #[case::delete("delete", "DELETE")]
    #[case::connect("connect", "CONNECT")]
    #[case::options("options", "OPTIONS")]
    #[case::trace("trace", "TRACE")]
    #[case::patch("patch", "PATCH")]
    #[case::uppercase("POST", "POST")]
    #[case::mixed_case("PoSt", "POST")]
    #[case::webdav_extension_token("propfind", "PROPFIND")]
    fn test_parse_method_accepts_symbol_and_string(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(parse_method(&symbol(input)).unwrap().as_str(), expected);
        assert_eq!(
            parse_method(&RuntimeValue::String(input.into())).unwrap().as_str(),
            expected
        );
    }

    #[rstest]
    #[case::empty("")]
    #[case::space_in_token("in valid")]
    #[case::control_char("get\n")]
    fn test_parse_method_rejects_invalid_method_strings(#[case] input: &str) {
        assert!(parse_method(&symbol(input)).is_err());
        assert!(parse_method(&RuntimeValue::String(input.into())).is_err());
    }

    #[rstest]
    #[case::number(RuntimeValue::from(1usize))]
    #[case::boolean(RuntimeValue::from(true))]
    #[case::none(RuntimeValue::NONE)]
    fn test_parse_method_rejects_non_string_non_symbol(#[case] value: RuntimeValue) {
        assert!(parse_method(&value).is_err());
    }

    // `capability::NET_ALLOWED` is a single process-wide flag, so every case that flips it
    // must run in one #[test] function — cargo test runs tests in parallel by default, and
    // two tests toggling the same global independently would race and flake.
    #[test]
    fn test_net_capability_gate_and_https_enforcement() {
        capability::set_allow_net(false);
        for name in ALL_METHODS {
            assert!(
                request(&symbol(name), "https://example.com", None, None).is_err(),
                "http({name}, ..) should be blocked when --allow-net is not set"
            );
        }
        assert!(
            request(&symbol("post"), "https://example.com", Some("{}"), None).is_err(),
            "http should be blocked when --allow-net is not set, even with a body"
        );

        capability::set_allow_net(true);
        for name in ALL_METHODS {
            assert!(
                request(&symbol(name), "http://example.com", None, None).is_err(),
                "http({name}, ..) should reject non-https URLs"
            );
        }
        assert!(
            request(
                &RuntimeValue::String("bogus method".into()),
                "https://example.com",
                None,
                None
            )
            .is_err(),
            "http should reject unknown methods"
        );
        assert!(
            request(
                &symbol("get"),
                "https://this-domain-should-not-exist-mq-test.invalid",
                None,
                None
            )
            .is_err(),
            "http should surface a request error for an unresolvable host"
        );
        assert!(
            request(
                &symbol("delete"),
                "https://this-domain-should-not-exist-mq-test.invalid",
                None,
                None
            )
            .is_err(),
            "http should surface a request error for an unresolvable host regardless of method"
        );
        assert!(
            request(
                &symbol("get"),
                "https://this-domain-should-not-exist-mq-test.invalid",
                None,
                Some(&BTreeMap::from([(
                    Ident::new("Authorization"),
                    RuntimeValue::String("Bearer token".into())
                )]))
            )
            .is_err(),
            "http should surface a request error for an unresolvable host even with headers set"
        );

        capability::set_allow_net(false);
    }

    #[test]
    fn test_apply_headers_accepts_string_values() {
        let builder = http::Request::builder()
            .method(http::Method::GET)
            .uri("https://example.com");
        let headers = BTreeMap::from([
            (Ident::new("X-Test"), RuntimeValue::String("value".into())),
            (
                Ident::new("Content-Type"),
                RuntimeValue::String("application/json".into()),
            ),
        ]);
        let request = apply_headers(builder, Some(&headers)).unwrap().body(()).unwrap();

        assert_eq!(request.headers().get("X-Test").unwrap(), "value");
        assert_eq!(request.headers().get("Content-Type").unwrap(), "application/json");
    }

    #[test]
    fn test_apply_headers_rejects_non_string_values() {
        let builder = http::Request::builder()
            .method(http::Method::GET)
            .uri("https://example.com");
        let headers = BTreeMap::from([(Ident::new("X-Test"), RuntimeValue::from(1usize))]);

        assert!(apply_headers(builder, Some(&headers)).is_err());
    }

    #[test]
    fn test_apply_headers_passthrough_when_none() {
        let builder = http::Request::builder()
            .method(http::Method::GET)
            .uri("https://example.com");

        assert!(apply_headers(builder, None).unwrap().body(()).is_ok());
    }
}
