use std::str::FromStr;

use serde::{Deserialize, Serialize};
use url::Url;
use utoipa::{OpenApi, ToSchema};
use worker::*;

#[derive(Deserialize, Serialize, ToSchema, Clone, Debug)]
pub struct ApiRequest {
    #[schema(example = ".h")]
    pub query: String,
    #[schema(example = "## Markdown Content

This is an example markdown string.")]
    pub input: String,
    pub input_format: Option<InputFormat>,
}

#[derive(Serialize, Deserialize, ToSchema, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub enum InputFormat {
    #[serde(rename = "markdown")]
    Markdown,
    #[serde(rename = "mdx")]
    Mdx,
    #[serde(rename = "text")]
    Text,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_query_api,
        post_query_api,
    ),
    components(
        schemas(ApiRequest),
        schemas(InputFormat)
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
        (status = 200, description = "Query executed successfully", body = ApiRequest),
        (status = 400, description = "Invalid request parameters"),
        (status = 405, description = "Unsupported HTTP method")
    ),
    params(
        ("query" = String, Query, description = "mq query string to execute"),
        ("markdown" = String, Query, description = "Markdown content to process"),
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
        (status = 200, description = "Query executed successfully", body = ApiRequest),
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
        let markdown_param = params.get("markdown").cloned();
        let input_format = params
            .get("input_format")
            .and_then(|v| InputFormat::deserialize(serde_json::Value::String(v.clone())).ok());

        match (query_param, markdown_param, input_format) {
            (Some(query), Some(input), input_format) => ApiRequest {
                query,
                input,
                input_format,
            },
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

    let input = match request.input_format.unwrap_or(InputFormat::Markdown) {
        format @ (InputFormat::Markdown | InputFormat::Mdx) => {
            let md = if matches!(format, InputFormat::Mdx) {
                mq_markdown::Markdown::from_mdx_str(&request.input)
            } else {
                mq_markdown::Markdown::from_str(&request.input)
            }?;

            md.nodes
                .into_iter()
                .map(mq_lang::Value::from)
                .collect::<Vec<_>>()
        }
        InputFormat::Text => request
            .input
            .lines()
            .map(mq_lang::Value::from)
            .collect::<Vec<_>>(),
    };

    engine
        .eval(&request.query, input.into_iter())
        .map_err(|e| miette::miette!("Error executing query: {}", e))
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
