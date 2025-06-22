# mq Neovim Plugin

This plugin provides support for the `mq` language in Neovim. `mq` is a jq-like tool for Markdown processing.

## Features

- Syntax Highlighting for `.mq` files.
- Language Server Protocol (LSP) integration for features like:
    - Autocompletion
    - Diagnostics (errors/warnings)
    - Go to definition
    - Hover information
    - Code actions
    - Renaming
    - Formatting (if supported by the LSP server)
- Code Execution: Run `mq` scripts from within Neovim.

## Installation

Use your favorite plugin manager.

### [Packer](https://github.com/wbthomason/packer.nvim)

```lua
use {
  'your-username/mq.nvim', -- Replace with the actual path if published separately
  -- Or if it's part of the main mq repo:
  -- '/path/to/mq/editors/nvim', -- Adjust if you clone the mq repo locally
  -- For example, if your plugins are in ~/.config/nvim/lua/plugins.lua
  -- and mq repo is cloned to ~/projects/mq
  -- use '~/projects/mq/editors/nvim'
  -- It's generally better if this plugin is a standalone repository or part of a plugin distribution.
  -- For local testing:
  -- use { dir = '/path/to/your/local/mq/editors/nvim' }
  config = function()
    -- No explicit setup needed if using the init.lua correctly.
    -- The plugin should load automatically for .mq files.
  end
}
```

### [vim-plug](https://github.com/junegunn/vim-plug)

```vim
" Assuming the plugin is in a standard plugin structure and accessible.
" Plug 'your-username/mq.nvim'
" For local testing:
Plug 'path/to/your/local/mq/editors/nvim'

" Initialize the plugin (usually after Plug#end())
" No specific setup call is strictly needed if init.lua handles it.
" LSP setup is automatic on FileType mq.
```

**Note:** You need to have the `mq` executable in your `PATH` for LSP and code execution to work.
If `mq` is not found, the plugin will attempt to notify you. The LSP server (`mq lsp`) is expected to be provided by the `mq` installation.

## Usage

### Syntax Highlighting

Syntax highlighting is automatically enabled for files with the `.mq` extension.

### LSP

The LSP client will automatically start when you open an `.mq` file, provided `mq` is in your `PATH`.
Standard LSP keybindings can be used. Some common ones are configured by default in `lua/mq/lsp.lua`:

- `gD`: Go to Declaration
- `gd`: Go to Definition
- `K`: Hover
- `gi`: Go to Implementation
- `<C-k>`: Signature Help (Ctrl+k)
- `<space>rn`: Rename
- `<space>ca`: Code Action
- `<space>f`: Format (if LSP supports it)

You can customize these mappings in your Neovim configuration.

### Commands

The plugin provides the following commands:

-   `:MqRunFile`
    -   Runs the entire content of the current `.mq` buffer.
    -   It will prompt you to select a `.md` (Markdown) file from your current working directory and its subdirectories (up to 3 levels deep) to use as input for the `mq` script.
    -   The output will be displayed in a new vertical split.
-   `:MqRunSelected`
    -   Runs the visually selected text from the current buffer as an `mq` script.
    -   It will also prompt you to select a `.md` file as input.
    -   The output will be displayed in a new vertical split.
-   `:MqLspStart`
    -   Manually starts or restarts the `mq` LSP client. Useful for troubleshooting.

## Configuration

The LSP client attempts to find the `mq` executable in your `PATH`.
Currently, there are no specific plugin-level configuration options exposed beyond Neovim's standard LSP client settings. You can configure the LSP client further using `lspconfig` if needed.

For example, to override the command for the LSP server:
```lua
require('lspconfig').mq_lsp.setup{
  cmd = {"/custom/path/to/mq", "lsp"},
  -- other lspconfig options
}
```
Place such custom configurations *after* your plugin manager loads this plugin.

## Development & Testing

The plugin files are located in `editors/nvim/`.
- `ftdetect/mq.vim`: Filetype detection.
- `syntax/mq.vim`: Syntax highlighting rules.
- `lua/mq/init.lua`: Main plugin entry point, command setup, and LSP auto-start.
- `lua/mq/lsp.lua`: LSP client configuration.
- `lua/mq/runner.lua`: Code execution logic.

To test locally:
1. Clone the `mq` repository (or the repository containing this plugin).
2. Add the local path to this plugin in your Neovim plugin manager. For example, with Packer:
   ```lua
   use { dir = '~/path/to/mq/editors/nvim' }
   ```
3. Restart Neovim and open an `.mq` file.

## Snippets

The plugin includes snippets for `mq` language constructs. These are compatible with [LuaSnip](https://github.com/L3MON4D3/LuaSnip).
If you have LuaSnip installed, the snippets should be automatically loaded when you open an `mq` file.

Available snippets include:
- `def`: Define function
- `fn`: Anonymous function
- `let`: Variable declaration
- `if`: If condition
- `ifelse`: If-else condition
- `elif`: Else-if condition
- `foreach`: Foreach loop
- `while`: While loop
- `until`: Until loop

You can view the snippet definitions in `lua/mq/snippets.lua`.

## TODO

-   [ ] More robust error handling for code execution.
-   [ ] Allow configuration of output window (float vs split) via a global variable/setup option. (Runner has internal option, but not user-configurable yet).
-   [ ] Option to configure `mq` executable path if not in `PATH`. (LSP part can be overridden, runner needs explicit change or global var)
-   [ ] Tests for Lua modules.Tool output for `create_file_with_block`:
