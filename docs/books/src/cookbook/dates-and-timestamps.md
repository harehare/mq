# Work with dates and timestamps

Goal: Parse a date from front matter, format it for display, and keep only documents from the last 7 days.

Prerequisites: None. Timestamps are Unix seconds, and `from_date` accepts RFC 3339 strings such as `2026-11-10T09:00:00Z`.

## Format a front matter date

```bash
$ mq '.yaml | frontmatter | if (is_string(get("date"))): strftime(from_date(get("date")), "%b %d, %Y") else: None' a.md b.md
```

## Input (`a.md`, `b.md`)

```markdown
---
title: Release notes
date: 2026-11-10T09:00:00Z
---

# Notes
```

```markdown
---
title: Old plan
date: 2026-10-01T09:00:00Z
---

# Plan
```

## Output

```text
Nov 10, 2026
Oct 01, 2026
```

`.yaml` yields `None` for the other nodes, so the `is_string` check skips them. Without it `from_date` fails on `None`.

## Keep documents from the last 7 days

`date_relative(base, expr)` resolves an expression such as `"7 days ago"` against a base timestamp. Use `now()` as the base:

```bash
$ mq '.yaml | frontmatter | if (is_string(get("date"))): do let t = from_date(get("date")) | if (t >= date_relative(now(), "7 days ago")): get("title") else: None end else: None' a.md b.md
```

```text
Release notes
```

Pass a fixed timestamp instead of `now()` for a reproducible result:

```bash
$ mq -I null 'strftime(date_relative(from_date("2026-11-15T00:00:00Z"), "7 days ago"), "%Y-%m-%d")'
2026-11-08
```

## Notes

- Expressions: `now`, `today`, `yesterday`, `tomorrow`, `<n> <unit> ago`, `in <n> <unit>`, `next <weekday>`, `last <weekday>`. Weekdays use full English names.
- `strftime` uses chrono format specifiers (`%Y-%m-%d`, `%H:%M`, `%A`, ...) and formats in UTC.
- `from_date` raises an error on strings that are not RFC 3339, so a date written as `2026-11-10` needs a time part appended first.
