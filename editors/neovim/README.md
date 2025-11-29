<h1 align="center">mq.nvim</h1>

Neovim plugin for [mq](https://mqlang.org/) - a jq-like tool for Markdown processing.

## Features

- 🎨 **Syntax Highlighting** - Full syntax support for `.mq` files
- 🔧 **LSP Integration** - Code completion, diagnostics, and more via Language Server Protocol
- 🐛 **DAP Integration** - Debug your mq queries with nvim-dap
- ⚡  **Commands** - Execute mq queries directly from Neovim
- 📝 **File Templates** - Create new mq files with helpful examples

## Requirements

- Neovim >= 0.8.0
- [mq](https://github.com/harehare/mq) installed (for LSP server and command execution)
  - Install using `:MqInstall` command or manually via cargo
- Rust toolchain (required for `:MqInstall` command)

### Optional Dependencies

**For Debugging:**
- [nvim-dap](https://github.com/mfussenegger/nvim-dap) - Required for debugging support
- [nvim-dap-ui](https://github.com/rcarriga/nvim-dap-ui) - Recommended for better debugging UI
- [nvim-dap-virtual-text](https://github.com/theHamsta/nvim-dap-virtual-text) - Shows variable values inline
- `mq-dbg` - Installed automatically via `:MqInstall` command

**For Enhanced File Searching:**

Improves performance and user experience for `:MqExecuteFile` and `:MqRunSelected` commands:
- [fd](https://github.com/sharkdp/fd) - Fastest option (highly recommended)
- [ripgrep](https://github.com/BurntSushi/ripgrep) - Fast alternative
- [telescope.nvim](https://github.com/nvim-telescope/telescope.nvim) - Enhanced file picker UI

The plugin automatically uses these tools if available, falling back to native Neovim functions otherwise.

**For Code Completion:**
- [nvim-cmp](https://github.com/hrsh7th/nvim-cmp) - Recommended for the best completion experience
- [cmp-nvim-lsp](https://github.com/hrsh7th/cmp-nvim-lsp) - LSP source for nvim-cmp
- [LuaSnip](https://github.com/L3MON4D3/LuaSnip) - For snippet support in function completions

## Installation

### Using [lazy.nvim](https://github.com/folke/lazy.nvim)

```lua
{
  "harehare/mq",
  dir = "path/to/mq/editors/neovim", -- or use git url when published
  config = function()
    require("mq").setup()
  end,
}
```

### Using [packer.nvim](https://github.com/wbthomason/packer.nvim)

```lua
use {
  "harehare/mq",
  config = function()
    require("mq").setup()
  end,
}
```

### Using [vim-plug](https://github.com/junegunn/vim-plug)

```vim
Plug 'harehare/mq', { 'rtp': 'editors/neovim' }
```

Then in your `init.lua`:

```lua
require("mq").setup()
```

## Configuration

Default configuration:

```lua
require("mq").setup({
  -- Path to mq executable (if not in PATH)
  cmd = "mq",

  -- LSP server arguments
  lsp_args = { "lsp" },

  -- Show examples when creating new file
  show_examples = true,

  -- Automatically start LSP server
  auto_start_lsp = true,

  -- LSP server configuration
  lsp = {
    -- Custom on_attach function
    on_attach = function(client, bufnr)
      -- Your custom on_attach logic
    end,

    -- Custom capabilities
    capabilities = nil,

    -- LSP settings
    settings = {},
  },
})
```

## Commands

| Command           | Description                                 |
| ----------------- | ------------------------------------------- |
| `:MqInstall`      | Install mq LSP server via cargo             |
| `:MqStartLSP`     | Start the mq LSP server                     |
| `:MqStopLSP`      | Stop the mq LSP server                      |
| `:MqRestartLSP`   | Restart the mq LSP server                   |
| `:MqRunSelected`  | Run selected text as mq query (visual mode) |
| `:MqExecuteQuery` | Execute mq query on current file            |
| `:MqExecuteFile`  | Execute mq file on current file             |
| `:MqDebugFile`    | Debug current mq file (requires nvim-dap)   |

## Usage Examples

### Run selected text as query

1. Select text in visual mode
2. Run `:MqRunSelected`
3. Select an input file (.md, .mdx, .html, etc.)
4. View results in a new buffer

### Execute a query on current file

```vim
:MqExecuteQuery
```

Enter your mq query when prompted, and the result will be shown in a new buffer.

### Debug mq file

1. Open an mq file
2. Run `:MqDebugFile`
3. Select an input file when prompted
4. Use nvim-dap commands to control debugging (continue, step over, step into, etc.)

This requires [nvim-dap](https://github.com/mfussenegger/nvim-dap) to be installed.

## Key Mappings (Example)

You can add custom key mappings in your config:

```lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = "mq",
  callback = function()
    local opts = { buffer = true, noremap = true, silent = true }

    -- Run selected text
    vim.keymap.set("v", "<leader>mr", ":MqRunSelected<CR>", opts)

    -- Execute query
    vim.keymap.set("n", "<leader>mq", ":MqExecuteQuery<CR>", opts)

    -- Execute file
    vim.keymap.set("n", "<leader>mf", ":MqExecuteFile<CR>", opts)

    -- Debug file
    vim.keymap.set("n", "<leader>md", ":MqDebugFile<CR>", opts)

    -- LSP commands
    vim.keymap.set("n", "<leader>ms", ":MqStartLSP<CR>", opts)
    vim.keymap.set("n", "<leader>mS", ":MqStopLSP<CR>", opts)
  end,
})
```

## LSP Features

When the LSP server is running, you get:

- 📝 **Code Completion** - Auto-complete for mq functions and keywords
  - Function names with parameter snippets
  - Variables and parameters in scope
  - Builtin selectors and functions
  - Module-qualified completions (e.g., `module::function`)
- 🔍 **Diagnostics** - Real-time error checking
- 📖 **Hover Documentation** - View function documentation
- 🎯 **Go to Definition** - Jump to function definitions
- 🔧 **Code Actions** - Quick fixes and refactoring

Use Neovim's built-in LSP commands:

- `vim.lsp.buf.hover()` - Show hover information
- `vim.lsp.buf.definition()` - Go to definition
- `vim.lsp.buf.code_action()` - Show code actions
- `vim.lsp.buf.rename()` - Rename symbol

### Completion Support

The LSP server provides intelligent code completion with:

- **Trigger Characters**: Completion is automatically triggered on ` `, `|`, and `:`
- **Snippet Support**: Function completions include parameter placeholders (requires snippet-capable completion plugin like nvim-cmp)
- **Context-Aware**: Only shows symbols available in the current scope
- **Module Completions**: Type `module::` to see functions from that module

For the best completion experience, use [nvim-cmp](https://github.com/hrsh7th/nvim-cmp):

```lua
{
  "hrsh7th/nvim-cmp",
  dependencies = {
    "hrsh7th/cmp-nvim-lsp",
    "L3MON4D3/LuaSnip", -- for snippet support
  },
  config = function()
    local cmp = require("cmp")
    cmp.setup({
      snippet = {
        expand = function(args)
          require("luasnip").lsp_expand(args.body)
        end,
      },
      sources = {
        { name = "nvim_lsp" },
      },
    })
  end,
}
```

## Debugging with nvim-dap

The mq.nvim plugin provides full integration with [nvim-dap](https://github.com/mfussenegger/nvim-dap) for debugging mq queries.

### Setup

1. Install [nvim-dap](https://github.com/mfussenegger/nvim-dap):

```lua
{
  "mfussenegger/nvim-dap",
  dependencies = {
    "rcarriga/nvim-dap-ui", -- Optional: Better debugging UI
    "theHamsta/nvim-dap-virtual-text", -- Optional: Show variable values inline
  },
}
```

2. Install mq and mq-dbg (Debug Adapter Protocol server):

The `:MqInstall` command automatically installs both `mq` and `mq-dbg`:

```vim
:MqInstall
```

Or install manually via cargo:

```bash
# Install mq LSP server
cargo install --git https://github.com/harehare/mq.git mq-run

# Install debugger
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
```

3. The DAP adapter is automatically configured when you call `require("mq").setup()` if nvim-dap is installed.

### Usage

1. Open an mq file
2. Set breakpoints using `:lua require('dap').toggle_breakpoint()`
3. Run `:MqDebugFile` to start debugging
4. Select an input file when prompted
5. Use nvim-dap commands to control debugging

### DAP UI (Optional)

For a better debugging experience, install [nvim-dap-ui](https://github.com/rcarriga/nvim-dap-ui):

```lua
{
  "rcarriga/nvim-dap-ui",
  dependencies = { "mfussenegger/nvim-dap", "nvim-neotest/nvim-nio" },
  config = function()
    local dap, dapui = require("dap"), require("dapui")
    dapui.setup()

    -- Automatically open/close DAP UI
    dap.listeners.after.event_initialized["dapui_config"] = function()
      dapui.open()
    end
    dap.listeners.before.event_terminated["dapui_config"] = function()
      dapui.close()
    end
    dap.listeners.before.event_exited["dapui_config"] = function()
      dapui.close()
    end
  end,
}
```

### How mq-dbg Works

The mq-dbg debugger allows you to:
- Set breakpoints in your mq query files
- Step through query execution line by line
- Inspect variable values and the current data being processed
- Evaluate expressions in the debug console
- See the call stack and scopes

The debugger uses the Debug Adapter Protocol (DAP), the same protocol used by VS Code, so you get a consistent debugging experience across editors.

## Installing mq

If you don't have mq installed, you can install it using the `:MqInstall` command:

```vim
:MqInstall
```

This command will install both:
- `mq` - The main LSP server and command-line tool
- `mq-dbg` - The Debug Adapter Protocol (DAP) server for debugging

Alternatively, you can install manually via cargo:

```bash
# Install mq LSP server and CLI tool
cargo install --git https://github.com/harehare/mq.git mq-run

# Install debugger (optional, for DAP support)
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
```

Or using the installation script from the main repository:

```bash
curl -sSL https://mqlang.org/install.sh | bash
```

**Note:** The `:MqInstall` command requires Rust and cargo to be installed on your system.

## License

MIT License - see the main [mq repository](https://github.com/harehare/mq) for details.

## Contributing

Contributions are welcome! Please see the main [mq repository](https://github.com/harehare/mq) for contribution guidelines.
