local M = {}

-- Check if mq command is available
function M.is_mq_available()
  return vim.fn.executable("mq") == 1
end

-- Check if mq-dbg command is available
function M.is_mq_dbg_available()
  return vim.fn.executable("mq-dbg") == 1
end

-- Get workspace folders
function M.get_workspace_folders()
  local workspace_folders = {}

  -- Get current working directory
  local cwd = vim.fn.getcwd()
  table.insert(workspace_folders, cwd)

  return workspace_folders
end

-- Show error message
function M.error(msg)
  vim.notify("mq: " .. msg, vim.log.levels.ERROR)
end

-- Show info message
function M.info(msg)
  vim.notify("mq: " .. msg, vim.log.levels.INFO)
end

-- Show warning message
function M.warn(msg)
  vim.notify("mq: " .. msg, vim.log.levels.WARN)
end

-- Get selected text
function M.get_selected_text()
  local mode = vim.fn.mode()

  if mode ~= "v" and mode ~= "V" and mode ~= "\22" then
    M.error("No text selected")
    return nil
  end

  -- Get visual selection
  local _, start_row, start_col, _ = table.unpack(vim.fn.getpos("'<"))
  local _, end_row, end_col, _ = table.unpack(vim.fn.getpos("'>"))

  local lines = vim.api.nvim_buf_get_lines(0, start_row - 1, end_row, false)

  if #lines == 0 then
    return nil
  end

  -- Handle single line selection
  if #lines == 1 then
    lines[1] = string.sub(lines[1], start_col, end_col)
  else
    lines[1] = string.sub(lines[1], start_col)
    lines[#lines] = string.sub(lines[#lines], 1, end_col)
  end

  return table.concat(lines, "\n")
end

function M.find_files(extensions)
  local files = {}
  local cwd = vim.fn.getcwd()

  -- Try fd first (fastest)
  if vim.fn.executable("fd") == 1 then
    local cmd_parts = { "fd" }
    for _, ext in ipairs(extensions) do
      table.insert(cmd_parts, "-e")
      table.insert(cmd_parts, ext)
    end
    table.insert(cmd_parts, "--type")
    table.insert(cmd_parts, "f")
    table.insert(cmd_parts, "--absolute-path")
    table.insert(cmd_parts, ".")
    table.insert(cmd_parts, cwd)

    local cmd = table.concat(cmd_parts, " ")
    local output = vim.fn.systemlist(cmd)

    for _, file in ipairs(output) do
      if file ~= "" and not vim.startswith(file, "error:") then
        table.insert(files, file)
      end
    end

    return files
  end

  -- Try rg as fallback
  if vim.fn.executable("rg") == 1 then
    local cmd_parts = { "rg", "--files" }
    for _, ext in ipairs(extensions) do
      table.insert(cmd_parts, "-g")
      table.insert(cmd_parts, string.format("*.%s", ext))
    end

    local cmd = table.concat(cmd_parts, " ")
    local output = vim.fn.systemlist(cmd)

    for _, file in ipairs(output) do
      if file ~= "" then
        -- Make path absolute
        local abs_path = file
        if not vim.startswith(file, "/") then
          abs_path = cwd .. "/" .. file
        end
        table.insert(files, abs_path)
      end
    end

    return files
  end

  -- Fallback to globpath (slower)
  local pattern = "**/*.{" .. table.concat(extensions, ",") .. "}"
  local results = vim.fn.globpath(cwd, pattern, false, true)

  -- globpath returns a list when 4th arg is true
  if type(results) == "table" then
    for _, file in ipairs(results) do
      table.insert(files, file)
    end
  elseif type(results) == "string" and results ~= "" then
    -- Split by newline if it returned a string
    for file in vim.gsplit(results, "\n") do
      if file ~= "" then
        table.insert(files, file)
      end
    end
  end

  print("Debug - final files count: " .. #files)
  return files
end

function M.select_file(files, prompt, callback)
  if #files == 0 then
    M.info("No files found")
    return
  end

  -- Simple and reliable: use vim.ui.select
  local items = {}
  for _, file in ipairs(files) do
    local relative_path = vim.fn.fnamemodify(file, ":~:.")
    table.insert(items, relative_path)
  end

  vim.ui.select(items, {
    prompt = prompt,
    format_item = function(item)
      return item
    end,
  }, function(choice, idx)
    if choice and idx then
      callback(files[idx])
    end
  end)
end

-- Get input format from file extension
function M.get_input_format(filepath)
  local ext = vim.fn.fnamemodify(filepath, ":e")

  local format_map = {
    md = "markdown",
    mdx = "mdx",
    html = "html",
    txt = "text",
  }

  return format_map[ext] or "markdown"
end

-- Show content in new buffer
function M.show_in_buffer(content, filetype)
  -- Create a new buffer
  local buf = vim.api.nvim_create_buf(false, true)

  -- Set buffer content
  local lines = vim.split(content, "\n")
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)

  -- Set filetype
  if filetype then
    vim.api.nvim_buf_set_option(buf, "filetype", filetype)
  end

  -- Open buffer in new window
  vim.cmd("split")
  vim.api.nvim_win_set_buf(0, buf)
end

return M
