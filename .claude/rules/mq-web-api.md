---
paths: crates/mq-web-api/**
---

# mq-web-api Rules

## Purpose

Web API bindings for mq, providing HTTP/REST interface.

## Coding Rules

- Follow REST/HTTP best practices
- Provide clear, versioned API endpoints
- Use appropriate HTTP status codes and error responses
- Document all endpoints with OpenAPI/Swagger or similar
- Implement proper authentication and authorization if needed
- Handle errors gracefully with clear error messages
- Use `miette` for error handling on the Rust side
- Write integration tests for all API endpoints
- Support CORS appropriately for web usage
- Provide rate limiting and security measures
- Validate all input thoroughly
- Return appropriate Content-Type headers
- Support JSON request/response format
- Test with various HTTP clients and browsers
- Document API versioning and deprecation policies
- Keep the API minimal and focused
- Optimize for performance and low latency
