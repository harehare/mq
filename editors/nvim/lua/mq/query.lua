-- Query runner module for mq.nvim
local M = {}

-- Run an mq query against a markdown file
function M.run_query(query_path, markdown_path, opts)
    opts = opts or {}
    local output_format = opts.format or 'markdown'
    local split = opts.split or 'new'

    -- Validate inputs
    if not query_path or query_path == '' then
        vim.notify('Query path is required', vim.log.levels.ERROR)
        return false
    end

    if not markdown_path or markdown_path == '' then
        vim.notify('Markdown path is required', vim.log.levels.ERROR)
        return false
    end

    -- Check if mq is available
    if vim.fn.executable('mq') ~= 1 then
        vim.notify('mq binary not found in PATH. Please install mq.', vim.log.levels.ERROR)
        return false
    end

    -- Construct the command
    local cmd = string.format('mq %s %s', query_path, markdown_path)
    if output_format ~= 'markdown' then
        cmd = cmd .. ' --format ' .. output_format
    end

    -- Execute the command
    local output = vim.fn.systemlist(cmd)

    -- Handle errors
    if vim.v.shell_error ~= 0 then
        vim.notify('Error running mq query: ' .. table.concat(output, '\n'), vim.log.levels.ERROR)
        return false
    end

    -- Create output buffer
    if split == 'current' then
        vim.api.nvim_buf_set_lines(0, 0, -1, false, output)
        vim.api.nvim_buf_set_option(0, 'modified', false)
    else
        -- Create a new split and put the output there
        vim.cmd(split)
        local buf = vim.api.nvim_get_current_buf()
        vim.api.nvim_buf_set_lines(buf, 0, -1, false, output)
        vim.api.nvim_buf_set_option(buf, 'buftype', 'nofile')
        vim.api.nvim_buf_set_option(buf, 'bufhidden', 'wipe')
        vim.api.nvim_buf_set_option(buf, 'filetype', output_format == 'markdown' and 'markdown' or 'text')
        vim.api.nvim_buf_set_name(buf, string.format('mq: %s ➤ %s',
            vim.fn.fnamemodify(query_path, ':t'),
            vim.fn.fnamemodify(markdown_path, ':t')))
    end

    return true
end

-- Run the active buffer as an mq query against a markdown file
function M.run_buffer_query(markdown_path, opts)
    local query_content = vim.api.nvim_buf_get_lines(0, 0, -1, false)

    -- Skip if buffer is empty
    if #query_content == 0 then
        vim.notify('Current buffer is empty', vim.log.levels.ERROR)
        return false
    end

    -- Create a temporary file for the query
    local temp_file = vim.fn.tempname() .. '.mq'
    local f = io.open(temp_file, 'w')
    if not f then
        vim.notify('Failed to create temporary file', vim.log.levels.ERROR)
        return false
    end

    f:write(table.concat(query_content, '\n'))
    f:close()

    -- Run the query
    local success = M.run_query(temp_file, markdown_path, opts)

    -- Clean up temporary file
    os.remove(temp_file)

    return success
end

-- Select a markdown file to run a query against
function M.select_markdown_file(callback)
    -- Get all markdown files in the current directory
    local dir = vim.fn.expand('%:p:h')
    local files = vim.fn.glob(dir .. '/*.md', true, true)

    if #files == 0 then
        vim.notify('No markdown files found in the current directory', vim.log.levels.WARN)
        return
    end

    -- Add relative paths to the files
    for i, file in ipairs(files) do
        files[i] = vim.fn.fnamemodify(file, ':t')
    end

    -- Select a file using vim.ui.select
    vim.ui.select(files, {
        prompt = 'Select a markdown file to run query against:',
        format_item = function(item)
            return item
        end,
    }, function(selected)
        if selected then
            callback(dir .. '/' .. selected)
        end
    end)
end

return M
