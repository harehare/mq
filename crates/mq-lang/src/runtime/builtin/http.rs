//! `http` builtin: issues an HTTPS request using any method (`get`, `post`, `put`, `delete`,
//! `patch`, `head`, ...) and returns the response body as a string.
//!
//! Gated at compile time by the `http` feature (implied by `http-import-ureq`) and at
//! runtime by the ambient [`Io`](crate::io::Io)'s net permission (see
//! [`super::io_context`]); both must be satisfied before a request is made. Requests
//! ultimately go through the same SSRF-hardened agent used for HTTP module imports (see
//! [`crate::module::resolver::ssrf`]): HTTPS only, no automatic redirects, and DNS
//! resolution filtered to publicly routable addresses so a hostname can't be rebound to
//! an internal address after the initial check.

use crate::DictMap;
use crate::Ident;

use super::Error;
use super::io_context;
use crate::RuntimeValue;
use crate::Shared;
use crate::runtime::reader_handle::{HandleKind, ReaderHandle};

/// Builds an `Error::Runtime` with the `http: ` prefix shared by every error in this module.
fn err(msg: impl std::fmt::Display) -> Error {
    Error::Runtime(format!("http: {msg}"))
}

/// Accepts either a string (`"post"`) or a symbol (`:post`) method name, case-insensitively,
/// returning the normalized (uppercased) method name.
fn parse_method(value: &RuntimeValue) -> Result<String, Error> {
    let name = match value {
        RuntimeValue::Symbol(name) => name.to_string(),
        RuntimeValue::String(name) => name.to_string(),
        other => return Err(err(format!("method must be a string or symbol, got {other}"))),
    };
    let upper = name.to_ascii_uppercase();
    upper
        .parse::<ureq::http::Method>()
        .map_err(|_| err(format!("invalid HTTP method {name:?}")))?;
    Ok(upper)
}

