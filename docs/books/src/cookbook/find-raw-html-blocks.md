# Find raw HTML blocks

**Goal**: Locate embedded raw HTML in a Markdown document — useful before converting to a stricter Markdown flavor (e.g. plain CommonMark) or a renderer that doesn't allow raw HTML passthrough.

**Prerequisites**: None.

## Query

```mq
.html
```

```bash
$ mq '.html' README.md
```

## Input

```markdown
# Doc

Some text.

<div class="callout">
  <strong>Note:</strong> important info.
</div>

More text.
```

## Output

```markdown
<div class="callout">
  <strong>Note:</strong> important info.
</div>
```

## Notes

- Run across a whole docs tree (`mq '.html' docs/**/*.md`) as a quick audit of how much raw HTML you'd need to rewrite before switching renderers.
- **Gotcha**: inline HTML tags inside a paragraph (e.g. `text <span>inline</span> text`) are matched too, but the opening and closing tags come back as two *separate* `.html` nodes (`<span class="x">` and `</span>`) — and the text between them is dropped entirely, since that's a plain text node this selector doesn't touch. `.html` is reliable for auditing standalone HTML blocks; for inline HTML mixed into prose, treat the count as a rough signal rather than a clean extraction.
