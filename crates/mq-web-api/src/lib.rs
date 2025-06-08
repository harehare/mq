pub mod api;

pub use api::{ApiRequest, InputFormat, execute};
use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::{OpenApi, ToSchema};
use worker::*;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ApiResponse {
    /// The results of the query execution as strings.
    pub results: Vec<String>,
    /// The total number of results returned.
    pub total_count: usize,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_query_api,
        post_query_api,
    ),
    components(
        schemas(ApiRequest),
        schemas(InputFormat),
        schemas(ApiResponse)
    ),
    tags(
        (name = "mq-api", description = "Markdown Query API")
    )
)]
struct ApiDoc;

#[utoipa::path(
    get,
    path = "/mq",
    responses(
        (status = 200, description = "Query executed successfully", body = ApiResponse, content_type = "application/json",),
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
    handle_request_logic(req, ctx, Method::Get).await
}

#[utoipa::path(
    post,
    path = "/mq",
    responses(
        (status = 200, description = "Query executed successfully", body = ApiResponse, content_type = "application/json"),
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
    handle_request_logic(req, ctx, Method::Post).await
}

async fn handle_request_logic(
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
                input,
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

    match api::execute(request_data) {
        Ok(values) => {
            let response = values
                .into_iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>();
            let total_count = response.len();

            Response::from_json(&ApiResponse {
                results: response,
                total_count,
            })
        }
        Err(e) => Response::error(format!("Execution error: {}", e), 400),
    }
}

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> worker::Result<Response> {
    Router::new()
        .get_async("/mq", get_query_api)
        .post_async("/mq", post_query_api)
        .get_async("/openapi.json", |_req, _ctx| async move {
            Response::from_json(&ApiDoc::openapi())
        })
        .run(req, env)
        .await
}
