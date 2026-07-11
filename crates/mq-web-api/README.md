<h1 align="center">mq-web-api</h1>

HTTP/REST server that exposes the [mq](https://mqlang.org/) markdown query language over the network.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Health check |
| `POST` | `/{query}` | Curl-friendly shortcut: query in the path, raw body (Markdown/HTML/XML/JSON/CSV/...) |
| `GET` | `/api/v1/query` | Execute a query (query-string parameters) |
| `POST` | `/api/v1/query` | Execute a query (JSON body) |
| `POST` | `/api/v1/check` | Type-check a query |
| `POST` | `/api/v1/format` | Format a query |
| `GET` | `/api/v1/functions` | List builtin mq functions |
| `GET` | `/api/v1/selectors` | List builtin mq selectors |
| `POST` | `/api/v1/lint` | Lint a query |
| `GET` | `/api/v1/openapi.json` | OpenAPI specification |
| `GET` | `/docs` | Swagger UI |

Legacy paths (`/api/query`, `/api/check`, `/api/format`, `/openapi.json`) redirect permanently to the `/api/v1/` equivalents.

### `POST /{query}`

Curl-friendly shortcut for one-off queries: the mq query goes in the URL path, the input content is the raw request body.

```bash
curl --data-binary @doc.md https://api.mqlang.org/.h1
```

| Parameter | Location | Required | Description |
|-----------|----------|----------|--------------|
| `query` | Path | Yes | mq query expression. Reserved characters (`\|`, `?`, `#`) must be percent-encoded, e.g. `.h1 \| .text` → `.h1%20%7C%20.text` |
| (body) | Body | No | Raw input content |
| `input_format` | Query | No | `markdown` (default), `mdx`, `text`, `html`, `raw`, `null`, `csv`, `tsv`, `psv`, `json`, `yaml`, `toml`, `xml`, `hcl`, `toon`. See auto-detection below. |
| `output_format` | Query | No | `markdown` (default), `html`, `text`, `json`, `none` |

`html`, `xml`, and `json` are auto-detected from the body's leading bytes when `input_format` is omitted (`<!doctype html>`/`<html`, `<?xml`, and `{`/`[` respectively). `csv`, `tsv`, `psv`, `yaml`, `toml`, `hcl`, and `toon` have no reliable content signature and always need an explicit `input_format`.

```bash
# HTML/XML/JSON are auto-detected — no ?input_format= needed
curl --data-binary @page.html https://api.mqlang.org/.h1
curl --data-binary @data.json 'https://api.mqlang.org/json::json_to_markdown_table()'

# CSV (and tsv/psv/yaml/toml/hcl/toon) need an explicit input_format,
# then use the matching builtin module inside the query
curl --data-binary @data.csv 'https://api.mqlang.org/csv::csv_to_markdown_table()?input_format=csv'

# Force a format explicitly if auto-detection isn't what you want
curl --data-binary @page.html "https://api.mqlang.org/.h1?input_format=text"
```

> **Use `--data-binary`, not `-d`/`--data`.** `curl -d @file` strips newlines from the file, which breaks Markdown/HTML/XML parsing (blank lines/whitespace are often significant). `--data-binary` sends the file exactly as-is.

For queries that need `modules`, `args`, or `aggregate`, use `POST /api/v1/query` instead.

### `GET /api/v1/query`

| Parameter | Required | Description |
|-----------|----------|-------------|
| `query` | Yes | mq query string |
| `input` | No | Input content |
| `input_format` | No | `markdown` (default), `mdx`, `text`, `html`, `raw`, `null`, `csv`, `tsv`, `psv`, `json`, `yaml`, `toml`, `xml`, `hcl`, `toon` |

### `POST /api/v1/query`

```json
{
  "query": ".h",
  "input": "## Markdown Content\n\nBody text.",
  "input_format": "markdown",
  "output_format": "markdown",
  "modules": ["json"],
  "args": { "key": "value" },
  "aggregate": false
}
```

| Field | Type | Description |
|-------|------|-------------|
| `query` | `string` | mq query string |
| `input` | `string?` | Input content |
| `input_format` | `string?` | `markdown` (default), `mdx`, `text`, `html`, `raw`, `null`, `csv`, `tsv`, `psv`, `json`, `yaml`, `toml`, `xml`, `hcl`, `toon` |
| `output_format` | `string?` | `markdown` (default), `html`, `text`, `json`, `none` |
| `modules` | `string[]?` | Builtin module names to load (e.g. `"json"`, `"csv"`) |
| `args` | `object?` | String variables passed to the engine |
| `aggregate` | `bool?` | Aggregate all input nodes before querying (equivalent to CLI `-A`) |

### `POST /api/v1/check`

```json
{ "query": "upcase | downcase" }
```

Returns an array of errors (empty means no issues). Always returns HTTP 200.

### `GET /api/v1/functions`

Returns all builtin mq functions with their descriptions and parameters.

### `GET /api/v1/selectors`

Returns all builtin mq selectors with their descriptions and parameters.

### `POST /api/v1/lint`

```json
{ "query": "let x = .h1 | .text" }
```

Returns an array of style/correctness diagnostics (empty means no issues). Always returns HTTP 200.

### `POST /api/v1/format`

```json
{
  "query": "if(a):1 elif(b):2 else:3",
  "indent_width": 2,
  "sort_imports": false,
  "sort_functions": false,
  "sort_fields": false
}
```

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

```bash
curl --data-binary @doc.md https://api.mqlang.org/.h1
```

### Curl-friendly shortcut with HTML input (auto-detected)

```bash
curl --data-binary @page.html https://api.mqlang.org/.h1
```

### Curl-friendly shortcut with CSV input

```bash
curl --data-binary @data.csv 'https://api.mqlang.org/csv::csv_to_markdown_table()?input_format=csv'
```

### Curl-friendly shortcut with JSON input (auto-detected)

```bash
curl --data-binary @data.json 'https://api.mqlang.org/json::json_to_markdown_table()'
```

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

### List builtin functions

```bash
curl http://localhost:8080/api/v1/functions
```

### List builtin selectors

```bash
curl http://localhost:8080/api/v1/selectors
```

### Lint a query

```bash
curl -X POST http://localhost:8080/api/v1/lint \
  -H "Content-Type: application/json" \
  -d '{"query": "let x = .h1 | .text"}'
```

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `use_mimalloc` | enabled | Use mimalloc as the global allocator |
| `otel` | disabled | Enable OpenTelemetry tracing via OTLP |

## Support

- [Report bugs](https://github.com/harehare/mq/issues)
- [Request features](https://github.com/harehare/mq/issues)
- [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
