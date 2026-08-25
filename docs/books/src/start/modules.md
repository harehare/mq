# Modules

## Standard Library

Standard modules are built into `mq` — use them with `include` or `import`, no installation needed.

| Module | Description |
|---|---|
| `json` | JSON parser and formatter |
| `yaml` | YAML 1.2 parser and formatter |
| `toml` | TOML parser and formatter |
| `xml` | XML parser and formatter |
| `html` | HTML parser and formatter (requires the `css-selector` build feature) |
| `csv` | CSV / TSV parser and formatter |
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

Type to search by name or description, or click a category to filter.

<div class="module-toolbar">
  <input type="text" id="module-search-input" class="module-search-input" placeholder="Search modules…" aria-label="Search extension modules">
  <span id="module-count" class="module-count"></span>
</div>
<div id="module-category-filters" class="module-category-chips">
  <button type="button" class="module-chip active" data-category="all">All</button>
  <button type="button" class="module-chip" data-category="format-parsers">Format Parsers</button>
  <button type="button" class="module-chip" data-category="diagram-graph">Diagram &amp; Graph</button>
  <button type="button" class="module-chip" data-category="devops-infrastructure">DevOps &amp; Infrastructure</button>
  <button type="button" class="module-chip" data-category="terminal-text">Terminal &amp; Text</button>
  <button type="button" class="module-chip" data-category="generators">Generators</button>
  <button type="button" class="module-chip" data-category="interpreters-examples">Interpreters &amp; Examples</button>
  <button type="button" class="module-chip" data-category="libraries-toolkits">Libraries &amp; Toolkits</button>
</div>

