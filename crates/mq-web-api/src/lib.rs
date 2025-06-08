use std::str::FromStr;

use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::{OpenApi, ToSchema};
use worker::*;
use worker::kv::KvStore; // Added for KV store
use worker::Date; // Added for timestamp

#[derive(Serialize, Deserialize, Debug)] // Added Debug
struct RateLimitEntry {
    count: u32,
    timestamp: u64, // Unix timestamp in seconds
}

#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct ApiRequest {
    #[schema(example = ".h")]
    pub query: String,
    #[schema(example = "## Markdown Content

This is an example markdown string.")]
    pub input: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_query_api,
        post_query_api,
    ),
    components(
        schemas(ApiRequest)
    ),
    tags(
        (name = "mq-api", description = "Markdown Query API")
    )
)]
struct ApiDoc;

#[utoipa::path(
    get,
    path = "/query",
    responses(
        (status = 200, description = "Query executed successfully", body = ApiRequest),
        (status = 400, description = "Invalid request parameters"),
        (status = 405, description = "Unsupported HTTP method")
    ),
    params(
        ("query" = String, Query, description = "mq query string to execute"),
        ("markdown" = String, Query, description = "Markdown content to process")
    )
)]
#[worker::send]
async fn get_query_api(req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    handle_request_logic(req, ctx, Method::Get).await
}

#[utoipa::path(
    post,
    path = "/query",
    responses(
        (status = 200, description = "Query executed successfully", body = ApiRequest),
        (status = 400, description = "Invalid request parameters"),
        (status = 405, description = "Unsupported HTTP method")
    ),
    params(
        ("query" = String, Query, description = "mq query string to execute"),
        ("markdown" = String, Query, description = "Markdown content to process")
    )
)]
#[worker::send]
async fn post_query_api(req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    handle_request_logic(req, ctx, Method::Post).await
}

