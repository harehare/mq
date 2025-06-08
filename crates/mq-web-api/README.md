# Markdown Query Web API (mq-web-api)

The `mq-web-api` crate provides an HTTP web service for executing Markdown Query (mq) language queries. This allows users to process Markdown content remotely via API requests.

The service is built using the `worker` framework for deployment on Cloudflare Workers.

## Features

- Execute `mq` queries over HTTP.
- Supports GET and POST requests for submitting queries and Markdown input.
- Configurable IP-based rate limiting to ensure fair usage and prevent abuse.
- OpenAPI specification available at `/openapi.json`.

## API Usage

- **POST `/query`**: Submit `query` and `input` (Markdown content) in the JSON request body.
  ```json
  {
    "query": ".h1",
    "input": "# Hello World\nThis is a test."
  }
  ```
- **GET `/query`**: Submit `query` and `markdown` (Markdown content) as URL query parameters.
  `GET /query?query=.h1&markdown=%23%20Hello%0AThis%20is%20content.`

## Rate Limiting

The API implements IP-based rate limiting to manage request load and ensure service stability.

- **Mechanism**: The client's IP address (primarily identified using the `CF-Connecting-IP` header) is used to track request counts. If this header is absent, a default placeholder IP ("0.0.0.0") is used for tracking, meaning all requests missing this header will share the same rate limit bucket.
- **Configuration**: The rate limiting behavior is controlled by the following environment variables when deploying the Cloudflare Worker:
    - `RATE_LIMIT_KV_NAMESPACE`: Specifies the name of the Cloudflare KV namespace used to store rate limiting data (counters and timestamps).
        - **Default**: `"RATE_LIMIT_KV"`
    - `RATE_LIMIT_WINDOW_SECONDS`: Defines the duration of the time window for rate limiting, in seconds.
        - **Default**: `60` (seconds)
    - `MAX_REQUESTS_PER_WINDOW`: Sets the maximum number of requests allowed from a single IP address within the defined time window.
        - **Default**: `100`
- **Behavior on Exceeding Limit**: If a client exceeds the configured number of requests within the time window, the API will respond with an HTTP `429 Too Many Requests` error.
- **KV Store Prerequisite**: For rate limiting to function, a Cloudflare KV namespace must be created and bound to the worker with the name specified by the `RATE_LIMIT_KV_NAMESPACE` environment variable. If the KV store is inaccessible or an error occurs during its operation, the API may "fail-open" (i.e., bypass rate limiting for that request) to maintain availability, logging an error in the process.

## Deployment

This service is designed for Cloudflare Workers. Configuration (like KV namespace bindings and environment variables for rate limiting) should be managed within your Cloudflare dashboard or via `wrangler.toml`.

Example `wrangler.toml` snippet for KV binding:
```toml
[[kv_namespaces]]
binding = "RATE_LIMIT_KV" # Should match RATE_LIMIT_KV_NAMESPACE env var if default is not used
id = "your_kv_namespace_id_here"
```

And for environment variables:
```toml
[vars]
RATE_LIMIT_KV_NAMESPACE = "MY_CUSTOM_KV_NAMESPACE"
RATE_LIMIT_WINDOW_SECONDS = "120"
MAX_REQUESTS_PER_WINDOW = "200"
```

Refer to Cloudflare Workers documentation for detailed deployment instructions.
