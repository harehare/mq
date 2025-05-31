-- MQ LSP Configuration Module

local lspconfig = require('lspconfig')
local util = require('lspconfig.util')

local M = {}

-- Function to find the root directory for MQ projects
-- This is a basic implementation. It looks for a '.git' directory
-- or an 'mq_project.json' (hypothetical project file) or a 'package.json'
-- to determine the project root.
local function find_mq_root(startpath)
  return util.root_pattern('.git', 'mq_project.json', 'package.json')(startpath)
end

-- Main function to setup and start the mq-lsp client
function M.setup(options)
  options = options or {}

  local server_command = options.cmd or { "mq-lsp" } -- Default command to start the LSP server
  local capabilities = vim.lsp.protocol.make_client_capabilities()

  -- Enhance capabilities with completion, definition, hover, references, etc.
  -- You might want to use a capabilities object from a plugin like nvim-cmp
  -- for better completion integration in the future.
  capabilities.textDocument = {
    completion = {
      completionItem = {
        snippetSupport = true,
        resolveSupport = {
          properties = { "documentation", "detail" },
        },
        documentationFormat = { "markdown", "plaintext" },
      },
      contextSupport = true,
    },
    definition = {
      dynamicRegistration = true,
      linkSupport = true,
    },
    hover = {
      dynamicRegistration = true,
      contentFormat = { "markdown", "plaintext" },
    },
    references = {
      dynamicRegistration = true,
    },
    signatureHelp = {
      signatureInformation = {
        documentationFormat = { "markdown", "plaintext" },
        parameterInformation = {
          labelOffsetSupport = true,
        },
        activeParameterSupport = true,
      },
    },
    synchronization = {
      didSave = true,
      willSave = true,
      willSaveWaitUntil = true,
    },
    declaration = {linkSupport = true},
    implementation = {linkSupport = true},
    typeDefinition = {linkSupport = true},
    codeAction = {
        codeActionLiteralSupport = {
            codeActionKind = {
                valueSet = vim.tbl_flatten({
                    "",
                    "quickfix",
                    "refactor",
                    "refactor.extract",
                    "refactor.inline",
                    "refactor.rewrite",
                    "source",
                    "source.organizeImports",
                }),
            },
        },
    },
    documentSymbol = {
        symbolKind = {
            valueSet = vim.tbl_range(1, 26) -- All SymbolKind values
        },
        hierarchicalDocumentSymbolSupport = true,
    },
    formatting = {dynamicRegistration = true},
    rangeFormatting = {dynamicRegistration = true},
  }
  capabilities.workspace = {
    applyEdit = true,
    workspaceEdit = {
      documentChanges = true,
      resourceOperations = { "create", "rename", "delete" },
      failureHandling = "abort",
    },
    didChangeConfiguration = {
      dynamicRegistration = true,
    },
    symbol = {
       symbolKind = {
           valueSet = vim.tbl_range(1,26)
       }
    }
  }

  -- Setup the LSP client configuration
  lspconfig['mq_lsp'] = {
    default_config = {
      cmd = server_command,
      filetypes = { 'mq', 'markdown.mq', 'html.mq' }, -- Filetypes to activate for
      root_dir = find_mq_root,
      capabilities = capabilities,
      flags = {
        debounce_text_changes = 150,
      },
      -- Single file support can be enabled if the LSP server supports it
      -- and if you want to use it for standalone .mq files outside a project.
      -- single_file_support = true,

      -- You can add other LSP specific settings here, for example:
      -- settings = {
      --   mq_lsp = {
      --     setting1 = "value1",
      --     setting2 = true,
      --   }
      -- }
    },
    -- This function is called when the server attaches to a buffer
    on_attach = function(client, bufnr)
      -- Default on_attach function from lspconfig.
      -- You can customize this further.
      lspconfig.util.default_on_attach(client, bufnr)

      -- Example: Enable completion triggered by <c-x><c-o>
      -- vim.api.nvim_buf_set_option(bufnr, 'omnifunc', 'v:lua.vim.lsp.omnifunc')

      -- Example: Keymaps for LSP actions (requires Neovim 0.7+)
      -- These are just examples, you'll likely want to set these globally
      -- or in a more organized way.
      local map = function(mode, lhs, rhs, opts)
        opts = opts or {}
        opts.buffer = bufnr
        vim.keymap.set(mode, lhs, rhs, opts)
      end

      map('n', 'gd', vim.lsp.buf.definition, {desc = 'LSP: Go to Definition'})
      map('n', 'gr', vim.lsp.buf.references, {desc = 'LSP: Go to References'})
      map('n', 'gD', vim.lsp.buf.declaration, {desc = 'LSP: Go to Declaration'})
      map('n', 'gi', vim.lsp.buf.implementation, {desc = 'LSP: Go to Implementation'})
      map('n', 'K', vim.lsp.buf.hover, {desc = 'LSP: Hover Documentation'})
      map('n', '<leader>ls', vim.lsp.buf.signature_help, {desc = 'LSP: Signature Help'})
      map('n', '<leader>lr', vim.lsp.buf.rename, {desc = 'LSP: Rename'})
      map('n', '<leader>la', vim.lsp.buf.code_action, {desc = 'LSP: Code Action'})
      map('n', '<leader>lf', function() vim.lsp.buf.format { async = true } end, {desc = 'LSP: Format Document'})

      -- You might want to add more mappings for other LSP features.
      if client.supports_method("textDocument/documentHighlight") then
        vim.api.nvim_create_augroup("lsp_document_highlight", {clear = false})
        vim.api.nvim_clear_autocmds({buffer = bufnr, group = "lsp_document_highlight"})
        vim.api.nvim_create_autocmd({"CursorHold", "CursorHoldI"}, {
            buffer = bufnr,
            group = "lsp_document_highlight",
            callback = vim.lsp.buf.document_highlight,
            desc = "LSP: Document Highlight"
        })
        vim.api.nvim_create_autocmd("CursorMoved", {
            buffer = bufnr,
            group = "lsp_document_highlight",
            callback = vim.lsp.buf.clear_references,
            desc = "LSP: Clear Highlight"
        })
      end

      -- Inform the user that LSP has attached
      -- vim.notify("MQ LSP attached to buffer: " .. vim.api.nvim_buf_get_name(bufnr), vim.log.levels.INFO)
    end,
  }

  -- If options.on_attach is provided, use it instead of the default one defined above.
  if options.on_attach then
    lspconfig['mq_lsp'].on_attach = options.on_attach
  end

  -- If server specific settings are provided, merge them.
  if options.settings then
    lspconfig.mq_lsp.default_config.settings = vim.tbl_deep_extend('force', lspconfig.mq_lsp.default_config.settings or {}, options.settings)
  end

  -- If a custom command is passed, lspconfig will use it.
  -- Otherwise, it uses the cmd from default_config.
  -- The `lspconfig.mq_lsp.setup{}` call will typically be done
  -- in your main Neovim config, where you can pass the `cmd` if needed.
  -- For now, we are just defining the server configuration.
  -- The actual server starting will be like: `require('lspconfig').mq_lsp.setup({})`
  -- or `require('lspconfig').mq_lsp.setup{ cmd = {'custom-mq-lsp-path'} }`
