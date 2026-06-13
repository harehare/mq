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
| [kdl.mq](https://github.com/harehare/kdl.mq) | [KDL](https://kdl.dev) document language |
| [ini.mq](https://github.com/harehare/ini.mq) | INI file parser and serializer |
| [ndjson.mq](https://github.com/harehare/ndjson.mq) | [NDJSON](https://ndjson.org) / [JSON Lines](https://jsonlines.org) |
| [logfmt.mq](https://github.com/harehare/logfmt.mq) | logfmt structured log lines (`key=value`) |
| [cron.mq](https://github.com/harehare/cron.mq) | Cron expression parser and human-readable descriptions |
| [jwt.mq](https://github.com/harehare/jwt.mq) | JWT decoder — inspect header and payload without verification |

## Diagram & Graph

| Module | Description |
|---|---|
| [mermaid.mq](https://github.com/harehare/mermaid.mq) | [Mermaid](https://mermaid.js.org) diagrams — flowchart, sequence, pie, class |
| [dot.mq](https://github.com/harehare/dot.mq) | [Graphviz DOT](https://graphviz.org) — nodes, edges, attributes |
| [graphql.mq](https://github.com/harehare/graphql.mq) | GraphQL SDL — types, enums, interfaces, unions |

## DevOps & Infrastructure

| Module | Description |
|---|---|
| [dockerfile.mq](https://github.com/harehare/dockerfile.mq) | Dockerfile instruction parser |

## Terminal & Text

| Module | Description |
|---|---|
| [ansi.mq](https://github.com/harehare/ansi.mq) | ANSI terminal escape code utilities |
| [regex.mq](https://github.com/harehare/regex.mq) | Regular expression engine |

## Interpreters

| Module | Description |
|---|---|
| [lisp.mq](https://github.com/harehare/lisp.mq) | Scheme-like Lisp interpreter |
| [bf.mq](https://github.com/harehare/bf.mq) | Brainfuck interpreter |

## Algorithms & Functional Programming

| Module | Description |
|---|---|
| [monad.mq](https://github.com/harehare/monad.mq) | Monadic abstractions |
| [bm25.mq](https://github.com/harehare/bm25.mq) | BM25 text ranking algorithm |

## Simulations

| Module | Description |
|---|---|
| [game-of-life.mq](https://github.com/harehare/game-of-life.mq) | Conway's Game of Life |
