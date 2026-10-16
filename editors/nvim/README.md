# Neovim 划词造卡

视觉选中一段文字，变成忆栈卡片。依赖 `memstack` CLI 在 `PATH` 里。

## 安装

lazy.nvim：

```lua
{
  dir = "~/Projects/MemStack/editors/nvim",  -- 或克隆本仓库后的路径
  config = function()
    require("memstack").setup({
      -- db = vim.fn.expand("~/memstack.sqlite"),
      default_deck = "收件箱",
    })
  end,
}
```

或手动：

```vim
set runtimepath+=/path/to/memstack/editors/nvim
lua require("memstack").setup({})
```

## 用法

| 模式 | 命令 / 键 | 说明 |
|------|-----------|------|
| 视觉 | `:MemstackQa` / `<leader>mq` | 选区 → 正面，输入背面 → 问答卡 |
| 视觉 | `:MemstackCode` / `<leader>mc` | 选区 → 代码，可改成 `{{c1::}}` 挖空 |
| 视觉 | `:MemstackCloze` / `<leader>mm` | 选区整段挖成 `{{c1::…}}` |
| 普通 | `:MemstackCodeBlock` / `<leader>mb` | 光标所在 ``` 围栏 → 代码填空 |

流程：选区 → 提示补字段 / 牌组 / 标签 → 调 `memstack add-json` 落库。

关掉默认键位：`vim.g.memstack_no_mappings = 1`

## 配置

```lua
require("memstack").setup({
  db = "/path/to/memstack.sqlite",  -- 省略则用 CLI 默认
  default_deck = "收件箱",
  bin = "memstack",
})
```

## 底层协议

插件向 stdin 写 JSON，命令：

```bash
memstack --db PATH add-json --deck 牌组 --tags a,b -
```

```json
{"kind":"qa","front":"…","back":"…"}
{"kind":"code","code":"…","language":"rust","extra":"…"}
{"kind":"cloze","text":"…","extra":"…"}
```
