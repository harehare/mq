local M = {}

-- Get default LSP capabilities
local function get_default_capabilities()
  local capabilities = vim.lsp.protocol.make_client_capabilities()

  -- If nvim-cmp is available, use its capabilities
  local has_cmp, cmp_lsp = pcall(require, "cmp_nvim_lsp")
  if has_cmp then
    capabilities = cmp_lsp.default_capabilities(capabilities)
  end

  return capabilities
end

-- Default configuration
M.defaults = {
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
    on_attach = nil, -- User-defined on_attach function
    capabilities = nil, -- User-defined capabilities (will use default if nil)
    settings = {},
  },
}

-- Current configuration
M.options = {}

-- Setup configuration
function M.setup(opts)
  M.options = vim.tbl_deep_extend("force", {}, M.defaults, opts or {})
end

-- Get configuration value (auto-initialize with defaults if not setup)
function M.get(key)
  -- Auto-initialize with defaults if not configured
  if vim.tbl_isempty(M.options) then
    M.setup(M.defaults)
  end

  if key then
    return M.options[key]
  end
  return M.options
end

-- Get LSP capabilities (use default if not configured)
function M.get_capabilities()
  local lsp_config = M.get("lsp")
  if lsp_config.capabilities then
    return lsp_config.capabilities
  end
  return get_default_capabilities()
end

return M
