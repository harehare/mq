# Modules and Imports

mq provides several ways to organize and reuse code: `module`, `import`, and `include`.

## Module

Defines a module to group related functions and prevent naming conflicts using the syntax `module name: ... end`.

```js
module module_name:
  def function1(): ...
  def function2(): ...
end
```

Functions within a module can be accessed using qualified access syntax:

```js
module_name::function1()
```

### Examples

```python
# Define a math module
module math:
  def add(a, b): a + b;
  def sub(a, b): a - b;
  def mul(a, b): a * b;
end

# Use functions from the module
| math::add(5, 3)  # Returns 8
| math::mul(4, 2)  # Returns 8
```

## Import

Loads a module from an external file using the syntax `import "module_path"`.
The imported module is available with its defined name and can be accessed using qualified access syntax.

The import directive searches for .mq files in the following locations:

- `$HOME/.mq` - User's home directory mq folder
- `$ORIGIN/../lib/mq` - Library directory relative to the source file
- `$ORIGIN/../lib` - Parent lib directory relative to the source file
- `$ORIGIN` - Current directory relative to the source file

```js
import "module_name"
```

### Examples

**math.mq:**
```python
def add(a, b): a + b;
def sub(a, b): a - b;
```

**main.mq:**
```python
# Import the math module
import "math"

# Use functions with qualified access
| math::add(10, 5)  # Returns 15
| math::sub(10, 5)  # Returns 5
```

## Include

Loads functions from an external file directly into the current namespace using the syntax `include "module_name"`.
Unlike `import`, functions are available without a namespace prefix.

The include directive searches for .mq files in the same locations as `import`.

```js
include "module_name"
```

### Examples

**math.mq:**
```python
def add(a, b): a + b;
def sub(a, b): a - b;
```

**main.mq:**
```python
# Include math functions
include "math"

# Functions are available directly
| add(2, 3)  # Returns 5
| sub(10, 4) # Returns 6
```

## Comparison

| Feature  | `module`                          | `import`                          | `include`               |
| -------- | --------------------------------- | --------------------------------- | ----------------------- |
| Purpose  | Define a module                   | Load external module              | Load external functions |
| Access   | Qualified access (`module::func`) | Qualified access (`module::func`) | Direct access (`func`)  |
| Use case | Organize code within a file       | Reuse modules across files        | Simple function sharing |
