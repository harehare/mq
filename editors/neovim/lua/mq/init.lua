local M = {}

local config = require("mq.config")
local lsp = require("mq.lsp")
local commands = require("mq.commands")

function M.setup(opts)
  vim.g.mq_initialized = true
  config.setup(opts)
  commands.register_commands()
  lsp.setup_autostart()
end

return M