async fn handle_request_logic(
    mut req: Request,
    ctx: RouteContext<()>, // Changed _ctx to ctx to access env
    method: Method,
) -> worker::Result<Response> {
    // Read rate limiting parameters from environment variables with defaults
    let rate_limit_kv_namespace = match ctx.env.var("RATE_LIMIT_KV_NAMESPACE") {
        Ok(var) => var.to_string(),
        Err(_) => {
            console_warn!("'RATE_LIMIT_KV_NAMESPACE' env var not found, using default 'RATE_LIMIT_KV'");
            "RATE_LIMIT_KV".to_string()
        }
    };

    let rate_limit_window_seconds: u64 = match ctx.env.var("RATE_LIMIT_WINDOW_SECONDS") {
        Ok(var_item) => match var_item.to_string().parse::<u64>() {
            Ok(val) => val,
            Err(e) => {
                console_error!(
                    "Failed to parse 'RATE_LIMIT_WINDOW_SECONDS' env var ('{}') as u64: {}. Using default 60.",
                    var_item.to_string(),
                    e
                );
                60
            }
        },
        Err(_) => {
            console_warn!("'RATE_LIMIT_WINDOW_SECONDS' env var not found, using default 60");
            60
        }
    };

    let max_requests_per_window: u32 = match ctx.env.var("MAX_REQUESTS_PER_WINDOW") {
        Ok(var_item) => match var_item.to_string().parse::<u32>() {
            Ok(val) => val,
            Err(e) => {
                console_error!(
                    "Failed to parse 'MAX_REQUESTS_PER_WINDOW' env var ('{}') as u32: {}. Using default 100.",
                    var_item.to_string(),
                    e
                );
                100
            }
        },
        Err(_) => {
            console_warn!("'MAX_REQUESTS_PER_WINDOW' env var not found, using default 100");
            100
        }
    };

    let client_ip = match req.headers().get("CF-Connecting-IP")? {
        Some(ip) => ip,
        None => {
            console_warn!("'CF-Connecting-IP' header not found, using default '0.0.0.0' for rate limiting key.");
            "0.0.0.0".to_string() // Use placeholder for key, actual handling might differ
        }
    };
    console_log!("Client IP: {} for request.", client_ip);

    // Rate limiting logic
    let current_timestamp_secs = Date::now().as_millis() / 1000;
    // Use the configured KV namespace
    let kv_store = ctx.env.kv(&rate_limit_kv_namespace)?;
    let kv_key = format!("rate_limit_{}", client_ip);

    let entry_opt: Option<RateLimitEntry> = match kv_store.get(&kv_key).await {
        Ok(Some(value)) => match value.as_json().await { // Use as_json for worker::kv::Value
            Ok(entry) => Some(entry),
            Err(e) => {
                console_error!(
                    "Failed to deserialize RateLimitEntry for IP {}: {}. Assuming no entry (fail-open).",
                    client_ip,
                    e
                );
                None
            }
        },
        Ok(None) => None, // No entry found in KV store, normal case
        Err(e) => {
            console_error!(
                "KV store GET error for IP {}: {}. Assuming no entry (fail-open).",
                client_ip,
                e
            );
            None
        }
    };

    let mut entry_to_store = match entry_opt {
        Some(mut existing_entry) => {
            // Check if current request is within the window
            // Use the configured window seconds
            if current_timestamp_secs < existing_entry.timestamp + rate_limit_window_seconds {
                // Within window
                // Use the configured max requests
                if existing_entry.count >= max_requests_per_window {
                    console_log!(
                        "Rate limit exceeded for IP: {}. Count: {}, Stored Timestamp: {}, Current Timestamp: {}",
                        client_ip,
                        existing_entry.count,
                        existing_entry.timestamp,
                        current_timestamp_secs
                    );
                    return Response::error("Too Many Requests", 429);
                }
                existing_entry.count += 1;
                // Update timestamp to slide the window with each request
                existing_entry.timestamp = current_timestamp_secs;
                existing_entry
            } else {
                // Window expired, reset count and set new timestamp
                RateLimitEntry {
                    count: 1,
                    timestamp: current_timestamp_secs,
                }
            }
        }
        None => {
            // No entry, create new
            RateLimitEntry {
                count: 1,
                timestamp: current_timestamp_secs,
            }
        }
    };

    // Persist the new or updated entry to KV store
    match kv_store.put(&kv_key, &entry_to_store)?.execute().await {
        Ok(_) => {
            console_log!(
                "Successfully stored rate limit entry for IP: {}: count={}, timestamp={}",
                client_ip, entry_to_store.count, entry_to_store.timestamp
            );
        }
        Err(e) => {
            console_error!(
                "KV store PUT error for IP {}: {}. Rate limit count may not have been saved.",
                client_ip,
                e
            );
        }
    }

    let request_data: ApiRequest = if method == Method::Post {
        match req.json().await {
            Ok(json_data) => json_data,
            Err(e) => return Response::error(format!("Failed to parse JSON body: {}", e), 400),
        }
    } else if method == Method::Get {
        let url = match Url::parse(req.url()?.as_str()) {
            Ok(u) => u,
            Err(e) => return Response::error(format!("Failed to parse URL: {}", e), 400),
        };
        let mut query_param = None;
        let mut markdown_param = None;

        for (key, value) in url.query_pairs() {
            if key == "query" {
                query_param = Some(value.into_owned());
            } else if key == "markdown" {
                markdown_param = Some(value.into_owned());
            }
        }

        match (query_param, markdown_param) {
            (Some(query), Some(input)) => ApiRequest { query, input },
            _ => {
                return Response::error(
                    "Missing 'query' or 'markdown' query parameters for GET request",
                    400,
                );
            }
        }
    } else {
        return Response::error(format!("Unsupported method: {:?}", method), 405);
    };

    dbg!(&request_data);
    match execute(request_data) {
        Ok(values) => {
            let response = values
                .into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>();
            Response::from_json(&response)
        }
        Err(e) => Response::error(format!("Execution error: {}", e), 400),
    }
}

