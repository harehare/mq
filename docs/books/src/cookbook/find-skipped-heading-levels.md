# Find skipped heading levels with a generator

Goal: Report places where a heading jumps more than one level deeper than the previous heading (for example `##` followed by `####`), which breaks document outlines and accessibility tools.

Prerequisites: None. The query uses a generator function, a `def` whose body contains `yield`.

## Query

```bash
$ mq 'def pairs(xs):
  var i = 1
  | while (i < len(xs)):
      yield: [xs[i - 1], xs[i]]
      | i += 1
    end
end
| nodes
| filter(is_h)
| pairs()
| filter(fn(p): attr(p[1], "depth") - attr(p[0], "depth") > 1;)
| map(fn(p): s"${to_text(p[0])} (h${attr(p[0], "depth")}) -> ${to_text(p[1])} (h${attr(p[1], "depth")})";)
| collect()' guide.md
```

## Input (`guide.md`)

```markdown
# Guide

## Install

#### Details

## Usage

### Basic

##### Deep
```

## Output

```
Install (h2) -> Details (h4)
Basic (h3) -> Deep (h5)
```

## How it works

Calling `pairs(xs)` does not run its body. It returns a coroutine that yields one `[previous, current]` pair each time it is advanced. `filter` and `map` wrap it in further coroutines, and `collect()` pulls every value through the chain.

The same pattern works for any sliding-window check over nodes, such as two consecutive code blocks or a heading immediately followed by another heading.

## Notes

- To stop at the first problem instead of listing all of them, replace `| collect()` with `| first()`.
- A generator can also receive values with `send(stream, value)`. See [Generators](../reference/generators.md).
