# AGENTS.md — MemStack 上手契约

> 项目根 = 仓库根 = `~/Projects/MemStack`。
> **工作区统一规范（红线）见 `~/Projects/Atlas/prompts/PROMPT-00-workspace-norms.md` 与 `~/Projects/Atlas/CONVENTIONS.md`，本文件只补项目特有的部分。**

> **⚠️ 每轮开工先读 `~/Projects/Atlas/brief.md`** —— 那是 Atlas 的当前轮次派发单，
> 读完这一个文件就知道本轮该做什么、不该做什么。没有你的任务就回报「无新增任务」，
> **不要自行找活干**。派发单里的任务**幂等**：已完成就核对并跳过，不要重做。

---

## 0. 30 秒认知

**MemStack（忆栈）** = 开源间隔重复记忆卡（SRS），面向**程序员**：背算法模板、命令、SQL、设计模式。Rust workspace，8 crate，约 10.5k 行。

| 项 | 值 |
|---|---|
| 分支 / 远端 | `master` ← `git@github.com:SyntaxEntity/MemStack.git`（SSH） |
| 版本 | v0.2.0（tag `v0.2.0`；上一版 v0.1.0 = `af80b56`） |
| 技术栈 | Rust 2021 · SQLite + sqlx/rusqlite · pulldown-cmark · ratatui · Tauri 2 · axum · fsrs 6.6.2 |
| 许可证 | **按 crate 分**：客户端 (core/storage/markdown/import/cli/desktop/sync) = MIT OR Apache-2.0；仅 `server` = AGPL-3.0-only |
| 文档 | `PROJECT.md`（现状与约定）· `docs/方案.md` · 旧 `HANDOFF.md` 已删除 |

**差异化四条**：① Markdown 即卡片本体，代码块/表格原生支持 ② FSRS 调度，复习记录可完整导出复算 ③ 单文件 SQLite，**导出永不设限** ④ 客户端免费无付费墙，同步可自建。

---

## 1. 🔴 待 Atlas / 用户决策的遗留问题

### 1.1 许可证误识别（D3）—— 已修复并验证

根目录三份 LICENSE 中 GitHub 曾自动选中 AGPL，但**客户端实为 MIT OR Apache-2.0**。这会让只想集成客户端的开发者顾虑"传染性"而放弃。

**已按规范 §7 处置并推送验证**：根目录 `LICENSE` = MIT 全文（主许可），保留 `LICENSE-AGPL` / `LICENSE-APACHE` / `LICENSE-MIT`，README 顶部有分许可边界说明。
复核结果：`gh repo view SyntaxEntity/MemStack --json licenseInfo` → `mit`。

### 1.2 Release（D4）—— v0.2.0 已建

`v0.2.0` 已创建 GitHub Release，附 **Linux x86_64** 产物（手工构建：CLI / server tarball + VSCode `.vsix` + `SHA256SUMS`），CLI 需 glibc ≥ 2.39。**后续 tag 由 `release.yml` 自动产出 Linux / macOS(arm64+x86_64) / Windows 四套产物**，不再手工打包。桌面端仍无安装包，只能源码运行。
版本节奏由 Atlas / 用户定，项目 agent 不自行决定。

### 1.3 仓库元信息（D2）—— 已处置

原 description 为 `[Vibe] MemStack - …类Anki记忆卡`，已按规范 §4.4 改写（去 `[Vibe]`、去「类 Anki」），并补 11 个 topics。`homepage` 仍为空——项目暂无官网。

### 1.4 导出功能（原 D7）—— CLI 已落地

`memstack export --format json|md` 已实现：JSON 含牌组参数 / 卡片调度状态 / `review_logs` 全量，可离线复算 FSRS；Markdown 每张笔记一个文件，可被 `memstack import` 再读入。**桌面端仍无导出入口。**

### 1.5 其他

- `gh` token 缺 `workflow` / `read:packages` scope → **只影响 `gh api` 写 `.github/workflows/`**；经 SSH 的普通 `git push` 改 workflow **不受此限制**（本轮已实测推上 `release.yml`）。
- `master` 无分支保护（待 Atlas 定协作模型）。
- 24 个提交曾被改写并 force push（既成事实）。**后续不再建议做此类操作**；确需改写先报 Atlas。
- GHCR 镜像已推送，但 token 缺 `read:packages`，无法核实包可见性。

---

## 2. 构建与验证

```bash
cargo test --workspace                                    # 应为 82 passed / 0 failed
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check

# 本地 vs 远程
git ls-remote origin && git status -sb

# CLI 端到端 smoke
cargo run -q --bin memstack -- --help
cargo run -q --bin memstack -- --db /tmp/memstack-smoke.sqlite add --deck 算法 qa --front f --back b
cargo run -q --bin memstack -- --db /tmp/memstack-smoke.sqlite export --format json | head -5
```

### 发布

**产物由 CI 从 tag 构建**（`.github/workflows/release.yml`），不要再本地手工打包上传。

```bash
git tag -a vX.Y.Z -m "…" && git push origin vX.Y.Z     # 触发构建 + 上传
gh workflow run release.yml -f tag=vX.Y.Z -f dry_run=true   # 只构建不传，验证用
```

**硬要求**：tag 版本必须同时等于 `Cargo.toml` 的 `version` 与 `editors/vscode/package.json` 的 `version`，否则流水线直接失败。这是为了杜绝「tag 与产物不同源」。

> 已知例外：`v0.2.0` 的 tag 早于版本统一提交，其扩展版本为 `0.1.0`，故对该 tag 重跑流水线会在 vsix 步骤失败——属预期，不必修。

---

## 3. 硬性约束

1. **`master` 直推**（单人维护期）。分支保护 / PR 模型待 Atlas 定。
2. **禁止 `git clean` / `git reset --hard` / `git checkout -- .`** —— 工作区可能有多 agent 改动。
3. **`.gitignore` 已覆盖** `*.sqlite` / `*.db` / `.env` / `target` —— **不要提交任何数据库文件**。本地库**不要放仓库根**：用 `--db /tmp/...` 或 `~/.memstack/...`（裁定 D-26 / S5）。
4. **无 `sudo`** —— 需提权的操作打印命令交用户。
5. **CI 变更**：改 workflow 走 SSH `git push` 即可（不受 token scope 限制）；只有需要 `gh api` 直写 workflow 时才要 `workflow` scope，当前无。

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
