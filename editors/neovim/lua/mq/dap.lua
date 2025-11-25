local M = {}
local config = require("mq.config")
local utils = require("mq.utils")

-- Setup DAP configuration for mq
function M.setup()
  local dap_ok, dap = pcall(require, "dap")
  if not dap_ok then
    utils.warn("nvim-dap is not installed. DAP support will not be available.")
    return false
  end

  local dap_cmd = config.get("dap_cmd")
  if not utils.is_mq_dbg_available() and dap_cmd == "mq-dbg" then
    utils.warn("mq-dbg command not found in PATH. Debugger will not be available.")
    return false
  end

  -- Configure mq adapter
  dap.adapters.mq = {
    type = "executable",
    command = dap_cmd,
    args = config.get("dap_args"),
  }

  -- Configure mq configurations
  dap.configurations.mq = {
    {
      type = "mq",
      request = "launch",
      name = "Debug mq file",
      queryFile = "${file}",
      inputFile = function()
        -- Prompt user to select input file
        local files = utils.find_files({ "md", "mdx", "html", "csv", "tsv", "txt" })
        local selected_file = nil

        utils.select_file(files, "Select input file:", function(file)
          selected_file = file
        end)

        return selected_file
      end,
    },
  }

  return true
end

-- Debug current file
function M.debug_current_file()
  local dap_ok, dap = pcall(require, "dap")
  if not dap_ok then
    utils.error("nvim-dap is not installed")
    return
  end

  local bufnr = vim.api.nvim_get_current_buf()
  local filetype = vim.bo[bufnr].filetype

  if filetype ~= "mq" then
    utils.error("Current file is not an mq file")
    return
  end

  -- Check if file is saved
  if vim.bo[bufnr].modified then
    utils.warn("File has unsaved changes. Saving...")
    vim.cmd("write")
  end

  local query_file = vim.api.nvim_buf_get_name(bufnr)

  -- Prompt for input file
  local files = utils.find_files({ "md", "mdx", "html", "csv", "tsv", "txt" })

  utils.select_file(files, "Select input file for debugging:", function(input_file)
    if not input_file then
      return
    end

    local debug_config = {
      type = "mq",
      request = "launch",
      name = "Debug Current File",
      queryFile = query_file,
      inputFile = input_file,
    }

    dap.run(debug_config)
  end)
end

return M
