local M = {}

local MAX_SIZE = 20

local function history_file()
  local dir = vim.fn.stdpath("state") .. "/mq"
  vim.fn.mkdir(dir, "p")
  return dir .. "/query_history.json"
end

function M.load()
  local path = history_file()
  if vim.fn.filereadable(path) == 0 then
    return {}
  end

  local ok, decoded = pcall(vim.json.decode, table.concat(vim.fn.readfile(path), "\n"))
  if ok and type(decoded) == "table" then
    return decoded
  end
  return {}
end

function M.add(query)
  if not query or query == "" then
    return
  end

  local history = { query }
  for _, existing in ipairs(M.load()) do
    if existing ~= query then
      table.insert(history, existing)
    end
  end
  while #history > MAX_SIZE do
    table.remove(history)
  end

  vim.fn.writefile({ vim.json.encode(history) }, history_file())
end

return M
