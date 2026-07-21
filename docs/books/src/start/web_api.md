# Web API

`mq-web-api` is an HTTP/REST server that exposes mq queries over the network. It provides a curl-friendly shortcut endpoint, a JSON API, an OpenAPI specification, and a Swagger UI.

## Overview

The server exposes the following endpoints:

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/{query}` | Curl-friendly shortcut: query in the path, raw body (Markdown/HTML/XML/JSON/CSV/...) |
| `GET` | `/api/v1/query` | Execute a query (query-string parameters) |
| `POST` | `/api/v1/query` | Execute a query (JSON body) |
| `POST` | `/api/v1/batch` | Execute a query against multiple documents in one request |
| `POST` | `/api/v1/check` | Type-check a query |
| `POST` | `/api/v1/format` | Format a query |
| `GET` | `/api/v1/functions` | List builtin mq functions |
| `GET` | `/api/v1/selectors` | List builtin mq selectors |
| `POST` | `/api/v1/lint` | Lint a query |
| `GET` | `/api/v1/openapi.json` | OpenAPI specification |
| `GET` | `/docs` | Swagger UI |

Legacy paths (`/api/query`, `/api/check`, `/api/format`, `/openapi.json`) redirect permanently to the `/api/v1/` equivalents.

A public instance is hosted at `https://api.mqlang.org/` for quick trials (rate-limited, see [Rate Limiting](#rate-limiting)). For production or higher-volume use, self-host the server yourself (see [Usage](#usage) below).

## Configuration

All settings are controlled through environment variables.

### Server

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Bind address |
| `PORT` | `8080` | Bind port |
| `RUST_LOG` | `mq_web_api=debug,tower_http=debug` | Log level filter |
| `LOG_FORMAT` | `json` | Log format: `json` or `text` |
| `CORS_ORIGINS` | `*` | Comma-separated allowed origins |
| `QUERY_TIMEOUT_SECONDS` | `10` | Max seconds a single query may run before it's aborted |

### Rate Limiting

| Variable | Default | Description |
|----------|---------|-------------|
| `RATE_LIMIT_REQUESTS_PER_WINDOW` | `100` | Maximum requests per window |
| `RATE_LIMIT_WINDOW_SIZE_SECONDS` | `3600` | Window size in seconds |
| `RATE_LIMIT_CLEANUP_INTERVAL_SECONDS` | `3600` | Expired-entry cleanup interval |

### OpenTelemetry (requires `otel` feature)

| Variable | Default | Description |
|----------|---------|-------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | — | OTLP exporter endpoint (e.g. `http://localhost:4317`) |
| `OTEL_SERVICE_NAME` | `mq-web-api` | Service name reported to the collector |

## Usage

### Running locally

```bash
# Default settings
cargo run --bin mq-web-api

# Custom host and port
HOST=localhost PORT=3000 cargo run --bin mq-web-api

# Text-format logs
LOG_FORMAT=text cargo run --bin mq-web-api

# Restrict CORS origins
CORS_ORIGINS="https://example.com,https://app.example.com" cargo run --bin mq-web-api

# Enable OpenTelemetry
cargo run --features otel --bin mq-web-api
```

### Docker

Build and run from the workspace root:

```bash
docker build -f crates/mq-web-api/Dockerfile.vercel -t mq-web-api .
docker run -p 8080:8080 mq-web-api
```

With custom configuration:

```bash
docker run -p 3000:3000 \
  -e PORT=3000 \
  -e LOG_FORMAT=text \
  -e CORS_ORIGINS="https://example.com" \
  mq-web-api
```

## Examples

### Curl-friendly shortcut

The mq query goes in the URL path; the input content is the raw request body.

```bash
curl --data-binary @doc.md https://api.mqlang.org/.h1
```

`html`, `xml`, and `json` input are auto-detected from the body's leading bytes. Other formats (`csv`, `tsv`, `psv`, `yaml`, `toml`, `hcl`, `toon`) need an explicit `input_format`:

```bash
curl --data-binary @page.html https://api.mqlang.org/.h1
curl --data-binary @data.json 'https://api.mqlang.org/json::json_to_markdown_table()'
curl --data-binary @data.csv 'https://api.mqlang.org/csv::csv_to_markdown_table()?input_format=csv'
```

> **Use `--data-binary`, not `-d`/`--data`.** `curl -d @file` strips newlines from the file, which breaks Markdown/HTML/XML parsing. `--data-binary` sends the file exactly as-is.

For queries that need `modules`, `args`, or `aggregate`, use `POST /api/v1/query` instead.

### Execute a query (GET)

```bash
curl "http://localhost:8080/api/v1/query?query=.h&input=%23%20Title%0A%0AContent&input_format=markdown"
```

### Execute a query (POST)

```bash
curl -X POST http://localhost:8080/api/v1/query \
  -H "Content-Type: application/json" \
  -d '{
    "query": ".h",
    "input": "# Title\n\nContent",
    "input_format": "markdown"
  }'
```

### Batch query (multiple documents in one request)

`POST /api/v1/batch` runs one query against multiple documents in a single request, avoiding an HTTP round trip per document. Each document is processed independently — one failing document doesn't fail the others, and `items` in the response is ordered like `inputs` (max 100 entries).

```bash
curl -X POST http://localhost:8080/api/v1/batch \
  -H "Content-Type: application/json" \
  -d '{
    "query": ".h1",
    "inputs": ["# Doc One\n\nBody.", "# Doc Two\n\nBody."],
    "input_format": "markdown"
  }'
```

### Type-check a query

```bash
curl -X POST http://localhost:8080/api/v1/check \
  -H "Content-Type: application/json" \
  -d '{"query": "upcase | downcase"}'
```

### Format a query

```bash
curl -X POST http://localhost:8080/api/v1/format \
  -H "Content-Type: application/json" \
  -d '{"query": "if(a):1 elif(b):2 else:3"}'
```

### Lint a query

```bash
curl -X POST http://localhost:8080/api/v1/lint \
  -H "Content-Type: application/json" \
  -d '{"query": "let x = .h1 | .text"}'
```

### List builtin functions and selectors

```bash
curl http://localhost:8080/api/v1/functions
curl http://localhost:8080/api/v1/selectors
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `use_mimalloc` | enabled | Use mimalloc as the global allocator |
| `otel` | disabled | Enable OpenTelemetry tracing via OTLP |

See the [mq-web-api crate](https://github.com/harehare/mq/tree/main/crates/mq-web-api) for the full README and source.
