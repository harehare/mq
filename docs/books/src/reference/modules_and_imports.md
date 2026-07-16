# Modules and Imports

mq provides several ways to organize and reuse code: `module`, `import`, and `include`.

## Module

Defines a module to group related functions and prevent naming conflicts using the syntax `module name: ... end`.

```mq
module module_name:
  def function1(): ...
  def function2(): ...
end
```

Functions within a module can be accessed using qualified access syntax:

```mq
module_name::function1()
```

### Examples

```mq
# Define a math module
module math:
  def add(a, b): a + b;
  def sub(a, b): a - b;
  def mul(a, b): a * b;
end

# Use functions from the module
| math::add(5, 3)  # Returns 8
| math::mul(4, 2)  # Returns 8
```

## Import

Loads a module from an external file using the syntax `import "module_path"`.
The imported module is available with its defined name and can be accessed using qualified access syntax.

The import directive searches for .mq files in the following locations:

- `$HOME/.mq` - User's home directory mq folder
- `$ORIGIN/../lib/mq` - Library directory relative to the source file
- `$ORIGIN/../lib` - Parent lib directory relative to the source file
- `$ORIGIN` - Current directory relative to the source file

```mq
import "module_name"
```

### Examples

**math.mq:**

```mq
def add(a, b): a + b;
def sub(a, b): a - b;
```

**main.mq:**

```mq
# Import the math module
import "math"

# Use functions with qualified access
| math::add(10, 5)  # Returns 15
| math::sub(10, 5)  # Returns 5
```

### Import Aliases

Use `import "module_path" as alias` to bind the module under a different name, useful for
shortening long module paths or avoiding naming conflicts. Only the alias is bound; the
module's original name is not available.

```mq
import "math" as m

| m::add(10, 5)  # Returns 15
| m::sub(10, 5)  # Returns 5
```

## Include

Loads functions from an external file directly into the current namespace using the syntax `include "module_name"`.
Unlike `import`, functions are available without a namespace prefix.

The include directive searches for .mq files in the same locations as `import`.

```mq
include "module_name"
```

### Examples

**math.mq:**

```mq
def add(a, b): a + b;
def sub(a, b): a - b;
```

**main.mq:**

```mq
# Include math functions
include "math"

# Functions are available directly
| add(2, 3)  # Returns 5
| sub(10, 4) # Returns 6
```

## Built-in modules

mq ships several built-in modules for parsing common structured data formats.
They are available via `import` without any additional installation.

| Module | Parse function               | Description                 |
| ------ | ---------------------------- | --------------------------- |
| `json` | `json::json_parse()`         | Parses a JSON string        |
| `yaml` | `yaml::yaml_parse()`         | Parses a YAML string        |
| `toml` | `toml::toml_parse()`         | Parses a TOML string        |
| `xml`  | `xml::xml_parse()`           | Parses an XML string        |
| `toon` | `toon::toon_parse()`         | Parses a Toon string        |
| `csv`  | `csv::csv_parse(has_header)` | Parses CSV (`,` delimiter)  |
| `csv`  | `csv::tsv_parse(has_header)` | Parses TSV (`\t` delimiter) |
| `csv`  | `csv::psv_parse(has_header)` | Parses PSV (`\|` delimiter) |

