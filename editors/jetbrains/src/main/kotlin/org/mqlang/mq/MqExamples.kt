package org.mqlang.mq

/** Example queries inserted into new `.mq` files, mirrored from the mq VS Code extension. */
object MqExamples {
    const val TEXT: String = """# To hide these examples, disable "Show examples in new file" in mq settings
# Extract js code
select(.code.lang == "js")

# Extract list
.[]

# Extract table
.[][]

# Extract MDX
select(is_mdx())

# Custom function
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
    let first_char = upcase(first(word))
    | let rest_str = downcase(slice(word, 1, len(word)))
    | s"${'$'}{first_char}${'$'}{rest_str}";
  | join("");
| snake_to_camel()

# Markdown Toc
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, to_number(level))

# CSV parse
include "csv" | csv_parse("a,b,c
1,2,3
4,5,6", true) | csv_to_markdown_table()

# Extract Front Matter
import "yaml" | if (.yaml): yaml::yaml_parse() | get(:title)

# Sort by property
[{"name": "Bob", "age": 30}, {"name": "Alice", "age": 25}]
| sort_by(fn(x): get(x, "age");)

# Group by predicate
[1, 2, 3, 4, 5, 6, 7, 8]
| group_by(fn(x): if (x % 2 == 0): "even" else: "odd";)

# Sum with fold
fold([1, 2, 3, 4, 5], 0, fn(acc, x): acc + x;)

# Pick specific dict fields
{"name": "mq", "version": "0.7.0", "internal_id": 42}
| pick(["name", "version"])

# Parse relative dates
date_relative(now(), "3 days ago") | strftime("%Y-%m-%d")

# Compare semantic versions
include "semver" | semver_gt(semver_parse("2.1.0"), semver_parse("2.0.5"))

# Stringify to TOML
include "toml" | {"title": "mq", "version": "0.7.0"} | toml_stringify()

# Uppercase all headings (run with the -U/--update flag to apply in place)
.h | upcase()
"""
}
