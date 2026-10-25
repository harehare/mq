# Evaluation Model

This page describes how a query runs against its input and how `None` moves through it. The examples use this input, saved as `doc.md`:

```markdown
# Title

para one

## Sub

- a
- b
```

## Overview

```
 input            split into          query runs          None is          printed
 (doc.md)         top-level nodes     once per node       dropped          in input order

                  ┌─ # Title ──────▶  query ──▶ "# TITLE" ──────────────▶ # TITLE
                  ├─ para one ─────▶  query ──▶ None ──────╳
 Markdown text ──▶├─ ## Sub ───────▶  query ──▶ "## SUB" ───────────────▶ ## SUB
                  ├─ a ────────────▶  query ──▶ None ──────╳
                  └─ b ────────────▶  query ──▶ None ──────╳

                  (the query above is `.h | upcase()`)
```

Each node is processed on its own. The runs do not share a result, and the order of the output follows the order of the input.

## One run per top-level node

The input is split into top-level nodes and the query runs once for each of them. A list is split into its items, so the input above has five nodes: the two headings, the paragraph, and the items `a` and `b`.

```sh
mq 'to_text() | len()' doc.md
```

```
5
8
3
1
1
```

| Node | `self` | `len()` of its text |
| ---- | ------ | ------------------- |
| 1 | `# Title` | 5 |
| 2 | `para one` | 8 |
| 3 | `## Sub` | 3 |
| 4 | `a` | 1 |
| 5 | `b` | 1 |

Each run sees only its own node as `self`.

## `None` produces no output

A run that ends with `None` prints nothing. This is what makes selectors and `select` filter:

```sh
mq '.h' doc.md
```

```markdown
# Title
## Sub
```

`.h` returns `None` for the nodes that are not headings, and those runs are dropped.

```
 node         .h            printed?
 ──────────   ───────────   ────────
 # Title      # Title       yes
 para one     None          no
 ## Sub       ## Sub        yes
 a            None          no
 b            None          no
```

These forms all end in `None` for a non-matching node, so they drop it:

| Query | Non-matching node |
| ----- | ----------------- |
| `.h \| upcase()` | `.h` is `None`, and `upcase()` of `None` is `None` |
| `select(.h) \| upcase()` | `select` returns `None` |
| `if (is_h()): upcase()` | the missing `else` branch evaluates to `None` |

Adding an `else` keeps the node:

```sh
mq 'if (is_h()): upcase() else: self' doc.md
```

```markdown
# TITLE

para one

## SUB

- a
- b
```

## `None` does not stop a pipe

`None` is an ordinary value. A pipe keeps running after it, and a function that receives `None` decides what to do with it:

```
 1 ──▶ None ──▶ 2
       │         │
       │         └─ still runs: the result is 2
       └─ the pipe does not stop here
```

```sh
mq '1 | None | 2' doc.md
```

The query prints `2` five times, once per node. If you want a pipe to stop on a non-match, use `select` or an `if`, so the whole run ends in `None`.

## Updating in place

With `-U` / `--update`, a run that ends with `None` keeps the original node instead of removing it, so only the nodes you touch change.

```
              without -U                     with -U
              ──────────                     ───────
 # Title  ──▶ # TITLE                        # TITLE
 para one ──▶ (dropped)                      para one
 ## Sub   ──▶ ## SUB                         ## SUB
 a        ──▶ (dropped)                      - a
 b        ──▶ (dropped)                      - b
```

```sh
mq -U '.h | upcase()' doc.md
```

```markdown
# TITLE

para one

## SUB

- a
- b
```

`select(.h) | upcase()` and `if (is_h()): upcase()` give the same result with `-U`.

## Running once over all nodes

Two things run the query once over the whole input instead of once per node:

- `-A` / `--aggregate` collects the nodes into a single array before the query runs.
- `nodes` collects the per-node results of everything before it into an array, and the rest of the query runs once against that array.

```
 per node (default)                    -A / nodes

 n1 ──▶ query ──▶ r1                   n1 ┐
 n2 ──▶ query ──▶ r2                   n2 ├──▶ [n1..n5] ──▶ query ──▶ r
 ...                                   ...┘
 n5 ──▶ query ──▶ r5                   n5 ┘
```

```sh
mq -A 'len()' doc.md
```

```
5
```

## Null input

With `-I null` there is no input to split. The query runs once and `self` is `None`:

```sh
mq -I null '1 + 1'
```

```
2
```

## Exit status

With `-e` / `--exit-status` on Markdown input, a run where every result is `None` or a non-matching `select` exits with `4`. See [Input and Output Behavior](input-and-output.md#exit-codes) for the full table.
