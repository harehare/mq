-- Example configuration for mq.nvim
-- Add this to your Neovim configuration file (init.lua)

-- Basic setup with default configuration
require("mq").setup()

-- Advanced setup with custom configuration
require("mq").setup({
  -- Path to mq executable (if not in PATH)
  cmd = "mq",

  -- LSP server arguments
  lsp_args = { "lsp" },

  -- DAP server command
  dap_cmd = "mq-dbg",

  -- DAP server arguments
  dap_args = { "dap" },

  -- Show examples when creating new file
  show_examples = true,

  -- Automatically start LSP server
  auto_start_lsp = true,

  -- LSP server configuration
  lsp = {
    -- Custom on_attach function
    on_attach = function(client, bufnr)
      -- Set up keymaps for LSP features
      local opts = { buffer = bufnr, noremap = true, silent = true }

      vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
      vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
      vim.keymap.set("n", "gi", vim.lsp.buf.implementation, opts)
      vim.keymap.set("n", "<C-k>", vim.lsp.buf.signature_help, opts)
      vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)
      vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)
      vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)
      vim.keymap.set("n", "<leader>f", function()
        vim.lsp.buf.format({ async = true })
      end, opts)
    end,

    -- Custom capabilities (optional, for nvim-cmp integration)
    capabilities = (function()
      local has_cmp, cmp_nvim_lsp = pcall(require, "cmp_nvim_lsp")
      if has_cmp then
        return cmp_nvim_lsp.default_capabilities()
      end
      return vim.lsp.protocol.make_client_capabilities()
    end)(),

    -- LSP settings
    settings = {},
  },
})

-- Custom keymaps for mq commands
vim.api.nvim_create_autocmd("FileType", {
  pattern = "mq",
  callback = function()
    local opts = { buffer = true, noremap = true, silent = true }

    -- Run selected text
    vim.keymap.set("v", "<leader>mr", ":MqRunSelected<CR>", opts)

    -- Execute query
    vim.keymap.set("n", "<leader>mq", ":MqExecuteQuery<CR>", opts)

    -- Execute file
    vim.keymap.set("n", "<leader>mf", ":MqExecuteFile<CR>", opts)

    -- Debug
    vim.keymap.set("n", "<leader>md", ":MqDebug<CR>", opts)

    -- LSP commands
    vim.keymap.set("n", "<leader>ms", ":MqStartLSP<CR>", opts)
    vim.keymap.set("n", "<leader>mS", ":MqStopLSP<CR>", opts)
    vim.keymap.set("n", "<leader>mR", ":MqRestartLSP<CR>", opts)
  end,
})

-- Optional: Set up nvim-dap UI
local has_dap, dap = pcall(require, "dap")
if has_dap then
  -- DAP keymaps
  vim.keymap.set("n", "<F5>", dap.continue, { desc = "DAP: Continue" })
  vim.keymap.set("n", "<F10>", dap.step_over, { desc = "DAP: Step Over" })
  vim.keymap.set("n", "<F11>", dap.step_into, { desc = "DAP: Step Into" })
  vim.keymap.set("n", "<F12>", dap.step_out, { desc = "DAP: Step Out" })
  vim.keymap.set("n", "<leader>b", dap.toggle_breakpoint, { desc = "DAP: Toggle Breakpoint" })
  vim.keymap.set("n", "<leader>B", function()
    dap.set_breakpoint(vim.fn.input("Breakpoint condition: "))
  end, { desc = "DAP: Set Conditional Breakpoint" })

  -- Optional: Set up nvim-dap-ui
  local has_dapui, dapui = pcall(require, "dapui")
  if has_dapui then
    dapui.setup()

    -- Automatically open/close dap-ui
    dap.listeners.after.event_initialized["dapui_config"] = function()
      dapui.open()
    end
    dap.listeners.before.event_terminated["dapui_config"] = function()
      dapui.close()
    end
    dap.listeners.before.event_exited["dapui_config"] = function()
      dapui.close()
    end
  end
end
