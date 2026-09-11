# Generators

`yield` and generator functions are a low-level building block, primarily intended for internal
use as groundwork for future lazy evaluation of `foreach`, selectors, and pipelines. The surface
API is intentionally minimal.

## Syntax

A function whose body directly contains `yield` (not inside a nested `def`/`fn`) becomes a
generator function. Calling it does not run the body; it returns a coroutine value instead.
`next(stream)` drives that coroutine forward one step, returning `{ value, done }`.

```
def function_name(parameters):
  yield: expr
  yield
```

`yield` alone yields `None`. `yield` is only valid inside a `def`/`fn` body; using it elsewhere
is a compile error.

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

`next()` past the last `yield` completes the coroutine: its `value` is `None` and `done` is
`true`. A generator function's final expression is not exposed as a completion value. Calling
`next()` again after completion keeps returning `{"value": None, "done": true}`.

As with other single-input functions, a coroutine can be supplied through the pipeline:

```mq
stream | next() # equivalent to next(stream)
```

## Sending a value back in

`send(stream, value)` resumes a coroutine like `next`, but the suspended `yield` expression
evaluates to `value` instead of `None`:

```mq
def g():
  let a = yield: 1
  | yield: a + 1;
| let stream = g()
| next(stream)          # {"value": 1, "done": false}
| send(stream, 10)      # {"value": 11, "done": false}
```

`send(value)` (or `stream | send(value)`) resumes the piped-in coroutine, mirroring `next()`.
Sending a value to a not-yet-started (`Created`) coroutine has nothing to resume into, so the
value is silently discarded, same as an ordinary `next()`.

## Introspection

- `is_coroutine(value)` reports whether `value` is a coroutine.
- `status(stream)` returns the coroutine's lifecycle state as a symbol: `:created`,
  `:suspended`, `:running`, `:completed`, or `:failed`.

## Closing early

`close(stream)` forces a coroutine straight to completion, releasing its suspended state without
waiting for `stream` itself to go out of scope. Useful when a coroutine wraps a resource (e.g. an
open file) that a script may stop iterating before reaching `done`. It returns `stream`:

```mq
def g(): yield: 1;
| let stream = g()
| next(stream)
| close(stream)
| next(stream) # {"value": None, "done": true}
```

Closing an already-`completed`/`failed` coroutine is a no-op; closing a `failed` one does not
hide its error (`next()` still re-raises it). Closing a `running` coroutine is a runtime error,
same as reentrant `next()`.

## Rules

- `next(stream)`/`send(stream, value)` (or piped as `stream | next()`/`stream | send(value)`)
  are plain calls, not keywords. A local `def next(...)`/`def send(...)` shadows them.
- Re-entering the same coroutine (calling `next()`/`send()`/`close()` on it while it is already
  running) is a runtime error.
- A nested `def`/`fn`'s `yield` only makes that nested function a generator; it does not affect
  the enclosing function.
- `foreach`, selectors, and pipelines do not yet consume streams lazily. That is future work.
