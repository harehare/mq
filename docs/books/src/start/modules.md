# Modules

## Standard Library

Standard modules are built into `mq` — use them with `include` or `import`, no installation needed.

| Module | Description |
|---|---|
| `json` | JSON parser and formatter |
| `yaml` | YAML 1.2 parser and formatter |
| `toml` | TOML parser and formatter |
| `xml` | XML parser and formatter |
| `csv` | CSV / TSV parser and formatter |
| `hcl` | HCL (HashiCorp Configuration Language) parser |
| `cbor` | CBOR binary format support |
| `semver` | Semantic versioning (SemVer) utilities |
| `section` | Markdown section extraction helpers |
| `table` | Table rendering utilities |
| `fuzzy` | Fuzzy string matching |
| `toon` | TOON format support |
| `test` | Testing framework (`assert_eq`, `assert_true`, …) |

## Extension Modules

These modules extend mq with additional parsers, utilities, and domain-specific languages.
All modules support [HTTP Import](../reference/modules_and_imports.md#http-imports) — no local installation required.

```mq
import "github.com/harehare/<module-name>"
```

## Format Parsers

| Module | Description |
|---|---|
| [json5.mq](https://github.com/harehare/json5.mq) | [JSON5](https://json5.org) — comments, trailing commas, unquoted keys |
| [pkl.mq](https://github.com/harehare/pkl.mq) | [PKL](https://pkl-lang.org) — Apple's configuration language, with type annotations and collection literals |
| [kdl.mq](https://github.com/harehare/kdl.mq) | [KDL](https://kdl.dev) document language |
| [ini.mq](https://github.com/harehare/ini.mq) | INI file parser and serializer |
| [ndjson.mq](https://github.com/harehare/ndjson.mq) | [NDJSON](https://ndjson.org) / [JSON Lines](https://jsonlines.org) |
| [logfmt.mq](https://github.com/harehare/logfmt.mq) | logfmt structured log lines (`key=value`) |
| [cron.mq](https://github.com/harehare/cron.mq) | Cron expression parser and human-readable descriptions |
| [jwt.mq](https://github.com/harehare/jwt.mq) | JWT decoder — inspect header and payload without verification |
| [okf.mq](https://github.com/harehare/okf.mq) | [OKF](https://github.com/GoogleCloudPlatform/knowledge-catalog) (Open Knowledge Format) reader/writer — concept documents, cross-links, citations, log/index entries |
| [url.mq](https://github.com/harehare/url.mq) | URL parsing, building, and relative-resolution utilities for mq. |
| [changelog.mq](https://github.com/harehare/changelog.mq) | [Keep a Changelog](https://keepachangelog.com) Markdown parser and serializer |
| [dotenv.mq](https://github.com/harehare/dotenv.mq) | `.env` file parser and serializer — quotes, comments, and escape sequences |
| [jsonpath.mq](https://github.com/harehare/jsonpath.mq) | [JSONPath](https://en.wikipedia.org/wiki/JSONPath) ([RFC 9535](https://www.rfc-editor.org/rfc/rfc9535)-style) query engine for mq's parsed JSON values |
| [xpath.mq](https://github.com/harehare/xpath.mq) | Abbreviated [XPath](https://www.w3.org/TR/1999/REC-xpath-19991116/) query engine for `xml.mq`'s parsed value tree |


## Diagram & Graph

| Module | Description |
|---|---|
| [mermaid.mq](https://github.com/harehare/mermaid.mq) | [Mermaid](https://mermaid.js.org) diagrams — flowchart, sequence, pie, class |
| [dot.mq](https://github.com/harehare/dot.mq) | [Graphviz DOT](https://graphviz.org) — nodes, edges, attributes |
| [graphql.mq](https://github.com/harehare/graphql.mq) | GraphQL SDL — types, enums, interfaces, unions |
| [tree.mq](https://github.com/harehare/tree.mq) | A tree-rendering utility module for mq |


## DevOps & Infrastructure

| Module | Description |
|---|---|
| [dockerfile.mq](https://github.com/harehare/dockerfile.mq) | Dockerfile instruction parser |
| [k8s.mq](https://github.com/harehare/k8s.mq) | [Kubernetes](https://kubernetes.io) manifest parser — metadata, containers, images, ports, resources |
| [gha.mq](https://github.com/harehare/gha.mq) | [GitHub Actions](https://docs.github.com/en/actions) workflow parser — jobs, steps, triggers, matrix |
| [openapi.mq](https://github.com/harehare/openapi.mq) | [OpenAPI 3.x](https://spec.openapis.org/oas/v3.1.0) spec parser — paths, operations, schemas, security schemes |
| [aws.mq](https://github.com/harehare/aws.mq) | AWS CLI / SDK JSON response processor — filter, extract, and render Markdown tables for EC2, S3, IAM, Lambda, RDS, ECS, EKS, and 50+ other services |

## Terminal & Text

| Module | Description |
|---|---|
| [ansi.mq](https://github.com/harehare/ansi.mq) | ANSI terminal escape code utilities |
| [case.mq](https://github.com/harehare/case.mq) | String case conversion utilities implemented as an mq module |
| [emoji.mq](https://github.com/harehare/emoji.mq) | GitHub-style emoji shortcode <-> Unicode emoji conversion |

## Interpreters

| Module | Description |
|---|---|
| [lisp.mq](https://github.com/harehare/lisp.mq) | Scheme-like Lisp interpreter |
| [bf.mq](https://github.com/harehare/bf.mq) | Brainfuck interpreter |

## Libraries & Toolkits

| Module | Description |
|---|---|
| [parser_combinator.mq](https://github.com/harehare/parser_combinator.mq) | A small parser-combinator toolkit, in the spirit of Rust's [nom](https://github.com/rust-bakery/nom) |
| [diff.mq](https://github.com/harehare/diff.mq) | Text and array diffing utilities, built on mq's native Myers-diff engine |


