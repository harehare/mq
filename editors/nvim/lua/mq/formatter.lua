-- Formatter module for mq.nvim
-- Uses mq fmt command to format mq queries

local M = {}

-- Format buffer using mq fmt command
function M.format_buffer()
    -- Check if mq is available
    if vim.fn.executable('mq') ~= 1 then
        vim.notify('mq binary not found in PATH. Formatting is not available.', vim.log.levels.ERROR)
        return false
    end

    -- Get current buffer content
    local bufnr = vim.api.nvim_get_current_buf()
    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, -1, false)
    local content = table.concat(lines, '\n')

    -- Get cursor position before formatting
    local cursor_pos = vim.api.nvim_win_get_cursor(0)

    -- Run mq fmt command
    local cmd = 'mq fmt'
    local job_id = vim.fn.jobstart(cmd, {
        stdin = 'pipe',
        stdout_buffered = true,
        stderr_buffered = true,
        on_stdout = function(_, data)
            if not data or #data <= 1 then
                return
            end

            -- Replace buffer content with formatted code
            vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, data)

            -- Restore cursor position
            vim.api.nvim_win_set_cursor(0, cursor_pos)

            -- Mark buffer as not modified if it was not modified before
            vim.api.nvim_buf_set_option(bufnr, 'modified', false)

            vim.notify('mq: Formatted buffer', vim.log.levels.INFO)
        end,
        on_stderr = function(_, data)
            if not data or #data <= 1 then
                return
            end

            local error_msg = table.concat(data, '\n')
            vim.notify('mq fmt error: ' .. error_msg, vim.log.levels.ERROR)
        end,
        on_exit = function(_, exit_code)
            if exit_code ~= 0 then
                vim.notify('mq fmt failed with exit code: ' .. exit_code, vim.log.levels.ERROR)
            end
        end,
    })

    -- Write content to stdin
    vim.fn.chansend(job_id, content)
    vim.fn.chanclose(job_id, 'stdin')

    return true
end

-- Format a string using mq fmt
function M.format_string(content)
    -- Check if mq is available
    if vim.fn.executable('mq') ~= 1 then
        return nil, 'mq binary not found in PATH'
    end

    local result = nil
    local error_msg = nil

    -- Run mq fmt command
    local cmd = 'mq fmt'
    local job_id = vim.fn.jobstart(cmd, {
        stdin = 'pipe',
        stdout_buffered = true,
        stderr_buffered = true,
        on_stdout = function(_, data)
            if data and #data > 1 then
                result = table.concat(data, '\n')
            end
        end,
        on_stderr = function(_, data)
            if data and #data > 1 then
                error_msg = table.concat(data, '\n')
            end
        end,
        on_exit = function(_, exit_code)
            if exit_code ~= 0 and not error_msg then
                error_msg = 'mq fmt failed with exit code: ' .. exit_code
            end
        end,
    })

    -- Write content to stdin
    vim.fn.chansend(job_id, content)
    vim.fn.chanclose(job_id, 'stdin')

    -- Wait for job to finish
    vim.fn.jobwait({ job_id }, -1)

    if error_msg then
        return nil, error_msg
    end

    return result
end

return M
