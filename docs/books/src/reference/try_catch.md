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
try: "value" catch: "unknown"

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

## Error Suppression (`?`)

The error suppression operator `?` provides a concise way to handle errors by returning `None` when an expression fails, instead of raising an error. This is equivalent to using a regular `try-catch` with a default fallback.

### Examples

```python
# Equivalent to a regular try-catch with a default value
get("missing")?
```

In this example, if `get("missing")` fails, the result will be `None` rather than an error.
