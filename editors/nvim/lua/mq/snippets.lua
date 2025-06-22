-- LuaSnip compatible snippets
-- To use these, you'll need LuaSnip plugin and to load these snippets.
-- e.g., require("luasnip.loaders.from_lua").load({paths = {"path/to/this/directory_or_file"}})

local ls = require("luasnip")
local s = ls.snippet
local t = ls.text_node
local i = ls.insert_node
local f = ls.function_node

local M = {}

M.get_snippets = function()
  return {
    s("foreach", {
      t("foreach ("),
      i(1, "item"),
      t(", "),
      i(2, "values"),
      t("): "),
      i(0, "body"),
      t(";")
    }),
    s("while", {
      t("while ("),
      i(1, "condition"),
      t("): "),
      i(0, "body"),
      t(";")
    }),
    s("until", {
      t("until ("),
      i(1, "condition"),
      t("): "),
      i(0, "body"),
      t(";")
    }),
    s("def", {
      t("def "),
      i(1, "function_name"),
      t("("),
      i(2, "args"),
      t("): "),
      i(0, "body"),
      t(";")
    }),
    s("fn", {
      t("fn("),
      i(1, "args"),
      t("): "),
      i(0, "body"),
      t(";")
    }),
    s("let", {
      t("let "),
      i(1, "variable"),
      t(" = "),
      i(0, "value"),
      t(";")
    }),
    s("if", {
      t("if ("),
      i(1, "condition"),
      t("):"),
      i(0, "body"),
      t(";")
    }),
    s("ifelse", {
      t("if ("),
      i(1, "condition"),
      t("):"),
      i(2, "body"),
      t(" else: "),
      i(0, "other_body"),
      t(";")
    }),
    s("elif", {
      t("elif ("),
      i(1, "condition"),
      t("):"),
      i(0, "body"),
      t(";")
    }),
  }
end

return M

-- Example of how to load these snippets in your LuaSnip config:
-- require("luasnip.loaders.from_lua").load({paths = {"lua/mq/snippets.lua"}}) -- Adjust path as necessary
-- Or, more directly if this file is placed correctly in rtp:
-- require('luasnip').add_snippets("mq", require('mq.snippets').get_snippets())
