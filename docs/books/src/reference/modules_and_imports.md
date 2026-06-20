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

| Module  | Parse function              | Description                               |
| ------- | --------------------------- | ----------------------------------------- |
| `json`  | `json::json_parse()`        | Parses a JSON string                      |
| `yaml`  | `yaml::yaml_parse()`        | Parses a YAML string                      |
| `toml`  | `toml::toml_parse()`        | Parses a TOML string                      |
| `xml`   | `xml::xml_parse()`          | Parses an XML string                      |
| `toon`  | `toon::toon_parse()`        | Parses a Toon string                      |
| `csv`   | `csv::csv_parse(has_header)` | Parses CSV (`,` delimiter)               |
| `csv`   | `csv::tsv_parse(has_header)` | Parses TSV (`\t` delimiter)              |
| `csv`   | `csv::psv_parse(has_header)` | Parses PSV (`\|` delimiter)              |

These modules are also used automatically when you process a file whose extension matches (see [CLI auto-parsing](./cli.md#auto-parsing-by-file-extension)).

### Example

```mq
import "json"
| json::json_parse()
```

## Markdown Builder (`md`)

The `md` module provides functions for constructing markdown nodes from scratch, rather than
filtering or transforming existing ones. Each function returns a markdown value that can be
combined with others using `md::doc()`, which merges an array of nodes into a single markdown
value.

> **Note:** This module is under development. APIs and behavior may change without notice.

```mq
import "md"
| md::doc([
    md::h("My Project", 1),
    md::text("Run `cargo install mq`."),
    md::code("cargo install mq", "bash"),
  ])
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

| Shorthand | Resolved URL |
|---|---|
| `github.com/alice/mymod` | `raw.githubusercontent.com/alice/mymod/HEAD/mymod.mq` |
| `github.com/alice/mymod.mq` | `raw.githubusercontent.com/alice/mymod.mq/HEAD/mymod.mq` |
| `github.com/alice/mymod@v1.0` | `raw.githubusercontent.com/alice/mymod/v1.0/mymod.mq` |
| `github.com/alice/repo/lib/util.mq@v2.0` | `raw.githubusercontent.com/alice/repo/v2.0/lib/util.mq` |

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

### CLI options

| Flag | Description |
|---|---|
| `--refresh-modules` | Discard cached mutable-ref modules and re-fetch them. |
| `--allowed-domain <domain>` | Allow HTTP imports from an additional domain beyond the default (`raw.githubusercontent.com/harehare`). Repeat to add multiple domains. |

**Examples:**

```sh
# Force re-fetch of any HEAD/branch modules
mq --refresh-modules 'self' file.md

# Only allow imports from example.com
mq --allowed-domain example.com 'self' file.md

# Allow multiple domains
mq --allowed-domain example.com --allowed-domain raw.githubusercontent.com 'self' file.md
```

## Comparison

| Feature  | `module`                          | `import`                          | `include`               |
| -------- | --------------------------------- | --------------------------------- | ----------------------- |
| Purpose  | Define a module                   | Load external module              | Load external functions |
| Access   | Qualified access (`module::func`) | Qualified access (`module::func`) | Direct access (`func`)  |
| Use case | Organize code within a file       | Reuse modules across files        | Simple function sharing |
