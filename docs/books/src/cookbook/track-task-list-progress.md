# Track task-list (checkbox) progress

**Goal**: Count how many `- [x]` items are checked off out of the total in a Markdown task list — e.g. to report progress on a TODO list or project checklist.

**Prerequisites**: `-A`, since the counts are computed across the whole document at once.

## Query

List only the completed items:

```bash
$ mq 'select(.list.checked == true)' TODO.md
```

Count done vs. total:

```bash
$ mq -A 'let total = count_by(fn(x): x | select(.list);)
| let done = count_by(fn(x): x | select(.list.checked == true);)
| s"${done}/${total} done"' TODO.md
```

## Input

```markdown
# TODO

- [x] Write docs
- [ ] Add tests
- [x] Fix bug
- [ ] Ship release
```

## Output

```
2/4 done
```

## Notes

- `.list.checked` is `true`/`false` for task-list items and absent (`None`) for a plain bullet or numbered item — so `select(.list)` alone counts every list item regardless of whether it's a checkbox.
- Swap `== true` for `== false` to list what's left to do instead of what's done.
