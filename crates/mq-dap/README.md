# mq-dap

Debug Adapter Protocol implementation for mq.

## Overview

`mq-dap` provides a Debug Adapter Protocol (DAP) server for the [mq](https://github.com/harehare/mq) query language, enabling debugging support in editors and IDEs that support DAP.

## Features

- **Standard DAP Implementation**: Compatible with any editor or IDE that supports the Debug Adapter Protocol
- **Breakpoint Support**: Set and manage breakpoints in mq queries
- **Step Debugging**: Step through query execution line by line
- **Variable Inspection**: Inspect variables and intermediate values during execution
- **Call Stack**: View the call stack during debugging sessions

## Usage

The DAP server is typically started by an editor or IDE that supports DAP. You can also start it manually for testing:

```bash
# Start the DAP server (typically done by your editor)
mq-dap
```

## Editor Integration

To use `mq-dap` with your editor, configure your DAP client to launch the `mq-dap` binary. The specific configuration depends on your editor or IDE.

## License

Licensed under the MIT License.
