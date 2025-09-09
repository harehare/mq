# Debugger

The `mq` debugger allows you to step through execution, set breakpoints, and inspect the state of your mq programs during runtime. This is particularly useful for debugging complex queries and understanding how data flows through your transformations.

## Getting Started

The debugger is available through the `mq-dbg` binary when the debugger feature is enabled.

```bash
# Enable debugging for an mq script
mq-dbg -f your-script.mq input.md
```

## Debugger Interface

Once the debugger starts, you'll see a prompt `(mqdbg)` where you can enter debugging commands. The debugger will automatically display the current source code location with line numbers, highlighting the current execution point.

```
   10| def process_headers() {
=> 11|   . | select(.type == "heading")
   12|     | map(.level)
   13| }
(mqdbg)
```

## Available Commands

The debugger supports the following commands:

### Navigation Commands

| Command    | Alias | Description                                                    |
| ---------- | ----- | -------------------------------------------------------------- |
| `step`     | `s`   | Step into the next expression, diving into function calls      |
| `next`     | `n`   | Step over the current expression, skipping over function calls |
| `finish`   | `f`   | Run until the current function returns                         |
| `continue` | `c`   | Continue normal execution until the next breakpoint            |

### Breakpoint Commands

| Command             | Alias      | Description                                   |
| ------------------- | ---------- | --------------------------------------------- |
| `breakpoint [line]` | `b [line]` | Set a breakpoint at the specified line number |
| `breakpoint`        | `b`        | List all active breakpoints                   |
| `clear [id]`        | `cl [id]`  | Clear a specific breakpoint by ID             |
| `clear`             | `cl`       | Clear all breakpoints                         |

### Inspection Commands

| Command     | Alias | Description                                         |
| ----------- | ----- | --------------------------------------------------- |
| `info`      | `i`   | Display current environment variables and context   |
| `list`      | `l`   | Show source code around the current execution point |
| `long-list` | `ll`  | Show the entire source code with line numbers       |
| `backtrace` | `bt`  | Print the current call stack                        |

### Control Commands

| Command | Alias | Description                               |
| ------- | ----- | ----------------------------------------- |
| `help`  | -     | Display help information for all commands |
| `quit`  | `q`   | Quit the debugger and exit                |

## Setting Breakpoints

You can set breakpoints in several ways:

### Interactive Breakpoints

You can set breakpoints interactively from the debugger prompt:

```
(mqdbg) breakpoint 15

(mqdbg) breakpoint
Breakpoints:
  [1] 15:10 (enabled)
```

### Programmatic Breakpoints

You can also set breakpoints directly in your mq code using the `breakpoint()` function:

```mq
def process_data(items) {
   breakpoint()  # Execution will pause here when debugger is attached
   | items | filter(fn(item): item == "test")
}
```

When the debugger encounters a `breakpoint()` function call during execution, it will automatically pause and enter interactive debugging mode.

**Note**: The `breakpoint()` function only has an effect when running under the debugger (`mq-dbg`). In normal execution (`mq`), it is ignored and has no impact on performance.
