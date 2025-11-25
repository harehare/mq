local M = {}

local config = require("mq.config")
local lsp = require("mq.lsp")
local dap = require("mq.dap")
local commands = require("mq.commands")

-- Setup function
function M.setup(opts)
  -- Mark as initialized to prevent plugin/mq.lua from re-initializing
  vim.g.mq_initialized = true

  -- Setup configuration
  config.setup(opts)

  -- Register commands (safe to call multiple times)
  commands.register_commands()

  -- Setup LSP autostart
  lsp.setup_autostart()

  -- Setup DAP if available
  dap.setup()
end

return M
