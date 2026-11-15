# Coming from jq

mq reads Markdown by default, but it also reads JSON (`-I json`, or `-T json` for JSON in and out). The query language is not jq syntax, so this page maps common jq filters to mq. Every row was run against mq 0.10.1.

Examples use this input:

```json
{"name":"a","n":3,"items":[{"id":1,"t":"x"},{"id":2,"t":"y"}],"tags":["p","q"]}
```

## Key differences

- `.name` is a Markdown selector in mq, not a field access. Use `.["name"]`.
- Function calls need parentheses: `len()`, `keys()`, `sort()`.
- Lambdas are written `fn(x): body;`, for example `map(fn(x): x["id"];)`.
- `if`, `elif` and `else` take a colon: `if (cond): a else: b`.
- Chained indexing such as `.["a"]["b"]` is not supported. Pipe instead: `.["a"] | .["b"]`, or use `get(get(., "a"), "b")`.
- A filter does not produce multiple outputs. `.[]` returns the array itself, so use `map`, `filter` or `foreach` to work per element.

## Command-line options

| jq | mq | Notes |
| -- | -- | ----- |
| `jq . f.json` | `mq -I json -F json . f.json` | `-T json` sets both |
| `jq -c` | `mq -F json -c` | |
| `jq -r` | `mq -F raw` | |
| `jq -n` | `mq -I null` | |
| `jq -s` | `mq -A` | Collects the input into one array. Use `--eval-all` across files |
| `jq --tab`, `--indent n` | `--tab`, `--indent n` | Only with `-F json` or `-F xml` |
| `jq -e` | `mq -e` | Exit status 4 when every result is empty |
| `jq -f file.jq` | `mq -f file.mq` | |
| `jq --arg k v` | `mq --arg k v` | `k` is a variable in the query |
| `jq --argjson k 3` | `mq --argjson k 3` | |
| `jq --slurpfile k f` | `mq --slurpfile k f` | `k` is an array of the values in `f` |
| `jq --rawfile k f` | `mq --rawfile k f` | |
| `jq --args a b` | `mq --args a b` | Read as `ARGS."positional"`. Place it after the query and files |
| `jq --stream` | `mq --stream` | Line-by-line processing of large files |
| `jq 'env.HOME'`, `$ENV.HOME` | `mq -E '$HOME'` | Environment access is off unless `-E` is given |

```sh
$ mq -I json --arg k name '.[k]' d.json
a
$ mq -I json --argjson k 3 '.["n"] + k' d.json
6
```

## Filters

| jq | mq | Result |
| -- | -- | ------ |
| `.name` | `.["name"]` | `a` |
| `.items[0]` | `.["items"] \| .[0]` | `{"id": 1, "t": "x"}` |
| `.items \| first` | `.["items"] \| first()` | same |
| `.tags[-1]` | `.["tags"] \| .[-1]` | `q` |
| `.tags[:1]` | `.["tags"] \| slice(0, 1)` | `["p"]` |
| `.items \| map(.id)` | `.["items"] \| map(fn(x): x["id"];)` | `[1, 2]` |
| `map(select(.id > 1))` | `.["items"] \| filter(fn(x): x["id"] > 1;)` | `[{"id": 2, "t": "y"}]` |
| `.items \| length` | `.["items"] \| len()` | `2` |
| `keys` | `keys()` | `["name", "n", "items", "tags"]` |
| `[.[]]` of values | `values()` | `["a", 3, [...], [...]]` |
| `to_entries` | `entries()` | `[["name", "a"], ...]` (pairs, not objects) |
| `from_entries` | `from_entries([["a", 1]])` | `{"a": 1}` |
| `paths` | `paths` | `[["name"], ["n"], ...]` |
| `{n: .n}` | `{"n": .["n"]}` | keys are quoted |
| `.a + .b` on objects | `{"a": 1} + {"b": 2}` | `{"a": 1, "b": 2}` |
| `del(.name)` | `del(., "name")` | |
| `.name = "z"` | `set(., "name", "z")` | |
| `.missing // "none"` | `.["missing"] \|\| "none"` | `none` |
| `if .n > 2 then "big" else "small" end` | `if (.["n"] > 2): "big" else: "small"` | `big` |
| `try error("x") catch "c"` | `try: error("x") catch: "c"` | `c` |
| `select(.n > 1)` | `select(.["n"] > 1)` | the input, or nothing |
| `.. ` | `..` | every nested value, as one array |
| `type` | `type()` | `dict` |
| `. as $x \| $x.n` | `let x = . \| x["n"]` | |
| `def f(x): x + 1;` | `def f(x): x + 1;` | same form |
| `"x \(.n)"` | `s"x ${.["n"]}"` | `x 3` |
| `[.[] \| . * 2]` | `map(fn(x): x * 2;)` | on an array |
| `reduce .[] as $x (0; . + $x)` | `fold(0, fn(a, x): a + x;)` | |
| `limit(2; .[])` | `take(2)` | on an array |
| `.[1:]` | `skip(1)` | on an array |

