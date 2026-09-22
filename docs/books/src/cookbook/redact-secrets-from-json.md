# Redact secrets from JSON, YAML, TOML or Markdown data

Goal: Before writing a config file, an API response, or a Markdown document into a report, a log, or an LLM prompt, mask the fields or text that might hold a password, API key, or other secret, without hand-writing a recursive walk over the data.

Prerequisites: None. `redact` is a core builtin.

## Query

```bash
$ mq -I json -F json 'redact(., [{type: "key", pattern: "api_key|password|secret|token"}])' config.json
```

Or with an explicit value:

```mq
redact(
  {"name": "demo", "api_key": "sk-abc123def456", "config": {"db_password": "hunter2"}},
  [{type: "key", pattern: "api_key|password|secret|token"}]
)
```

## Output

```json
{
  "name": "demo",
  "api_key": "[REDACTED]",
  "config": {
    "db_password": "[REDACTED]"
  }
}
```

## Redacting a Markdown document

`redact` also walks a Markdown document, masking matching secrets inside prose, code blocks, and YAML/TOML frontmatter alike:

```bash
$ mq 'redact(., [{type: "value", pattern: "sk-[a-z0-9]+"}])' notes.md
```

Given `notes.md`:

````markdown
---
api_key: sk-abc123def456
---

Run with `sk-abc123def456` set as the key.
````

the output is:

````markdown
---
api_key: [REDACTED]
---

Run with `[REDACTED]` set as the key.
````

## Notes

- `redact(value, rules)` walks a dict/array/string/markdown node recursively. `rules` is an array of:
  - `{type: "key", pattern, replacement = "[REDACTED]", case_insensitive = true}`: masks a dict value outright when its key matches `pattern`, without inspecting the value itself. Since Markdown nodes have no keys, `key` rules only ever apply inside dicts.
  - `{type: "value", pattern, replacement = "[REDACTED]", case_insensitive = true}`: replaces matches of `pattern` inside a string or Markdown node's text via `gsub`, leaving the rest of the text intact (useful for masking an email address or token embedded in a longer message or paragraph).
- `pattern` matches as a substring by default, so `"password"` also matches `db_password` and `PASSWORD_HASH` (matched case-insensitively unless `case_insensitive: false`). Anchor with `^...$` for an exact key-name match instead.
- mq's regex engine has no `(?i)` inline flag support, so `case_insensitive` downcases the subject before matching. Write `pattern` itself in lowercase.
- Numbers, booleans and `None` pass through unchanged. Rules run top to bottom for `value` rules on the same string, and a `key` rule short-circuits (the value is masked without recursing into it, so a secret nested inside an already-redacted key is never inspected).
- For a Markdown node with children (a heading, list, link, table cell, …), `redact` recurses into each child. A leaf node (text, code, code_inline, html, yaml, toml, image, definition, …) has `value` rules applied directly to its own text.
