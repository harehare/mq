use axum::{
    extract::rejection::QueryRejection,
    extract::{FromRequestParts, Path, Query, State},
    http::{StatusCode, request::Parts},
    response::Json,
};
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use tracing::{debug, error, info};
use utoipa::{OpenApi, ToSchema};

use crate::{
    api::{
        ApiRequest, CheckApiRequest, CheckApiResponse, CheckError, FormatApiRequest, FormatApiResponse, FunctionDoc,
        FunctionsApiResponse, InputFormat, LintApiRequest, LintApiResponse, LintDiagnostic, OutputFormat,
        QueryApiResponse, SelectorDoc, SelectorsApiResponse,
    },
    problem::ProblemDetails,
};

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

#[derive(Deserialize)]
pub struct ShorthandQueryParams {
    pub input_format: Option<String>,
    pub output_format: Option<String>,
}

pub struct ValidatedQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: serde::de::DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = ProblemDetails;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        Query::<T>::from_request_parts(parts, state)
            .await
            .map(|Query(params)| ValidatedQuery(params))
            .map_err(|e: QueryRejection| {
                ProblemDetails::new(StatusCode::BAD_REQUEST)
                    .with_title("Invalid query parameters")
                    .with_detail("error", &e.body_text())
            })
    }
}

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    pub status: &'static str,
}

/// Returns 200 OK when the server is healthy.
pub async fn health_check() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_query_api,
        post_query_api,
        post_shorthand_query_api,
        post_check_api,
        post_format_api,
        get_functions_api,
        get_selectors_api,
        post_lint_api,
        openapi_json
    ),
    components(
        schemas(ApiRequest),
        schemas(InputFormat),
        schemas(OutputFormat),
        schemas(QueryApiResponse),
        schemas(CheckApiRequest),
        schemas(CheckApiResponse),
        schemas(CheckError),
        schemas(FormatApiRequest),
        schemas(FormatApiResponse),
        schemas(FunctionDoc),
        schemas(FunctionsApiResponse),
        schemas(SelectorDoc),
        schemas(SelectorsApiResponse),
        schemas(LintApiRequest),
        schemas(LintApiResponse),
        schemas(LintDiagnostic),
    ),
    tags(
        (name = "mq-api", description = "Markdown Query API")
    )
)]
pub struct ApiDoc;

