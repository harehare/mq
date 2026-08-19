# Extract code blocks by language

Goal: Pull out only the code blocks written in a specific language, e.g. to review all the JavaScript snippets in a doc.

Prerequisites: None.

## Query

```mq
select(.code.lang == "js")
```

```bash
$ mq 'select(.code.lang == "js")' README.md
```

## Input

````markdown
```js
const x = 1;
```

```python
x = 1
```

```js
const y = 2;
```
````

## Output

Returns only the two JavaScript code blocks (`const x = 1;` and `const y = 2;`), dropping the Python one.

## Notes

- Shorthand: `.code("js")` selects the same nodes as `select(.code.lang == "js")`. The language selector doubles as a filter when called with an argument.
- To see which languages appear in a document at all, use `.code.lang` on its own. It prints one language per code block, in document order, with duplicates (`js`, `python`, `js`, ...), not a deduplicated list. Pipe through `-A ... | unique_by(fn(x): x;)` if you need the distinct set.
- To strip code blocks out and keep only prose, invert the condition: `select(!.code)`.
