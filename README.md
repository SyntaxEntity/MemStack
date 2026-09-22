# 忆栈 MemStack

开源间隔重复记忆卡。Markdown 是卡片本体，代码块、表格原生支持，不用装插件。FSRS 调度，数据在你自己手里。

原则就一条：功能说话，不写小作文。设计说明见 [docs/方案.md](docs/方案.md)。

## 能做什么

- **三种题型**：问答、填空（`{{c1::答案}}`）、代码填空（挖洞补全，和标准答案对照）
- **FSRS 调度**：Anki 同源算法，参数可看可调，复习记录完整落库
- **键盘复习**：空格翻面，`1-4` 评分，`u` 撤销，每日新卡/复习上限
- **导入**：Markdown 批量卡片、Anki `.apkg`（牌组、标签、挖空、调度历史）
- **存储**：单文件 SQLite，放进坚果云 / WebDAV / Syncthing 就能多端同步
- **终端优先**：CLI + TUI（ratatui），桌面壳在计划里

## 安装

### 依赖

| 项 | 要求 |
|---|---|
| Rust | 1.85+（[rustup](https://rustup.rs)） |
| 系统库 | 无。SQLite 已静态链接 |
| 终端 | 能跑 TUI 的终端即可（Linux / macOS / Windows） |

### 从源码安装（推荐）

```bash
git clone https://gitcode.com/parolneatingebla/MemStack.git
cd MemStack
cargo install --path crates/cli
```

装完后可执行文件是 `memstack`，确认：

```bash
memstack --help
```

### 不安装、直接跑

```bash
git clone https://gitcode.com/parolneatingebla/MemStack.git
cd MemStack
cargo run -p memstack-cli -- --help
cargo run -p memstack-cli -- tui
```

### 用 cargo 从仓库安装

```bash
cargo install --git https://gitcode.com/parolneatingebla/MemStack.git memstack-cli
```

### 升级 / 卸载

```bash
cd MemStack && git pull && cargo install --path crates/cli   # 覆盖安装即升级
cargo uninstall memstack-cli
```

数据就是一个 SQLite 文件（默认 `./memstack.sqlite`，可用 `--db` 指定）。备份 = 拷走这个文件。卸载程序不影响它。

## 快速上手

```bash
# 造一张问答卡
memstack add --deck 算法 qa --front "二分左边界模板，循环不变式？" --back "区间 [l,r) 收敛"

# 挖空卡
memstack add --deck 地理 cloze --text "首都是{{c1::北京}}，是{{c2::政治}}中心"

# 代码填空
memstack add --deck 算法 code --language rust --code "let m = l + ({{c1::r - l}}) / 2;"

# 从 Markdown / Anki 导入
memstack import ./cards/ --dry-run
memstack import ./cards/
memstack import 部.apkg

# 浏览（牌组 → 卡片 → 复习）
memstack tui

# 或直接开复习
memstack review --deck 算法
```

### 复习键位

| 键 | 作用 |
|---|---|
| `空格` / `Enter` | 翻面 |
| `1` `2` `3` `4` | Again / Hard / Good / Easy |
| `u` | 撤销上一次 |
| `q` | 退出 |

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

## 命令一览

```text
memstack add       造卡（qa / cloze / code）
memstack import    导入 Markdown 文件、目录或 .apkg
memstack tui       交互浏览与复习
memstack review    纯键盘复习（脚本 / 远程友好）
memstack decks     列出牌组
memstack stats     按状态统计卡片
memstack --help    全部参数
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
crates/core       领域模型、FSRS 调度，不碰 IO
crates/storage    SQLite 与迁移
crates/markdown   渲染
crates/import     Markdown / apkg 导入
crates/cli        CLI + TUI
crates/desktop    Tauri 壳（骨架）
crates/server     同步服务（骨架，AGPLv3）
docs/             方案
```

推代码前跑一遍上面三条即可。CI 做的事相同（见 [.github/workflows/ci.yml](.github/workflows/ci.yml)）。

## 许可证

- 客户端（core / storage / markdown / import / cli / desktop）：**MIT OR Apache-2.0**，任选
- 同步服务 `server`：**AGPL-3.0-only**，防止套壳闭源 SaaS

详见 `LICENSE-MIT`、`LICENSE-APACHE`、`LICENSE-AGPL`。
