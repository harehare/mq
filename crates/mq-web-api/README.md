# mq-web-api

Web API server for the mq markdown query language.

## Configuration

The server can be configured using environment variables:

| Environment Variable | Description | Default Value |
|---------------------|-------------|---------------|
| `MQ_HOST` | Host to bind to | `0.0.0.0` |
| `MQ_PORT` | Port to bind to | `8080` |
| `RUST_LOG` or `MQ_LOG_LEVEL` | Log level | `mq_web_api=debug,tower_http=debug` |
| `MQ_LOG_FORMAT` | Log format: `json` or `text` | `json` |
| `MQ_CORS_ORIGINS` | Comma-separated CORS origins | `*` |

## Usage

### Running with default settings
```bash
cargo run --bin mq-web-api
```

### Running with custom configuration
```bash
MQ_HOST=localhost MQ_PORT=3000 RUST_LOG=info cargo run --bin mq-web-api
```

### Running with specific CORS origins
```bash
MQ_CORS_ORIGINS="https://example.com,https://app.example.com" cargo run --bin mq-web-api
```

### Running with text-format logs
```bash
MQ_LOG_FORMAT=text cargo run --bin mq-web-api
```

### Running with JSON logs (default)
```bash
MQ_LOG_FORMAT=json cargo run --bin mq-web-api
```

## API Endpoints

- `GET /api/query?query=<mq-query>&input=<content>&input_format=<format>` - Execute query via GET
- `POST /api/query` - Execute query via POST with JSON body
- `GET /api/query/diagnostics?query=<mq-query>` - Get query diagnostics
- `GET /openapi.json` - OpenAPI specification

## Examples

### Query via GET
```bash
curl "http://localhost:8080/api/query?query=.h&input=%23%20Title%0A%0AContent&input_format=markdown"
```

### Query via POST
```bash
curl -X POST http://localhost:8080/api/query \
  -H "Content-Type: application/json" \
  -d '{
    "query": ".h",
    "input": "# Title\n\nContent",
    "input_format": "markdown"
  }'
```

### Get diagnostics
```bash
curl "http://localhost:8080/api/query/diagnostics?query=invalid%20query"
```