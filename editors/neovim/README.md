# mq.nvim

Neovim plugin for [mq](https://mqlang.org/) - a jq-like tool for Markdown processing.

## Features

- 🎨 **Syntax Highlighting** - Full syntax support for `.mq` files
- 🔧 **LSP Integration** - Code completion, diagnostics, and more via Language Server Protocol
- 🐛 **DAP Integration** - Debug your mq queries with nvim-dap
- ⚡ **Commands** - Execute mq queries directly from Neovim
- 📝 **File Templates** - Create new mq files with helpful examples

## Requirements

- Neovim >= 0.8.0
- [mq](https://github.com/harehare/mq) installed (LSP and command execution)
- [mq-dbg](https://github.com/harehare/mq) installed (optional, for debugging)
- [nvim-dap](https://github.com/mfussenegger/nvim-dap) (optional, for debugging support)

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

  -- DAP server command
  dap_cmd = "mq-dbg",

  -- DAP server arguments
  dap_args = { "dap" },

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

| Command | Description |
|---------|-------------|
| `:MqNew` | Create a new mq file with examples |
| `:MqInstallServers` | Install mq LSP and DAP servers via cargo |
| `:MqStartLSP` | Start the mq LSP server |
| `:MqStopLSP` | Stop the mq LSP server |
| `:MqRestartLSP` | Restart the mq LSP server |
| `:MqRunSelected` | Run selected text as mq query (visual mode) |
| `:MqExecuteQuery` | Execute mq query on current file |
| `:MqExecuteFile` | Execute mq file on current file |
| `:MqDebug` | Debug current mq file (requires nvim-dap) |

## Usage Examples

### Create a new mq file

```vim
:MqNew
```

This creates a new buffer with mq filetype and helpful examples (if `show_examples` is enabled).

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

```vim
:MqDebug
```

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

    -- Debug
    vim.keymap.set("n", "<leader>md", ":MqDebug<CR>", opts)

    -- LSP commands
    vim.keymap.set("n", "<leader>ms", ":MqStartLSP<CR>", opts)
    vim.keymap.set("n", "<leader>mS", ":MqStopLSP<CR>", opts)
  end,
})
```

## LSP Features

When the LSP server is running, you get:

- 📝 **Code Completion** - Auto-complete for mq functions and keywords
- 🔍 **Diagnostics** - Real-time error checking
- 📖 **Hover Documentation** - View function documentation
- 🎯 **Go to Definition** - Jump to function definitions
- 🔧 **Code Actions** - Quick fixes and refactoring

Use Neovim's built-in LSP commands:

- `vim.lsp.buf.hover()` - Show hover information
- `vim.lsp.buf.definition()` - Go to definition
- `vim.lsp.buf.code_action()` - Show code actions
- `vim.lsp.buf.rename()` - Rename symbol

## Debugging with nvim-dap

To use debugging features:

1. Install [nvim-dap](https://github.com/mfussenegger/nvim-dap)
2. Install mq-dbg: `cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"`
3. Open an mq file
4. Run `:MqDebug`
5. Select an input file
6. Use nvim-dap commands to control debugging

Example nvim-dap setup:

```lua
local dap = require("dap")

-- Set breakpoint
vim.keymap.set("n", "<F5>", dap.continue)
vim.keymap.set("n", "<F10>", dap.step_over)
vim.keymap.set("n", "<F11>", dap.step_into)
vim.keymap.set("n", "<F12>", dap.step_out)
vim.keymap.set("n", "<leader>b", dap.toggle_breakpoint)
```

## Installing mq

If you don't have mq installed, you can install it via cargo:

```vim
:MqInstallServers
```

Or manually:

```bash
cargo install --git https://github.com/harehare/mq.git mq-run
cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"
```

## Troubleshooting

### LSP server not starting

1. Check if mq is installed: `which mq`
2. Check LSP server status: `:LspInfo`
3. View LSP logs: `:lua vim.cmd('e ' .. vim.lsp.get_log_path())`
4. Manually start server: `:MqStartLSP`

### mq command not found

Either:
- Add mq to your PATH
- Configure the path in setup: `cmd = "/path/to/mq"`
- Install via `:MqInstallServers`

## License

MIT License - see the main [mq repository](https://github.com/harehare/mq) for details.

## Contributing

Contributions are welcome! Please see the main [mq repository](https://github.com/harehare/mq) for contribution guidelines.
