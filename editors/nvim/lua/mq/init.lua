local lsp = require('mq.lsp')
local runner = require('mq.runner')

-- Autocmd to start LSP when an mq file is opened
vim.api.nvim_create_autocmd("FileType", {
  pattern = "mq",
  callback = function()
    lsp.start()
    -- Attempt to load snippets if LuaSnip is available
    local status_ok, luasnip = pcall(require, "luasnip")
    if not status_ok then
      return
    end
    local status_ok_loader, loader = pcall(require, "luasnip.loaders.from_lua")
    if not status_ok_loader then
        return
    end
    -- Assuming snippets.lua is in the same directory as init.lua under 'mq' module
    pcall(loader.load, {paths = {"mq.snippets"}})
    -- Or, more directly if you want to manage them from here:
    -- local snippets = require('mq.snippets').get_snippets()
    -- luasnip.add_snippets("mq", snippets)
  end,
})

-- User commands
vim.api.nvim_create_user_command('MqRunFile', function()
  runner.run_current_file()
end, {
  desc = "Run the current mq file against a selected Markdown file."
})

vim.api.nvim_create_user_command('MqRunSelected', function()
  runner.run_selected_text()
end, {
  desc = "Run the selected mq text against a selected Markdown file.",
  range = true -- Allows visual selection
})

vim.api.nvim_create_user_command('MqLspStart', function()
  lsp.start()
end, {
  desc = "Manually start the mq LSP server."
})

print("mq Neovim plugin loaded. Commands: MqRunFile, MqRunSelected, MqLspStart")
