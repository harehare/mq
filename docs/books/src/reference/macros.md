# Macros

Macros enable compile-time code generation and transformation in mq. They allow you to define reusable code templates that are expanded before evaluation.

## Syntax

```
macro name(parameters): body;
```

Macros are invoked like functions:

```
name(arguments)
```

## How Macros Work

Macros differ from functions:

- **Compile-time expansion**: Macros are expanded before the program executes
- **Code substitution**: Macro parameters are directly substituted into the macro body
- **No runtime overhead**: Macro definitions are removed from the final program

## Basic Examples

```elixir
# Simple value transformation
macro double(x) do
  x + x
end

| double(5)  # Returns 10

# Multiple parameters
macro add_three(a, b, c) do
  a + b + c
end

| add_three(1, 2, 3)  # Returns 6

# With control flow
macro max(a, b) do
  if(a > b): a else: b
end

| max(10, 5)  # Returns 10
```

## Advanced Examples

```python
# Nested macro calls
macro double(x): x + x
macro quadruple(x): double(double(x))

| quadruple(3)  # Returns 12

# Accepting functions as parameters
macro apply_twice(f, x) do
  f(f(x))
end

def inc(n): n + 1;
| apply_twice(inc, 5)  # Returns 7
```

## Quote and Unquote

`quote` and `unquote` provide advanced metaprogramming capabilities:

- **`quote(expr)`**: Delays evaluation, treating content as code to be generated
- **`unquote(expr)`**: Evaluates the expression immediately and injects the result

### Practical Examples

```python
# Basic injection
macro make_expr(x) do
  quote: unquote(x) + 1
end

| make_expr(5)  # Returns 6

# Pre-computation
macro compute(a, b) do
  quote: unquote(a) + unquote(b) * 2
end

| compute(10, 5)  # Returns 20

# Conditional code generation
macro conditional_expr(x) do
  quote do
    if(unquote(x) > 10):
        "large"
    else:
        "small"
  end
end

| conditional_expr(15)  # Returns "large"

# Complex pre-computation
macro compute_mixed(x) do
  let a = x * 2 |
  let b = x + 10 |
  quote: unquote(a) + unquote(b)
end

| compute_mixed(5)  # a=10, b=15, returns 25

# Generating data structures
macro make_array(a, b, c) do
  quote: [unquote(a), unquote(b), unquote(c)]
end

| make_array(1, 2, 3)  # Returns [1, 2, 3]
```
