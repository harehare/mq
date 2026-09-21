# Compact JSON with TOON for LLM prompts

Goal: Shrink a JSON document before pasting it into an LLM prompt, by writing it as TOON, which states each array's field names once instead of repeating them on every row.

Prerequisites: None on the command line, because `-F toon` writes TOON and `-I toon` reads it. The built-in `toon` module does the same conversion from inside a query.

## Query

```bash
$ mq -I json -F toon '.' users.json
```

## Input (`users.json`)

```json
{
  "users": [
    { "id": 1, "name": "Alice", "role": "admin" },
    { "id": 2, "name": "Bob", "role": "dev" },
    { "id": 3, "name": "Carol", "role": "dev" }
  ]
}
```

## Output

```
users[3]{id,name,role}:
  1,Alice,admin
  2,Bob,dev
  3,Carol,dev
```

Minified, the JSON above is 122 characters and the TOON is 65.

## Embed it in a prompt

Use `toon_stringify` to convert one part of the data and put it inside a larger string:

```bash
$ mq -I json 'import "toon" | "Users:\n" + toon::toon_stringify(get("users"))' users.json
```

```
Users:
[3]{id,name,role}:
  1,Alice,admin
  2,Bob,dev
  3,Carol,dev
```

## Read TOON back

```bash
$ mq -I json -F toon '.' users.json | mq -I toon -F json 'get("users") | map(fn(u): u["name"];)'
```

```json
[
  "Alice",
  "Bob",
  "Carol"
]
```

## Notes

- The saving comes from arrays of objects that share the same keys, such as database rows or API list responses. A plain nested object still gets shorter, since TOON drops the braces and most quotes, but by less. `{"name": "demo", "keywords": ["a", "b"]}` becomes `name: demo` and `keywords[2]: a,b`.
- Strings that could be confused with another type are still quoted. `"1.0.0"` is written as `"1.0.0"`, while `tsc` is written bare.
- The counts above are characters, not tokens. Token counts depend on the model's tokenizer, so measure with your own before relying on a number.
- `-I json` is only the source format. YAML, TOML, XML and CSV can be the input too.
- To cut a Markdown document down to size instead, see [Trim a document down to LLM-sized context](extract-context-for-llm-prompts.md).
