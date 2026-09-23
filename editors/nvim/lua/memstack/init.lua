--- 忆栈 MemStack 划词造卡。
--- 视觉选择 → 造问答 / 代码填空 / 填空卡。依赖 `memstack` CLI。
local M = {}

M.config = {
  --- SQLite 路径；nil 则用 CLI 默认 memstack.sqlite
  db = nil,
  --- 默认牌组
  default_deck = "收件箱",
  --- 命令名
  bin = "memstack",
}

local function notify(msg, level)
  vim.notify("[memstack] " .. msg, level or vim.log.levels.INFO)
end

local function visual_lines()
  local s = vim.fn.getpos("'<")
  local e = vim.fn.getpos("'>")
  local srow, scol = s[2], s[3]
  local erow, ecol = e[2], e[3]
  if srow > erow or (srow == erow and scol > ecol) then
    srow, scol, erow, ecol = erow, ecol, srow, scol
  end
  local lines = vim.api.nvim_buf_get_text(0, srow - 1, scol - 1, erow - 1, ecol, {})
  return table.concat(lines, "\n")
end

local function ft_language(ft)
  local map = {
    rust = "rust",
    python = "python",
    go = "go",
    c = "c",
    cpp = "cpp",
    java = "java",
    javascript = "javascript",
    typescript = "typescript",
    sql = "sql",
    sh = "bash",
    bash = "bash",
    lua = "lua",
    vim = "vim",
    ruby = "ruby",
    toml = "toml",
    yaml = "yaml",
    json = "json",
  }
  return map[ft] or ft
end

local function ask_deck()
  local deck = vim.fn.input("牌组（空= " .. M.config.default_deck .. "）: ")
  if deck == "" then
    return M.config.default_deck
  end
  return deck
end

local function ask(prompt, default)
  local v = vim.fn.input(prompt, default or "")
  return v
end

local function encode_json(tbl)
  if vim.json and vim.json.encode then
    return vim.json.encode(tbl)
  end
  return vim.fn.json_encode(tbl)
end

local function submit(deck, tags, payload)
  local args = { M.config.bin, "add-json", "--deck", deck }
  if M.config.db then
    table.insert(args, 2, "--db")
    table.insert(args, 3, M.config.db)
  end
  if tags and #tags > 0 then
    table.insert(args, "--tags")
    table.insert(args, table.concat(tags, ","))
  end
  table.insert(args, "-")
  local json = encode_json(payload)
  local out = vim.fn.system(args, json)
  if vim.v.shell_error ~= 0 then
    notify("造卡失败：" .. out, vim.log.levels.ERROR)
    return false
  end
  notify(vim.trim(out))
  return true
end

local function tags_from_input()
  local raw = ask("标签（逗号分隔，可空）: ")
  local tags = {}
  for part in raw:gmatch("[^,、%s]+") do
    table.insert(tags, part)
  end
  return tags
end

--- 视觉选择 → 问答卡（选区=正面）
function M.qa_from_selection()
  local front = visual_lines()
  if front == "" then
    notify("先在视觉模式选一段", vim.log.levels.WARN)
    return
  end
  local back = ask("背面: ")
  if back == "" then
    notify("已取消", vim.log.levels.WARN)
    return
  end
  local deck = ask_deck()
  submit(deck, tags_from_input(), { kind = "qa", front = front, back = back })
end

--- 视觉选择 → 代码填空。把选区里某段原文挖成 {{c1::}}。
function M.code_from_selection()
  local code = visual_lines()
  if code == "" then
    notify("先选中代码", vim.log.levels.WARN)
    return
  end
  local language = ft_language(vim.bo.filetype)
  local needle = ask("选区中要挖空的原文（回车=整段）: ")
  local edited = code
  if needle ~= "" then
    local pat = vim.pesc(needle)
    local replaced, n = code:gsub(pat, "{{c1::" .. needle .. "}}", 1)
    if n == 0 then
      notify("选区里没有该片段", vim.log.levels.WARN)
      return
    end
    edited = replaced
  else
    edited = "{{c1::" .. code .. "}}"
  end
  local deck = ask_deck()
  submit(deck, tags_from_input(), {
    kind = "code",
    code = edited,
    language = language,
    extra = ask("补充（可空）: "),
  })
end

--- 视觉选择 → 填空卡。选中内容默认挖成 c1。
function M.cloze_from_selection()
  local text = visual_lines()
  if text == "" then
    notify("先选中一段", vim.log.levels.WARN)
    return
  end
  if not text:find("{{c") then
    text = "{{c1::" .. text .. "}}"
  end
  local extra = ask("补充（可空）: ")
  local deck = ask_deck()
  submit(deck, tags_from_input(), { kind = "cloze", text = text, extra = extra })
end

--- 当前行所在围栏代码块 → 代码填空
function M.code_from_block()
  local node_ok, parser = pcall(vim.treesitter.get_parser, 0)
  if not node_ok then
    return M.code_from_selection()
  end
  -- 退回：取整个 buffer 中光标所在 ``` 围栏
  local lines = vim.api.nvim_buf_get_lines(0, 0, -1, false)
  local row = vim.api.nvim_win_get_cursor(0)[1]
  local start_row, end_row
  for i = row, 1, -1 do
    if lines[i]:match("^```") then
      start_row = i
      break
    end
  end
  for i = row, #lines do
    if i > row and lines[i]:match("^```") then
      end_row = i
      break
    end
  end
  if not start_row or not end_row then
    notify("光标不在 ``` 代码块里", vim.log.levels.WARN)
    return
  end
  local code = table.concat(vim.list_slice(lines, start_row + 1, end_row - 1), "\n")
  local lang = lines[start_row]:match("^```(%S+)") or ft_language(vim.bo.filetype)
  local needle = ask("块中要挖空的原文（回车=整段）: ")
  local edited = code
  if needle ~= "" then
    local replaced, n = code:gsub(vim.pesc(needle), "{{c1::" .. needle .. "}}", 1)
    if n == 0 then
      notify("代码块里没有该片段", vim.log.levels.WARN)
      return
    end
    edited = replaced
  else
    edited = "{{c1::" .. code .. "}}"
  end
  local deck = ask_deck()
  submit(deck, tags_from_input(), {
    kind = "code",
    code = edited,
    language = lang,
    extra = ask("补充（可空）: "),
  })
end

function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
end

return M
