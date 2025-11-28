local M = {}
local utils = require("mq.utils")

-- Check if nvim-dap is available
local has_dap, dap = pcall(require, "dap")

-- Setup DAP adapter for mq
function M.setup()
  if not has_dap then
    utils.warn("nvim-dap not found. Please install nvim-dap to use debugging features.")
    return false
  end

  -- Find mq-dbg executable
  local mq_dbg_path = M.find_mq_dbg()
  if not mq_dbg_path then
    utils.warn(
      'mq-dbg not found in PATH or workspace. Install it using: cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger"'
    )
    return false
  end

  dap.adapters.mq = {
    type = "executable",
    command = mq_dbg_path,
    args = { "dap" },
  }

  dap.configurations.mq = {
    {
      type = "mq",
      request = "launch",
      name = "Debug mq file",
      queryFile = "${file}",
    },
  }

  utils.info("DAP adapter configured successfully")
  return true
end

function M.find_mq_dbg()
  -- Check environment variable for debug binary
  local debug_bin = vim.env._MQ_DBG_DEBUG_BIN
  if debug_bin and vim.fn.filereadable(debug_bin) == 1 then
    return debug_bin
  end

  -- Check workspace for local build
  local cwd = vim.fn.getcwd()
  local local_mq_dbg = cwd .. "/target/debug/mq-dbg"
  if vim.fn.filereadable(local_mq_dbg) == 1 then
    return local_mq_dbg
  end

  -- Check if mq-dbg is in PATH
  if vim.fn.executable("mq-dbg") == 1 then
    return "mq-dbg"
  end

  return nil
end

function M.debug_current_file()
  if not has_dap then
    utils.error("nvim-dap not found. Please install nvim-dap to use debugging features.")
    return
  end

  local bufnr = vim.api.nvim_get_current_buf()
  local filetype = vim.api.nvim_buf_get_option(bufnr, "filetype")

  if filetype ~= "mq" then
    utils.error("Current file is not an mq file")
    return
  end

  local filepath = vim.api.nvim_buf_get_name(bufnr)

  -- Check if file is saved
  if vim.api.nvim_buf_get_option(bufnr, "modified") then
    vim.cmd("write")
  end

  -- Select input file
  local files = utils.find_files({ "md", "mdx", "html", "csv", "tsv", "txt" })

  if #files == 0 then
    utils.error("No input files found in workspace")
    return
  end

  utils.select_file(files, "Select input file for debugging:", function(input_file)
    if not input_file then
      return
    end

    -- Start debugging with custom configuration
    dap.run({
      type = "mq",
      request = "launch",
      name = "Debug Current File",
      queryFile = filepath,
      inputFile = input_file,
    })
  end)
end

function M.install()
  if vim.fn.executable("cargo") ~= 1 then
    utils.error("cargo not found in PATH. Please install Rust toolchain.")
    return
  end

  utils.info("Installing mq-dbg...")

  local install_cmd =
  'cargo install --git https://github.com/harehare/mq.git mq-run --bin mq-dbg --features="debugger" --force'

  vim.fn.jobstart(install_cmd, {
    on_exit = function(_, exit_code)
      if exit_code == 0 then
        utils.info("mq-dbg installation completed successfully")
        -- Try to setup DAP again
        vim.defer_fn(function()
          M.setup()
        end, 500)
      else
        utils.error("mq-dbg installation failed with exit code: " .. exit_code)
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

return M
