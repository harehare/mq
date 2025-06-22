local M = {}

local function get_visual_selection()
  local s_start = vim.fn.getpos("'<")
  local s_end = vim.fn.getpos("'>")
  local n_lines = math.abs(s_end[2] - s_start[2]) + 1
  local lines = vim.api.nvim_buf_get_lines(0, s_start[2] - 1, s_end[2], false)
  if n_lines == 1 then
    lines[1] = string.sub(lines[1], s_start[3], s_end[3])
  else
    lines[1] = string.sub(lines[1], s_start[3])
    lines[n_lines] = string.sub(lines[n_lines], 1, s_end[3])
  end
  return table.concat(lines, "\n")
end

M.run_mq_script = function(script_content, input_content, display_mode)
  display_mode = display_mode or "split" -- 'split' or 'float'

  local mq_exe = vim.fn.exepath('mq')
  if mq_exe == '' then
    vim.notify('mq executable not found in PATH.', vim.log.levels.ERROR)
    return
  end

  local script_file = vim.fn.tempname()
  local input_file = vim.fn.tempname()
  local output_file = vim.fn.tempname() -- For stdout

  local script_h = io.open(script_file, "w")
  if not script_h then
    vim.notify("Error creating temp script file: " .. script_file, vim.log.levels.ERROR)
    return
  end
  script_h:write(script_content)
  script_h:close()

  local input_h = io.open(input_file, "w")
  if not input_h then
    vim.notify("Error creating temp input file: " .. input_file, vim.log.levels.ERROR)
    vim.fn.delete(script_file)
    return
  end
  input_h:write(input_content)
  input_h:close()

  local stderr_output = {}
  local command_str = string.format('%s run %s %s', mq_exe, script_file, input_file)

  -- Using vim.system to capture output and error more directly
  local cmd_with_redir = string.format('%s > %s', command_str, output_file)
  local job_id = vim.fn.jobstart(cmd_with_redir, {
    shell = vim.o.shell, -- ensure it uses a shell that understands redirection
    stderr_buffered = true, -- Buffer stderr
    on_stderr = function(_, data, _)
      if data then
        for _, line in ipairs(data) do
          if line ~= "" then table.insert(stderr_output, line) end
        end
      end
    end,
    on_exit = function(_, exit_code)
      vim.schedule(function() -- Ensure API calls are made on the main thread
        vim.fn.delete(script_file)
        vim.fn.delete(input_file)

        if exit_code ~= 0 then
          local err_msg = 'mq execution failed. Exit code: ' .. exit_code
          if #stderr_output > 0 then
            err_msg = err_msg .. '\nError:\n' .. table.concat(stderr_output, "\n")
          else
            -- If stderr was empty, try to read output_file for error messages
            -- as some tools might output errors to stdout on failure
            local f_err_check = io.open(output_file, "r")
            if f_err_check then
                local content = f_err_check:read("*a")
                f_err_check:close()
                if content and #content > 0 then
                    err_msg = err_msg .. "\nOutput (potentially error):\n" .. content
                end
            end
          end
          vim.notify(err_msg, vim.log.levels.ERROR)
          vim.fn.delete(output_file)
          return
        end

        local f = io.open(output_file, "r")
        if not f then
          vim.notify("Could not open mq output file: " .. output_file, vim.log.levels.ERROR)
          vim.fn.delete(output_file) -- still try to delete
          return
        end
        local result = f:read("*a")
        f:close()
        vim.fn.delete(output_file)

        if result == nil or result == "" then
            if #stderr_output > 0 then -- It might have succeeded but printed to stderr
                 result = "-- Standard output was empty. Standard error contained: --\n" .. table.concat(stderr_output, "\n")
            else
                result = "-- No output produced --"
            end
        end

        if display_mode == "float" then
          local width = math.floor(vim.o.columns * 0.8)
          local height = math.floor(vim.o.lines * 0.8)
          local row = math.floor((vim.o.lines - height) / 2)
          local col = math.floor((vim.o.columns - width) / 2)

          local buf = vim.api.nvim_create_buf(false, true)
          vim.api.nvim_buf_set_lines(buf, 0, -1, false, vim.fn.split(result, "\n"))
          vim.api.nvim_open_win(buf, true, {
            relative = 'editor',
            width = width,
            height = height,
            row = row,
            col = col,
            style = 'minimal',
            border = 'single',
          })
          vim.api.nvim_buf_set_option(buf, 'filetype', 'markdown')
        else -- split
          vim.cmd('vnew')
          vim.api.nvim_put(vim.fn.split(result, "\n"), 'c', true, true)
          vim.bo.filetype = 'markdown'
          vim.bo.buftype = 'nofile'
          vim.bo.bufhidden = 'wipe'
        end
      end)
    end,
  })

  if not job_id or job_id == 0 or job_id == -1 then
    vim.notify('Failed to start mq job.', vim.log.levels.ERROR)
    vim.schedule(function()
        vim.fn.delete(script_file)
        vim.fn.delete(input_file)
        vim.fn.delete(output_file) -- cleanup output file if job failed to start
    end)
  end
