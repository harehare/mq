local M = {}
local config = require("mq.config")
local lsp = require("mq.lsp")
local dap = require("mq.dap")
local utils = require("mq.utils")

local EXAMPLES = [[# To hide these examples, set show_examples to false in setup
# Extract js code
.code("js")

# Extract list
.[]

# Extract table
.[][]

# Extract MDX
select(is_mdx())

# Custom function
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
    let first_char = upcase(first(word))
    | let rest_str = downcase(slice(word, 1, len(word)))
    | s"${first_char}${rest_str}";
  | join("");
| snake_to_camel()

# Markdown Toc
.h
| let link = to_link("#" + to_text(self), to_text(self), "")
| let level = .h.depth
| if (!is_none(level)): to_md_list(link, to_number(level))

# CSV parse
include "csv" | csv_parse("a,b,c\n1,2,3\n4,5,6", true) | csv_to_markdown_table()
]]

-- Create new mq file
function M.new_file()
  local content = ""
  if config.get("show_examples") then
    content = EXAMPLES
  end

  -- Create new buffer
  vim.cmd("enew")
  local bufnr = vim.api.nvim_get_current_buf()

  -- Set filetype
  vim.api.nvim_buf_set_option(bufnr, "filetype", "mq")

  -- Set content
  local lines = vim.split(content, "\n")
  vim.api.nvim_buf_set_lines(bufnr, 0, -1, false, lines)
end

-- Start LSP server
function M.start_lsp()
  lsp.start()
end

-- Stop LSP server
function M.stop_lsp()
  lsp.stop()
end

-- Restart LSP server
function M.restart_lsp()
  lsp.restart()
end

-- Install mq servers
function M.install_servers()
  if vim.fn.executable("cargo") ~= 1 then
    utils.error("cargo not found in PATH. Please install Rust toolchain.")
    return
  end

  utils.info("Installing mq servers...")

  -- Stop LSP server if running
  if lsp.is_running() then
    lsp.stop()
  end

  -- Run installation command
  local install_cmd =
    "cargo install --git https://github.com/harehare/mq.git mq-run && "
    .. 'cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"'

  vim.fn.jobstart(install_cmd, {
    on_exit = function(_, exit_code)
      if exit_code == 0 then
        utils.info("Installation completed successfully")
        -- Auto-start LSP server
        vim.defer_fn(function()
          lsp.start()
        end, 500)
      else
        utils.error("Installation failed with exit code: " .. exit_code)
      end
    end,
    on_stdout = function(_, data)
      if data then
        for _, line in ipairs(data) do
          if line ~= "" then
            print(line)
          end
        end
      end
    end,
    on_stderr = function(_, data)
      if data then
        for _, line in ipairs(data) do
          if line ~= "" then
            print(line)
          end
        end
      end
    end,
  })
end

-- Run selected text as mq query
function M.run_selected_text()
  local selected = utils.get_selected_text()
  if not selected then
    return
  end

  -- Select input file
  local files = utils.find_files({ "md", "mdx", "html", "csv", "tsv", "txt" })

  utils.select_file(files, "Select input file:", function(input_file)
    if not input_file then
      return
    end

    -- Read file content
    local content = table.concat(vim.fn.readfile(input_file), "\n")
    local input_format = utils.get_input_format(input_file)

    -- Execute command
    lsp.execute_command("mq/run", selected, content, input_format)
  end)
end

-- Execute mq query on current file
function M.execute_query()
  local bufnr = vim.api.nvim_get_current_buf()
  local content = table.concat(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false), "\n")
  local filepath = vim.api.nvim_buf_get_name(bufnr)
  local input_format = utils.get_input_format(filepath)

  -- Prompt for query
  vim.ui.input({
    prompt = "Enter mq query: ",
    default = ".[]",
  }, function(query)
    if not query or query == "" then
      utils.error("No query entered")
      return
    end

    lsp.execute_command("mq/run", query, content, input_format)
  end)
end

-- Execute mq file on current file
function M.execute_file()
  local bufnr = vim.api.nvim_get_current_buf()
  local content = table.concat(vim.api.nvim_buf_get_lines(bufnr, 0, -1, false), "\n")
  local filepath = vim.api.nvim_buf_get_name(bufnr)
  local input_format = utils.get_input_format(filepath)

  -- Select mq file
  local mq_files = utils.find_files({ "mq" })

  utils.select_file(mq_files, "Select mq file to execute:", function(mq_file)
    if not mq_file then
      return
    end

    -- Read mq file content
    local query = table.concat(vim.fn.readfile(mq_file), "\n")

    lsp.execute_command("mq/run", query, content, input_format)
  end)
end

-- Debug current file
function M.debug_current_file()
  dap.debug_current_file()
end

-- Register all commands
function M.register_commands()
  -- Prevent duplicate registration
  if vim.g.mq_commands_registered then
    return
  end

  vim.api.nvim_create_user_command("MqNew", M.new_file, {
    desc = "Create new mq file",
  })

  vim.api.nvim_create_user_command("MqInstallServers", M.install_servers, {
    desc = "Install mq LSP and DAP servers",
  })

  vim.api.nvim_create_user_command("MqStartLSP", M.start_lsp, {
    desc = "Start mq LSP server",
  })

  vim.api.nvim_create_user_command("MqStopLSP", M.stop_lsp, {
    desc = "Stop mq LSP server",
  })

  vim.api.nvim_create_user_command("MqRestartLSP", M.restart_lsp, {
    desc = "Restart mq LSP server",
  })

  vim.api.nvim_create_user_command("MqRunSelected", M.run_selected_text, {
    desc = "Run selected text as mq query",
    range = true,
  })

  vim.api.nvim_create_user_command("MqExecuteQuery", M.execute_query, {
    desc = "Execute mq query on current file",
  })

  vim.api.nvim_create_user_command("MqExecuteFile", M.execute_file, {
    desc = "Execute mq file on current file",
  })

  vim.api.nvim_create_user_command("MqDebug", M.debug_current_file, {
    desc = "Debug current mq file",
  })

  vim.g.mq_commands_registered = true
end

return M
