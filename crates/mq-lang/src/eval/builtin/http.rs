//! `http` builtin: issues an HTTPS request using any method (`get`, `post`, `put`, `delete`,
//! `patch`, `head`, ...) and returns the response body as a string.
//!
//! Gated at compile time by the `http` feature (implied by `http-import-ureq`) and at
//! runtime by the ambient [`Io`](crate::io::Io)'s net permission (see
//! [`super::io_context`]) — both must be satisfied before a request is made. Requests
//! ultimately go through the same SSRF-hardened agent used for HTTP module imports (see
//! [`crate::module::resolver::ssrf`]): HTTPS only, no automatic redirects, and DNS
//! resolution filtered to publicly routable addresses so a hostname can't be rebound to
//! an internal address after the initial check.

use std::collections::BTreeMap;

use super::Error;
use super::io_context;
use crate::{Ident, RuntimeValue};

/// Builds an `Error::Runtime` with the `http: ` prefix shared by every error in this module.
fn err(msg: impl std::fmt::Display) -> Error {
    Error::Runtime(format!("http: {msg}"))
}

/// Accepts either a string (`"post"`) or a symbol (`:post`) method name, case-insensitively,
/// returning the normalized (uppercased) method name.
fn parse_method(value: &RuntimeValue) -> Result<String, Error> {
    let name = match value {
        RuntimeValue::Symbol(name) => name.as_str().to_string(),
        RuntimeValue::String(name) => name.clone(),
        other => return Err(err(format!("method must be a string or symbol, got {other}"))),
    };
    let upper = name.to_ascii_uppercase();
    upper
        .parse::<ureq::http::Method>()
        .map_err(|_| err(format!("invalid HTTP method {name:?}")))?;
    Ok(upper)
}

/// Extracts `(name, value)` pairs from `headers`, requiring every value to be a string.
fn extract_headers(headers: Option<&BTreeMap<Ident, RuntimeValue>>) -> Result<Vec<(String, String)>, Error> {
    let Some(headers) = headers else {
        return Ok(Vec::new());
    };
    headers
        .iter()
        .map(|(name, value)| match value {
            RuntimeValue::String(value) => Ok((name.as_str(), value.clone())),
            other => Err(err(format!("header {name:?} must be a string, got {other}"))),
        })
        .collect()
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
    let method = parse_method(method)?;
    let headers = extract_headers(headers)?;
    io_context::current()
        .http_request(&method, url, body, &headers)
        .map(RuntimeValue::String)
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::Ident;
    use crate::io::{MemIo, NativeIo, SandboxedIo};

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
        assert_eq!(parse_method(&symbol(input)).unwrap(), expected);
        assert_eq!(parse_method(&RuntimeValue::String(input.into())).unwrap(), expected);
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

    #[test]
    fn test_net_capability_gate_and_https_enforcement() {
        // No guard installed: ambient Io falls back to all-denied.
        for name in ALL_METHODS {
            assert!(
                request(&symbol(name), "https://example.invalid", None, None).is_err(),
                "http({name}, ..) should be blocked when net access is not allowed"
            );
        }
        assert!(
            request(&symbol("post"), "https://example.invalid", Some("{}"), None).is_err(),
            "http should be blocked when net access is not allowed, even with a body"
        );

        let _guard = io_context::scoped(crate::Shared::new(
            SandboxedIo::new(NativeIo::default()).allow_net(true),
        ));

        for name in ALL_METHODS {
            assert!(
                request(&symbol(name), "http://example.invalid", None, None).is_err(),
                "http({name}, ..) should reject non-https URLs"
            );
        }
        assert!(
            request(
                &RuntimeValue::String("bogus method".into()),
                "https://example.invalid",
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
    }

    #[test]
    fn test_request_uses_ambient_mem_io() {
        let _guard = io_context::scoped(crate::Shared::new(
            SandboxedIo::new(MemIo::default().with_fetch_response("https://example.invalid", "body")).allow_net(true),
        ));
        assert_eq!(
            request(&symbol("get"), "https://example.invalid", None, None).unwrap(),
            RuntimeValue::String("body".to_string())
        );
    }

    #[test]
    fn test_extract_headers_accepts_string_values() {
        let headers = BTreeMap::from([
            (Ident::new("X-Test"), RuntimeValue::String("value".into())),
            (
                Ident::new("Content-Type"),
                RuntimeValue::String("application/json".into()),
            ),
        ]);
        let extracted = extract_headers(Some(&headers)).unwrap();
        assert!(extracted.contains(&("Content-Type".to_string(), "application/json".to_string())));
        assert!(extracted.contains(&("X-Test".to_string(), "value".to_string())));
    }

    #[test]
    fn test_extract_headers_rejects_non_string_values() {
        let headers = BTreeMap::from([(Ident::new("X-Test"), RuntimeValue::from(1usize))]);
        assert!(extract_headers(Some(&headers)).is_err());
    }

    #[test]
    fn test_extract_headers_passthrough_when_none() {
        assert!(extract_headers(None).unwrap().is_empty());
    }
}
