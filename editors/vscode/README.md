# VSCode 划词造卡

选中文字/代码，一键变成忆栈卡片。依赖 `memstack` CLI 在 `PATH` 里。

## 安装

开发加载：

1. 把本目录（`editors/vscode`）拷到 `~/.vscode/extensions/memstack-0.1.0`，或
2. `code --extensionDevelopmentPath=/path/to/memstack/editors/vscode`

打包安装：

```bash
cd editors/vscode
npx @vscode/vsce package   # 生成 .vsix
code --install-extension memstack-0.1.0.vsix
```

## 用法

| 命令 | 快捷键 | 说明 |
|------|--------|------|
| `MemStack: 选区 → 问答卡` | `Ctrl+Alt+M Q` | 选区=正面，输入背面 |
| `MemStack: 选区 → 代码填空` | `Ctrl+Alt+M C` | 指定片段挖成 `{{c1::}}` |
| `MemStack: 选区 → 填空卡` | `Ctrl+Alt+M M` | 选区整段挖空 |

命令面板搜 `MemStack` 即可。

流程：选区 → 补字段 / 牌组 / 标签 → `memstack add-json` 落库。

## 设置

| 键 | 默认 | 说明 |
|----|------|------|
| `memstack.db` | 空 | SQLite 路径，空则用 CLI 默认 |
| `memstack.defaultDeck` | `收件箱` | 默认牌组 |
| `memstack.bin` | `memstack` | 可执行文件名或路径 |

## 底层协议

与 Neovim 插件相同：

```bash
memstack --db PATH add-json --deck 牌组 --tags a,b -
```

stdin JSON：

```json
{"kind":"qa","front":"…","back":"…"}
{"kind":"code","code":"…","language":"rust","extra":"…"}
{"kind":"cloze","text":"…","extra":"…"}
```
