# Extract all URLs from links

**Goal**: Collect every link target in a document — handy for a broken-link checker or a quick inventory of external references.

**Prerequisites**: None.

## Query

```mq
.link.url
```

```bash
$ mq '.link.url' README.md
```

## Input

```markdown
Check out [mq](https://mqlang.org) and [GitHub](https://github.com).
```

## Output

```
https://mqlang.org
https://github.com
```

## Notes

- Want the link's visible text instead? Use `.link.value` in place of `.link.url`. (`.link.title` is the separate, optional `"title"` attribute from `[text](url "title")` syntax — usually empty.)
- Feed the URLs into a checker with `-F raw`, or pipe to `xargs curl -sfI` outside mq to validate them.
