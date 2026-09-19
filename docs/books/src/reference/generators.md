# Generators

`yield` and generator functions are a low-level building block, primarily intended for internal use as groundwork for future lazy evaluation of `foreach`, selectors, and pipelines. The surface API is intentionally minimal.

## Syntax

A function whose body directly contains `yield` (not inside a nested `def`/`fn`) becomes a generator function. Calling it does not run the body; it returns a coroutine value instead. `next(stream)` drives that coroutine forward one step, returning `{ value, done }`.

```
def function_name(parameters):
  yield: expr
  yield
```

`yield` alone yields `None`. `yield` is only valid inside a `def`/`fn` body; using it elsewhere is a compile error.

## Examples

```mq
def range(n):
  var i = 0 |
  while (i < n):
    yield: i |
    i += 1
  end
end
| let stream = range(3)
| next(stream)
# Output: {"value": 0, "done": false}
```

Calling a generator function does not execute it:

```mq
def g():
  yield: 1;
| let stream = g()
# `stream` is a coroutine value; the body has not run yet
```

`next()` past the last `yield` completes the coroutine: its `value` is `None` and `done` is `true`. A generator function's final expression is not exposed as a completion value. Calling `next()` again after completion keeps returning `{"value": None, "done": true}`.

As with other single-input functions, a coroutine can be supplied through the pipeline:

```mq
stream | next() # equivalent to next(stream)
```

## Sending a value back in

`send(stream, value)` resumes a coroutine like `next`, but the suspended `yield` expression evaluates to `value` instead of `None`:

```mq
def g():
  let a = yield: 1
  | yield: a + 1;
| let stream = g()
| next(stream)          # {"value": 1, "done": false}
| send(stream, 10)      # {"value": 11, "done": false}
```

`send(value)` (or `stream | send(value)`) resumes the piped-in coroutine, mirroring `next()`. Sending a value to a not-yet-started (`Created`) coroutine has nothing to resume into, so the value is silently discarded, same as an ordinary `next()`.

Both helpers can be used as values when a higher-order workflow is useful:

```mq
def g(): yield: 1;
| let advance = next
| let stream = g()
| advance(stream)
```

## Introspection

- `is_coroutine(value)` reports whether `value` is a coroutine.
- `status(stream)` returns the coroutine's lifecycle state as a symbol: `:created`, `:suspended`, `:running`, `:completed`, or `:failed`.

## Creating a coroutine from a value

`to_coroutine(value)` (or `stream::from(value)`, see [Stream sources](#stream-sources)) wraps an array or dictionary in a coroutine that lazily yields its elements (a dictionary yields its `[key, value]` entry pairs, matching `entries()`). A coroutine input is returned unchanged, so `to_coroutine` is safe to use as an input boundary when a value may already be lazy:

```mq
to_coroutine([1, 2, 3]) | collect()
# Output: [1, 2, 3]
```

It composes with the coroutine-aware combinators below, so an eager array can be fed through them lazily without writing a generator function by hand:

```mq
to_coroutine([1, 2, 3, 4, 5]) | take(2) | collect()
# Output: [1, 2]
```

## Closing early

`close(stream)` forces a coroutine straight to completion, releasing its suspended state without waiting for `stream` itself to go out of scope. Useful when a coroutine wraps a resource (e.g. an open file) that a script may stop iterating before reaching `done`. It returns `stream`:

```mq
def g(): yield: 1;
| let stream = g()
| next(stream)
| close(stream)
| next(stream) # {"value": None, "done": true}
```

Closing an already-`completed`/`failed` coroutine is a no-op; closing a `failed` one does not hide its error (`next()` still re-raises it). Closing a `running` coroutine is a runtime error, same as reentrant `next()`.

## Stream sources

The `stream` module collects the functions that create coroutines. Combinators such as `map`, `filter`, `take` and `collect` stay global because they also accept arrays.

- `stream::from(value)` is the same as `to_coroutine(value)`.

### Reading files lazily

With the `file-io` feature and read permission (`--allow-read`), a file can be read incrementally instead of all at once. Without `file-io`, these functions fail with an undefined function error. They open the file immediately, so a missing file or a permission error is raised at the call, not at the first `next()`:

- `stream::lines(path)` yields each line without its `\n`/`\r\n` terminator.
- `stream::chunks(path, size)` yields `bytes` chunks of up to `size` bytes.
- `stream::bytes(path)` yields each byte as a number from 0 to 255.

```mq
import "stream"
| stream::lines("app.log")
| filter(contains("ERROR"))
| take(10)
| collect()
```

#### File handles and closing

The generators are built on a file handle. `open_file(path)` returns one, `read_line(handle)` and `read_bytes(handle, size)` read from it and return `None` at end of file, and `close(handle)` closes it (`status(handle)` is `:open` or `:closed`). Reading from a closed handle is a runtime error. `size` must be a positive integer.

```mq
let f = open_file("data.txt")
| let first = read_line(f)
| close(f)
```

The file is closed when a coroutine holding it finishes or is closed with `close(stream)`, and when the last reference to it is dropped. There is no `finally`, so this relies on the handle being released rather than on code in the generator body. Stopping early with `take`, `first`, `any` and similar does not close the source stream, since it may still be advanced afterwards. Call `close(stream)` when you are done with it.

## Rules

- `next(stream)`/`send(stream, value)` (or piped as `stream | next()`/`stream | send(value)`) are plain, first-class calls, not keywords: they can be stored or passed like other builtins. A local `def next(...)`/`def send(...)` shadows them.
- Re-entering the same coroutine (calling `next()`/`send()`/`close()` on it while it is already running) is a runtime error.
- A nested `def`/`fn`'s `yield` only makes that nested function a generator; it does not affect the enclosing function.
- `map`, `flat_map`, `filter`, `reject`, `compact_map`, `skip`, `skip_while`, `take`, and `take_while` accept coroutines in addition to their existing eager collection inputs and return a coroutine. They evaluate upstream values only when the returned coroutine is advanced.
- `first`, `last`, `find_index`, `any`, `all`, `fold`, and `each` consume coroutine inputs. `any`, `all`, and `find_index` stop advancing the upstream coroutine as soon as their result is known. `each` and `fold` drive the coroutine to completion. Since `sum` and `sum_by` use `fold`, they also accept coroutine inputs.
- Use `collect()` to consume a coroutine completely into an array. `mq-run` applies `collect()` automatically when a query's final output value is a coroutine.
- `foreach`, selectors, and pipelines do not yet consume streams lazily. That is future work.
