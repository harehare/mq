# mq-web-api

Web API server for the mq markdown query language.

## Configuration

The server can be configured using environment variables:

### Basic Configuration

| Environment Variable | Description                  | Default Value                       |
| -------------------- | ---------------------------- | ----------------------------------- |
| `HOST`               | Host to bind to              | `0.0.0.0`                           |
| `PORT`               | Port to bind to              | `8080`                              |
| `RUST_LOG`           | Log level                    | `mq_web_api=debug,tower_http=debug` |
| `LOG_FORMAT`         | Log format: `json` or `text` | `json`                              |
| `CORS_ORIGINS`       | Comma-separated CORS origins | `*`                                 |

### Rate Limiting Configuration

| Environment Variable                  | Description                                    | Default Value |
| ------------------------------------- | ---------------------------------------------- | ------------- |
| `DATABASE_URL`                        | Database URL for rate limiting (libsql format) | `:memory:`    |
| `DATABASE_TOKEN`                      | Database authentication token (for remote DB)  | None          |
| `RATE_LIMIT_REQUESTS_PER_WINDOW`      | Maximum requests per time window               | `100`         |
| `RATE_LIMIT_WINDOW_SIZE_SECONDS`      | Time window size in seconds                    | `3600`        |
| `RATE_LIMIT_CLEANUP_INTERVAL_SECONDS` | Cleanup interval for expired records           | `3600`        |
| `RATE_LIMIT_POOL_MAX_SIZE`            | Maximum database connection pool size          | `10`          |
| `RATE_LIMIT_POOL_TIMEOUT_SECONDS`     | Database connection timeout in seconds         | `30`          |

## Usage

### Running with default settings

```bash
cargo run --bin mq-web-api
```

### Running with custom configuration

```bash
HOST=localhost PORT=3000 RUST_LOG=info cargo run --bin mq-web-api
```

### Running with specific CORS origins

```bash
CORS_ORIGINS="https://example.com,https://app.example.com" cargo run --bin mq-web-api
```

### Running with text-format logs

```bash
LOG_FORMAT=text cargo run --bin mq-web-api
```

### Running with JSON logs (default)

```bash
LOG_FORMAT=json cargo run --bin mq-web-api
```

### Running with rate limiting using Turso database

```bash
DATABASE_URL="libsql://your-database.turso.io" \
DATABASE_TOKEN="your-auth-token" \
RATE_LIMIT_REQUESTS_PER_WINDOW=50 \
RATE_LIMIT_WINDOW_SIZE_SECONDS=3600 \
cargo run --bin mq-web-api
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

## Features

- **Axum-based HTTP server** - Fast and reliable web framework
- **Rate limiting** - Built-in rate limiting with libsql/SQLite backend support
- **CORS support** - Configurable CORS origins for web applications
- **Structured logging** - JSON and text format logging with tracing
- **OpenAPI documentation** - Auto-generated API documentation
- **Database connection pooling** - Efficient database connections with deadpool-libsql
