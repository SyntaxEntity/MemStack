# AGENTS.md — MemStack 上手契约

> 项目根 = 仓库根 = `~/Projects/MemStack`。
> **工作区统一规范（红线）见 `~/Projects/Atlas/prompts/PROMPT-00-workspace-norms.md` 与 `~/Projects/Atlas/CONVENTIONS.md`，本文件只补项目特有的部分。**

---

## 0. 30 秒认知

**MemStack（忆栈）** = 开源间隔重复记忆卡（SRS），面向**程序员**：背算法模板、命令、SQL、设计模式。Rust workspace，8 crate，约 10.5k 行。

| 项 | 值 |
|---|---|
| 分支 / 远端 | `master` ← `git@github.com:SyntaxEntity/MemStack.git`（SSH） |
| 版本 | v0.1.0（tag `af80b56`） |
| 技术栈 | Rust 2021 · SQLite + sqlx/rusqlite · pulldown-cmark · ratatui · Tauri 2 · axum · fsrs 6.6.2 |
| 许可证 | **按 crate 分**：客户端 (core/storage/markdown/import/cli/desktop/sync) = MIT OR Apache-2.0；仅 `server` = AGPL-3.0-only |
| 文档 | `PROJECT.md`（现状与约定）· `docs/方案.md` · 旧 `HANDOFF.md` 已删除 |

**差异化四条**：① Markdown 即卡片本体，代码块/表格原生支持 ② FSRS 调度，复习记录可完整导出复算 ③ 单文件 SQLite，**导出永不设限** ④ 客户端免费无付费墙，同步可自建。

---

## 1. 🔴 待 Atlas / 用户决策的遗留问题

### 1.1 许可证被 GitHub 误识别为 AGPL-3.0（D3）—— 本地已修，待推送复核

根目录三份 LICENSE 中 GitHub 自动选中了 AGPL，但**客户端实为 MIT OR Apache-2.0**。这会让只想集成客户端的开发者顾虑"传染性"而放弃。

**已按规范 §7 处置**：根目录 `LICENSE` = MIT 全文（主许可），保留 `LICENSE-AGPL` / `LICENSE-APACHE` / `LICENSE-MIT`，README 顶部加显著分许可边界说明。
`gh repo view --json licenseInfo` **只在推送后才更新**，故该复核项仍待推送后进行。

### 1.2 无 Release，`master` 领先 tag 8 个提交（D4）

tag 存在但未建 Release，用户拿不到二进制。且用户在 v0.1.0 上仍会遇到 CI 修复前的 Tauri 依赖问题。
**版本节奏由 Atlas / 用户定，项目 agent 不自行决定。**

### 1.3 仓库元信息（D2）

description 陈旧偏题：`[Vibe] MemStack - 忆栈记忆卡：支持添加和显示代码块的类Anki记忆卡`
→ 去掉 `[Vibe]` 前缀（规范 §4.4）、去掉与差异化叙事冲突的「类 Anki」自述，并补 topics / homepage。

### 1.4 产品级缺口：「导出永不设限」无落地功能（D7）

这是**对外承诺与实现之间的实质缺口**，不是猜测：CLI `--help` 无 `export` 子命令；桌面端 20 余个 `tauri::command` 中无导出命令；全仓 `export` 仅以 `memstack_sync::export_items` 存在（同步协议内部函数）。

底层数据导出能力已具备，缺的是 CLI 入口 + 桌面入口（建议至少 Markdown / JSON + `review_logs` 全量，以便用户复算 FSRS）。**成本低、收益高、与核心理念一致。**

### 1.5 其他

- `gh` token 缺 `workflow` / `read:packages` scope → 需用户补。
- `master` 无分支保护、无 topics / homepage。
- 24 个提交曾被改写并 force push（既成事实）。**后续不再建议做此类操作**；确需改写先报 Atlas。
- GHCR 镜像已推送，但 token 缺 `read:packages`，无法核实包可见性。

---

## 2. 构建与验证

```bash
cargo test --workspace                                    # 应为 72 passed / 0 failed
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# 本地 vs 远程
git ls-remote origin && git status -sb

# CLI 端到端 smoke
cargo run -q --bin memstack -- --help
cargo run -q --bin memstack -- --db smoke.sqlite add --deck 算法 qa --front f --back b
```

---

## 3. 硬性约束

1. **`master` 直推**（单人维护期）。分支保护 / PR 模型待 Atlas 定。
2. **禁止 `git clean` / `git reset --hard` / `git checkout -- .`** —— 工作区可能有多 agent 改动。
3. **`.gitignore` 已覆盖** `*.sqlite` / `*.db` / `.env` / `target` —— **不要提交任何数据库文件**（`memstack.sqlite` 在仓库根，属本地数据）。
4. **无 `sudo`** —— 需提权的操作打印命令交用户。
5. **CI 变更走 PR 或先报 Atlas** —— workflow 文件需要 `workflow` scope，当前无。

---

## 4. 文档地图

| 文件 | 什么时候读 |
|---|---|
| `README.md` | 项目定位、功能、安装 |
| `PROJECT.md` | 项目现状与约定 |
| `docs/方案.md` | 完整方案与路线图 |

---

## 5. 汇报

跨项目事务（推送、发版、仓库元信息、许可证布局、需用户提权、跨项目约定）→ 上报 **Atlas**。
报告写到 `~/Projects/Atlas/reports/inbox/`，命名 `AGENT-REPORT-MemStack-<YYYYMMDD>-<NN>.md`。
**不要把任何文件写到 `~/Projects/` 根目录。**
