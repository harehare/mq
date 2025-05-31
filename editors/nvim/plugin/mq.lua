-- File: editors/nvim/plugin/mq.lua
-- Description: Main plugin file for MQ language support in Neovim.
-- This file is automatically sourced by Neovim when it starts
-- because it's in the 'plugin' directory.

-- Ensure the code only runs once
if vim.g.mq_plugin_loaded then
  return
end
vim.g.mq_plugin_loaded = true

-- Create an augroup for MQ specific autocommands.
-- Using an augroup ensures that we can clear and redefine the autocommands
-- without them stacking up if this file is sourced multiple times (e.g., during development).
local mq_augroup = vim.api.nvim_create_augroup("MqFileTypeGroup", { clear = true })

-- Autocommand to initialize the MQ LSP client when an MQ file is opened.
vim.api.nvim_create_autocmd("FileType", {
  group = mq_augroup,
  pattern = "mq", -- Trigger for 'mq' filetype
  callback = function()
    -- Attempt to load and setup the MQ LSP.
    -- Using pcall to gracefully handle potential errors, for example,
    -- if the 'mq_lsp' module is not found or has issues.
    local success, mq_lsp_module = pcall(require, "mq_lsp")

    if success and mq_lsp_module and mq_lsp_module.setup then
      -- Call the setup function from the mq_lsp module.
      -- You can pass options here if needed, e.g., mq_lsp_module.setup({ cmd = {"/path/to/custom/mq-lsp"} })
      local setup_success, err = pcall(mq_lsp_module.setup, {})
      if not setup_success then
        vim.notify("Failed to setup MQ LSP: " .. tostring(err), vim.log.levels.ERROR)
      else
        -- Optional: Notify that LSP setup was called. For debugging or user feedback.
        -- vim.notify("MQ LSP setup initiated for " .. vim.api.nvim_buf_get_name(0), vim.log.levels.INFO)
      end
    else
      vim.notify("MQ LSP module ('mq_lsp') not found or setup function missing.", vim.log.levels.WARN)
    end
  end,
  desc = "Initialize MQ LSP client for .mq files",
})

-- Optional: Add a command to manually setup the LSP for the current buffer if it's an mq file.
vim.api.nvim_create_user_command("MqSetupLsp", function()
    if vim.bo.filetype == "mq" then
        local success, mq_lsp_module = pcall(require, "mq_lsp")
        if success and mq_lsp_module and mq_lsp_module.setup then
            local setup_success, err = pcall(mq_lsp_module.setup, {})
            if not setup_success then
                vim.notify("Failed to setup MQ LSP: " .. tostring(err), vim.log.levels.ERROR)
            else
                vim.notify("MQ LSP setup manually initiated for current buffer.", vim.log.levels.INFO)
            end
        else
            vim.notify("MQ LSP module ('mq_lsp') not found or setup function missing.", vim.log.levels.WARN)
        end
    else
        vim.notify("Not an MQ file. Current filetype: " .. vim.bo.filetype, vim.log.levels.WARN)
    end
end, {
    desc = "Manually setup MQ LSP for the current buffer (if it's an mq file)",
})

-- You could add other plugin-wide settings or commands here in the future.
-- For example, setting up filetype detection if it's not handled by ftplugin.
-- However, for ftplugin based detection, a file in `ftdetect/mq.vim` with
-- `au BufRead,BufNewFile *.mq setfiletype mq` would be the standard way.
-- This task did not ask for ftdetect, so this plugin file focuses on LSP init.

-- vim.notify("MQ Plugin Loaded", vim.log.levels.INFO) -- For debugging plugin loading itself
