# Keep only sections at a given heading level

Goal: Drop deeper subsections and keep just the top-level (or any one level) sections of a document.

Prerequisites: The `section` module. Section functions need all document nodes at once, so pass `-A` on the command line, or `nodes` in a script.

## Query

```bash
$ mq -A 'section::sections | section::by_level(1)' README.md
```

`by_level` also accepts a range:

```bash
$ mq -A 'section::sections | section::by_level(1..2)' README.md
```

## Input

```markdown
# Chapter 1

Intro.

## Section 1.1

Detail.

# Chapter 2

Content.
```

## Output (`by_level(1)`)

```markdown
# Chapter 1

Intro.

# Chapter 2

Content.
```

## Notes

- `by_level(2)` on the same input would return `## Section 1.1` only, since `Chapter 1` and `Chapter 2` are level 1.
