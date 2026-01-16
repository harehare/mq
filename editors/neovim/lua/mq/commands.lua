local M = {}
local lsp = require("mq.lsp")
local utils = require("mq.utils")
local dap = require("mq.dap")

function M.start_lsp()
  lsp.start()
end

function M.stop_lsp()
  lsp.stop()
end

function M.restart_lsp()
  lsp.restart()
end

function M.install()
  if vim.fn.executable("cargo") ~= 1 then
    utils.error("cargo not found in PATH. Please install Rust toolchain.")
    return
  end

  utils.info("Installing mq...")

  -- Stop LSP server if running
  if lsp.is_running() then
    lsp.stop()
  end

  -- Run installation command
  local install_cmd =
  "cargo install --git https://github.com/harehare/mq.git mq-lsp --force && cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features=\"debugger\" --force"

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

function M.debug_current_file()
  dap.debug_current_file()
end

function M.register_commands()
  -- Prevent duplicate registration
  if vim.g.mq_commands_registered then
    return
  end

  vim.api.nvim_create_user_command("MqInstall", M.install, {
    desc = "Install mq LSP server",
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

  vim.api.nvim_create_user_command("MqDebugFile", M.debug_current_file, {
    desc = "Debug current mq file",
  })

  vim.g.mq_commands_registered = true
end

return M