fn execute(request: ApiRequest) -> miette::Result<mq_lang::Values> {
    let mut engine = mq_lang::Engine::default();
    engine.load_builtin_module();

    let md = mq_markdown::Markdown::from_str(&request.input)?;
    let input = md
        .nodes
        .into_iter()
        .map(mq_lang::Value::from)
        .collect::<Vec<_>>();

    engine
        .eval(&request.query, input.into_iter())
        .map_err(|e| miette::miette!("Error executing query: {}", e))
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> worker::Result<Response> {
    Router::new()
        .get_async("/query", get_query_api)
        .post_async("/query", post_query_api)
        .get_async("/openapi.json", |_req, _ctx| async move {
            Response::from_json(&ApiDoc::openapi())
        })
        .run(req, env)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use worker_test::{TestCtx, TestKvNamespace};
    use worker::Headers;
    use std::collections::HashMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    // Helper to get current time in seconds, similar to Date::now().as_millis() / 1000
    fn current_time_secs() -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
    }

    // Helper to build a RouteContext
    fn build_test_route_context(env_vars: HashMap<String, String>, kv_namespaces: HashMap<String, TestKvNamespace>) -> RouteContext<()> {
        let mut test_ctx = TestCtx::new();
        for (k, v) in env_vars {
            test_ctx = test_ctx.with_var(&k, &v);
        }
        for (name, kv_ns) in kv_namespaces {
            test_ctx = test_ctx.with_kv_namespace(&name, kv_ns);
        }

        // The RouteContext needs an Env. We get it from the TestCtx.
        // This is a bit of a workaround as TestCtx itself isn't directly an Env.
        // We are essentially creating a dummy route context because handle_request_logic
        // primarily uses ctx.env.
        // In a real scenario, worker-test might provide a more direct way or
        // one might need to simulate the Env part more thoroughly.
        // For now, we'll assume that the Env obtained from TestCtx works for kv() and var().
        let env = test_ctx.env();
        RouteContext::new(env, ()) // Assuming D is ()
    }

    // Helper to build a Request
    // For simplicity, these tests will use GET requests.
    // The body parsing for POST is not the focus of rate limiting tests.
    fn build_test_request(ip_header: Option<&str>, url_str: &str) -> Request {
        let mut headers = Headers::new();
        if let Some(ip) = ip_header {
            headers.append("CF-Connecting-IP", ip).unwrap();
        }
        // A basic URL is needed for the GET request parsing logic.
        Request::new_with_init(
            url_str,
            RequestInit::new().with_method(Method::Get).with_headers(headers),
        )
        .unwrap()
    }

    const TEST_KV_NAMESPACE: &str = "TEST_RATE_LIMIT_KV";
    const DEFAULT_QUERY: &str = ".";
    const DEFAULT_MARKDOWN: &str = "test";

    fn test_url(query: &str, markdown: &str) -> String {
        format!("http://localhost/query?query={}&markdown={}", query, markdown)
    }

    #[tokio::test]
    async fn test_no_existing_entry() {
        let kv_ns = TestKvNamespace::new();
        let mut kvs = HashMap::new();
        kvs.insert(TEST_KV_NAMESPACE.to_string(), kv_ns.clone());

        let mut envs = HashMap::new();
        envs.insert("RATE_LIMIT_KV_NAMESPACE".to_string(), TEST_KV_NAMESPACE.to_string());
        // Use defaults for window and max requests for this test

        let ctx = build_test_route_context(envs, kvs);
        let req = build_test_request(Some("1.2.3.4"), &test_url(DEFAULT_QUERY, DEFAULT_MARKDOWN));

        let response = handle_request_logic(req, ctx, Method::Get).await.unwrap();
        assert_eq!(response.status_code(), 200);

        let kv_key = format!("rate_limit_{}", "1.2.3.4");
        let stored_value = kv_ns.get(&kv_key).await.unwrap();
        assert!(stored_value.is_some());

        let entry: RateLimitEntry = serde_json::from_str(&stored_value.unwrap()).unwrap();
        assert_eq!(entry.count, 1);
        // Timestamp should be recent. Allow a small delta.
        assert!(current_time_secs() >= entry.timestamp && current_time_secs() - entry.timestamp < 5);
    }

    #[tokio::test]
    async fn test_within_limit() {
        let kv_ns = TestKvNamespace::new();
        let client_ip = "1.2.3.5";
        let kv_key = format!("rate_limit_{}", client_ip);

        // Pre-populate KV with an entry
        let initial_timestamp = current_time_secs() - 10; // 10 seconds ago
        let initial_entry = RateLimitEntry { count: 1, timestamp: initial_timestamp };
        kv_ns.put(&kv_key, serde_json::to_string(&initial_entry).unwrap()).await.unwrap();

        let mut kvs = HashMap::new();
        kvs.insert(TEST_KV_NAMESPACE.to_string(), kv_ns.clone());
        let mut envs = HashMap::new();
        envs.insert("RATE_LIMIT_KV_NAMESPACE".to_string(), TEST_KV_NAMESPACE.to_string());
        envs.insert("RATE_LIMIT_WINDOW_SECONDS".to_string(), "60".to_string());
        envs.insert("MAX_REQUESTS_PER_WINDOW".to_string(), "100".to_string());

        let ctx = build_test_route_context(envs, kvs);
        let req = build_test_request(Some(client_ip), &test_url(DEFAULT_QUERY, DEFAULT_MARKDOWN));

        let response = handle_request_logic(req, ctx, Method::Get).await.unwrap();
        assert_eq!(response.status_code(), 200);

        let stored_value = kv_ns.get(&kv_key).await.unwrap().unwrap();
        let entry: RateLimitEntry = serde_json::from_str(&stored_value).unwrap();
        assert_eq!(entry.count, 2);
        assert!(entry.timestamp > initial_timestamp); // Timestamp should update
    }

    #[tokio::test]
    async fn test_exceeds_limit() {
        let kv_ns = TestKvNamespace::new();
        let client_ip = "1.2.3.6";
        let max_req: u32 = 2; // Low limit for easy testing

        // Pre-populate KV to be one less than max
        let initial_timestamp = current_time_secs() - 10;
        let initial_entry = RateLimitEntry { count: max_req -1 , timestamp: initial_timestamp };
        let kv_key = format!("rate_limit_{}", client_ip);
        kv_ns.put(&kv_key, serde_json::to_string(&initial_entry).unwrap()).await.unwrap();

        let mut kvs = HashMap::new();
        kvs.insert(TEST_KV_NAMESPACE.to_string(), kv_ns.clone());
        let mut envs = HashMap::new();
        envs.insert("RATE_LIMIT_KV_NAMESPACE".to_string(), TEST_KV_NAMESPACE.to_string());
        envs.insert("RATE_LIMIT_WINDOW_SECONDS".to_string(), "60".to_string());
        envs.insert("MAX_REQUESTS_PER_WINDOW".to_string(), max_req.to_string());

        let ctx = build_test_route_context(envs.clone(), kvs.clone());
        let req1 = build_test_request(Some(client_ip), &test_url("q1", "m1"));

        // First request (reaches max_req)
        let response1 = handle_request_logic(req1, ctx, Method::Get).await.unwrap();
        assert_eq!(response1.status_code(), 200);

        let stored_value1 = kv_ns.get(&kv_key).await.unwrap().unwrap();
        let entry1: RateLimitEntry = serde_json::from_str(&stored_value1).unwrap();
        assert_eq!(entry1.count, max_req);

        // Second request (exceeds max_req)
        let ctx2 = build_test_route_context(envs, kvs); // Rebuild context for fresh env access if needed by logic
        let req2 = build_test_request(Some(client_ip), &test_url("q2", "m2"));
        let response2 = handle_request_logic(req2, ctx2, Method::Get).await.unwrap();
        assert_eq!(response2.status_code(), 429); // Too Many Requests
    }

    #[tokio::test]
    async fn test_after_window_expires() {
        let kv_ns = TestKvNamespace::new();
        let client_ip = "1.2.3.7";
        let window_secs: u64 = 5; // Short window for testing

        // Pre-populate KV with an old entry
        let old_timestamp = current_time_secs() - (window_secs * 2);
        let initial_entry = RateLimitEntry { count: 10, timestamp: old_timestamp }; // Count is high, but ts is old
        let kv_key = format!("rate_limit_{}", client_ip);
        kv_ns.put(&kv_key, serde_json::to_string(&initial_entry).unwrap()).await.unwrap();

        let mut kvs = HashMap::new();
        kvs.insert(TEST_KV_NAMESPACE.to_string(), kv_ns.clone());
        let mut envs = HashMap::new();
        envs.insert("RATE_LIMIT_KV_NAMESPACE".to_string(), TEST_KV_NAMESPACE.to_string());
        envs.insert("RATE_LIMIT_WINDOW_SECONDS".to_string(), window_secs.to_string());
        envs.insert("MAX_REQUESTS_PER_WINDOW".to_string(), "20".to_string());

        let ctx = build_test_route_context(envs, kvs);
        let req = build_test_request(Some(client_ip), &test_url(DEFAULT_QUERY, DEFAULT_MARKDOWN));

        let response = handle_request_logic(req, ctx, Method::Get).await.unwrap();
        assert_eq!(response.status_code(), 200);

        let stored_value = kv_ns.get(&kv_key).await.unwrap().unwrap();
        let entry: RateLimitEntry = serde_json::from_str(&stored_value).unwrap();
        assert_eq!(entry.count, 1); // Count should reset
        assert!(entry.timestamp > old_timestamp);
        assert!(current_time_secs() >= entry.timestamp && current_time_secs() - entry.timestamp < 5);
    }

    #[tokio::test]
    async fn test_missing_ip_header_uses_default_ip() {
        let kv_ns = TestKvNamespace::new();
        let mut kvs = HashMap::new();
        kvs.insert(TEST_KV_NAMESPACE.to_string(), kv_ns.clone());
        let mut envs = HashMap::new();
        envs.insert("RATE_LIMIT_KV_NAMESPACE".to_string(), TEST_KV_NAMESPACE.to_string());

        let ctx = build_test_route_context(envs, kvs);
        // Request with no CF-Connecting-IP header
        let req = build_test_request(None, &test_url(DEFAULT_QUERY, DEFAULT_MARKDOWN));

        let response = handle_request_logic(req, ctx, Method::Get).await.unwrap();
        assert_eq!(response.status_code(), 200);

        // Rate limiting should be applied to the default IP "0.0.0.0"
        let kv_key_default_ip = format!("rate_limit_{}", "0.0.0.0");
        let stored_value = kv_ns.get(&kv_key_default_ip).await.unwrap();
        assert!(stored_value.is_some());
        let entry: RateLimitEntry = serde_json::from_str(&stored_value.unwrap()).unwrap();
        assert_eq!(entry.count, 1);
    }
}
