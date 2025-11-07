use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
};
use serde::Deserialize;
use tracing::{debug, error, info};
use utoipa::OpenApi;

use crate::api::{ApiRequest, DiagnosticsApiResponse, InputFormat, QueryApiResponse};

#[derive(Clone)]
pub struct AppState {}

#[derive(Deserialize)]
pub struct QueryParams {
    pub query: String,
    pub input: Option<String>,
    pub input_format: Option<String>,
}

#[derive(Deserialize)]
pub struct DiagnosticsParams {
    pub query: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(get_query_api, post_query_api, get_diagnostics_api, openapi_json),
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
pub struct ApiDoc;

#[utoipa::path(
    get,
    path = "/api/query",
    responses(
        (status = 200, description = "Query executed successfully", body = QueryApiResponse),
        (status = 400, description = "Invalid request parameters"),
    ),
    params(
        ("query" = String, Query, description = "mq query string to execute"),
        ("input" = String, Query, description = "Input content to process"),
        ("input_format" = Option<String>, Query, description = "Input format: markdown, mdx, text, html, raw, or null")
    )
)]
pub async fn get_query_api(
    Query(params): Query<QueryParams>,
    State(_state): State<AppState>,
) -> Result<Json<QueryApiResponse>, StatusCode> {
    debug!("GET /query called with query: {}", params.query);

    let input_format = params
        .input_format
        .and_then(|v| serde_json::from_str::<InputFormat>(&format!("\"{}\"", v)).ok());

    let request = ApiRequest {
        query: params.query.clone(),
        input: params.input.clone(),
        input_format: input_format.clone(),
    };

    debug!("Processing request with input_format: {:?}", input_format);

    match crate::api::query(request) {
        Ok(response) => {
            info!(
                "Successfully processed query: {}, results count: {}",
                params.query,
                response.results.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to process query '{}': {}", params.query, e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/query",
    responses(
        (status = 200, description = "Query executed successfully", body = QueryApiResponse),
        (status = 400, description = "Invalid request parameters"),
    ),
    request_body = ApiRequest
)]
pub async fn post_query_api(
    State(_state): State<AppState>,
    Json(request): Json<ApiRequest>,
) -> Result<Json<QueryApiResponse>, StatusCode> {
    debug!("POST /query called with query: {}", request.query);
    debug!("Processing request with input_format: {:?}", request.input_format);

    match crate::api::query(request.clone()) {
        Ok(response) => {
            info!(
                "Successfully processed query: {}, results count: {}",
                request.query,
                response.results.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to process query '{}': {}", request.query, e);
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/query/diagnostics",
    responses(
        (status = 200, description = "Diagnostics executed successfully", body = DiagnosticsApiResponse),
        (status = 400, description = "Invalid request parameters"),
    ),
    params(
        ("query" = String, Query, description = "mq query string to analyze"),
    )
)]
pub async fn get_diagnostics_api(
    Query(params): Query<DiagnosticsParams>,
    State(_state): State<AppState>,
) -> Json<DiagnosticsApiResponse> {
    debug!("GET /query/diagnostics called with query: {}", params.query);

    let request = ApiRequest {
        query: params.query.clone(),
        input: None,
        input_format: None,
    };

    let response = crate::api::diagnostics(request);
    info!(
        "Diagnostics for query '{}': {} diagnostics found",
        params.query,
        response.diagnostics.len()
    );

    Json(response)
}

#[utoipa::path(
    get,
    path = "/openapi.json",
    responses(
        (status = 200, description = "OpenAPI specification", body = String),
    )
)]
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    debug!("GET /openapi.json called");
    Json(ApiDoc::openapi())
}