<table id="modules-table">
<thead>
<tr><th>Module</th><th>Category</th><th>Description</th></tr>
</thead>
<tbody>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/json5.mq">json5.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://json5.org">JSON5</a> — comments, trailing commas, unquoted keys</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/hcl.mq">hcl.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://github.com/hashicorp/hcl">HCL</a> (HashiCorp Configuration Language) — blocks, labels, attributes</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/pkl.mq">pkl.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://pkl-lang.org">PKL</a> — Apple's configuration language, with type annotations and collection literals</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/kdl.mq">kdl.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://kdl.dev">KDL</a> document language</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/ini.mq">ini.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>INI file parser and serializer</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/ndjson.mq">ndjson.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://ndjson.org">NDJSON</a> / <a href="https://jsonlines.org">JSON Lines</a></td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/logfmt.mq">logfmt.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>logfmt structured log lines (<code>key=value</code>)</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/cron.mq">cron.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>Cron expression parser and human-readable descriptions</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/jwt.mq">jwt.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>JWT decoder — inspect header and payload without verification</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/okf.mq">okf.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://github.com/GoogleCloudPlatform/knowledge-catalog">OKF</a> (Open Knowledge Format) reader/writer — concept documents, cross-links, citations, log/index entries</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/url.mq">url.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>URL parsing, building, and relative-resolution utilities for mq.</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/changelog.mq">changelog.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://keepachangelog.com">Keep a Changelog</a> Markdown parser and serializer</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/dotenv.mq">dotenv.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><code>.env</code> file parser and serializer — quotes, comments, and escape sequences</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/jsonpath.mq">jsonpath.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://en.wikipedia.org/wiki/JSONPath">JSONPath</a> (<a href="https://www.rfc-editor.org/rfc/rfc9535">RFC 9535</a>-style) query engine for mq's parsed JSON values</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/jsonschema.mq">jsonschema.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://json-schema.org">JSON Schema</a> validator</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/xpath.mq">xpath.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>Abbreviated <a href="https://www.w3.org/TR/1999/REC-xpath-19991116/">XPath</a> query engine for <code>xml.mq</code>'s parsed value tree</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/codeowners.mq">codeowners.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>GitHub <a href="https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners">CODEOWNERS</a> parser and matcher</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/feed.mq">feed.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td>RSS 2.0 / Atom feed parser</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/gitignore.mq">gitignore.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://git-scm.com/docs/gitignore">.gitignore</a> pattern parser and matcher</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/asciidoc.mq">asciidoc.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://docs.asciidoctor.org/asciidoc/latest/">AsciiDoc</a> to Markdown converter</td></tr>
<tr data-category="format-parsers"><td><a href="https://github.com/harehare/jsonld.mq">jsonld.mq</a></td><td><span class="module-category-badge">Format Parsers</span></td><td><a href="https://www.w3.org/TR/json-ld11/">JSON-LD</a> <code>&lt;script type="application/ld+json"&gt;</code> extractor</td></tr>
<tr data-category="diagram-graph"><td><a href="https://github.com/harehare/mermaid.mq">mermaid.mq</a></td><td><span class="module-category-badge">Diagram &amp; Graph</span></td><td><a href="https://mermaid.js.org">Mermaid</a> diagrams — flowchart, sequence, pie, class</td></tr>
<tr data-category="diagram-graph"><td><a href="https://github.com/harehare/dot.mq">dot.mq</a></td><td><span class="module-category-badge">Diagram &amp; Graph</span></td><td><a href="https://graphviz.org">Graphviz DOT</a> — nodes, edges, attributes</td></tr>
<tr data-category="diagram-graph"><td><a href="https://github.com/harehare/graphql.mq">graphql.mq</a></td><td><span class="module-category-badge">Diagram &amp; Graph</span></td><td>GraphQL SDL — types, enums, interfaces, unions</td></tr>
<tr data-category="diagram-graph"><td><a href="https://github.com/harehare/tree.mq">tree.mq</a></td><td><span class="module-category-badge">Diagram &amp; Graph</span></td><td>A tree-rendering utility module for mq</td></tr>
<tr data-category="devops-infrastructure"><td><a href="https://github.com/harehare/dockerfile.mq">dockerfile.mq</a></td><td><span class="module-category-badge">DevOps &amp; Infrastructure</span></td><td>Dockerfile instruction parser</td></tr>
<tr data-category="devops-infrastructure"><td><a href="https://github.com/harehare/k8s.mq">k8s.mq</a></td><td><span class="module-category-badge">DevOps &amp; Infrastructure</span></td><td><a href="https://kubernetes.io">Kubernetes</a> manifest parser — metadata, containers, images, ports, resources</td></tr>
<tr data-category="devops-infrastructure"><td><a href="https://github.com/harehare/gha.mq">gha.mq</a></td><td><span class="module-category-badge">DevOps &amp; Infrastructure</span></td><td><a href="https://docs.github.com/en/actions">GitHub Actions</a> workflow parser — jobs, steps, triggers, matrix</td></tr>
<tr data-category="devops-infrastructure"><td><a href="https://github.com/harehare/openapi.mq">openapi.mq</a></td><td><span class="module-category-badge">DevOps &amp; Infrastructure</span></td><td><a href="https://spec.openapis.org/oas/v3.1.0">OpenAPI 3.x</a> spec parser — paths, operations, schemas, security schemes</td></tr>
<tr data-category="devops-infrastructure"><td><a href="https://github.com/harehare/aws.mq">aws.mq</a></td><td><span class="module-category-badge">DevOps &amp; Infrastructure</span></td><td>AWS CLI / SDK JSON response processor — filter, extract, and render Markdown tables for EC2, S3, IAM, Lambda, RDS, ECS, EKS, and 50+ other services</td></tr>
<tr data-category="terminal-text"><td><a href="https://github.com/harehare/ansi.mq">ansi.mq</a></td><td><span class="module-category-badge">Terminal &amp; Text</span></td><td>ANSI terminal escape code utilities</td></tr>
<tr data-category="terminal-text"><td><a href="https://github.com/harehare/case.mq">case.mq</a></td><td><span class="module-category-badge">Terminal &amp; Text</span></td><td>String case conversion utilities implemented as an mq module</td></tr>
<tr data-category="terminal-text"><td><a href="https://github.com/harehare/emoji.mq">emoji.mq</a></td><td><span class="module-category-badge">Terminal &amp; Text</span></td><td>GitHub-style emoji shortcode &lt;-&gt; Unicode emoji conversion</td></tr>
<tr data-category="generators"><td><a href="https://github.com/harehare/qrcode.mq">qrcode.mq</a></td><td><span class="module-category-badge">Generators</span></td><td><a href="https://www.qrcode.com/en/about/standards.html">QR Code</a> (ISO/IEC 18004) encoder — ASCII-art and SVG rendering</td></tr>
<tr data-category="generators"><td><a href="https://github.com/harehare/badge.mq">badge.mq</a></td><td><span class="module-category-badge">Generators</span></td><td><a href="https://shields.io">shields.io</a> badge generator</td></tr>
<tr data-category="generators"><td><a href="https://github.com/harehare/sparkline.mq">sparkline.mq</a></td><td><span class="module-category-badge">Generators</span></td><td>A pure Unicode sparkline renderer</td></tr>
<tr data-category="interpreters-examples"><td><a href="https://github.com/harehare/lisp.mq">lisp.mq</a></td><td><span class="module-category-badge">Interpreters &amp; Examples</span></td><td>Scheme-like Lisp interpreter</td></tr>
<tr data-category="interpreters-examples"><td><a href="https://github.com/harehare/bf.mq">bf.mq</a></td><td><span class="module-category-badge">Interpreters &amp; Examples</span></td><td>Brainfuck interpreter</td></tr>
<tr data-category="interpreters-examples"><td><a href="https://github.com/harehare/jq.mq">jq.mq</a></td><td><span class="module-category-badge">Interpreters &amp; Examples</span></td><td>Implementation of the <a href="https://jqlang.org">jq</a> JSON processor, written in mq</td></tr>
<tr data-category="libraries-toolkits"><td><a href="https://github.com/harehare/parser_combinator.mq">parser_combinator.mq</a></td><td><span class="module-category-badge">Libraries &amp; Toolkits</span></td><td>A small parser-combinator toolkit, in the spirit of Rust's <a href="https://github.com/rust-bakery/nom">nom</a></td></tr>
<tr data-category="libraries-toolkits"><td><a href="https://github.com/harehare/diff.mq">diff.mq</a></td><td><span class="module-category-badge">Libraries &amp; Toolkits</span></td><td>Text and array diffing utilities, built on mq's native Myers-diff engine</td></tr>
<tr data-category="libraries-toolkits"><td><a href="https://github.com/harehare/template.mq">template.mq</a></td><td><span class="module-category-badge">Libraries &amp; Toolkits</span></td><td>A lightweight Mustache/Handlebars-style templating engine</td></tr>
<tr data-category="libraries-toolkits"><td><a href="https://github.com/harehare/returns.mq">returns.mq</a></td><td><span class="module-category-badge">Libraries &amp; Toolkits</span></td><td>Result and Maybe types for railway-oriented error handling, inspired by <a href="https://github.com/dry-python/returns">dry-python/returns</a></td></tr>
</tbody>
</table>
