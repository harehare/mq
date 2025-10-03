# try-catch

The try-catch expression allows you to handle errors gracefully by providing a fallback value when an expression fails.

## Syntax

```
try <expr> catch <expr>
```

## Behavior

- If the `try` expression succeeds, its result is returned
- If the `try` expression fails (produces an error), the `catch` expression is evaluated instead
- The `catch` expression receives the same input as the `try` expression

## Examples

### Basic Error Handling

```python
# When the expression succeeds
try: error("error") catch: "unknown"

# When the expression fails
try: get("missing") catch: "default"
```

### Chaining with Pipe

```python
# Try to parse as JSON, fallback to raw string
try: from_json() catch: self

# Complex fallback logic
try: do get("data") | from_json(); catch: []
```

### Nested Try-Catch

```python
# Multiple fallback levels
try: get("primary") catch: try: get("secondary") catch: "default"
```
