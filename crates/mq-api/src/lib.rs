use serde::{Deserialize, Serialize};
use worker::*;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use url::Url;

#[derive(Deserialize, Serialize, ToSchema, Clone)]
pub struct ApiRequest {
    #[schema(example = "What is the meaning of life?")]
    pub query: String,
    #[schema(example = "## Markdown Content

This is an example markdown string.")]
    pub markdown: String,
}

#[derive(OpenApi)]
#[openapi(
    paths(
        get_api,
        post_api,
    ),
    components(
        schemas(ApiRequest)
    ),
    tags(
        (name = "mq-api", description = "Markdown Query API")
    )
)]
struct ApiDoc;

#[worker::send]
async fn get_api(req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    // This is a specific handler for GET, reusing the logic from handle_request
    // For Utoipa to generate distinct paths, we need separate functions or macros
    handle_request_logic(req, ctx, Method::Get).await
}

#[worker::send]
async fn post_api(req: Request, ctx: RouteContext<()>) -> worker::Result<Response> {
    // This is a specific handler for POST
    handle_request_logic(req, ctx, Method::Post).await
}

// Extracted logic to be shared between get_api and post_api for Utoipa path generation
async fn handle_request_logic(mut req: Request, _ctx: RouteContext<()>, method: Method) -> worker::Result<Response> {
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
            (Some(q), Some(m)) => ApiRequest { query: q, markdown: m },
            _ => return Response::error("Missing 'query' or 'markdown' query parameters for GET request", 400),
        }
    } else {
        // This case should ideally not be reached if router is configured correctly
        return Response::error(format!("Unsupported method: {:?}", method), 405);
    };

    Response::from_json(&request_data)
}


#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: worker::Context) -> worker::Result<Response> {
    let openapi_json_path = "/openapi.json";
    let swagger_ui_path = "/swagger-ui";

    let router = Router::new();

    router
        .get_async("/", get_api)
        .post_async("/", post_api)
        .get_async(openapi_json_path, |_req, _ctx| async move {
            Response::from_json(&ApiDoc::openapi())
        })
        .get_async(&format!("{swagger_ui_path}/*tail"), move |req, _ctx| async move {
            // Construct the URL for openapi.json dynamically
            // This requires knowing the base URL of the worker.
            // For simplicity, assuming it's served from the root.
            // In a real deployment, this might need to be configured or detected.
            let mut url = req.url()?;
            url.set_path(openapi_json_path); // Set path to our openapi spec
            url.set_query(None); // Clear any existing query params

            match SwaggerUi::new(swagger_ui_path)
                .url(openapi_json_path, ApiDoc::openapi()) // Serve the schema directly
                .handle_request_async(req.url()?.path()).await {
                    Ok(response) => Ok(response),
                    Err(e) => Response::error(format!("Swagger UI error: {}", e), 500),
                }
        })
        .run(req, env)
        .await
}


#[cfg(test)]
mod tests {
    use super::*;
    use worker::Method;
    // web-sys and wasm-bindgen are needed for more detailed Request construction in tests
    use wasm_bindgen::JsValue;
    use web_sys::{RequestInit as WebSysRequestInit, Request as WebSysRequest};


    // Helper to create a mock worker::Request for testing GET query parameter parsing.
    fn mock_get_request_with_query(path_and_query: &str) -> worker::Request {
        let url = format!("http://example.com{}", path_and_query);
        Request::new(&url, Method::Get).unwrap()
    }

    #[test]
    fn api_request_deserialization() {
        let json_data = r#"{"query": "test query", "markdown": "test markdown"}"#;
        let request: ApiRequest = serde_json::from_str(json_data).unwrap();
        assert_eq!(request.query, "test query");
        assert_eq!(request.markdown, "test markdown");
    }

