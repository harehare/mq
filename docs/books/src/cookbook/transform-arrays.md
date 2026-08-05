# Transform, filter, and reduce arrays

**Goal**: Apply the usual `map`/`filter`/`fold` trio to arrays inside an mq query — useful once a query has collected values into a list and you need to post-process them.

**Prerequisites**: None.

## Map: transform each element

```bash
$ mq -I null 'map([1, 2, 3, 4, 5], fn(x): x + 1;)'
```

```
[2, 3, 4, 5, 6]
```

## Filter: keep elements matching a condition

```bash
$ mq -I null 'filter([5, 15, 8, 20, 3], fn(x): x > 10;)'
```

```
[15, 20]
```

## Fold: combine elements into a single value

```bash
$ mq -I null 'fold([1, 2, 3, 4], 0, fn(acc, x): acc + x;)'
```

```
10
```

## Notes

- These compose naturally with Markdown selectors — e.g. `.h.depth | filter(fn(x): x <= 2;)` to keep only the depths of h1/h2 headings collected across a document (with `-A`).
