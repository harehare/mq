# Write a reusable custom function

**Goal**: Package a multi-step transformation — like converting `snake_case` to `CamelCase` — into a named function you can call like a builtin.

**Prerequisites**: None.

## Query

```mq
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
      let first_char = upcase(first(word))
      | let rest_str = downcase(slice(word, 1, len(word)))
      | s"${first_char}${rest_str}";
  | join("")
end
| snake_to_camel("hello_world")
```

```bash
$ mq -I null 'def snake_to_camel(x): ... end | snake_to_camel("hello_world")'
```

## Output

```
HelloWorld
```

## Notes

- `def ... end` must appear before its first use in the pipeline.
- Reuse the same function across many invocations: put it in a `.mq` module file and load it with `-M path/to/module` (path without the `.mq` extension, function used unqualified), or put the file on a search path with `-L dir` and `import "module"` (used as `module::fn(...)`).
