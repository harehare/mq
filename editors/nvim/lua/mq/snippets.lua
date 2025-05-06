-- Snippets module for mq.nvim
-- Provides integration with snippet engines

local M = {}

-- Path to snippets directory
local snippets_dir = vim.fn.expand('<sfile>:p:h:h:h') .. '/snippets'

-- Get path to mq snippets file
function M.get_snippets_file()
    return snippets_dir .. '/mq.json'
end

-- Setup LuaSnip integration
function M.setup_luasnip()
    local has_luasnip, luasnip = pcall(require, 'luasnip')
    if not has_luasnip then
        return false
    end

    local has_loader, loader = pcall(require, 'luasnip.loaders.from_vscode')
    if not has_loader then
        return false
    end

    -- Load snippets from the snippets directory
    loader.load({
        paths = { snippets_dir }
    })

    return true
end

-- Setup coc.nvim integration
function M.setup_coc()
    -- Check if coc.nvim is available
    if vim.fn.exists('*coc#rpc#start_server') ~= 1 then
        return false
    end

    -- Add the snippets directory to coc-snippets
    local config_file = vim.fn.expand('~/.config/coc/ultisnips')

    -- Create the directory if it doesn't exist
    if vim.fn.isdirectory(config_file) == 0 then
        vim.fn.mkdir(config_file, 'p')
    end

    -- Create a symlink to the mq snippets
    local source = snippets_dir .. '/mq.json'
    local target = config_file .. '/mq.json'

    if vim.fn.filereadable(target) == 0 then
        os.execute('ln -sf ' .. source .. ' ' .. target)
    end

    return true
end

-- Setup vsnip integration
function M.setup_vsnip()
    -- Check if vsnip is available
    if vim.fn.exists('g:vsnip_snippet_dir') == 0 then
        return false
    end

    local vsnip_dir = vim.g.vsnip_snippet_dir

    -- Create a symlink to the mq snippets
    local source = snippets_dir .. '/mq.json'
    local target = vsnip_dir .. '/mq.json'

    if vim.fn.filereadable(target) == 0 then
        os.execute('ln -sf ' .. source .. ' ' .. target)
    end

    return true
end

-- Setup snippets for all supported engines
function M.setup()
    local luasnip_ok = M.setup_luasnip()
    local coc_ok = M.setup_coc()
    local vsnip_ok = M.setup_vsnip()

    if not (luasnip_ok or coc_ok or vsnip_ok) then
        vim.notify('No snippet engine found. Snippets will not be available.', vim.log.levels.WARN)
        vim.notify('Supported engines: LuaSnip, coc.nvim, vsnip', vim.log.levels.INFO)
        return false
    end

    return true
end

return M
