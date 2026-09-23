# 忆栈 MemStack

开源间隔重复记忆卡。Markdown 是卡片本体，代码块、表格原生支持，不用装插件。FSRS 调度，数据在你自己手里。

原则就一条：功能说话，不写小作文。设计说明见 [docs/方案.md](docs/方案.md)。

## 能做什么

**造卡与复习**

- 三种题型：问答、填空（`{{c1::答案}}`）、代码填空（挖洞补全，和标准答案对照）
- 卡片模板：概念 / 命令 / 算法 / SQL / 填空 / 踩坑复盘，起手不用对着空白页
- FSRS 调度：Anki 同源算法，参数可看可调，复习记录完整落库
- 键盘复习：空格翻面，`1-4` 评分，`u` 撤销，每日新卡/复习上限

**界面**

- CLI + TUI（ratatui）：终端里造卡、浏览、复习
- 桌面端（Tauri 2）：浏览 / 新建 / 编辑 / 跑测 / 统计面板
- 划词造卡：Neovim、VSCode 选中即成卡

**数据**

- 单文件 SQLite，备份 = 拷走文件
- 导入：Markdown 批量卡片、CSV/TSV、Anki `.apkg`（牌组、标签、挖空、调度历史）
- 导出不设限；复习记录只增不改
- 同步服务（自建免费）：账号、增量 push/pull、冲突合并；或直接把库丢进坚果云 / Syncthing

**统计**

- 概况、近 30 天正确率、连续天数
- 复习热力图、到期预测曲线（CLI + 桌面）

## 安装

### 依赖

| 项 | 要求 |
|---|---|
| Rust | 1.85+（[rustup](https://rustup.rs)） |
| CLI / TUI | 无额外系统库（SQLite 静态链接） |
| 桌面端 | Linux 需 `webkit2gtk4.1` 等 Tauri 2 依赖 |
| 同步服务 | Docker，或本机 Rust 编译 |

### CLI

```bash
git clone https://gitcode.com/parolneatingebla/MemStack.git
cd MemStack
cargo install --path crates/cli
memstack --help
```

或不安装直接跑：

```bash
cargo run -q --bin memstack -- --help
cargo run -q --bin memstack -- tui
```

### 桌面端

```bash
cargo run -q --bin memstack-desktop
```

数据默认 `~/.memstack/memstack.db`，可用 `MEMSTACK_DB` 指定。

### 同步服务

```bash
docker compose up -d
# 或
podman build -t memstack-server .
podman run -d -p 8080:8080 -v memstack-data:/data memstack-server
```

部署、HTTPS、AGPL 义务见 [crates/server/README.md](crates/server/README.md)。

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
| Neovim | `editors/nvim` | 视觉选择 `:MemstackQa` / `:MemstackCode` / `:MemstackCloze` |
| VSCode | `editors/vscode` | `Ctrl+Alt+M Q/C/M` |

两者都走 `memstack add-json --deck … -`（stdin JSON），安装见各自 README。

## Markdown 卡片格式

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
- 问答用 `---` 分正反面；填空 / 代码填空整段就是正文，用 `{{c1::答案}}` 或 `{{c1::答案::提示}}` 挖空
- 也认 `正面：` / `背面：` 标注写法
- 目录导入时递归 `*.md`，子目录名作默认牌组

### CSV 格式

```csv
front,back,tags,deck
什么是二分？,有序数组上对半查找,"leetcode,二分",算法
```

- 有表头时列名认 `front/back/text/code/type/deck/tags/language/extra`（也认中文列名）
- 无表头按 `正面,背面[,标签]`（Anki 习惯）
- 题型自动：`code` 列 → 代码填空；`text`/`front` 含 `{{c1::` → 填空；否则问答
- `.tsv` 用制表符分隔。`--deck` 覆盖行内 `deck`

### 编辑器集成 JSON

```bash
echo '{"kind":"qa","front":"…","back":"…"}' | memstack add-json --deck 收件箱 -
```

`kind`：`qa` / `cloze` / `code`。字段与桌面端一致。

## 命令一览

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

所有命令都认 `--db <路径>`，默认当前目录的 `memstack.sqlite`。

## 开发

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

结构：

```text
crates/core       领域模型、FSRS、挖空、卡片模板（不碰 IO）
crates/storage    SQLite 与迁移，统计查询
crates/markdown   渲染
crates/import     Markdown / CSV / apkg 导入
crates/cli        CLI + TUI + 同步/统计/模板命令
crates/desktop    Tauri 2 桌面端（浏览/新建/编辑/跑测/统计）
crates/server     同步服务（AGPLv3，见其 README）
crates/sync       客户端同步（UUID 映射 + push/pull）
editors/nvim      Neovim 划词造卡
editors/vscode    VSCode 划词造卡
frontend/         桌面端 web UI
Dockerfile        同步服务镜像（compose.yaml）
docs/             方案
```

测试约 67 个。推代码前跑上面三条；CI 相同（[.github/workflows/ci.yml](.github/workflows/ci.yml)）。镜像发布见 [docker.yml](.github/workflows/docker.yml)。

## 许可证

- 客户端（core / storage / markdown / import / cli / desktop / sync）：**MIT OR Apache-2.0**，任选
- 同步服务 `server`：**AGPL-3.0-only**，防止套壳闭源 SaaS

详见 `LICENSE-MIT`、`LICENSE-APACHE`、`LICENSE-AGPL`。自托管同步与 AGPL 义务见 [crates/server/README.md](crates/server/README.md)。

## 钱的事

核心功能永远免费，导出永远不设限。收入靠官方托管同步的订阅；**自建同步免费，体验一样**。不设付费墙。
