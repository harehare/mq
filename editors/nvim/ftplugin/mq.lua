-- File type plugin for mq files
-- Sets buffer-specific settings for mq files

-- Set comment string for mq files (uses # for comments)
vim.bo.commentstring = '# %s'

-- Set indentation settings
vim.bo.expandtab = true
vim.bo.shiftwidth = 2
vim.bo.tabstop = 2
vim.bo.softtabstop = 2

-- Enable LSP features for mq files
if vim.fn.exists(':LspStart') == 2 then
    vim.cmd('LspStart')
end
