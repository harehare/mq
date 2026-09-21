# Flatten JSON into greppable paths

Goal: Turn a nested JSON document into one `path = value;` line per value, so `grep` can find where a value lives, and rebuild JSON from the lines you keep.

Prerequisites: None for the command line, because `-F gron` writes gron and `-I gron` reads it. The built-in `gron` module does the same conversion from inside a query.

## Query

Find where `vitest` and `react` appear in a `package.json`:

```bash
$ mq -I json -F gron '.' package.json | grep -i 'vitest\|react'
```

## Input (`package.json`)

```json
{
  "name": "demo",
  "version": "1.0.0",
  "scripts": { "build": "tsc", "test": "vitest" },
  "dependencies": { "react": "^18.2.0", "zod": "^3.22.0" },
  "keywords": ["a", "b"]
}
```

## Output

```
json.scripts.test = "vitest";
json.dependencies.react = "^18.2.0";
```

Without the `grep`, every value gets its own line, rooted at `json`:

```
json = {};
json.name = "demo";
json.version = "1.0.0";
json.scripts = {};
json.scripts.build = "tsc";
json.scripts.test = "vitest";
json.dependencies = {};
json.dependencies.react = "^18.2.0";
json.dependencies.zod = "^3.22.0";
json.keywords = [];
json.keywords[0] = "a";
json.keywords[1] = "b";
```

## Rebuild JSON from the lines you keep

Pipe the filtered lines back through `-I gron` to get JSON again. This keeps only the dependencies:

```bash
$ mq -I json -F gron '.' package.json | grep '^json.dependencies' | mq -I gron -F json '.'
```

```json
{
  "dependencies": {
    "react": "^18.2.0",
    "zod": "^3.22.0"
  }
}
```

And this drops the `scripts` object instead:

```bash
$ mq -I json -F gron '.' package.json | grep -v '^json.scripts' | mq -I gron -F json '.'
```

```json
{
  "name": "demo",
  "version": "1.0.0",
  "dependencies": {
    "react": "^18.2.0",
    "zod": "^3.22.0"
  },
  "keywords": [
    "a",
    "b"
  ]
}
```

## Notes

- `-I json` is only the source format. YAML, TOML, XML and CSV work the same way, for example `mq -I yaml -F gron '.' config.yaml`.
- Keys that are not plain identifiers use bracket notation, so `{"a-b": {"c d": 1}}` becomes `json["a-b"]["c d"] = 1;`.
- Inside a query, `import "gron" | gron::gron_stringify` produces the same lines, and `gron::gron_parse` reads them back. Unlike `-F gron`, `gron_stringify` sorts object keys alphabetically, so use `-F gron` when you want the lines in the file's own key order.
- You do not need the `json.dependencies = {};` line to rebuild. `-I gron` creates parent objects from the paths, so `grep '^json.dependencies\.'`, which matches only the leaf lines, gives the same result as the example above.
- To convert the whole file rather than search it, see [Convert between YAML and JSON](convert-yaml-and-json.md).