#[utoipa::path(
    get,
    path = "/api/v1/query",
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
    ValidatedQuery(params): ValidatedQuery<QueryParams>,
    State(_state): State<AppState>,
) -> Result<Json<QueryApiResponse>, ProblemDetails> {
    debug!("GET /query called with query: {}", params.query);

    let input_format = params
        .input_format
        .and_then(|v| serde_json::from_str::<InputFormat>(&format!("\"{}\"", v)).ok());

    debug!("Processing request with input_format: {:?}", input_format);

    let query_str = params.query.clone();
    let request = ApiRequest {
        query: params.query,
        input: params.input,
        input_format,
        modules: None,
        args: None,
        output_format: None,
        aggregate: None,
    };

    match tokio::task::spawn_blocking(move || crate::api::query(request))
        .await
        .map_err(|e| {
            error!("Query task panicked: {}", e);
            ProblemDetails::new(StatusCode::INTERNAL_SERVER_ERROR)
                .with_title("Internal error")
                .with_detail("error", &e.to_string())
        })? {
        Ok(response) => {
            info!(
                "Successfully processed query: {}, results count: {}",
                query_str,
                response.results.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to process query '{}': {}", query_str, e);
            Err(ProblemDetails::new(StatusCode::BAD_REQUEST)
                .with_title("Invalid query")
                .with_detail("error", &e.to_string()))
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/query",
    responses(
        (status = 200, description = "Query executed successfully", body = QueryApiResponse),
        (status = 400, description = "Invalid request parameters"),
    ),
    request_body = ApiRequest
)]
pub async fn post_query_api(
    State(_state): State<AppState>,
    Json(request): Json<ApiRequest>,
) -> Result<Json<QueryApiResponse>, ProblemDetails> {
    debug!("POST /query called with query: {}", request.query);
    debug!("Processing request with input_format: {:?}", request.input_format);

    let query_str = request.query.clone();
    match tokio::task::spawn_blocking(move || crate::api::query(request))
        .await
        .map_err(|e| {
            error!("Query task panicked: {}", e);
            ProblemDetails::new(StatusCode::INTERNAL_SERVER_ERROR)
                .with_title("Internal error")
                .with_detail("error", &e.to_string())
        })? {
        Ok(response) => {
            info!(
                "Successfully processed query: {}, results count: {}",
                query_str,
                response.results.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to process query '{}': {}", query_str, e);
            Err(ProblemDetails::new(StatusCode::BAD_REQUEST)
                .with_title("Invalid query")
                .with_detail("error", &e.to_string()))
        }
    }
}

/// Detects whether `body` looks like a full HTML document, ignoring leading
/// whitespace and a UTF-8 BOM. Used by the shorthand endpoint to auto-select
/// `InputFormat::Html` when the caller doesn't pass `?input_format=`.
fn looks_like_html(body: &str) -> bool {
    let trimmed = body.trim_start_matches('\u{feff}').trim_start();
    let prefix: String = trimmed.chars().take(14).collect::<String>().to_ascii_lowercase();
    prefix.starts_with("<!doctype html") || prefix.starts_with("<html")
}

#[utoipa::path(
    post,
    path = "/{query}",
    tag = "mq-api",
    summary = "Run an mq query against raw Markdown (curl-friendly shortcut)",
    description = "Curl-friendly shortcut that reads the mq query from the URL path and the input content from the raw request body, e.g. `curl -d @doc.md https://api.mqlang.org/.h1`. Reserved characters in the query (`|`, `?`, `#`) must be percent-encoded.",
    params(
        ("query" = String, Path, description = "mq query expression", example = ".h1"),
        ("input_format" = Option<String>, Query, description = "Input format: markdown, mdx, text, html, raw, or null. If omitted, HTML is auto-detected when the body starts with `<!doctype html>` or `<html>`."),
        ("output_format" = Option<String>, Query, description = "Output format: markdown, html, text, json, or none"),
    ),
    request_body(content = String, content_type = "text/markdown", description = "Raw Markdown/MDX/HTML/text content to query"),
    responses(
        (status = 200, description = "Query executed successfully", body = QueryApiResponse),
        (status = 400, description = "Invalid query or request"),
    )
)]
pub async fn post_shorthand_query_api(
    State(_state): State<AppState>,
    Path(query): Path<String>,
    ValidatedQuery(params): ValidatedQuery<ShorthandQueryParams>,
    body: String,
) -> Result<Json<QueryApiResponse>, ProblemDetails> {
    debug!("POST /{{query}} called with query: {}", query);

    let input_format = params
        .input_format
        .and_then(|v| serde_json::from_str::<InputFormat>(&format!("\"{}\"", v)).ok())
        .or_else(|| looks_like_html(&body).then_some(InputFormat::Html));
    let output_format = params
        .output_format
        .and_then(|v| serde_json::from_str::<OutputFormat>(&format!("\"{}\"", v)).ok());

    let request = ApiRequest {
        query: query.clone(),
        input: Some(body),
        input_format,
        modules: None,
        args: None,
        output_format,
        aggregate: None,
    };

    match tokio::task::spawn_blocking(move || crate::api::query(request))
        .await
        .map_err(|e| {
            error!("Shorthand query task panicked: {}", e);
            ProblemDetails::new(StatusCode::INTERNAL_SERVER_ERROR)
                .with_title("Internal error")
                .with_detail("error", &e.to_string())
        })? {
        Ok(response) => {
            info!(
                "Successfully processed shorthand query: {}, results count: {}",
                query,
                response.results.len()
            );
            Ok(Json(response))
        }
        Err(e) => {
            error!("Failed to process shorthand query '{}': {}", query, e);
            Err(ProblemDetails::new(StatusCode::BAD_REQUEST)
                .with_title("Invalid query")
                .with_detail("error", &e.to_string()))
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/check",
    responses(
        (status = 200, description = "Type check completed", body = CheckApiResponse),
    ),
    request_body = CheckApiRequest
)]
pub async fn post_check_api(
    State(_state): State<AppState>,
    Json(request): Json<CheckApiRequest>,
) -> Json<CheckApiResponse> {
    debug!("POST /check called with query: {}", request.query);

    let query_str = request.query.clone();
    let response = tokio::task::spawn_blocking(move || crate::api::check(request))
        .await
        .unwrap_or_else(|e| {
            error!("Check task panicked: {}", e);
            crate::api::CheckApiResponse { errors: vec![] }
        });
    info!(
        "Type check for query '{}': {} errors found",
        query_str,
        response.errors.len()
    );

    Json(response)
}

#[utoipa::path(
    post,
    path = "/api/v1/format",
    responses(
        (status = 200, description = "Format completed", body = FormatApiResponse),
        (status = 400, description = "Invalid query syntax"),
    ),
    request_body = FormatApiRequest
)]
pub async fn post_format_api(
    State(_state): State<AppState>,
    Json(request): Json<FormatApiRequest>,
) -> Result<Json<FormatApiResponse>, ProblemDetails> {
    debug!("POST /format called");

    match tokio::task::spawn_blocking(move || crate::api::format_query(request))
        .await
        .map_err(|e| {
            error!("Format task panicked: {}", e);
            ProblemDetails::new(StatusCode::INTERNAL_SERVER_ERROR)
                .with_title("Internal error")
                .with_detail("error", &e.to_string())
        })? {
        Ok(response) => {
            info!("Format completed successfully");
            Ok(Json(response))
        }
        Err(e) => {
            error!("Format failed: {}", e);
            Err(ProblemDetails::new(StatusCode::BAD_REQUEST)
                .with_title("Format error")
                .with_detail("error", &e.to_string()))
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/functions",
    responses(
        (status = 200, description = "List of builtin mq functions", body = FunctionsApiResponse),
    )
)]
pub async fn get_functions_api(State(_state): State<AppState>) -> Json<FunctionsApiResponse> {
    debug!("GET /functions called");
    Json(crate::api::list_functions())
}

#[utoipa::path(
    get,
    path = "/api/v1/selectors",
    responses(
        (status = 200, description = "List of builtin mq selectors", body = SelectorsApiResponse),
    )
)]
pub async fn get_selectors_api(State(_state): State<AppState>) -> Json<SelectorsApiResponse> {
    debug!("GET /selectors called");
    Json(crate::api::list_selectors())
}

