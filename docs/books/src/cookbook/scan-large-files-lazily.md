# Scan a large file lazily and stop early

Goal: Search a large log or text file line by line without loading the whole file into memory, and stop reading as soon as you have what you need.

Prerequisites: The `--allow-read` flag, since file access is disabled by default. Pass `-I null` because the file is opened by the query itself, not given as input.

## Query

The first two error lines:

```bash
$ mq -I null --allow-read 'stream_lines("app.log") | filter(fn(line): contains(line, " ERROR ");) | take(2) | collect()'
```

## Input (`app.log`)

```
2026-09-01T10:00:01 INFO  server started
2026-09-01T10:00:05 ERROR db connection refused
2026-09-01T10:00:09 INFO  retrying
2026-09-01T10:00:12 ERROR db connection refused
2026-09-01T10:00:20 WARN  slow query
2026-09-01T10:00:31 ERROR disk full
```

## Output

```
["2026-09-01T10:00:05 ERROR db connection refused", "2026-09-01T10:00:12 ERROR db connection refused"]
```

`stream_lines` returns a coroutine, and `filter` returns another one. Nothing is read until `take(2)` pulls values through, so the lines after the second error are never read.

## Check whether a line exists

`any` stops at the first match:

```bash
$ mq -I null --allow-read 'stream_lines("app.log") | any(fn(line): contains(line, "disk full");)'
```

```
true
```

## Count matching lines

`fold` drives the stream to the end while keeping only the running count in memory:

```bash
$ mq -I null --allow-read 'stream_lines("app.log") | fold(0, fn(n, line): if (contains(line, " ERROR ")): n + 1 else: n;)'
```

```
3
```

## Notes

- `collect()` is required to turn a coroutine into an array. Without it, the output shows an opaque `coroutine` placeholder.
- `stream_chunks(path, size)` and `stream_bytes(path)` read binary data the same way, and `http_lines(:get, url)` streams an HTTP response body line by line (requires `--allow-net`).
- Stopping early with `take`, `first` or `any` does not close the file. Call `close(stream)` if the script keeps running afterwards.
- See [Generators](../reference/generators.md) for the full list of coroutine-aware functions.
