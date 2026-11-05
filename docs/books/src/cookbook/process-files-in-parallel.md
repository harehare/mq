# Control when large file sets run in parallel

Goal: Understand and tune mq's automatic parallel processing for batches of many files, so a big run doesn't stall on I/O one file at a time.

Prerequisites: None.

## Query

By default, mq automatically switches to parallel processing once a run covers more than 10 files, no flag needed:

```bash
$ mq '.h1' docs/**/*.md
```

Use `-P <n>` to change that threshold, e.g. force parallel mode even for a small batch while testing:

```bash
$ mq -P 1 '.h1' docs/**/*.md
```

Output stays in input order. Add `--unordered` to print each file's result as soon as it finishes:

```bash
$ mq -P 1 --unordered '.h1' docs/**/*.md
```

Or raise the threshold, to keep small-to-medium batches sequential (useful if the query has side effects):

```bash
$ mq -P 50 '.h1' docs/**/*.md
```

## Notes

- `-P` is a *threshold* (files to process before switching to parallel), not a worker count. It doesn't limit how many files run concurrently.
- Parallel output is held back until earlier files have printed, so results match a sequential run. `--unordered` skips that wait, and output order then varies between runs.
- Output a query writes itself (e.g. `stderr`) is not reordered.
