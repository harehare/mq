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

## Rules

- `next(stream)` (or `stream | next()`) is a plain call, not a keyword. A local `def next(...)`
  shadows it.
- Re-entering the same coroutine (calling `next()` on it while it is already running) is a
  runtime error.
- A nested `def`/`fn`'s `yield` only makes that nested function a generator; it does not affect
  the enclosing function.
- `foreach`, selectors, and pipelines do not yet consume streams lazily. That is future work.
