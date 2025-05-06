# mq.nvim

Neovim plugin for [mq](https://github.com/harehare/mq), a jq-like command-line tool for Markdown processing.

## Features

- Language Server Protocol integration for mq queries
- Syntax highlighting for mq query files
- File type detection and settings for mq files

## Requirements

- Neovim 0.6.0+
- [mq-lsp](https://github.com/harehare/mq) binary installed and available in PATH

## Installation

### Using [packer.nvim](https://github.com/wbthomason/packer.nvim)

```lua
use {
  'josephscade/mq',
  rtp = 'editors/nvim',
  requires = {
    'neovim/nvim-lspconfig' -- Optional, for LSP configuration
  }
}
```

### Using [lazy.nvim](https://github.com/folke/lazy.nvim)

```lua
{
  'josephscade/mq',
  dir = 'editors/nvim',
  dependencies = {
    'neovim/nvim-lspconfig' -- Optional, for LSP configuration
  }
}
```

## Configuration

### Basic Setup

```lua
require('mq').setup()
```

### Advanced Configuration

```lua
require('mq').setup({
  -- Path to the mq-lsp executable
  -- If not specified, the plugin will try to find it in PATH
  lsp_bin = 'mq-lsp',

  -- Additional LSP server settings
  lsp_settings = {
    -- LSP server settings here
  },

  -- Enable or disable features
  features = {
    highlighting = true,
    formatting = true
  }
})
```

## Usage

Once installed and configured, the plugin will automatically:

- Detect `.mq` files and apply syntax highlighting
- Start the mq language server for `.mq` files
- Provide completion, hover information, and other LSP features

## Commands

- `:MqFormat`: Format the current buffer using mq formatter
- `:MqRun`: Run the current mq query file on a selected Markdown file

## License

MIT License - See LICENSE for details
