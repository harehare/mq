# Extract MDX components

Goal: Pull out every JSX-like component from an MDX document, e.g. to audit which components a doc site actually uses.

Prerequisites: Parse the file as MDX with `-I mdx` (plain `.md` parsing does not recognize JSX syntax).

## Query

```mq
select(is_mdx())
```

```bash
$ mq -I mdx 'select(is_mdx())' page.mdx
```

## Input

```markdown
Regular paragraph.

<CustomComponent prop="value" />

Another paragraph.

<AnotherComponent>
  Content
</AnotherComponent>
```

## Output

```markdown
<CustomComponent prop="value" />
<AnotherComponent>Content</AnotherComponent>
```

## Notes

- Files with an `.mdx` extension are parsed as MDX automatically; `-I mdx` is only needed when the content doesn't have that extension (e.g. piped from stdin).
- `.name` gives just the tag, e.g. `CustomComponent`. Combined with `unique_by`, this gives the distinct set of components a doc actually uses:

  ```bash
  $ mq -A -I mdx 'nodes | filter(fn(n): is_mdx(n);) | map(fn(n): n.name;) | unique_by(fn(x): x;)' page.mdx
  ```