/// Extracts `(name, value)` pairs from `headers`, requiring every value to be a string.
fn extract_headers(headers: Option<&DictMap>) -> Result<Vec<(String, String)>, Error> {
    let Some(headers) = headers else {
        return Ok(Vec::new());
    };
    headers
        .iter()
        .map(|(name, value)| match value {
            RuntimeValue::String(value) => Ok((name.to_string(), value.to_string())),
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
    body: Option<&[u8]>,
    headers: Option<&DictMap>,
) -> Result<RuntimeValue, Error> {
    let method = parse_method(method)?;
    let headers = extract_headers(headers)?;
    io_context::current()
        .http_request(&method, url, body, &headers)
        .map(|s| RuntimeValue::String(s.into()))
        .map_err(err)
}

/// Escapes a form field name or filename for use inside a quoted `Content-Disposition` parameter.
fn escape_disposition(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

/// A boundary that does not occur in any part body.
fn pick_boundary(bodies: &[&[u8]]) -> String {
    use std::hash::{BuildHasher, Hasher};

    let state = std::collections::hash_map::RandomState::new();
    loop {
        let mut hasher = state.build_hasher();
        hasher.write_u8(0);
        let boundary = format!(
            "mq-boundary-{:016x}{:016x}",
            hasher.finish(),
            state.hash_one(bodies.len())
        );
        let needle = boundary.as_bytes();
        if !bodies
            .iter()
            .any(|body| body.windows(needle.len()).any(|w| w == needle))
        {
            return boundary;
        }
    }
}

/// Encodes `parts` as `multipart/form-data`, returning `{body: bytes, content_type: string}`.
/// Each part is a dict with `name` and `value` (string or bytes), and optional `filename` and
/// `content_type`. Bytes values default to `application/octet-stream`.
pub(super) fn multipart(parts: &[RuntimeValue]) -> Result<RuntimeValue, Error> {
    struct Part<'a> {
        header: String,
        value: &'a [u8],
    }

    let text = |fields: &DictMap, key: &str| -> Result<Option<String>, Error> {
        match fields.get(&Ident::from(key)) {
            None => Ok(None),
            Some(v) if v.is_none() => Ok(None),
            Some(RuntimeValue::String(s)) => Ok(Some(s.to_string())),
            Some(other) => Err(err(format!("multipart: `{key}` must be a string, got {other}"))),
        }
    };

    let mut encoded = Vec::with_capacity(parts.len());
    for part in parts {
        let RuntimeValue::Dict(fields) = part else {
            return Err(err("multipart: each part must be a dict with `name` and `value`"));
        };
        let name = text(fields, "name")?.ok_or_else(|| err("multipart: each part needs a `name` string"))?;
        let (value, is_bytes): (&[u8], bool) = match fields.get(&Ident::from("value")) {
            Some(RuntimeValue::String(s)) => (s.as_bytes(), false),
            Some(RuntimeValue::Bytes(b)) => (b.as_slice(), true),
            Some(other) => {
                return Err(err(format!(
                    "multipart: `value` must be a string or bytes, got {other}"
                )));
            }
            None => return Err(err("multipart: each part needs a `value`")),
        };
        let mut header = format!("Content-Disposition: form-data; name=\"{}\"", escape_disposition(&name));
        let filename = text(fields, "filename")?;
        if let Some(filename) = &filename {
            header.push_str(&format!("; filename=\"{}\"", escape_disposition(filename)));
        }
        let content_type = match text(fields, "content_type")? {
            Some(ct) if ct.contains(['\r', '\n']) => {
                return Err(err("multipart: `content_type` must not contain line breaks"));
            }
            Some(ct) => Some(ct),
            None if is_bytes || filename.is_some() => Some("application/octet-stream".to_string()),
            None => None,
        };
        if let Some(ct) = content_type {
            header.push_str(&format!("\r\nContent-Type: {ct}"));
        }
        encoded.push(Part { header, value });
    }

    let bodies: Vec<&[u8]> = encoded.iter().map(|p| p.value).collect();
    let boundary = pick_boundary(&bodies);
    let mut body = Vec::new();
    for part in &encoded {
        body.extend_from_slice(format!("--{boundary}\r\n{}\r\n\r\n", part.header).as_bytes());
        body.extend_from_slice(part.value);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let mut result = DictMap::default();
    result.insert(Ident::from("body"), RuntimeValue::Bytes(Shared::new(body)));
    result.insert(
        Ident::from("content_type"),
        RuntimeValue::String(format!("multipart/form-data; boundary={boundary}").into()),
    );
    Ok(RuntimeValue::Dict(Shared::new(result)))
}

/// Opens the response body as a streaming reader handle instead of buffering it, backing
/// `open_http`. Same method/body/header handling as [`request`].
pub(super) fn open_stream(
    method: &RuntimeValue,
    url: &str,
    body: Option<&[u8]>,
    headers: Option<&DictMap>,
) -> Result<RuntimeValue, Error> {
    let method = parse_method(method)?;
    let headers = extract_headers(headers)?;
    io_context::current()
        .http_request_stream(&method, url, body, &headers)
        .map(|reader| {
            RuntimeValue::ReaderHandle(Shared::new(ReaderHandle::new(
                HandleKind::Http,
                url.to_string(),
                reader,
            )))
        })
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use crate::Shared;
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

    fn part(fields: &[(&str, RuntimeValue)]) -> RuntimeValue {
        let mut map = DictMap::default();
        for (key, value) in fields {
            map.insert(Ident::new(key), value.clone());
        }
        RuntimeValue::Dict(Shared::new(map))
    }

    fn text(value: &str) -> RuntimeValue {
        RuntimeValue::String(value.to_string().into())
    }

    fn field(result: &RuntimeValue, key: &str) -> RuntimeValue {
        let RuntimeValue::Dict(map) = result else {
            panic!("expected dict")
        };
        map.get(&Ident::new(key)).cloned().unwrap()
    }

    #[rstest]
    #[case::quote("a\"b", "a%22b")]
    #[case::backslash("a\\b", "a\\\\b")]
    #[case::trailing_backslash("a\\", "a\\\\")]
    #[case::line_breaks("a\r\nb", "a%0D%0Ab")]
    fn test_escape_disposition(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(escape_disposition(input), expected);
    }

    #[test]
    fn test_multipart_encodes_text_and_file_parts() {
        let parts = [
            part(&[("name", text("title")), ("value", text("hi"))]),
            part(&[
                ("name", text("file")),
                ("value", RuntimeValue::Bytes(Shared::new(vec![0, 255, 10]))),
                ("filename", text("a\"b\\c.bin")),
            ]),
        ];
        let result = multipart(&parts).unwrap();
        let RuntimeValue::String(content_type) = field(&result, "content_type") else {
            panic!()
        };
        let boundary = content_type.strip_prefix("multipart/form-data; boundary=").unwrap();
        let RuntimeValue::Bytes(body) = field(&result, "body") else {
            panic!()
        };

        let mut expected = Vec::new();
        expected.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"title\"\r\n\r\nhi\r\n").as_bytes(),
        );
        expected.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a%22b\\\\c.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n"
            )
            .as_bytes(),
        );
        expected.extend_from_slice(&[0, 255, 10]);
        expected.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        assert_eq!(*body, expected);
    }

    #[rstest]
    #[case::not_dict(text("x"))]
    #[case::missing_name(part(&[("value", text("v"))]))]
    #[case::missing_value(part(&[("name", text("n"))]))]
    #[case::bad_value(part(&[("name", text("n")), ("value", RuntimeValue::Number(1.into()))]))]
    #[case::header_injection(part(&[("name", text("n")), ("value", text("v")), ("content_type", text("a\r\nX: y"))]))]
    fn test_multipart_rejects_invalid_parts(#[case] bad: RuntimeValue) {
        assert!(multipart(&[bad]).is_err());
    }

    #[test]
    fn test_pick_boundary_avoids_body_content() {
        let boundary = pick_boundary(&[b"abc"]);
        assert!(boundary.starts_with("mq-boundary-"));
        assert!(!boundary.contains("abc"));
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
        assert_eq!(
            parse_method(&RuntimeValue::String(Shared::new(input.into()))).unwrap(),
            expected
        );
    }

    #[rstest]
    #[case::empty("")]
    #[case::space_in_token("in valid")]
    #[case::control_char("get\n")]
    fn test_parse_method_rejects_invalid_method_strings(#[case] input: &str) {
        assert!(parse_method(&symbol(input)).is_err());
        assert!(parse_method(&RuntimeValue::String(Shared::new(input.into()))).is_err());
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
            request(&symbol("post"), "https://example.invalid", Some(b"{}"), None).is_err(),
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
                &RuntimeValue::String(Shared::new("bogus method".into())),
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
                Some(&DictMap::from_iter([(
                    Ident::new("Authorization"),
                    RuntimeValue::String(Shared::new("Bearer token".into()))
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
            RuntimeValue::String(Shared::new("body".to_string()))
        );
    }

    #[test]
    fn test_extract_headers_accepts_string_values() {
        let headers = DictMap::from_iter([
            (Ident::new("X-Test"), RuntimeValue::String(Shared::new("value".into()))),
            (
                Ident::new("Content-Type"),
                RuntimeValue::String(Shared::new("application/json".into())),
            ),
        ]);
        let extracted = extract_headers(Some(&headers)).unwrap();
        assert!(extracted.contains(&("Content-Type".to_string(), "application/json".to_string())));
        assert!(extracted.contains(&("X-Test".to_string(), "value".to_string())));
    }

    #[test]
    fn test_extract_headers_rejects_non_string_values() {
        let headers = DictMap::from_iter([(Ident::new("X-Test"), RuntimeValue::from(1usize))]);
        assert!(extract_headers(Some(&headers)).is_err());
    }

    #[test]
    fn test_extract_headers_passthrough_when_none() {
        assert!(extract_headers(None).unwrap().is_empty());
    }

    #[cfg(feature = "mock-io")]
    #[rstest]
    #[case::no_body_or_headers(r#"http_lines(:get, "https://example.invalid")"#)]
    #[case::headers_as_third_arg(r#"http_lines(:get, "https://example.invalid", {"Authorization": "Bearer t"})"#)]
    #[case::body_as_third_arg(r#"http_lines(:post, "https://example.invalid", "payload")"#)]
    #[case::none_body_with_headers(
        r#"http_lines(:get, "https://example.invalid", None, {"Authorization": "Bearer t"})"#
    )]
    #[case::body_and_headers(
        r#"http_lines(:post, "https://example.invalid", "payload", {"Authorization": "Bearer t"})"#
    )]
    fn test_http_lines_accepts_optional_body_and_headers(#[case] call: &str) {
        use crate::{DefaultModuleResolver, Engine, RuntimeValue, null_input};

        let io = Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true));
        let mut engine = Engine::with_io(DefaultModuleResolver::default(), Shared::clone(&io));
        engine.load_builtin_module();
        engine
            .eval(
                r#"mock_fetch("https://example.invalid", "a\nb")"#,
                null_input().into_iter(),
            )
            .unwrap();

        let result = engine
            .eval(&format!("collect({call})"), null_input().into_iter())
            .unwrap_or_else(|e| panic!("{call} failed: {e:?}"));
        let value = result.into_iter().next().unwrap();
        let RuntimeValue::Array(items) = value else {
            panic!("expected an array, got {value:?}")
        };
        assert_eq!(
            items.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
            vec!["a".to_string(), "b".to_string()],
            "{call}"
        );
    }

    #[cfg(feature = "mock-io")]
    #[test]
    fn test_http_lines_rejects_dict_body_with_headers() {
        use crate::{DefaultModuleResolver, Engine, null_input};

        let io = Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true));
        let mut engine = Engine::with_io(DefaultModuleResolver::default(), Shared::clone(&io));
        engine.load_builtin_module();
        engine
            .eval(
                r#"mock_fetch("https://example.invalid", "a")"#,
                null_input().into_iter(),
            )
            .unwrap();

        assert!(
            engine
                .eval(
                    r#"http_lines(:get, "https://example.invalid", {"A": "1"}, {"B": "2"})"#,
                    null_input().into_iter(),
                )
                .is_err()
        );
    }

    #[cfg(feature = "mock-io")]
    #[test]
    fn test_http_lines_streams_the_mocked_response_line_by_line() {
        use crate::{DefaultModuleResolver, Engine, RuntimeValue, null_input};

        let io = Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true));
        let mut engine = Engine::with_io(DefaultModuleResolver::default(), Shared::clone(&io));
        engine.load_builtin_module();
        engine
            .eval(
                r#"mock_fetch("https://example.invalid", "a\nb\nc")"#,
                null_input().into_iter(),
            )
            .unwrap();

        let result = engine
            .eval(
                r#"collect(http_lines(:get, "https://example.invalid"))"#,
                null_input().into_iter(),
            )
            .unwrap();
        let value = result.into_iter().next().unwrap();
        let RuntimeValue::Array(items) = value else {
            panic!("expected an array, got {value:?}")
        };
        assert_eq!(
            items.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }
}
