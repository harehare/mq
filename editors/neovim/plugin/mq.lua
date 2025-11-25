-- mq.nvim plugin entry point
-- This file is loaded automatically by Neovim

-- Prevent loading the plugin twice
if vim.g.loaded_mq then
  return
end
vim.g.loaded_mq = 1

-- Set up basic filetype detection for .mq files
vim.filetype.add({
  extension = {
    mq = "mq",
  },
})

-- Initialize with default configuration if setup() is not called
-- This ensures commands are available even without explicit setup
vim.defer_fn(function()
  -- Check if already initialized
  if not vim.g.mq_initialized then
    local config = require("mq.config")

    -- Setup with defaults if not already configured
    if vim.tbl_isempty(config.options) then
      config.setup(config.defaults)
    end

    -- Register commands
    local commands = require("mq.commands")
    commands.register_commands()

    -- Setup LSP autostart
    local lsp = require("mq.lsp")
    lsp.setup_autostart()

    -- Setup DAP if available
    local dap = require("mq.dap")
    dap.setup()

    vim.g.mq_initialized = true
  end
end, 0)
