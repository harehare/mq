#!/bin/bash
# mq.nvim のローカルテストセットアップスクリプト

set -e

echo "=== mq.nvim Local Test Setup ==="
echo ""

# 1. テスト用ディレクトリを作成
TEST_DIR="/tmp/mq-nvim-test"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "✓ Test directory created: $TEST_DIR"

# 2. テスト用の mq ファイルを作成
cat > test.mq << 'EOF'
# Test mq syntax highlighting and LSP

def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
    let first_char = upcase(first(word))
    | let rest_str = downcase(slice(word, 1, len(word)))
    | s"${first_char}${rest_str}";
  | join("");

# Extract code blocks
.code("js")

# Extract lists
.[]

# Extract headings
.h

# Select MDX
select(is_mdx())

# TODO: Add more tests
# FIXME: Test error handling
EOF

echo "✓ Test mq file created: test.mq"

# 3. テスト用の Markdown ファイルを作成
cat > test.md << 'EOF'
# Sample Markdown

This is a test markdown file for mq.

## Code Example

```js
console.log("Hello, world!");
```

## List

- Item 1
- Item 2
- Item 3

## Heading Levels

### Level 3
#### Level 4
EOF

echo "✓ Test markdown file created: test.md"

# 4. Neovim 設定ファイルを作成
cat > init.lua << 'EOF'
-- Minimal Neovim config for testing mq.nvim

-- Add mq.nvim to runtimepath
local mq_path = vim.fn.expand("~/git/mq/editors/neovim")
vim.opt.runtimepath:append(mq_path)

-- Setup mq.nvim
require("mq").setup({
  cmd = "mq",
  auto_start_lsp = true,
  show_examples = true,
  lsp = {
    on_attach = function(client, bufnr)
      print("mq LSP attached to buffer " .. bufnr)

      local opts = { buffer = bufnr, noremap = true, silent = true }

      -- LSP keymaps
      vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
      vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
      vim.keymap.set("n", "gi", vim.lsp.buf.implementation, opts)
      vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)
      vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)
      vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)
    end,
  },
})

-- mq-specific keymaps
vim.api.nvim_create_autocmd("FileType", {
  pattern = "mq",
  callback = function()
    local opts = { buffer = true, noremap = true, silent = true }

    -- mq commands
    vim.keymap.set("v", "<leader>mr", ":MqRunSelected<CR>", opts)
    vim.keymap.set("n", "<leader>mq", ":MqExecuteQuery<CR>", opts)
    vim.keymap.set("n", "<leader>mf", ":MqExecuteFile<CR>", opts)
    vim.keymap.set("n", "<leader>md", ":MqDebug<CR>", opts)
    vim.keymap.set("n", "<leader>ms", ":MqStartLSP<CR>", opts)
    vim.keymap.set("n", "<leader>mS", ":MqStopLSP<CR>", opts)
    vim.keymap.set("n", "<leader>mR", ":MqRestartLSP<CR>", opts)
  end,
})

-- Show startup message
print("mq.nvim test environment loaded!")
print("Available commands: :MqNew, :MqStartLSP, :MqExecuteQuery")
print("Keymaps: <leader>mr (run selected), <leader>mq (execute query)")
EOF

echo "✓ Neovim test config created: init.lua"

# 5. 使用方法を表示
cat << 'EOF'

=== Setup Complete! ===

To test mq.nvim locally, run:

    cd /tmp/mq-nvim-test
    nvim -u init.lua test.mq

Test checklist:

1. Syntax Highlighting:
   - Open test.mq and verify colors for keywords, functions, comments
   - Check that 'def snake_to_camel' has different colors
   - Verify TODO/FIXME are highlighted

2. LSP Features (if mq is installed):
   - Run :LspInfo to check LSP status
   - Run :MqStartLSP if not auto-started
   - Place cursor on a function and press 'K' for hover info
   - Press 'gd' to go to definition

3. Commands:
   - Run :MqNew to create a new mq file with examples
   - Run :command Mq<Tab> to see all available commands

4. Query Execution (if mq is installed):
   - Open test.mq
   - Select some text in visual mode (V)
   - Press <leader>mr to run selected query on test.md
   - Or run :MqExecuteQuery and enter '.[]'

5. File Type Detection:
   - Open any .mq file
   - Run :set filetype? and verify it shows 'filetype=mq'

Keymaps:
   <leader>mr  - Run selected text (visual mode)
   <leader>mq  - Execute query
   <leader>mf  - Execute mq file
   <leader>ms  - Start LSP
   <leader>mS  - Stop LSP
   <leader>mR  - Restart LSP

EOF

echo "Test directory: $TEST_DIR"
echo ""
echo "Run the following command to start testing:"
echo "  cd $TEST_DIR && nvim -u init.lua test.mq"
