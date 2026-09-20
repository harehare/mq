# Sort versions and find the latest release

Goal: Sort a list of version tags by SemVer precedence (not alphabetically), and pick the newest one, or the newest one within a range.

Prerequisites: The built-in `semver` module. Read a plain-text list with `-I raw` to get it as a single string.

## Query

Sort the tags:

```bash
$ mq -I raw -F json 'import "semver" | split("\n") | filter(fn(l): l != "";) | map(semver::semver_parse) | semver::semver_sort | map(semver::semver_to_string)' tags.txt
```

## Input (`tags.txt`)

```
v1.2.0
v1.10.1
v1.9.3
v2.0.0-rc.1
v2.0.0
v1.10.0
```

## Output

```json
[
  "1.2.0",
  "1.9.3",
  "1.10.0",
  "1.10.1",
  "2.0.0-rc.1",
  "2.0.0"
]
```

## Latest version

```bash
$ mq -I raw 'import "semver" | split("\n") | filter(fn(l): l != "";) | map(semver::semver_parse) | semver::semver_max | semver::semver_to_string' tags.txt
```

```
2.0.0
```

## Latest version in a range

`semver_max_satisfying` takes version strings and a range, and returns the highest match:

```bash
$ mq -I raw 'import "semver" | split("\n") | filter(fn(l): l != "";) | semver::semver_max_satisfying("~1.10.0")' tags.txt
```

```
1.10.1
```

## Notes

- `semver_parse` accepts a leading `v` and returns a dict with `major`, `minor`, `patch`, `pre` and `build`. `semver_to_string` turns it back into a string, without the `v`.
- `semver_sort`, `semver_max` and `semver_min` work on parsed dicts. `semver_satisfies(version, range)` and `semver_max_satisfying` work on strings.
- Pre-releases sort before their release, so `2.0.0-rc.1` comes before `2.0.0`.
- Bump a version: `semver::semver_parse("1.10.1") | semver::semver_bump_minor | semver::semver_to_string` gives `1.11.0`. `semver_bump_major` and `semver_bump_patch` work the same way.
- Compare two parsed versions with `semver_gt`, `semver_gte`, `semver_lt`, `semver_lte` and `semver_eq`.
