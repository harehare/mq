# Redact secrets from JSON, YAML or TOML data

Goal: Before writing a config file, an API response, or any structured document into a
report, a log, or an LLM prompt, mask the fields that might hold a password, API key, or
other secret — without hand-writing a recursive walk over the data.

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

## Notes

- `redact(value, rules)` walks a dict/array/string recursively. `rules` is an array of:
  - `{type: "key", pattern, replacement = "[REDACTED]", case_insensitive = true}` — masks a
    dict value outright when its key matches `pattern`, without inspecting the value itself.
  - `{type: "value", pattern, replacement = "[REDACTED]", case_insensitive = true}` — replaces
    matches of `pattern` inside string values via `gsub`, leaving the rest of the string intact
    (useful for masking an email address or token embedded in a longer message).
- `pattern` matches as a substring by default, so `"password"` also matches `db_password` and
  `PASSWORD_HASH` (matched case-insensitively unless `case_insensitive: false`). Anchor with
  `^...$` for an exact key-name match instead.
- mq's regex engine has no `(?i)` inline flag support, so `case_insensitive` downcases the
  subject before matching — write `pattern` itself in lowercase.
- Numbers, booleans and `None` pass through unchanged. Rules run top to bottom for `value`
  rules on the same string, and a `key` rule short-circuits (the value is masked without
  recursing into it, so a secret nested inside an already-redacted key is never inspected).
- Markdown nodes are walked (children are redacted recursively) but a node's own text is not
  rewritten in place — call `redact(to_text(node), rules)` for prose text instead.
