# Find the closest heading to a misspelled name

Goal: Look up a section by a name that may be misspelled, such as `instalation`, and get the closest heading back.

Prerequisites: The built-in `fuzzy` module. To extract the matching section, the `section` module, which needs all nodes at once (see [Extract a section by its heading](extract-section-by-heading.md)).

## Query

The closest heading:

```bash
$ mq 'import "fuzzy" | nodes | filter(is_h) | map(to_text) | fuzzy::fuzzy_best_match("instalation")' guide.md
```

## Input (`guide.md`)

```markdown
# Guide

## Installation

Install with cargo.

## Configuration

Edit the config file.

## Troubleshooting

Common problems.
```

## Output

```
{"text": "Installation", "score": 0.914141}
```

## Extract the matching section

Feed the best match into `section::section` to get the section itself:

```bash
$ mq 'import "fuzzy" | import "section" | nodes | let best = fuzzy::fuzzy_best_match(map(filter(., is_h), to_text), "instalation") | section::section(get(best, "text"))' guide.md
```

```markdown
## Installation

Install with cargo.
```

## Notes

- `fuzzy_best_match` and `fuzzy_match` use Jaro-Winkler similarity, a score from 0 to 1 where 1 is an exact match. `fuzzy_match` returns every candidate sorted from best to worst, and `fuzzy_filter(candidates, query, threshold)` keeps only those at or above the threshold.
- `fuzzy_match_levenshtein` scores by edit distance instead, where lower is better and 0 is an exact match. `fuzzy::levenshtein("kitten", "sitting")` is `3`.
- `fuzzy_best_match` returns `None` for an empty candidate list, and it always returns something otherwise, so check `score` if a poor match should count as no match.