#[utoipa::path(
    post,
    path = "/api/v1/lint",
    responses(
        (status = 200, description = "Lint completed", body = LintApiResponse),
    ),
    request_body = LintApiRequest
)]
pub async fn post_lint_api(
    State(_state): State<AppState>,
    Json(request): Json<LintApiRequest>,
) -> Json<LintApiResponse> {
    debug!("POST /lint called with query: {}", request.query);

    let query_str = request.query.clone();
    let response = tokio::task::spawn_blocking(move || crate::api::lint(request))
        .await
        .unwrap_or_else(|e| {
            error!("Lint task panicked: {}", e);
            crate::api::LintApiResponse { diagnostics: vec![] }
        });
    info!(
        "Lint for query '{}': {} diagnostics found",
        query_str,
        response.diagnostics.len()
    );

    Json(response)
}

static OPENAPI_SPEC: OnceLock<utoipa::openapi::OpenApi> = OnceLock::new();

#[utoipa::path(
    get,
    path = "/api/v1/openapi.json",
    responses(
        (status = 200, description = "OpenAPI specification", body = String),
    )
)]
pub async fn openapi_json() -> Json<utoipa::openapi::OpenApi> {
    debug!("GET /openapi.json called");
    Json(OPENAPI_SPEC.get_or_init(ApiDoc::openapi).clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("<html><body>Hi</body></html>", true)]
    #[case("<HTML><body>Hi</body></html>", true)]
    #[case("<!DOCTYPE html><html></html>", true)]
    #[case("  \n<!doctype html>\n<html></html>", true)]
    #[case("\u{feff}<html></html>", true)]
    #[case("# Title\n\nBody text.", false)]
    #[case("<div>fragment</div>", false)]
    #[case("", false)]
    fn test_looks_like_html(#[case] body: &str, #[case] expected: bool) {
        assert_eq!(looks_like_html(body), expected, "body: {:?}", body);
    }
}