## Arrays and aggregation

| jq | mq | Result on `[3, 1, 2]` |
| -- | -- | --------------------- |
| `sort` | `sort()` | `[1, 2, 3]` |
| `sort_by(.id)` | `sort_by(fn(x): x["id"];)` | |
| `reverse` | `reverse()` | `[2, 1, 3]` |
| `unique` | `uniq()` | |
| `add` | `sum()` | `6` |
| `min`, `max` | `min(a, b)`, `max(a, b)` | two values only, so use `sort() \| first()` or `last()` for an array |
| `flatten` | `flatten()` | |
| `last` | `last()` | `2` |
| `any(. == "p")` | `any(fn(x): x == "p";)` | |
| `all(. == "p")` | `all(fn(x): x == "p";)` | |
| `index("q")` | `index("q")` | `1` |
| `contains(["p"])` | `contains("p")` | `true` |
| `join(",")` | `join(",")` | |
| `transpose` | `transpose()` | |
| `group_by(.t)` | `group_by(fn(x): x["t"];)` | a dict keyed by the group value, not an array of arrays |
| `map(select(f))` | `filter(fn(x): f;)` | `map` with `select` leaves `None` holes |
| `range(0; 3)` | `range(0, 3)` | `[0, 1, 2, 3]`, inclusive and an array |

## Strings

| jq | mq |
| -- | -- |
| `ascii_downcase`, `ascii_upcase` | `downcase()`, `upcase()` |
| `length` | `len()` |
| `tostring` | `to_string()` |
| `tonumber` | `to_number()` |
| `split("-")` | `split("-")` |
| `startswith("a")`, `endswith("a")` | `starts_with("a")`, `ends_with("a")` |
| `ltrimstr("a")`, `rtrimstr("a")` | `ltrimstr("a")`, `rtrimstr("a")` |
| `test("a")` | `test("a")` |
| `sub("a"; "b")` | `replace("a", "b")` (`sub` is subtraction in mq) |
| `gsub("a"; "b")` | `gsub("a", "b")` |
| `capture("(?<x>b)")` | `capture("(?<x>b)")` |
| `@base64` | `base64()` |
| `explode` | `explode()` |
| `tojson` | `to_string()` on an array or dict |
| `fromjson` | `json_parse(s)` with `-M json` |

## No equivalent

These have no mq counterpart, or only an indirect one.

| jq | In mq |
| -- | ----- |
| `.[]?`, `?//` | none. Use `try: ... catch: ...` |
| `.a.b.c` path chaining | none. Pipe with `.["a"] \| .["b"]` |
| `limit(n; f)`, `first(f)`, `until`, `recurse(f)` | none. Use `take`, `first()`, `while` or `foreach` |
| `getpath`, `setpath` | none. Nest `get` and `set` |
| `to_entries` as `{key, value}` objects, `with_entries` | none. `entries()` gives pairs |
| `@csv`, `@tsv`, `@sh`, `@html` | `-F csv`, `-F shell`, `-F html` on output |
| `input`, `inputs` | none. Use `-A` or `--eval-all` |

Use `mq help <name>` for the exact signature of any function, and the [CLI reference](../reference/cli.md) for the full option list.
