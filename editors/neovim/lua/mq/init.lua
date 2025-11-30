local M = {}

local config = require("mq.config")
local lsp = require("mq.lsp")
local commands = require("mq.commands")
local dap = require("mq.dap")

function M.setup(opts)
  vim.g.mq_initialized = true
  config.setup(opts)
  commands.register_commands()
  lsp.setup_autostart()

  -- Setup DAP adapter if nvim-dap is available
  dap.setup()
end

return M
