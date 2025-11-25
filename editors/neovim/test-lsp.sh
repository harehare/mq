#!/bin/bash
# mq LSP テストスクリプト

set -e

echo "=== mq LSP Test ==="
echo ""

# 1. mq コマンドの確認
echo "1. Checking mq command..."
if ! command -v mq &> /dev/null; then
    echo "❌ mq command not found in PATH"
    echo ""
    echo "Please install mq first:"
    echo "  cargo install --git https://github.com/harehare/mq.git mq-run"
    echo ""
    echo "Or use :MqInstallServers in Neovim"
    exit 1
fi

MQ_VERSION=$(mq --version 2>&1 || echo "unknown")
echo "✅ mq command found: $MQ_VERSION"
echo ""

# 2. mq lsp サブコマンドの確認
echo "2. Testing mq lsp subcommand..."
if ! mq --help 2>&1 | grep -q "lsp"; then
    echo "❌ mq lsp subcommand not available"
    echo "Please update mq to the latest version"
    exit 1
fi
echo "✅ mq lsp subcommand available"
echo ""

# 3. テストディレクトリの作成
TEST_DIR="/tmp/mq-lsp-test"
mkdir -p "$TEST_DIR"
cd "$TEST_DIR"

echo "3. Creating test files in $TEST_DIR"

# テスト用の .mq ファイルを作成
cat > test.mq << 'EOF'
# Test file for mq LSP

# Function definition
def snake_to_camel(x):
  let words = split(x, "_")
  | foreach (word, words):
    let first_char = upcase(first(word))
    | let rest_str = downcase(slice(word, 1, len(word)))
    | s"${first_char}${rest_str}";
  | join("");

# Test selectors
.code("js")
.[]
.h1

# Test built-in functions
upcase("hello")
split("a,b,c", ",")
select(is_mdx())

# Test conditionals
if true:
  "yes"
elif false:
  "no"
else:
  "maybe"
end
EOF

echo "✅ test.mq created"

# テスト用 Markdown ファイルを作成
cat > test.md << 'EOF'
# Test Markdown

```js
console.log("Hello");
```

- Item 1
- Item 2
EOF

echo "✅ test.md created"

# 4. Neovim 設定ファイルを作成
cat > init.lua << 'EOF'
-- Minimal config for LSP testing

-- Add mq.nvim to runtimepath
local mq_path = vim.fn.expand("~/git/mq/editors/neovim")
vim.opt.runtimepath:append(mq_path)

-- Setup mq.nvim with LSP
require("mq").setup({
  cmd = "mq",
  auto_start_lsp = true,
  lsp = {
    on_attach = function(client, bufnr)
      print("✅ mq LSP attached!")
      print("   Client: " .. client.name)
      print("   Buffer: " .. bufnr)
      print("   Server capabilities: " .. vim.inspect(client.server_capabilities))

      local opts = { buffer = bufnr, noremap = true, silent = true }

      -- LSP keymaps
      vim.keymap.set("n", "gd", vim.lsp.buf.definition, opts)
      vim.keymap.set("n", "K", vim.lsp.buf.hover, opts)
      vim.keymap.set("n", "gi", vim.lsp.buf.implementation, opts)
      vim.keymap.set("n", "<C-k>", vim.lsp.buf.signature_help, opts)
      vim.keymap.set("n", "<leader>rn", vim.lsp.buf.rename, opts)
      vim.keymap.set("n", "<leader>ca", vim.lsp.buf.code_action, opts)
      vim.keymap.set("n", "gr", vim.lsp.buf.references, opts)

      -- Diagnostics
      vim.keymap.set("n", "[d", vim.diagnostic.goto_prev, opts)
      vim.keymap.set("n", "]d", vim.diagnostic.goto_next, opts)
      vim.keymap.set("n", "<leader>e", vim.diagnostic.open_float, opts)
    end,
  },
})

-- Diagnostic configuration
vim.diagnostic.config({
  virtual_text = true,
  signs = true,
  update_in_insert = false,
})

-- Show startup instructions
vim.defer_fn(function()
  print("\n========================================")
  print("mq LSP Test Environment")
  print("========================================")
  print("\nTest checklist:")
  print("1. Run :LspInfo to check LSP status")
  print("2. Type 'upcase(' and check for completion")
  print("3. Place cursor on 'upcase' and press 'K' for hover")
  print("4. Press 'gd' on a function call to go to definition")
  print("5. Run :lua vim.lsp.buf.hover() manually")
  print("\nAvailable keymaps:")
  print("  K         - Hover documentation")
  print("  gd        - Go to definition")
  print("  gr        - References")
  print("  <leader>rn - Rename")
  print("  [d / ]d   - Previous/Next diagnostic")
  print("\nCommands:")
  print("  :LspInfo       - Show LSP status")
  print("  :MqStartLSP    - Start LSP server")
  print("  :MqStopLSP     - Stop LSP server")
  print("  :MqRestartLSP  - Restart LSP server")
  print("\nTo view LSP log:")
  print("  :lua vim.cmd('e ' .. vim.lsp.get_log_path())")
  print("========================================\n")
end, 100)
EOF

echo "✅ init.lua created"
echo ""

# 5. 使用方法を表示
cat << 'EOF'
=== Setup Complete! ===

To test mq LSP, run:

    cd /tmp/mq-lsp-test
    nvim -u init.lua test.mq

LSP Test Checklist:

1. Check LSP Status:
   :LspInfo

   Expected: You should see "Client: mq (id: 1, bufnr: [X])"

2. Test Completion:
   - Type 'up' and wait
   - You should see 'upcase' in completion list
   - Press Ctrl-n to cycle through completions

3. Test Hover:
   - Place cursor on 'upcase' function
   - Press 'K'
   - You should see documentation popup

4. Test Go to Definition:
   - Place cursor on 'snake_to_camel' call
   - Press 'gd'
   - Should jump to function definition

5. Test Diagnostics:
   - Add syntax error: "let x ="
   - You should see error highlighting
   - Run :lua vim.diagnostic.get() to see errors

6. View LSP Log:
   :lua vim.cmd('e ' .. vim.lsp.get_log_path())

Troubleshooting:

If LSP doesn't start:
  :MqStartLSP
  :messages

If you see errors:
  :lua vim.cmd('e ' .. vim.lsp.get_log_path())

Manual LSP test:
  $ mq lsp
  (Should start and wait for JSON-RPC input)

EOF

echo "Test directory: $TEST_DIR"
echo ""
echo "Run: cd $TEST_DIR && nvim -u init.lua test.mq"
