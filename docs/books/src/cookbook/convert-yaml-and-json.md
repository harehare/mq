# Convert between YAML and JSON

Goal: Turn a YAML config into JSON, or the other way around, and pick out values on the way.

Prerequisites: The built-in `yaml` module (and `json` for the reverse direction). Read the file with `-I raw` to get its text as a single string.

## Query

YAML to JSON:

```bash
$ mq -I raw -F json 'import "yaml" | yaml::yaml_parse' config.yaml
```

JSON to YAML:

```bash
$ echo '{"name": "web", "replicas": 3, "tags": ["a", "b"]}' | mq -I raw 'import "json" | import "yaml" | json::json_parse | yaml::yaml_stringify'
```

## Input (`config.yaml`)

```yaml
name: web
replicas: 3
env:
  - name: LOG_LEVEL
    value: debug
  - name: PORT
    value: "8080"
```

## Output

YAML to JSON:

```json
{
  "name": "web",
  "replicas": 3,
  "env": [
    {
      "name": "LOG_LEVEL",
      "value": "debug"
    },
    {
      "name": "PORT",
      "value": "8080"
    }
  ]
}
```

JSON to YAML:

```yaml
name: web
replicas: 3
tags:
  - a
  - b
```

## Notes

- Pick out one part before converting, for example the `env` list as a table: `mq -I raw 'import "yaml" | yaml::yaml_parse | get("env") | yaml::yaml_to_markdown_table' config.yaml`.
- A file with several `---`-separated documents is parsed into an array with one entry per document.
- `yaml_stringify` sorts keys alphabetically, so the key order of the source is not kept.
- To read YAML frontmatter from a Markdown file, see [Extract frontmatter metadata](extract-frontmatter.md).
