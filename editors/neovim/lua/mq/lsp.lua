local M = {}
local config = require("mq.config")
local utils = require("mq.utils")

local client_id = nil

-- Check if LSP client is running
function M.is_running()
  return client_id ~= nil
end

-- Start LSP server
function M.start()
  if M.is_running() then
    utils.info("LSP server is already running")
    return
  end

  local cmd = config.get("cmd")
  if not utils.is_mq_available() and cmd == "mq" then
    utils.error("mq command not found in PATH. Please install mq or configure mq.cmd")
    return
  end

  local workspace_folders = utils.get_workspace_folders()
  local multi_workspace_args = {}

  for _, folder in ipairs(workspace_folders) do
    table.insert(multi_workspace_args, "-M")
    table.insert(multi_workspace_args, folder)
  end

  local args = vim.list_extend(vim.deepcopy(config.get("lsp_args")), multi_workspace_args)

  local lsp_config = config.get("lsp")

  -- Build cmd array with executable and arguments
  local cmd_array = { cmd }
  vim.list_extend(cmd_array, args)

  local client_config = {
    name = "mq",
    cmd = cmd_array,
    filetypes = { "mq" },
    root_dir = vim.loop.cwd(),
    on_attach = lsp_config.on_attach,
    capabilities = lsp_config.capabilities,
    settings = lsp_config.settings,
  }

  -- Start the client
  client_id = vim.lsp.start_client(client_config)

  if not client_id then
    utils.error("Failed to start LSP server")
    return
  end

  -- Attach to current buffer if it's an mq file
  local bufnr = vim.api.nvim_get_current_buf()
  if vim.bo[bufnr].filetype == "mq" then
    vim.lsp.buf_attach_client(bufnr, client_id)
  end

  utils.info("LSP server started")
end

-- Stop LSP server
function M.stop()
  if not M.is_running() then
    utils.info("LSP server is not running")
    return
  end

  vim.lsp.stop_client(client_id)
  client_id = nil
  utils.info("LSP server stopped")
end

-- Restart LSP server
function M.restart()
  M.stop()
  vim.defer_fn(function()
    M.start()
  end, 100)
end

-- Execute mq command via LSP
function M.execute_command(command, script, input, input_format)
  if not M.is_running() then
    utils.error("LSP server is not running")
    return
  end

  local client = vim.lsp.get_client_by_id(client_id)
  if not client then
    utils.error("LSP client not found")
    return
  end

  local params = {
    command = command,
    arguments = { script, input, input_format },
  }

  client.request("workspace/executeCommand", params, function(err, result)
    if err then
      utils.error("Failed to execute command: " .. vim.inspect(err))
      return
    end

    if result then
      -- Show result in a new buffer
      utils.show_in_buffer(result, "markdown")

      -- Ask if user wants to copy to clipboard
      vim.ui.select({ "Yes", "No" }, {
        prompt = "Copy result to clipboard?",
      }, function(choice)
        if choice == "Yes" then
          vim.fn.setreg("+", result)
          utils.info("Result copied to clipboard")
        end
      end)
    else
      utils.error("No result from LSP server")
    end
  end, 0)
end

-- Setup LSP for mq buffers
function M.setup_buffer(bufnr)
  if not M.is_running() then
    return
  end

  vim.lsp.buf_attach_client(bufnr, client_id)
end

-- Auto-start LSP server when opening mq file
function M.setup_autostart()
  vim.api.nvim_create_autocmd("FileType", {
    pattern = "mq",
    callback = function(args)
      if config.get("auto_start_lsp") and not M.is_running() then
        M.start()
      elseif M.is_running() then
        M.setup_buffer(args.buf)
      end
    end,
  })
end

return M
