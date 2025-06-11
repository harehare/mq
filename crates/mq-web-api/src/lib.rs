pub mod api;

pub use api::{ApiRequest, InputFormat, query};
use url::Url;
use utoipa::OpenApi;
use worker::*;

use crate::api::{DiagnosticsApiResponse, QueryApiResponse};

#[derive(OpenApi)]
#[openapi(
    paths(
        get_query_api,
        post_query_api,
    ),
    components(
        schemas(ApiRequest),
        schemas(InputFormat),
        schemas(QueryApiResponse),
        schemas(DiagnosticsApiResponse)
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
        (status = 200, description = "Query executed successfully", body = QueryApiResponse, content_type = "application/json",),
        (status = 400, description = "Invalid request parameters"),
        (status = 405, description = "Unsupported HTTP method")
    ),
    params(
        ("query" = String, Query, description = "mq query string to execute"),
        ("input" = String, Query, description = "Input content to process"),
        ("input_format" = Option<String>, Query, description = "Input format: markdown, mdx, or text (optional)")
    )
)]
#[worker::send]
async fn get_query_api(req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    handle_query_request(req, ctx, Method::Get).await
}

#[utoipa::path(
    get,
    path = "/query/diagnostics",
    responses(
        (status = 200, description = "Diagnostics executed successfully", body = DiagnosticsApiResponse, content_type = "application/json"),
        (status = 400, description = "Invalid request parameters"),
    ),
    params(
        ("query" = String, Query, description = "mq query string to analyze"),
    )
)]
#[worker::send]
async fn get_diagnostics_api(req: Request, _ctx: RouteContext<()>) -> worker::Result<Response> {
    handle_diagnostics_request(req).await
}

#[utoipa::path(
    post,
    path = "/query",
    responses(
        (status = 200, description = "Query executed successfully", body = QueryApiResponse, content_type = "application/json"),
        (status = 400, description = "Invalid request parameters"),
        (status = 405, description = "Unsupported HTTP method")
    ),
    request_body(
        content = ApiRequest,
        description = "JSON body containing the mq query, markdown content, and optional input format",
        content_type = "application/json"
    )
)]
#[worker::send]
async fn post_query_api(req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    handle_query_request(req, ctx, Method::Post).await
}

async fn handle_query_request(
    mut req: Request,
    _ctx: RouteContext<()>,
    method: Method,
) -> worker::Result<Response> {
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
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        let query_param = params.get("query").cloned();
        let input_param = params.get("input").cloned();
        let input_format = params
            .get("input_format")
            .and_then(|v| serde_json::from_str::<InputFormat>(&format!("\"{}\"", v)).ok());

        match (query_param, input_param, input_format) {
            (Some(query), Some(input), input_format) => ApiRequest {
                query,
                input: Some(input),
                input_format,
            },
            _ => {
                return Response::error(
                    "Missing 'query' or 'input' query parameters for GET request",
                    400,
                );
            }
        }
    } else {
        return Response::error(format!("Unsupported method: {:?}", method), 405);
    };

    match api::query(request_data) {
        Ok(response) => Response::from_json(&response),
        Err(e) => Response::error(format!("Execution error: {}", e), 400),
    }
}

async fn handle_diagnostics_request(req: Request) -> worker::Result<Response> {
    let request_data: ApiRequest = {
        let url = match Url::parse(req.url()?.as_str()) {
            Ok(u) => u,
            Err(e) => return Response::error(format!("Failed to parse URL: {}", e), 400),
        };
        let params: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        let query_param = params.get("query").cloned();

        match query_param {
            Some(query) => ApiRequest {
                query,
                input: None,
                input_format: None,
            },
            _ => {
                return Response::error(
                    "Missing 'query' or 'input' query parameters for GET request",
                    400,
                );
            }
        }
    };

    Response::from_json(&api::diagnostics(request_data))
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> worker::Result<Response> {
    // Rate limit constants
    const RATE_LIMIT: u32 = 10; // 10 requests
    const WINDOW_SECS: u64 = 60; // per 60 seconds
    let kv = match env.kv("RATE_LIMIT_KV") {
        Ok(kv) => kv,
        Err(_) => {
            let resp = Response::from_json(&serde_json::json!({
            "error": "Internal Server Error",
            "message": "KV not configured"
            }))?;
            return Ok(resp.with_status(500));
        }
    };

    let ip = req
        .headers()
        .get("CF-Connecting-IP")?
        .unwrap_or_else(|| "unknown".to_string());
    let now = worker::Date::now().as_millis();
    let window = now / (WINDOW_SECS * 1000);
    let key = format!("rate:{}:{}", ip, window);
    let count = kv
        .get(&key)
        .text()
        .await?
        .unwrap_or("0".to_string())
        .parse::<u32>()
        .unwrap_or(0);
    if count >= RATE_LIMIT {
        let mut resp = Response::from_json(&serde_json::json!({
            "error": "Too Many Requests",
            "message": "You have exceeded the rate limit. Please try again later."
        }))?;
        resp.headers_mut()
            .set("Retry-After", &WINDOW_SECS.to_string())?;
        return Ok(resp.with_status(429));
    }
    kv.put(&key, (count + 1).to_string())?
        .expiration_ttl(WINDOW_SECS)
        .execute()
        .await?;

    Router::new()
        .get_async("/query", get_query_api)
        .get_async("/query/diagnostics", get_diagnostics_api)
        .post_async("/query", post_query_api)
        .get_async("/openapi.json", |_req, _ctx| async move {
            Response::from_json(&ApiDoc::openapi())
        })
        .run(req, env)
        .await
}
