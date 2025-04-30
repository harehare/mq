# Nodes

The `nodes` in mq allows you to access and manipulate all Markdown nodes as a single flat array.

## Basic Usage

The `nodes` filter returns an array of all nodes in a Markdown document:

```mq
nodes
```

## Examples

### Finding all headings

```mq
nodes | select(.h)
```

### Extracting all links

```mq
nodes | map(upcase)
```

### Counting nodes by type

```mq
nodes | len()
```