end

-- This function will be called by lspconfig when it needs to start the server
-- for a buffer. We are defining how 'mq_lsp' should be configured.
-- The actual `lspconfig.mq_lsp.setup({})` call would typically be in your
-- main Neovim init.lua or a dedicated lsp setup file.
--
-- For the purpose of this module, we are defining the server configuration
-- and the M.setup function can be used to initialize this configuration.
--
-- To actually start the server, one would typically do:
-- require('lspconfig').mq_lsp.setup{
--   on_attach = custom_on_attach_function,
--   capabilities = my_capabilities,
--   -- Any other overrides
-- }
--
-- Our M.setup function essentially prepares this configuration.
-- Let's adjust it to be more conventional for an lspconfig module.

-- The standard way is that this module would be `require`d and its `setup`
-- function called by the user's main LSP configuration.
-- So, the module should return a table with the `setup` function.

-- Re-thinking the structure slightly for typical lspconfig custom server setup:
-- The module itself doesn't call lspconfig.servername.setup{}.
-- It provides the configuration table to be used by lspconfig.
-- However, the request is to "Define a function to start and configure".
-- So, the current M.setup is more like a wrapper that registers the config.

-- Let's make M.setup register the configuration with lspconfig.
-- The user would then call require('mq_lsp').setup() or require('mq_lsp').setup{cmd = {'/path/to/mq-lsp'}}

return M