    #[test]
    fn api_request_serialization() {
        let request = ApiRequest {
            query: "test query".to_string(),
            markdown: "test markdown".to_string(),
        };
        let json_data = serde_json::to_string(&request).unwrap();
        assert!(json_data.contains(r#""query":"test query""#));
        assert!(json_data.contains(r#""markdown":"test markdown""#));
    }

    #[test]
    fn openapi_schema_generated() {
        let schema = ApiDoc::openapi();
        assert!(!schema.info.title.is_empty(), "Schema title should not be empty");
        assert!(schema.paths.paths.get("/").is_some());
        let operations = schema.paths.paths.get("/").unwrap();
        assert!(operations.get.is_some());
        assert!(operations.post.is_some());
        assert!(schema.components.is_some());
        assert!(schema.components.clone().unwrap().schemas.get("ApiRequest").is_some());
    }

    #[tokio::test]
    async fn handle_get_request_logic_success() {
        let ctx = worker::RouteContext::default();
        let req = mock_get_request_with_query("/?query=hello&markdown=world");

        let response_result = handle_request_logic(req, ctx, Method::Get).await;
        assert!(response_result.is_ok());
        let response = response_result.unwrap();
        assert_eq!(response.status_code(), 200);

        let body_bytes = response.bytes().await.unwrap();
        let body_str = String::from_utf8(body_bytes).unwrap();
        let api_req: ApiRequest = serde_json::from_str(&body_str).unwrap();

        assert_eq!(api_req.query, "hello");
        assert_eq!(api_req.markdown, "world");
    }

    #[tokio::test]
    async fn handle_get_request_logic_missing_params() {
        let ctx = worker::RouteContext::default();
        let req = mock_get_request_with_query("/?query=hello"); // Missing markdown

        let response_result = handle_request_logic(req, ctx, Method::Get).await;
        assert!(response_result.is_ok());
        let response = response_result.unwrap();
        assert_eq!(response.status_code(), 400);

        let body_bytes = response.bytes().await.unwrap();
        let error_msg = String::from_utf8(body_bytes).unwrap();
        assert!(error_msg.contains("Missing 'query' or 'markdown' query parameters"));
    }

    #[tokio::test]
    async fn handle_post_request_logic_mocked_json_failure() {
        let mut req_init = WebSysRequestInit::new();
        req_init.method("POST");
        req_init.body(Some(&JsValue::from_str("this is not json")));

        let cf_req = WebSysRequest::new_with_str_and_init("http://example.com/", &req_init).unwrap();
        let req = worker::Request::from(cf_req);

        let ctx = worker::RouteContext::default();
        let response_result = handle_request_logic(req, ctx, Method::Post).await;
        assert!(response_result.is_ok());
        let response = response_result.unwrap();
        assert_eq!(response.status_code(), 400);
        let body_bytes = response.bytes().await.unwrap();
        let error_msg = String::from_utf8(body_bytes).unwrap();
        assert!(error_msg.contains("Failed to parse JSON body"));
    }

    /*
    #[tokio::test]
    async fn handle_post_request_logic_success() {
        let api_data = ApiRequest { query: "post query".to_string(), markdown: "post md".to_string() };
        let json_body = serde_json::to_string(&api_data).unwrap();

        let mut req_init = WebSysRequestInit::new();
        req_init.method("POST");
        req_init.body(Some(&JsValue::from_str(&json_body)));
        // If Content-Type important for `req.json()`:
        // let headers = web_sys::Headers::new().unwrap();
        // headers.set("Content-Type", "application/json").unwrap();
        // req_init.headers(&headers);

        let cf_req = WebSysRequest::new_with_str_and_init("http://example.com/", &req_init).unwrap();
        let req = worker::Request::from(cf_req);

        let ctx = worker::RouteContext::default();
        let response_result = handle_request_logic(req, ctx, Method::Post).await;
        assert!(response_result.is_ok());
        let response = response_result.unwrap();
        assert_eq!(response.status_code(), 200);

        let body_bytes = response.bytes().await.unwrap();
        let received_api_req: ApiRequest = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(received_api_req.query, "post query");
        assert_eq!(received_api_req.markdown, "post md");
    }
    */
}
