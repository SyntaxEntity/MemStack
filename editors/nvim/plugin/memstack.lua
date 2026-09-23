-- 忆栈 MemStack：划词造卡命令与键位。
-- 用法：把 editors/nvim 加进 runtimepath，或用 lazy.nvim 指向本目录。

if vim.g.loaded_memstack then
  return
end
vim.g.loaded_memstack = true

local ms = require("memstack")

vim.api.nvim_create_user_command("MemstackQa", function()
  ms.qa_from_selection()
end, { range = true, desc = "选区 → 问答卡" })

vim.api.nvim_create_user_command("MemstackCode", function()
  ms.code_from_selection()
end, { range = true, desc = "选区 → 代码填空" })

vim.api.nvim_create_user_command("MemstackCloze", function()
  ms.cloze_from_selection()
end, { range = true, desc = "选区 → 填空卡" })

vim.api.nvim_create_user_command("MemstackCodeBlock", function()
  ms.code_from_block()
end, { desc = "光标围栏代码块 → 代码填空" })

-- 默认键位（可关：vim.g.memstack_no_mappings = 1）
if not vim.g.memstack_no_mappings then
  vim.keymap.set("x", "<leader>mq", ms.qa_from_selection, { desc = "MemStack 问答卡" })
  vim.keymap.set("x", "<leader>mc", ms.code_from_selection, { desc = "MemStack 代码填空" })
  vim.keymap.set("x", "<leader>mm", ms.cloze_from_selection, { desc = "MemStack 填空卡" })
  vim.keymap.set("n", "<leader>mb", ms.code_from_block, { desc = "MemStack 代码块造卡" })
end
