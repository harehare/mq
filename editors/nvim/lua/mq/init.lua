-- mq.nvim: A Neovim plugin for mq markdown processing tool
-- Implements LSP client for mq-lsp

local M = {}

-- Default configuration
local default_config = {
    lsp_bin = 'mq-lsp',
    lsp_settings = {},
    features = {
        highlighting = true,
        formatting = true,
        snippets = true
    }
}

-- Store user config
M.config = {}

-- Setup function to initialize the plugin
function M.setup(opts)
    -- Merge user config with default config
    M.config = vim.tbl_deep_extend('force', default_config, opts or {})

    -- Set up LSP if available
    local has_lspconfig, lspconfig = pcall(require, 'lspconfig')
    if has_lspconfig then
        M.setup_lsp(lspconfig)
    else
        vim.notify('nvim-lspconfig not found, LSP features disabled for mq.', vim.log.levels.WARN)
    end

    -- Register commands
    M.setup_commands()

    -- Check for mq-lsp binary
    if M.config.features.formatting or M.config.features.completion then
        M.check_lsp_binary()
    end

    -- Set up formatters if enabled
    if M.config.features.formatting then
        M.setup_formatters()
    end

    -- Set up snippets if enabled
    if M.config.features.snippets then
        local snippets = require('mq.snippets')
        snippets.setup()
    end
end

-- Setup LSP configuration
function M.setup_lsp(lspconfig)
    local util = require 'lspconfig.util'

    -- Define mq LSP
    lspconfig.mq_lsp = {
        default_config = {
            cmd = { M.config.lsp_bin },
            filetypes = { 'mq' },
            root_dir = util.find_git_ancestor,
            settings = M.config.lsp_settings,
        }
    }

    -- Setup the LSP
    lspconfig.mq_lsp.setup({
        capabilities = vim.lsp.protocol.make_client_capabilities(),
        flags = {
            debounce_text_changes = 150,
        },
        on_attach = function(client, bufnr)
            -- Enable completion triggered by <c-x><c-o>
            vim.api.nvim_buf_set_option(bufnr, 'omnifunc', 'v:lua.vim.lsp.omnifunc')

            -- Mappings
            local opts = { noremap = true, silent = true, buffer = bufnr }
            vim.keymap.set('n', 'gd', vim.lsp.buf.definition, opts)
            vim.keymap.set('n', 'K', vim.lsp.buf.hover, opts)
            vim.keymap.set('n', '<C-k>', vim.lsp.buf.signature_help, opts)
            vim.keymap.set('n', '<Leader>rn', vim.lsp.buf.rename, opts)
            vim.keymap.set('n', '<Leader>ca', vim.lsp.buf.code_action, opts)
            vim.keymap.set('n', 'gr', vim.lsp.buf.references, opts)

            -- We're using mq fmt instead of LSP formatting
            client.server_capabilities.documentFormattingProvider = false
            client.server_capabilities.documentRangeFormattingProvider = false
        end,
    })
end

-- Setup formatters
function M.setup_formatters()
    -- Check if formatter already set up
    if vim.b.mq_format_setup then
        return
    end

    vim.api.nvim_create_autocmd("BufWritePre", {
        pattern = "*.mq",
        callback = function()
            if M.config.features.formatting then
                local formatter = require('mq.formatter')
                formatter.format_buffer()
            end
        end,
    })

    vim.b.mq_format_setup = true
end

