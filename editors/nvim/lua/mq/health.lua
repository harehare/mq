-- Health check module for mq.nvim
local M = {}

function M.check()
    -- Start the health check report
    vim.health.start('mq.nvim')

    -- Check Neovim version (requires 0.6.0+)
    local nvim_version = vim.version()
    local version_ok = nvim_version.major > 0 or (nvim_version.major == 0 and nvim_version.minor >= 6)

    if version_ok then
        vim.health.ok('Neovim version 0.6.0+ is installed')
    else
        vim.health.error('Neovim version 0.6.0+ is required, but you have ' ..
            vim.version().major .. '.' .. vim.version().minor .. '.' .. vim.version().patch)
    end

    -- Check if mq-lsp binary is available
    local mq_lsp_binary = 'mq-lsp' -- We can read from config if available
    local has_binary = vim.fn.executable(mq_lsp_binary) == 1

    if has_binary then
        local version = vim.fn.system(mq_lsp_binary .. ' --version')
        vim.health.ok('mq-lsp is installed: ' .. version:gsub('\n', ''))
    else
        vim.health.error('mq-lsp binary not found. Please ensure it is installed and available in your PATH')
        vim.health.info('Install mq-lsp with: `cargo install mq-lsp` or download from GitHub releases')
    end

    -- Check if nvim-lspconfig is available
    local has_lspconfig = pcall(require, 'lspconfig')

    if has_lspconfig then
        vim.health.ok('nvim-lspconfig is installed')
    else
        vim.health.warn('nvim-lspconfig is not installed. LSP features may be limited')
        vim.health.info('Install nvim-lspconfig for better LSP integration')
    end

    -- Check for mq executable (for running queries and formatting)
    local has_mq = vim.fn.executable('mq') == 1

    if has_mq then
        local version = vim.fn.system('mq --version')
        vim.health.ok('mq cli is installed: ' .. version:gsub('\n', ''))

        -- Check if mq fmt is available
        local has_fmt = false
        local handle = io.popen('mq help fmt 2>/dev/null')
        local result = handle and handle:read('*a')
        if handle then handle:close() end

        if result and result ~= '' then
            has_fmt = true
            vim.health.ok('mq fmt command is available')
        else
            vim.health.error('mq fmt command not found. Formatting will not work.')
            vim.health.info('Update mq to a version that includes the fmt command')
        end

        if not has_fmt then
            vim.health.warn('Formatting will be disabled due to missing mq fmt command')
        end
    else
        vim.health.warn('mq cli not found. :MqRun command will not work.')
        vim.health.info('Install mq with: `cargo install mq` or download from GitHub releases')
    end
end

return M
