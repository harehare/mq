# Split a document into chunks at a heading level

Goal: Break a long document into standalone chunks wherever a heading of a given level appears, useful for feeding sections to an LLM one at a time, or generating one output file per chunk downstream.

Prerequisites: The `section` module, via `-A` or `nodes`.

## Query

```bash
$ mq -A 'section::split(2)' README.md
```

## Input

```markdown
# Chapter 1

Intro.

## Section 1.1

Detail 1.1

## Section 1.2

Detail 1.2

# Chapter 2

Content.

## Section 2.1

Detail 2.1
```

## Output

```markdown
## Section 1.1

Detail 1.1

## Section 1.2

Detail 1.2

# Chapter 2

Content.

## Section 2.1

Detail 2.1
```

## Notes

- Gotcha: content before the *first* heading at the split level is dropped: here, `# Chapter 1` and `Intro.` disappear because they precede the first `##`. Once the first split point is found, every following heading (at or above that level) starts a new chunk normally, which is why `# Chapter 2` survives. If you need that leading content preserved, prepend it to the output yourself, or split at level 1 instead.
- `split(1)` on the same input is a no-op here, since every section is already anchored under a level-1 heading.
- `section::collect` (flattening section objects back to plain Markdown nodes) isn't needed for CLI output. mq expands section objects to Markdown automatically there. It's only required when consuming section objects from Rust or other embedding code; see the equivalent note in [Extract a section by its heading](extract-section-by-heading.md).
