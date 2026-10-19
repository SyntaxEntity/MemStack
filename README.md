# 忆栈 MemStack

![Rust](https://img.shields.io/badge/Rust-1.94%2B-blue?style=for-the-badge&logo=rust)
![license](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=for-the-badge)
![DeepSeek Assisted Development](https://img.shields.io/badge/DeepSeek%20Assisted-Development-brightgreen?style=for-the-badge&logo=deepseek)

开源间隔重复记忆卡（SRS），面向程序员：背算法模板、命令、SQL、设计模式。Markdown 即卡片本体，代码块与表格原生支持；FSRS 调度，数据存单文件 SQLite。

> 版本 `0.2.0` ｜ 状态：开发中（v0.2 已发布，方案第三版未开始） ｜ 已知限制见 [已知限制](#已知限制)
> 许可证：客户端 MIT OR Apache-2.0 ｜ 仅 `server` 为 AGPL-3.0-only（详见 [许可证](#许可证)）

## 功能

**造卡与复习**

- 三种题型：问答、填空（`{{c1::答案}}`）、代码填空（挖洞补全，与标准答案对照）
- 卡片模板：概念 / 命令 / 算法 / SQL / 填空 / 踩坑复盘
- FSRS 调度（与 Anki 同源），复习记录完整落库
- 键盘复习：空格翻面，`1-4` 评分，`u` 撤销，每日新卡 / 复习上限
- 牌组、标签

**界面**

- CLI + TUI（ratatui）：终端造卡、浏览、复习
- 桌面端（Tauri 2）：浏览 / 新建 / 编辑 / 复习 / 统计
- 划词造卡：Neovim、VSCode 选中即成卡

**数据**

- 单文件 SQLite，备份 = 拷走文件；数据无锁定，任何 SQLite 工具均可读写；`review_logs` 只增不改
- 导出：JSON 全量（牌组参数、卡片调度状态、复习记录，可离线复算 FSRS）/ Markdown（每张笔记一个文件，可再导入）
- 导入：Markdown 批量、CSV / TSV、Anki `.apkg`（牌组、标签、挖空、调度历史）
- 同步服务：账号、增量 push / pull、冲突合并；也可用坚果云 / Syncthing 直接同步库文件

**统计**

- 概况、近 30 天正确率、连续天数
- 复习热力图、到期预测曲线（CLI + 桌面）

## 安装

三种形态按需安装：CLI 只需 Rust 工具链，桌面端另需系统库，同步服务可用容器。

### 依赖

| 项 | 要求 |
|---|---|
| Rust | 1.94+（[rustup](https://rustup.rs)） |
| CLI / TUI | 无额外系统库（SQLite 静态链接） |
| 桌面端 | Linux 需 `webkit2gtk4.1` 等 Tauri 2 依赖 |
| 同步服务 | Docker / Podman，或本机 Rust 编译 |

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

## 使用

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

# 导出
memstack export --format json --out backup.json
memstack export --format md --out ./cards
memstack export --deck 算法 --format json

# 复习 / 统计
memstack review --deck 算法
memstack tui
memstack stats
memstack stats --json --forecast-days 30

# 同步（自托管）
memstack sync register --url http://127.0.0.1:8080 --username you --password 'your-password'
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

三种输入格式：Markdown 单文件 / 目录、CSV / TSV、编辑器 JSON。导入时自动识别题型。

### Markdown

```markdown
---
deck: 算法
tags: [leetcode, 二分]
type: qa            # 可省，自动识别：qa / cloze / code
language: rust
---

正面内容（Markdown，可含代码块）

---

背面内容

***

第二张卡（格式同上）
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
echo '{"kind":"qa","front":"二分左边界？","back":"区间 [l,r) 收敛"}' | memstack add-json --deck 收件箱 -
```

`kind`：`qa` / `cloze` / `code`。字段与桌面端一致。

## 命令

```text
memstack add         造卡（qa / cloze / code，或 --template）
memstack add-json    JSON 造卡（编辑器集成，path=- 读 stdin）
memstack templates   列出 / 查看造卡模板
memstack import      导入 Markdown / CSV / .apkg
memstack export      导出 JSON 全量 / Markdown（可再导入）
memstack tui         交互浏览与复习
memstack review      纯键盘复习
memstack decks       列出牌组
memstack stats       概况、热力图、到期预测（--json）
memstack sync        register / login / status / push / pull / run
memstack --help      全部参数
```

## 配置

数据文件位置：

| 场景 | 路径 |
|---|---|
| CLI / TUI 默认 | 当前目录 `memstack.sqlite`（各命令均认 `--db <路径>`） |
| 桌面端默认 | `~/.memstack/memstack.db` |
| 环境变量覆盖 | `MEMSTACK_DB` |

## 构建（开发）

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

推代码前跑上面三条；CI 相同（[.github/workflows/ci.yml](.github/workflows/ci.yml)）。镜像发布见 [.github/workflows/docker.yml](.github/workflows/docker.yml)。

## 项目结构

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

## 技术栈

Rust workspace（edition 2021，rust-version 1.94）；调度 [fsrs](https://crates.io/crates/fsrs) 6.6；存储 SQLite + sqlx / rusqlite；Markdown pulldown-cmark；TUI ratatui；桌面 Tauri 2；同步 axum。

## 已知限制

- **本地验证环境**：开发与手工验证仅在 Fedora 44（Wayland）上进行，其他 Linux 发行版未验证。
- **CI 覆盖范围**：ubuntu / macos / windows 三平台跑 fmt + clippy + test + CLI smoke（见 [ci.yml](.github/workflows/ci.yml)）；**桌面端只在 CI 中编译，未在三平台实际运行验证**。
- **未产出安装包**：`tauri.conf.json` 中 `bundle.active: false`，桌面端只能源码运行；图标仅 32×32 `icon.png`，正式打包需补完整尺寸套图。
- **无预编译二进制**：`v0.2.0` 已创建 GitHub Release，但**未附预构建产物**，只能从源码构建（`cargo install --path crates/cli`）。
- **导出入口只在 CLI**：`memstack export` 提供 JSON 全量（含调度状态与复习记录）与 Markdown（可再导入）；**桌面端尚无导出按钮**。
- **搜索与标签浏览缺口**：无全文搜索；标签可写入与导入，但暂无按标签筛选或浏览的界面。
- **暂停 / 归档缺口**：数据库有 `suspended` 字段并在 TUI 中显示，但**没有界面可以切换**；「归档」未实现。
- **FSRS 参数不可调**：目标留存率、最大间隔、每日上限等存于 `deck_params` 表，但**暂无 CLI / 桌面界面查看或修改**，实际使用内置默认值（每日新卡 20 / 复习 200）。复习记录完整落库，可自行导出复算。
- **徽章图片**：依赖 shields.io，离线环境不显示。

## 路线图

- **已完成**：v0.1.0 —— 造卡三题型、模板、FSRS 复习、CLI + TUI、桌面端、Markdown / CSV / apkg 导入、同步服务、统计、编辑器划词造卡。
- **已完成**：v0.2.0 —— `memstack export`（JSON 全量可复算 FSRS / Markdown 可再导入）。
- **下一步**：桌面端导出入口；补齐[已知限制](#已知限制)中剩余的功能缺口。
- **方案第三版（未开始）**：移动端、社区牌组、插件系统 —— 见 [docs/方案.md](docs/方案.md)。
- **明确不做**：桌面端改用 egui 的备选路线（富文本与公式排版弱），当前走 Tauri 2 + webview。

## 项目文档

| 文档 | 内容 |
|---|---|
| [docs/方案.md](docs/方案.md) | 设计叙事与版本规划 |
| [PROJECT.md](PROJECT.md) | 项目现状、设计约束、已知偏差与协作约定 |
| [AGENTS.md](AGENTS.md) | AI / 自动化协作者的上手契约 |
| [CHANGELOG.md](CHANGELOG.md) | 版本能力清单 |
| [crates/server/README.md](crates/server/README.md) | 同步服务部署与 AGPL 义务 |

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
