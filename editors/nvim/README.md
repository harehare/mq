# MQ Neovim Plugin

This plugin provides support for the MQ programming language in Neovim, including syntax highlighting and Language Server Protocol (LSP) integration.

## Features

*   **Syntax Highlighting**: Basic syntax highlighting for MQ files (`.mq`).
*   **LSP Integration**: Connects to the `mq-lsp` language server to provide:
    *   Autocompletion
    *   Go to Definition
    *   Hover information
    *   Find References
    *   Diagnostics (errors, warnings)
    *   Formatting (if supported by `mq-lsp`)

## Prerequisites

*   Neovim (0.7.0 or later recommended for full LSP support).
*   [`nvim-lspconfig`](https://github.com/neovim/nvim-lspconfig): This plugin relies on `nvim-lspconfig` to manage LSP client configurations.
*   [`mq-lsp`](#lsp-server-mq-lsp): The MQ Language Server must be installed and accessible in your system's PATH.

## Installation

Choose your preferred plugin manager:

### `packer.nvim`

Add the following to your `plugins.lua` or equivalent Packer setup file:

```lua
use {
  -- Replace with the actual repository URL or local path
  'your-username/your-repo-name',
  -- If the plugin is in a subdirectory of your repo (like 'editors/nvim'):
  rtp = 'editors/nvim',
  requires = {'neovim/nvim-lspconfig'},
  config = function()
    -- The LSP setup is usually handled by the FileType autocommand
    -- in `plugin/mq.lua`. If you need to customize the LSP server command
    -- or other options, you might do it here or by directly calling
    -- require('mq_lsp').setup({ cmd = {"/path/to/your/mq-lsp"} })
    -- after this plugin is loaded.
  end
}
```

Ensure you have `nvim-lspconfig` installed as well:
```lua
use {'neovim/nvim-lspconfig'}
```

Then run `:PackerSync` or `:PackerCompile` followed by `:PackerInstall`.

### `vim-plug`

Add the following to your `init.vim` or `init.lua`:

```vim
" For Vimscript init.vim
Plug 'neovim/nvim-lspconfig'
" Replace with the actual repository URL
Plug 'your-username/your-repo-name', { 'rtp': 'editors/nvim' }
```

```lua
-- For Lua init.lua
require('packer').startup(function(use)
  use 'neovim/nvim-lspconfig'
  -- Replace with the actual repository URL
  use { 'your-username/your-repo-name', rtp = 'editors/nvim' }
end)
```

Then run `:PlugInstall`.

### Manual Installation (not recommended)

Clone this repository and copy the contents of `editors/nvim/` into your Neovim configuration directory (e.g., `~/.config/nvim/` or `~/AppData/Local/nvim/`).

## LSP Server (`mq-lsp`)

This Neovim plugin acts as a client for the `mq-lsp` language server. You **must** install `mq-lsp` separately.

*   **Installation**:
    *   If `mq-lsp` is hosted in its own repository, follow the installation instructions there: `[link-to-mq-lsp-repo]`
    *   If `mq-lsp` is part of this repository (e.g., in a `lang-server/` directory):
        1.  Navigate to the `mq-lsp` source directory (e.g., `cd lang-server`).
        2.  Build the server (example for a Rust project): `cargo build --release`.
        3.  Ensure the resulting binary (e.g., `lang-server/target/release/mq-lsp`) is in your system's PATH. You can copy it to a directory like `/usr/local/bin` or `~/.local/bin`, or add its location to your PATH environment variable.

The plugin will attempt to start `mq-lsp` using the command `mq-lsp` by default.

## Configuration (Optional)

The LSP client for `mq` files is automatically initialized when you open an `.mq` file, thanks to the `plugin/mq.lua` script.

If you need to customize the `mq-lsp` initialization, such as specifying a different command to run the LSP server or modifying `on_attach` behavior, you can do so by calling the setup function directly in your Neovim configuration *after* the plugin is loaded.

The `mq_lsp` module is located at `lua/mq_lsp/init.lua` within this plugin's structure.

**Example:** To use a custom path for the `mq-lsp` executable:

```lua
-- In your main init.lua or a dedicated lsp setup file:
-- Ensure this runs after your plugin manager has loaded the 'mq' plugin.

-- For Packer, you might put this in the config block or an ftplugin file.
-- For vim-plug, after Plug#end().

-- Wait for the 'mq' filetype to be processed by the plugin's autocommands
-- OR directly configure it if you manage LSP setups centrally.

-- If you want to override AFTER the plugin's default setup on FileType event:
-- This is a bit more complex. A simpler way is to prevent the default setup
-- and do it all yourself.

-- A more direct way to configure it centrally with nvim-lspconfig:
require('lspconfig').mq_lsp.setup{
  cmd = { "/path/to/your/custom/mq-lsp-executable" },
  -- You can also override other options like on_attach, capabilities, etc.
  -- on_attach = function(client, bufnr)
  --   print("Custom on_attach for MQ LSP")
  --   -- Your custom on_attach logic
  -- end
}
```

Refer to the `lua/mq_lsp/init.lua` file for the available options in its `setup` function. By default, it registers a server configuration named `mq_lsp` with `nvim-lspconfig`.

## Local Development

To contribute to this plugin or test local changes:

1.  **Clone the Repository**:
    ```bash
    git clone [URL-of-this-repository]
    cd [repository-directory]
    ```

2.  **Build `mq-lsp`**:
    (Assuming `mq-lsp` source is within this repository, e.g., in a `lang-server/` directory)
    ```bash
    cd lang-server  # Or the actual directory of mq-lsp
    cargo build --package mq-lsp # Or the specific cargo package name if different
    # For release: cargo build --package mq-lsp --release
    ```
    The binary will typically be found in `target/debug/mq-lsp` or `target/release/mq-lsp` relative to the `mq-lsp` package directory.

3.  **Make Neovim Use Local `mq-lsp`**:
    *   **Option A (Recommended for Development)**: Configure the plugin to point to your local binary.
        Create a file, for example, `after/plugin/mq_dev.lua` in your Neovim config directory (`~/.config/nvim/after/plugin/mq_dev.lua`):
        ```lua
        -- ~/.config/nvim/after/plugin/mq_dev.lua
        -- Adjust the path to where your locally built mq-lsp is.
        -- This assumes the main mq plugin is already loaded.

        local lspconfig = require('lspconfig')
        if lspconfig.mq_lsp then
            lspconfig.mq_lsp.setup{
                cmd = {"/full/path/to/your/repository/lang-server/target/debug/mq-lsp"}
                -- Add other custom settings for development if needed
            }
            print("MQ LSP configured to use local development binary.")
        else
            print("MQ LSP config not found. Ensure the main MQ plugin is loaded.")
        end
        ```
        This overrides the default `cmd` when `mq_lsp` is set up by `nvim-lspconfig`.
    *   **Option B**: Add the directory containing your locally built `mq-lsp` to your system's `PATH`.
        ```bash
        export PATH="/full/path/to/your/repository/lang-server/target/debug:$PATH"
        ```
        This change might only be for the current terminal session unless added to your shell's configuration file (e.g., `.bashrc`, `.zshrc`).

4.  **Load the Local Plugin in Neovim**:
    *   **Using a Plugin Manager (e.g., Packer)**:
        You can point your plugin manager to use your local clone of the repository.
        For `packer.nvim`:
        ```lua
        use {
          '/full/path/to/your/repository/', -- Path to the root of *this* repository
          rtp = 'editors/nvim',             -- If the plugin is in editors/nvim subdir
          requires = {'neovim/nvim-lspconfig'}
        }
        ```
    *   **Manual Symlinking (Advanced)**:
        Symlink the `editors/nvim` directory from your cloned repository into your Neovim's package path (e.g., `~/.config/nvim/pack/dev/start/mq-nvim`).
        ```bash
        # Example:
        mkdir -p ~/.config/nvim/pack/dev/start
        ln -s /full/path/to/your/repository/editors/nvim ~/.config/nvim/pack/dev/start/mq-nvim
        ```
    *   **Modifying `runtimepath` (Temporary)**:
        You can temporarily add your plugin's path to Neovim's runtime path when launching Neovim:
        ```bash
        nvim -c "set runtimepath+=/full/path/to/your/repository/editors/nvim"
        ```

After setting up, open an `.mq` file in Neovim. Check `:LspInfo` to see if `mq-lsp` is attached and configured correctly. Check `:messages` for any error messages from the plugin or LSP client.
