# 忆栈 MemStack

![DeepSeek Assisted Development](https://img.shields.io/badge/DeepSeek%20Assisted-Development-brightgreen?style=for-the-badge&logo=deepseek)

开源间隔重复记忆卡。Markdown 是卡片本体，代码块、表格原生支持。FSRS 调度，数据在你自己手里。

**许可证**：客户端（core / storage / markdown / import / cli / desktop / sync）为 **MIT OR Apache-2.0**，任选其一；仅同步服务 `server` 为 **AGPL-3.0-only**。根目录 `LICENSE` 为 MIT 全文。详见[许可证](#许可证)。

- 仓库：<https://github.com/SyntaxEntity/MemStack>
- 设计说明：[docs/方案.md](docs/方案.md)
- 版本记录：[CHANGELOG.md](CHANGELOG.md)

## 功能

**造卡与复习**

- 三种题型：问答、填空（`{{c1::答案}}`）、代码填空（挖洞补全，与标准答案对照）
- 卡片模板：概念 / 命令 / 算法 / SQL / 填空 / 踩坑复盘
- FSRS 调度（与 Anki 同源），参数可看可调，复习记录完整落库
- 键盘复习：空格翻面，`1-4` 评分，`u` 撤销，每日新卡 / 复习上限
- 牌组、标签、搜索、暂停 / 归档

**界面**

- CLI + TUI（ratatui）：终端造卡、浏览、复习
- 桌面端（Tauri 2）：浏览 / 新建 / 编辑 / 跑测 / 统计
- 划词造卡：Neovim、VSCode 选中即成卡

**数据**

- 单文件 SQLite，备份 = 拷走文件；导出不设限；`review_logs` 只增不改
- 导入：Markdown 批量、CSV / TSV、Anki `.apkg`（牌组、标签、挖空、调度历史）
- 同步服务：账号、增量 push / pull、冲突合并；也可用坚果云 / Syncthing 直接同步库文件

**统计**

- 概况、近 30 天正确率、连续天数
- 复习热力图、到期预测曲线（CLI + 桌面）

## 依赖

| 项 | 要求 |
|---|---|
| Rust | 1.85+（[rustup](https://rustup.rs)） |
| CLI / TUI | 无额外系统库（SQLite 静态链接） |
| 桌面端 | Linux 需 `webkit2gtk4.1` 等 Tauri 2 依赖 |
| 同步服务 | Docker，或本机 Rust 编译 |

## 安装

### CLI

```bash
git clone https://github.com/SyntaxEntity/MemStack.git
cd MemStack
cargo install --path crates/cli
memstack --help
```

不安装直接跑：

```bash
cargo run -q --bin memstack -- --help
cargo run -q --bin memstack -- tui
```

### 桌面端

```bash
cargo run -q --bin memstack-desktop
```

### 同步服务

```bash
docker compose up -d
# 或
podman build -t memstack-server .
podman run -d -p 8080:8080 -v memstack-data:/data memstack-server
```

部署与 AGPL 义务见 [crates/server/README.md](crates/server/README.md)。

## 数据位置

| 场景 | 路径 |
|---|---|
| 默认库 | 当前目录 `memstack.sqlite`（各命令均认 `--db <路径>`） |
| 桌面端默认 | `~/.memstack/memstack.db` |
| 覆盖 | 环境变量 `MEMSTACK_DB` |

## 快速上手

```bash
# 造卡
memstack add --deck 算法 qa --front "二分左边界，循环不变式？" --back "区间 [l,r) 收敛"
memstack add --deck 地理 cloze --text "首都是{{c1::北京}}"
memstack add --deck 算法 code --language rust --code "let m = l + ({{c1::r - l}}) / 2;"

# 模板起手
memstack templates
memstack add --deck 算法 --template algo-pattern

# 导入
memstack import ./cards/ --dry-run
memstack import cards.csv
memstack import 牌组.apkg

# 复习 / 统计
memstack review --deck 算法
memstack tui
memstack stats
memstack stats --json --forecast-days 30

# 同步（自托管）
memstack sync register --url http://127.0.0.1:8080 --username you --password ...
memstack sync run
```

### 复习键位

| 键 | 作用 |
|---|---|
| `空格` / `Enter` | 翻面 |
| `1` `2` `3` `4` | Again / Hard / Good / Easy |
| `u` | 撤销上一次 |
| `q` | 退出 |

### 划词造卡

| 编辑器 | 入口 | 说明 |
|--------|------|------|
| Neovim | [editors/nvim](editors/nvim) | 视觉选择 `:MemstackQa` / `:MemstackCode` / `:MemstackCloze` |
| VSCode | [editors/vscode](editors/vscode) | `Ctrl+Alt+M Q/C/M` |

两者都走 `memstack add-json --deck … -`（stdin JSON），安装见各自 README。

## 卡片格式

### Markdown

```markdown
---
deck: 算法
tags: [leetcode, 二分]
type: qa | cloze | code   # 可省，自动识别
language: rust
---

正面内容（Markdown，可含代码块）

---

背面内容

***

第二张卡……
```

- 文件级 frontmatter 写在最前；`***` 单独成行 = 下一张卡
- 问答用 `---` 分正反面；填空 / 代码填空整段即正文，用 `{{c1::答案}}` 或 `{{c1::答案::提示}}` 挖空
- 也认 `正面：` / `背面：` 标注写法
- 目录导入递归 `*.md`，子目录名作默认牌组

### CSV / TSV

```csv
front,back,tags,deck
什么是二分？,有序数组上对半查找,"leetcode,二分",算法
```

- 有表头时列名认 `front/back/text/code/type/deck/tags/language/extra`（也认中文列名）
- 无表头按 `正面,背面[,标签]`（Anki 习惯）
- 题型自动：`code` 列 → 代码填空；`text`/`front` 含 `{{c1::` → 填空；否则问答
- `.tsv` 用制表符分隔；`--deck` 覆盖行内 `deck`

### 编辑器 JSON

```bash
echo '{"kind":"qa","front":"…","back":"…"}' | memstack add-json --deck 收件箱 -
```

`kind`：`qa` / `cloze` / `code`。字段与桌面端一致。

## 命令

```text
memstack add         造卡（qa / cloze / code，或 --template）
memstack add-json    JSON 造卡（编辑器集成，path=- 读 stdin）
memstack templates   列出 / 查看造卡模板
memstack import      导入 Markdown / CSV / .apkg
memstack tui         交互浏览与复习
memstack review      纯键盘复习
memstack decks       列出牌组
memstack stats       概况、热力图、到期预测（--json）
memstack sync        register / login / status / push / pull / run
memstack --help      全部参数
```

## 架构

```text
crates/core       领域模型、FSRS、挖空、卡片模板（不碰 IO）
crates/storage    SQLite 与迁移，统计查询
crates/markdown   渲染
crates/import     Markdown / CSV / apkg 导入
crates/cli        CLI + TUI + 同步 / 统计 / 模板命令
crates/desktop    Tauri 2 桌面端
crates/server     同步服务（AGPL-3.0-only）
crates/sync       客户端同步（UUID 映射 + push/pull）
editors/nvim      Neovim 划词造卡
editors/vscode    VSCode 划词造卡
frontend/         桌面端 web UI
Dockerfile        同步服务镜像（compose.yaml）
docs/             方案
```

技术栈：Rust workspace；调度 [fsrs](https://crates.io/crates/fsrs)；存储 SQLite + sqlx / rusqlite；Markdown pulldown-cmark；TUI ratatui；桌面 Tauri 2；同步 axum。

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

推代码前跑上面三条；CI 相同（[.github/workflows/ci.yml](.github/workflows/ci.yml)）。镜像发布见 [docker.yml](.github/workflows/docker.yml)。

## 许可证

| 部分 | 许可 |
|---|---|
| core / storage / markdown / import / cli / desktop / sync | **MIT OR Apache-2.0**（任选） |
| server | **AGPL-3.0-only** |

根目录 `LICENSE` 为面向用户的主许可（MIT）全文；`LICENSE-APACHE`、`LICENSE-AGPL` 对应其余部分；`LICENSE-MIT` 与 `LICENSE` 正文一致，保留以对应 SPDX 标识。自托管同步与 AGPL 义务见 [crates/server/README.md](crates/server/README.md)。

## DeepSeek Assisted Development

![DeepSeek Assisted Development](https://img.shields.io/badge/DeepSeek%20Assisted-Development-brightgreen?style=for-the-badge&logo=deepseek)

本仓库代码由 **DeepSeek** 辅助编写（vibecoding）。AI 生成的代码可能包含缺陷、过时用法或不完整实现，
请自行评估后使用与集成。
