# Read values from a TOML file

Goal: Read a single value, or a whole table, out of a TOML file such as `Cargo.toml` or `pyproject.toml`.

Prerequisites: The built-in `toml` module. Read the file with `-I raw` to get its text as a single string.

## Query

The dependencies as a Markdown table:

```bash
$ mq -I raw 'import "toml" | toml::toml_parse | get("dependencies") | toml::toml_to_markdown_table' Cargo.toml
```

## Input (`Cargo.toml`)

```toml
[package]
name = "demo"
version = "0.3.1"
edition = "2021"

[dependencies]
serde = "1.0"
clap = { version = "4.5", features = ["derive"] }
miette = "7"
```

## Output

```markdown
| Key | Value |
| --- | --- |
| clap | {"features": ["derive"], "version": "4.5"} |
| miette | 7 |
| serde | 1.0 |
```

## Notes

- One value: `mq -I raw 'import "toml" | toml::toml_parse | get("package") | get("version")' Cargo.toml` prints `0.3.1`.
- The crate names only, as JSON: `mq -I raw -F json 'import "toml" | toml::toml_parse | get("dependencies") | keys' Cargo.toml` returns `["clap", "miette", "serde"]`.
- `toml_to_json(data)` serializes a parsed file as compact JSON.
- `toml_stringify` writes TOML back, but it sorts keys and drops comments. To change one value in place and keep the rest of the file as it is, see [Bump a version string across a file](update-text-in-place.md).
