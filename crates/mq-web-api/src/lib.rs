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
