<h1 align="center">mq-lint</h1>

Static analysis linter for the mq language.

`mq-lint` analyses mq programs by walking the HIR (High-level Intermediate Representation) and reporting diagnostics across five categories: correctness, style, complexity, selector, and module.

## Usage

### As a Library

```rust
use mq_lint::{Linter, LintContext, LintConfig};
use mq_hir::Hir;

// Build the HIR from mq source code
let mut hir = Hir::default();
let (source_id, _) = hir.add_code(None, "let x = .h1;");

// Configure and run the linter
let config = LintConfig::default();
let ctx = LintContext::new(&hir, source_id, &config);
let linter = Linter::with_default_rules();
let diagnostics = linter.run(&ctx);

for d in diagnostics {
    eprintln!("[{}] {} — {}", d.severity, d.rule_id(), d.message());
    if let Some(help) = d.help() {
        eprintln!("  help: {}", help);
    }
}
```

Each diagnostic's `rule_id()` and `message()` are derived from `d.kind`, a [`LintMessage`](src/message.rs) enum with one variant per rule. Rule identity (`RuleId`) and message text are both enums rather than free-form strings, so adding or renaming a rule is a compile-time-checked change in one place (`src/message.rs`).

### As a CLI

Build with the `cli` feature to get the `mq-lint` binary (also invocable as `mq lint` if placed on your `PATH`, see [External Subcommands](https://mqlang.org/book/start/external_subcommands.html)):

```bash
cargo install --path crates/mq-lint --features cli
```

```bash
mq-lint script.mq
mq-lint script.mq another.mq          # lint multiple files
echo "let x = .h1" | mq-lint           # read from stdin
mq-lint --disable naming_convention script.mq
mq-lint --min-severity warn script.mq # only show warn/error diagnostics
mq-lint --list-rules                  # print all rule IDs and their severity
```

Exits with a non-zero status if any diagnostic (at or above `--min-severity`) was reported.

### Disabling Rules

```rust
use mq_lint::RuleId;

let mut config = LintConfig::default();
config.disable_rule(RuleId::NamingConvention);
config.disable_rule(RuleId::ShadowVariable);
```

### Adjusting Complexity Thresholds

```rust
let mut config = LintConfig::default();
config.complexity.function_max_lines = 80;
config.complexity.max_params = 6;
config.complexity.max_nesting_depth = 5;
config.complexity.max_match_arms = 20;
config.complexity.max_interpolation_exprs = 4;
```

## Rules

### Correctness

| Rule ID                 | Severity | Description                                                                    |
| ----------------------- | -------- | ------------------------------------------------------------------------------ |
| `unused_variable`       | warn     | `let`/`var` variable declared but never referenced                             |
| `unused_function`       | warn     | `def` function defined but never called                                        |
| `unused_import`         | warn     | `import` module declared but never accessed                                    |
| `unreachable_code`      | error    | Code following `break`/`continue` that can never execute                       |
| `infinite_loop`         | warn     | `loop` body without a `break`                                                  |
| `duplicate_match_arm`   | error    | Same pattern appears more than once in a `match`                               |
| `shadow_variable`       | warn     | Variable re-declared in an inner scope with the same name as an outer variable |
| `missing_else_in_expr`  | warn     | `if` expression with no `else` branch (evaluates to `none` on false)           |
| `always_true_condition` | warn     | `if` condition is a literal `true` or `false`                                  |

**Example — `unused_variable`**

```mq
let x = .h1;  # warn: x is never used
.text
```

**Example — `missing_else_in_expr`**

```mq
if (.h1): "heading"  # warn: no else branch — produces `none` when condition is false
```

**Example — `always_true_condition`**

```mq
if (true): 1 else: 2;  # warn: condition is always `true`
```

### Style / Best Practices

| Rule ID                     | Severity | Description                                                          |
| --------------------------- | -------- | -------------------------------------------------------------------- |
| `prefer_let_over_var`       | warn     | `var` variable never reassigned — prefer `let`                       |
| `naming_convention`         | style    | Function or variable name is not `snake_case`                        |
| `boolean_comparison`        | style    | `x == true` → `x`, `x == false` → `not(x)`                           |
| `redundant_boolean_literal` | style    | `if (cond): true else: false` simplifies to `cond`                   |
| `prefer_specific_heading`   | style    | `.h` without a level — prefer `.h1`–`.h6`                            |
| `prefer_coalesce`           | style    | `if (x == none): fallback else: x` simplifies to `x ?? fallback`     |
| `prefer_pipe_style`         | style    | Nested unary call `f(g(x))` reads better as a pipe `x \| g() \| f()` |
| `redundant_try`             | style    | `try: ... catch: none` is exactly what the `?` operator does         |

**Example — `prefer_let_over_var`**

```mq
# Before (warn): x is never reassigned
var x = .h1
| x | to_text

# After
let x = .h1
| x | to_text
```

**Example — `boolean_comparison`**

```mq
if (.checked == true): "yes"   # style: simplify to `if (.checked): "yes"`
```

**Example — `redundant_boolean_literal`**

```mq
if (.h1): true else: false;  # style: simplify to `.h1`
```

**Example — `prefer_coalesce`**

```mq
if (.value == none): "default" else: .value  # style: simplify to `.value ?? "default"`
```

**Example — `prefer_pipe_style`**

```mq
to_text(to_upper(x))  # style: rewrite as `x | to_upper() | to_text()`
```

**Example — `redundant_try`**

```mq
try: get("x") catch: none  # style: rewrite as `get("x")?`
```

### Complexity

| Rule ID                 | Default Threshold | Description                                             |
| ----------------------- | ----------------- | ------------------------------------------------------- |
| `function_too_long`     | 50 lines          | `def` body exceeds the line limit                       |
| `too_many_params`       | 5 params          | Function has too many parameters                        |
| `deeply_nested`         | depth 4           | `if`/`match`/`loop`/`foreach` nesting exceeds the limit |
| `too_many_match_arms`   | 15 arms           | `match` expression has too many arms                    |
| `complex_interpolation` | 3 expressions     | Interpolated string has too many `${...}` parts         |

**Example — `complex_interpolation`**

```mq
# warn: 4 interpolated expressions exceeds the default limit of 3
s"${.h1} ${.h2} ${.h3} ${.h4}"
```

### Selector

| Rule ID                   | Severity | Description                                                                        |
| ------------------------- | -------- | ---------------------------------------------------------------------------------- |
| `inefficient_selector`    | perf     | `..` followed by a specific selector — use the specific selector directly          |
| `missing_depth_guard`     | warn     | `..` (recursive selector) used without any `.depth`/`.level` guard                 |
| `prefer_specific_heading` | style    | `.h` selector without a level — prefer `.h1`–`.h6`                                 |
| `selector_always_empty`   | warn     | Adjacent selectors that can never both match (e.g. `.h1 \| .h2`, `.todo \| .done`) |

**Example — `inefficient_selector`**

```mq
# Before (perf): .. traverses the whole document then filters to h1
.. | .h1

# After
.h1
```

**Example — `missing_depth_guard`**

```mq
..  # warn: no depth guard — may traverse the entire document
```

**Example — `selector_always_empty`**

```mq
.h1 | .h2  # warn: a heading already filtered to level 1 can never also be level 2
```

### Module

| Rule ID                      | Severity | Description                                                   |
| ---------------------------- | -------- | ------------------------------------------------------------- |
| `missing_module_doc`         | style    | `module` declaration has no documentation comment             |
| `ambiguous_qualified_access` | warn     | Same function name is defined in more than one `module` block |

**Example — `missing_module_doc`**

```mq
module a: def foo(): 1; end  # style: module `a` has no documentation comment
```

**Example — `ambiguous_qualified_access`**

```mq
# warn: `foo` is defined in both `a` and `b` — qualify the call to avoid ambiguity
module a: def foo(): 1; end
| module b: def foo(): 2; end
```

## Support

- 🐛 [Report bugs](https://github.com/harehare/mq/issues)
- 💡 [Request features](https://github.com/harehare/mq/issues)
- 📖 [Read the documentation](https://mqlang.org/book/)

## License

Licensed under the MIT License.