These modules are also used automatically when you process a file whose extension matches (see [CLI auto-parsing](./cli.md#auto-parsing-by-file-extension)).

### Example

```mq
import "json"
| json::json_parse()
```

## Markdown Builder (`md`)

The `md` module provides functions for constructing markdown nodes from scratch, rather than
filtering or transforming existing ones. Each function returns a markdown value that can be
combined with others using `md::doc()`, which merges nodes into a single markdown value.
`md::doc()` accepts either a variable number of arguments or a single array, and flattens nested
arrays automatically, so the result of `map()` (or any function returning a plain array of nodes)
can be spliced in directly as children, and `None` entries (e.g. from conditional branches) are
dropped.

> **Note:** This module is under development. APIs and behavior may change without notice.

```mq
import "md"
| md::doc(
    md::h("My Project", 1),
    md::text("Run `cargo install mq`."),
    md::code("cargo install mq", "bash"),
    map(["fast", "composable", "jq-like"], fn(x): md::list(x);),
  )
```

Since the current value (`self`) is automatically passed when a call is missing an argument,
builder calls also read naturally in pipeline position:

```mq
"My Project" | md::h(1)
# equivalent to md::h("My Project", 1)
```

Lists and tables are built the same way:

```mq
import "md"
| md::doc(
    # List
    md::list("Plain item"),
    md::list("Nested item", 1),
    md::list("Ordered item", 0, true),
    md::list("Checked item", 0, false, true),
    # Table
    md::table_row(["Name", "Age"]),
    md::table_align(["left", "right"]),
    md::table_row(["Alice", "30"]),
  )
```

## HTTP Imports

When `mq` is built with the `http-import` feature, `import` and `include` accept HTTP/HTTPS URLs
in addition to local file names.

> **Security note:** By default, only URLs under `github.com/harehare` (resolved to `raw.githubusercontent.com/harehare`) are allowed.
> Importing from any other domain requires explicitly enabling it with the `--allowed-domain` flag.

### Plain URL

```mq
import "https://example.com/mymod.mq"
```

### GitHub shorthand

The scheme can be omitted for GitHub repositories.
mq automatically maps the path to `raw.githubusercontent.com`.

```
github.com/{owner}/{path}[@{version}]
```

| Shorthand                                | Resolved URL                                             |
| ---------------------------------------- | -------------------------------------------------------- |
| `github.com/alice/mymod`                 | `raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq`    |
| `github.com/alice/mymod.mq`              | `raw.githubusercontent.com/alice/mymod.mq/HEAD/mymod.mq` |
| `github.com/alice/mymod@v1.0`            | `raw.githubusercontent.com/alice/mymod/v1.0/mymod.mq`    |
| `github.com/alice/repo/lib/util.mq@v2.0` | `raw.githubusercontent.com/alice/repo/v2.0/lib/util.mq`  |

**Example:**

```mq
import "github.com/harehare/kdl.mq"
| kdl::kdl_parse("title \"Hello, World!\"")
```

### Caching

Fetched modules are cached in `{system_cache_dir}/mq/` as `{md5(url)}.mq` files.

- **Versioned URLs** (e.g. `@v0.1.0`): cached indefinitely — the tag content is immutable.
- **Mutable refs** (`HEAD`, `main`, `master`, or no version): cached on first fetch.
  Pass `--refresh-modules` on the command line to discard the cache and re-fetch.

### Lock file (`mq.lock`)

Every fetched module URL's SHA-256 content hash is recorded in an `mq.lock` file, created in
the current directory the first time a URL is fetched. This makes HTTP imports reproducible
across machines and CI: if the same URL is fetched again with different content — from a
different machine, a fresh (cold) cache, or after `--refresh-modules` clears the disk cache
— mq fails with an error instead of silently using whatever the remote now serves.

The check also applies to disk-cache hits, not just network fetches. The module cache is
shared per machine while `mq.lock` is per project, so a project whose lock file expects
different content than the local cache holds fails the same way, and a project without an
`mq.lock` yet gets one created even when every module is served from the cache.

- **First use of a URL** (fetched or served from the cache): recorded in `mq.lock`.
- **Later use, content unchanged**: succeeds silently.
- **Later use, content changed**: fails with an error explaining the mismatch. Re-run with
  `--refresh-modules` to accept the new content and update `mq.lock` (for a versioned/tagged
  URL, whose cache `--refresh-modules` doesn't touch, use `--clear-cache` instead).
- `--refresh-modules` / `--clear-cache` also drop the corresponding entries from `mq.lock`
  (mutable-ref entries only, and all entries respectively), matching their disk-cache behavior.
- Pass `--no-lockfile` to disable the check entirely (no file is read or written).
- Pass `--lockfile <path>` to use a different location instead of `./mq.lock`. Missing parent
  directories are created automatically. Mutually exclusive with `--no-lockfile`.

Commit `mq.lock` alongside scripts that use HTTP imports so CI and teammates fetch the exact
content you locked, the same way `package-lock.json`/`deno.lock` work.

### CLI options

| Flag                        | Description                                                                                                                             |
| --------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| `--refresh-modules`         | Discard cached mutable-ref modules and re-fetch them, updating their `mq.lock` entries.                                                 |
| `--allowed-domain <domain>` | Allow HTTP imports from an additional domain beyond the default (`raw.githubusercontent.com/harehare`). Repeat to add multiple domains. |
| `--no-lockfile`             | Disable the `mq.lock` integrity check/update.                                                                                           |
| `--lockfile <path>`         | Use `<path>` instead of `./mq.lock`.                                                                                                     |

**Examples:**

```sh
# Force re-fetch of any HEAD/branch modules, accepting new content into mq.lock
mq --refresh-modules 'self' file.md

# Only allow imports from example.com
mq --allowed-domain example.com 'self' file.md

# Allow multiple domains
mq --allowed-domain example.com --allowed-domain raw.githubusercontent.com 'self' file.md

# Use a different lock file location
mq --lockfile config/mq.lock 'self' file.md

# Skip the mq.lock check entirely
mq --no-lockfile 'self' file.md
```

## Network and File-Write Capabilities

`http(method, url)` / `http(method, url, body)` / `http(method, url, headers)` /
`http(method, url, body, headers)` and `write_file(path, content)` are disabled by default and
must be explicitly enabled with `--allow-net` / `--allow-write`. Calling them without the
corresponding flag raises a runtime error explaining how to opt in.

`method` is a string or symbol (`"post"` or `:post`) and accepts any HTTP method — `get`, `post`,
`put`, `delete`, `patch`, `head`, and so on. The optional `body` argument is sent as the request
body regardless of method. The optional `headers` argument is a dict of string to string
(e.g. `{"Content-Type": "application/json"}`) applied to the request.

`http_get(url, headers = {})`, `http_post(url, body, headers = {})`,
`http_put(url, body, headers = {})`, `http_patch(url, body, headers = {})`,
`http_delete(url, headers = {})`, and `http_head(url, headers = {})` are convenience wrappers
around `http(:get, url, headers)`, `http(:post, url, body, headers)`, and so on, for the most
common cases — `headers` defaults to `{}` and can be omitted.

> **Security note:** `http` only accepts `https://` URLs and is routed through the same
> SSRF-hardened client used for HTTP imports — no automatic redirects, and DNS results are
> filtered to publicly routable addresses, so a loopback/private/link-local address can't be
> reached even with `--allow-net` set.

```sh
# Blocked by default
mq 'http_get("https://example.com")'

# Enabled explicitly
mq --allow-net 'http_get("https://example.com")'
mq --allow-net 'http(:delete, "https://example.com/resource/1")'
mq --allow-net 'http(:post, "https://example.com", "{}", {"Content-Type": "application/json"})'
mq --allow-net 'http_post("https://example.com", "{}", {"Content-Type": "application/json"})'
mq --allow-write 'write_file("out.md", "# Hello")'
```

## Comparison

| Feature  | `module`                          | `import`                          | `include`               |
| -------- | --------------------------------- | --------------------------------- | ----------------------- |
| Purpose  | Define a module                   | Load external module              | Load external functions |
| Access   | Qualified access (`module::func`) | Qualified access (`module::func`) | Direct access (`func`)  |
| Use case | Organize code within a file       | Reuse modules across files        | Simple function sharing |
