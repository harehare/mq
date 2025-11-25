-- Test script to verify mq.nvim commands are working
-- Run this with: nvim -u test-commands.lua

-- Add mq.nvim to runtimepath
local mq_path = vim.fn.expand("~/git/mq/editors/neovim")
vim.opt.runtimepath:append(mq_path)

-- Print function for debugging
local function test_print(msg)
  print("🔍 [TEST] " .. msg)
end

test_print("Loading mq.nvim...")

-- Method 1: Without calling setup()
test_print("Testing automatic initialization (without setup)...")

-- Wait for plugin to load
vim.defer_fn(function()
  -- Check if commands are registered
  test_print("Checking for commands...")

  local commands = vim.api.nvim_get_commands({})
  local mq_commands = {}

  for name, _ in pairs(commands) do
    if name:match("^Mq") then
      table.insert(mq_commands, name)
    end
  end

  if #mq_commands > 0 then
    test_print("✅ Found " .. #mq_commands .. " mq commands:")
    for _, cmd in ipairs(mq_commands) do
      test_print("   - " .. cmd)
    end
  else
    test_print("❌ No mq commands found!")
    test_print("Available commands:")
    for name, _ in pairs(commands) do
      if name:match("^[A-Z]") then
        print("   - " .. name)
      end
    end
  end

  -- Check if config is loaded
  test_print("\nChecking configuration...")
  local config = require("mq.config")
  local show_examples = config.get("show_examples")
  test_print("show_examples = " .. tostring(show_examples))

  -- Test MqNew command
  test_print("\nTesting :MqNew command...")
  vim.cmd("MqNew")

  local bufnr = vim.api.nvim_get_current_buf()
  local ft = vim.bo[bufnr].filetype
  test_print("New buffer filetype: " .. ft)

  if ft == "mq" then
    test_print("✅ :MqNew works correctly!")

    local lines = vim.api.nvim_buf_get_lines(bufnr, 0, 5, false)
    if #lines > 0 and lines[1]:match("^#") then
      test_print("✅ Examples are included in new file")
    else
      test_print("ℹ️  No examples in new file")
    end
  else
    test_print("❌ :MqNew did not create mq file correctly")
  end

  test_print("\n========================================")
  test_print("Test completed!")
  test_print("========================================")
  test_print("\nYou can now test other commands:")
  test_print("  :MqNew            - Create new mq file")
  test_print("  :MqStartLSP       - Start LSP server")
  test_print("  :MqStopLSP        - Stop LSP server")
  test_print("  :command Mq<Tab>  - List all mq commands")
end, 100)