end

M.run_current_file = function()
  local current_buf_content = table.concat(vim.api.nvim_buf_get_lines(0, 0, -1, false), "\n")
  if vim.bo.filetype ~= 'mq' then
    vim.notify("Current file is not an mq file.", vim.log.levels.WARN)
    return
  end

  local markdown_files = {}
  local current_dir = vim.fn.getcwd()
  -- Using Lua for file iteration for better portability, albeit simpler
  local function scan_dir_lua(path, depth, max_depth)
    if depth > max_depth then return end
    local handle = io.popen('ls -a "' .. path .. '"') -- Still uses ls, but avoids complex find
    if handle then
        for item_name in handle:lines() do
            if item_name ~= "." and item_name ~= ".." then
                local full_item_path = path .. "/" .. item_name
                local ftype_handle = io.popen('stat -c %F "' .. full_item_path .. '"') -- POSIX specific stat
                if ftype_handle then
                    local ftype = ftype_handle:read("*a")
                    ftype_handle:close()
                    ftype = vim.fn.trim(ftype) -- trim whitespace

                    if ftype == "regular file" and vim.fn.fnmatch("*.md", item_name) == 1 then
                        table.insert(markdown_files, vim.fn.fnamemodify(full_item_path, ":."))
                    elseif ftype == "directory" then
                        scan_dir_lua(full_item_path, depth + 1, max_depth)
                    end
                end
            end
        end
        handle:close()
    end
  end
  -- Fallback to find if ls/stat popen fails or for more robustness initially
  local find_command = string.format('find "%s" -maxdepth 3 -name "*.md" -type f -print0', current_dir)
  local find_job = vim.fn.jobstart(find_command, {
    stdout_buffered = true,
    on_stdout = function(_, data, _)
        if data then
            local full_output = table.concat(data, "")
            local files_split = vim.split(full_output, "\0") -- Split by NUL char
            for _, file_path in ipairs(files_split) do
                if file_path ~= "" then
                    table.insert(markdown_files, vim.fn.fnamemodify(file_path, ":."))
                end
            end
        end
    end,
    on_exit = function()
        vim.schedule(function() -- Ensure UI interaction is on main thread
            if #markdown_files == 0 then
                vim.notify("No .md files found in the current directory or subdirectories (up to 3 levels deep).", vim.log.levels.WARN)
                return
            end

            vim.ui.select(markdown_files, {
                prompt = "Select a Markdown file to run against:",
                format_item = function(item) return item end,
            }, function(choice)
                if not choice then
                vim.notify("No Markdown file selected.", vim.log.levels.INFO)
                return
                end
                local md_file_path = vim.fn.expand(current_dir .. "/" .. choice)
                local f = io.open(md_file_path, "r")
                if not f then
                vim.notify("Could not open Markdown file: " .. md_file_path, vim.log.levels.ERROR)
                return
                end
                local md_content = f:read("*a")
                f:close()

                M.run_mq_script(current_buf_content, md_content)
            end)
        end)
    end
  })
  if not find_job or find_job <=0 then
     vim.notify("Failed to start find job to locate .md files.", vim.log.levels.ERROR)
  end
end

M.run_selected_text = function()
  local selected_text = get_visual_selection()
  if selected_text == "" then
    vim.notify("No text selected.", vim.log.levels.WARN)
    return
  end

  local markdown_files = {}
  local current_dir = vim.fn.getcwd()
  local find_command = string.format('find "%s" -maxdepth 3 -name "*.md" -type f -print0', current_dir)
  local find_job = vim.fn.jobstart(find_command, {
    stdout_buffered = true,
    on_stdout = function(_, data, _)
        if data then
            local full_output = table.concat(data, "")
            local files_split = vim.split(full_output, "\0")
            for _, file_path in ipairs(files_split) do
                 if file_path ~= "" then
                    table.insert(markdown_files, vim.fn.fnamemodify(file_path, ":."))
                end
            end
        end
    end,
    on_exit = function()
        vim.schedule(function()
            if #markdown_files == 0 then
                vim.notify("No .md files found in the current directory or subdirectories (up to 3 levels deep).", vim.log.levels.WARN)
                return
            end

            vim.ui.select(markdown_files, {
                prompt = "Select a Markdown file to run the selected mq code against:",
                format_item = function(item) return item end,
            }, function(choice)
                if not choice then
                vim.notify("No Markdown file selected.", vim.log.levels.INFO)
                return
                end
                local md_file_path = vim.fn.expand(current_dir .. "/" .. choice)
                local f = io.open(md_file_path, "r")
                if not f then
                vim.notify("Could not open Markdown file: " .. md_file_path, vim.log.levels.ERROR)
                return
                end
                local md_content = f:read("*a")
                f:close()

                M.run_mq_script(selected_text, md_content)
            end)
        end)
    end
  })
  if not find_job or find_job <=0 then
     vim.notify("Failed to start find job to locate .md files.", vim.log.levels.ERROR)
  end
end

return M