-- Setup commands
function M.setup_commands()
    -- Format the current buffer
    vim.api.nvim_create_user_command('MqFormat', function()
        local formatter = require('mq.formatter')
        formatter.format_buffer()
    end, {
        desc = 'Format the current buffer using mq fmt'
    })

    -- Run the current mq query on a markdown file
    vim.api.nvim_create_user_command('MqRun', function(opts)
        local query = require('mq.query')
        local query_file = vim.fn.expand('%:p')
        local markdown_file = opts.args

        if markdown_file == '' then
            -- Prompt for a markdown file if not provided
            query.select_markdown_file(function(selected_file)
                if selected_file then
                    query.run_query(query_file, selected_file)
                end
            end)
        else
            query.run_query(query_file, markdown_file)
        end
    end, {
        nargs = '?',
        complete = 'file',
        desc = 'Run current mq query on a markdown file'
    })

    -- Run current buffer as mq query (even if not saved)
    vim.api.nvim_create_user_command('MqRunBuffer', function(opts)
        local query = require('mq.query')
        local markdown_file = opts.args

        if markdown_file == '' then
            -- Prompt for a markdown file if not provided
            query.select_markdown_file(function(selected_file)
                if selected_file then
                    query.run_buffer_query(selected_file)
                end
            end)
        else
            query.run_buffer_query(markdown_file)
        end
    end, {
        nargs = '?',
        complete = 'file',
        desc = 'Run current buffer as mq query on a markdown file'
    })

    -- Create a new mq query file
    vim.api.nvim_create_user_command('MqNew', function()
        -- Create a new buffer
        vim.cmd('enew')

        -- Set the filetype to mq
        vim.api.nvim_buf_set_option(0, 'filetype', 'mq')

        -- Add a default comment
        local lines = {
            '# mq query file',
            '# Created on ' .. os.date('%Y-%m-%d %H:%M:%S'),
            '',
            '# Enter your query below:',
            'select Heading1'
        }

        vim.api.nvim_buf_set_lines(0, 0, -1, false, lines)

        -- Move cursor to end of file
        vim.api.nvim_win_set_cursor(0, { 5, 14 })
    end, {
        desc = 'Create a new mq query file'
    })

    -- List available snippets
    vim.api.nvim_create_user_command('MqSnippets', function()
        -- Create a new buffer
        vim.cmd('new')
        local buf = vim.api.nvim_get_current_buf()

        -- Read snippets file and extract information
        local snippets_file = require('mq.snippets').get_snippets_file()
        local snippets_json = vim.fn.readfile(snippets_file)
        local snippets = vim.fn.json_decode(table.concat(snippets_json, '\n'))

        -- Create a list of snippets for display
        local lines = { '# Available mq Snippets', '' }

        for name, snippet in pairs(snippets) do
            table.insert(lines, '## ' .. name)
            table.insert(lines, 'Prefix: `' .. snippet.prefix .. '`')
            table.insert(lines, 'Description: ' .. snippet.description)
            table.insert(lines, '')
            table.insert(lines, '```mq')

            for _, line in ipairs(snippet.body) do
                -- Replace placeholders with their value or just the value if no alternative is given
                local cleaned_line = line:gsub('%${%d+:([^}]+)}', '%1')
                table.insert(lines, cleaned_line)
            end

            table.insert(lines, '```')
            table.insert(lines, '')
        end

        -- Set buffer content and options
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
        vim.api.nvim_buf_set_option(buf, 'buftype', 'nofile')
        vim.api.nvim_buf_set_option(buf, 'bufhidden', 'wipe')
        vim.api.nvim_buf_set_option(buf, 'filetype', 'markdown')
        vim.api.nvim_buf_set_name(buf, 'mq-snippets')
        vim.api.nvim_buf_set_option(buf, 'modifiable', false)
    end, {
        desc = 'List available mq snippets'
    })

    -- Check health of mq plugin
    vim.api.nvim_create_user_command('MqCheckHealth', function()
        vim.cmd('checkhealth mq')
    end, {
        desc = 'Check health of mq plugin'
    })
end

-- Check if LSP binary is available
function M.check_lsp_binary()
    local binary = M.config.lsp_bin
    local cmd = string.format('which %s >/dev/null 2>&1', binary)
    local ret = os.execute(cmd)

    if ret ~= 0 then
        vim.notify(string.format('mq-lsp binary "%s" not found in PATH. LSP features will not work.', binary),
            vim.log.levels.WARN)
        return false
    end

    return true
end

-- Check if mq fmt command is available
function M.check_fmt_command()
    if vim.fn.executable('mq') ~= 1 then
        vim.notify('mq binary not found in PATH. Formatting features will not work.',
            vim.log.levels.WARN)
        return false
    end

    -- Run a simple test to check if fmt subcommand is available
    local handle = io.popen('mq help fmt 2>/dev/null')
    local result = handle and handle:read('*a')
    if handle then handle:close() end

    if not result or result == '' then
        vim.notify('mq fmt command not available. Formatting features will not work.',
            vim.log.levels.WARN)
        return false
    end

    return true
end

return M
