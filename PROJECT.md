# 忆栈 MemStack — 项目现状与协作

> 本文取代旧的 `HANDOFF.md`（已删除）。接手者、协作者、自动化 agent 从这一篇入门即可掌握项目全貌。
> 功能核对基准：commit `bd6b7b4`（`master`），核对日期 **2026-10-16**；其后仅有文档 / 许可证 / 披露类提交。文中数字均为实测值，非估计。

**文档分工**（避免重复与漂移）：

| 文档 | 用途 |
|---|---|
| [README.md](README.md) | 对外用法：安装、命令、卡片格式。只描述项目本身，不写商业模式 |
| [docs/方案.md](docs/方案.md) | 设计叙事与版本规划 |
| [CHANGELOG.md](CHANGELOG.md) | 版本能力清单 |
| **PROJECT.md（本文）** | **现状、必须遵守的约定、已知偏差、交接** |
| [crates/server/README.md](crates/server/README.md) | 同步服务部署与 AGPL 义务 |

---

## 1. 项目是什么

开源间隔重复记忆卡（SRS），面向程序员：背算法模板、命令、SQL、设计模式。

差异化：

- Markdown 即卡片本体，代码块 / 表格原生支持（不靠插件）
- FSRS 调度（与 Anki 同源），参数可看可调，复习记录可完整导出复算
- 单文件 SQLite，数据在用户手里，**导出永不设限**
- 客户端免费、无付费墙；同步可自建

---

## 2. 当前状态（实测）

| 项 | 状态 |
|---|---|
| 版本 | **v0.2.0**（tag `v0.2.0`）；上一版 v0.1.0 = `af80b56` |
| `master` | 领先 `v0.1.0` tag **19 个提交**（8 功能/CI + 6 文档/许可证 + 2 README/SSH + 2 导出 + 1 CI MSRV） |
| 远程 | `git@github.com:SyntaxEntity/MemStack.git`（SSH；public，默认分支 `master`） |
| 同步 | `origin/master` = 本地 `master`，已推送 |
| 贡献者 | 全部提交为 `SyntaxEntity <265272538+SyntaxEntity@users.noreply.github.com>` |
| CI | `CI`（fmt + 三平台 clippy/test/smoke + **MSRV 1.94 钉住 job**）与 `docker`（GHCR 发布）**最近运行均 success** |
| 测试 | **82 项全绿**：core 24 / import 25 / storage 8 / server 5 / cli 15 / sync 3 / desktop 2 |
| 规模 | 约 10.5k 行 Rust（8 个 crate） |
| GitHub Release | **v0.2.0**（已建，未附预编译产物） |

**v0.2.0 范围**（见 [CHANGELOG.md](CHANGELOG.md)）：在 v0.1.0 全部能力之上新增 `memstack export`（JSON 全量可复算 FSRS / Markdown 可再导入）。

[docs/方案.md](docs/方案.md) 的**第三版**（移动端、社区牌组、插件系统）未开始。

---

## 3. 架构

Rust workspace（resolver 2，edition 2021，rust-version 1.94）：

```text
crates/core       领域模型、FSRS、挖空、模板 —— 不碰 IO、不依赖 GUI
crates/storage    SQLite + sqlx，迁移，统计
crates/markdown   渲染（pulldown-cmark）
crates/import     Markdown / CSV / apkg
crates/cli        CLI + TUI（clap + ratatui）
crates/desktop    Tauri 2 桌面壳
crates/server     同步服务（AGPL-3.0-only）
crates/sync       客户端同步（UUID 映射 + push/pull）
editors/nvim      Neovim 划词
editors/vscode    VSCode 划词
frontend/         桌面 web UI（vanilla HTML/JS，经 tauri.conf 的 frontendDist 编译期嵌入）
Dockerfile        同步服务镜像 + compose.yaml
```

二进制名：`memstack` / `memstack-desktop` / `memstack-server`。

同步服务端点仅 5 个：`/health`、`/v1/auth/{register,login}`、`/v1/sync/{pull,push}`。

### 关键设计约束

- `NoteContent` 闭合枚举：`QuestionAnswer` / `Cloze` / `CodeCloze`
- Cloze `ordinal = cN - 1`（允许空洞：c1+c3 → 0,2），**禁止密排枚举**——会破坏编辑时的卡片匹配；`cards` 表以 `UNIQUE (note_id, ordinal)` 兜底
- `review_logs` **只增不改**；undo 是例外：同事务删掉刚写的那一行 + 用日志里的 `*_before` 快照恢复卡片
- 编辑笔记：按 ordinal 匹配保留调度状态，**不要 delete + reinsert**
- 同步身份是客户端 UUID（`sync_ids` 映射本地 i64），不是本地自增 ID
- 冲突策略：`review_log` 按 item_id 插入去重；其余 LWW（严格 `>`）；deck 按 name 合并
- TUI 用 `unicode-width` 算显示宽度，**禁用 `chars().count()`** 对齐 CJK

---

## 4. 必须遵守的约定

1. 文案 / 实现：**功能说话，不写小作文**
2. `core` 零 IO、零 GUI 依赖
3. 导出永不设限
4. 许可证分 crate：客户端 MIT OR Apache-2.0；`server` 仅 AGPL-3.0-only
5. Commit 风格：`type(scope): 中文短句——细节`
   例：`feat(import): CSV/TSV 导入——表头/无表头、自动题型、CLI 接入`
6. 优先级偏好：先跑通功能，再抛光 UI
7. 推代码前必须过第 5 节的三条检查（CI 同款）

---

## 5. 开发与验证

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

```bash
cargo run -q --bin memstack -- --help
cargo run -q --bin memstack -- --db smoke.sqlite add --deck 算法 qa --front f --back b
cargo run -q --bin memstack-desktop
cargo run -q --bin memstack-server
```

### 本机环境（Fedora 44）

| 项 | 说明 |
|---|---|
| 工具链 | rustc / cargo **1.97.1** |
| 容器 | **有 podman，无 docker** |
| 搜索 | **无 `rg`**，用 `grep` |
| 镜像构建 | `podman build --build-arg RUST_IMAGE=docker.m.daocloud.io/library/rust:1.97-bookworm ...` |

### SSH 推送

走 SSH 密钥（`~/.ssh/id_ed25519`，已在 `~/.ssh/config` 按 Host 绑定）。**不要**用 `-F /dev/null` 之类的写法绕开系统 ssh 配置——那会一并丢弃 `~/.ssh/config` 里的 ProxyJump 等设置。SSH 若出故障，上报 Atlas 统一处理，不要各自发明绕行方案（历史上三个项目各自绕行，正是踩坑的根源）。

### 数据位置

| 场景 | 路径 |
|---|---|
| CLI 默认库 | 当前目录 `memstack.sqlite`（各命令均认 `--db <路径>`） |
| 桌面端默认 | `~/.memstack/memstack.db` |
| 覆盖 | 环境变量 `MEMSTACK_DB` |

### CLI 安装

```bash
cargo install --path crates/cli    # → ~/.cargo/bin/memstack
```

---

## 6. 已知偏差与待办

按重要性排序。**这些都是核对出的实证问题，不是猜测。**

**本轮（2026-10-17）已处置**：① README 加 DeepSeek 辅助编写披露 ② 许可证布局按规范 §7 修正并验证为 MIT ③ `.gitignore` 收敛内部协作产物黑名单段 ④ SSH 故障确认消失，作废绕行写法 ⑤ README 按 18 节骨架重排，补 `## 已知限制` ⑥ **新增 `memstack export`（JSON / Markdown）**，补齐「导出永不设限」的 CLI 落地。

### 6.1 导出功能：CLI 已落地，桌面端仍缺

约定 3「导出永不设限」原先没有面向用户的落地路径，**2026-10-17 已在 CLI 侧补齐**：

- `memstack export --format json`：牌组参数 + 卡片调度状态 + `review_logs` 全量，可离线复算 FSRS
- `memstack export --format md`：每张笔记一个文件，可被 `memstack import` 再读入
- `--deck` 过滤；JSON 省略 `--out` 时写 stdout

**仍缺**：桌面端导出入口（20 余个 `tauri::command` 中仍无导出命令）。

### 6.2 许可证识别（已修复并验证）

GitHub `licenseInfo` 曾返回 `agpl-3.0`——根目录三份 LICENSE 中它被自动识别选中。实际是**按 crate 分许可**：仅 `server` 为 AGPL-3.0-only，其余为 MIT OR Apache-2.0。

**已按工作区规范 §7 处置并验证**：根目录新增 `LICENSE`（MIT 全文，面向用户的主许可），`LICENSE-APACHE` / `LICENSE-AGPL` 保留，README 顶部加显著的分许可边界说明。推送后复核：

```text
$ gh repo view SyntaxEntity/MemStack --json licenseInfo
{"licenseInfo":{"key":"mit","name":"MIT License","nickname":""}}
```

### 6.3 ~~无 GitHub Release~~（v0.2.0 已建）

`v0.2.0` 已创建 GitHub Release。**未附预编译产物**——CLI 仍需 `cargo install --path crates/cli`，桌面端需源码运行。

### 6.4 ~~版本落后于 `master`~~（随 v0.2.0 闭环）

v0.1.0 之后积压的全部改动（CI 修复、导出功能、文档与许可证修正、MSRV 提升）已并入 `v0.2.0`。

### 6.5 `docs/方案.md` 与技术栈不符

- 方案写桌面 GUI 为「Tauri 2 + **Leptos**」，实际是 vanilla HTML/JS（`frontend/`），本机也未装 `wasm32` / trunk
- 该文档的目录树漏了 `crates/sync`

### 6.6 桌面图标分辨率不足

`crates/desktop/icons/` 仅 `icon.png`（**32×32**，104 B）与派生的 `icon.ico`。`tauri.conf.json` 中 `bundle.active: false`，故当前不影响 CI；正式打包 bundle 前需补完整尺寸套图。

### 6.7 SSH 系统配置故障（已修复，留档）

2026-10-16 前后，`/etc/ssh/ssh_config.d/*.conf` 属主为 `nobody:nobody`，OpenSSH 因权限不安全拒绝加载整个配置目录，导致全部 git 远程操作失败：

```text
Bad owner or permissions on /etc/ssh/ssh_config.d/20-systemd-ssh-proxy.conf
```

当时各项目的绕行写法是 `GIT_SSH_COMMAND='ssh -F /dev/null …'`。

**2026-10-17 复核：该故障已消失**，配置目录属主恢复 `root:root`。**绕行写法已作废，不要再用**（`-F /dev/null` 会丢弃 `~/.ssh/config`）。若故障复现，上报 Atlas 统一处理。

### 6.8 历史曾 force push

24 个提交的作者被改写并 force push，tag `v0.1.0` 一并更新。已推送到远程的旧 SHA 不可再作为协作基线；若存在旧克隆需重新 fetch。

---

## 7. 建议下一步

按 [docs/方案.md](docs/方案.md) 的第三版：**移动端 → 社区牌组 → 插件系统**。

工程侧优先级建议（与第 6 节对应）：

1. ~~导出产品化（6.1）~~ —— CLI 已落地（JSON / Markdown）；剩桌面端入口
2. 补 `v0.1.1` tag + GitHub Release（6.3 / 6.4）
3. ~~许可证展示修正（6.2）~~ —— 已完成，已验证 GitHub 识别为 MIT
4. ~~`docs/方案.md` 对齐实现（6.5）~~ —— 已完成
5. 正式打包所需的图标套图（6.6）

---

## 8. 易踩坑

| 坑 | 正确做法 |
|---|---|
| 多 bin workspace | `cargo run -q --bin memstack -- …` |
| `add` 的 `--tags` | 挂在 `add` 上，不在 `qa/cloze/code` 子命令 |
| apkg 导入 | 保留原调度 + revlog，勿从 note 重新 generate |
| Cloze 序号 | `cN-1` 可空洞，禁止 `enumerate()` |
| sqlx | runtime 查询 + `include_str!`；不用 `migrate!` / 动态 SQL |
| fsrs crate | 不管分钟级 learning steps，放在 `deck_params` |
| TUI 对齐 | `unicode-width`；`Margin` 在 `ratatui::layout` |
| Wayland 截图 | portal `gdbus … Screenshot`，不要 x11grab |
| 同步测试 crate | `use memstack_sync::…`，不是 `crate::` |
| SSH 故障 | 不要用 `-F /dev/null` 绕开系统配置；上报 Atlas 统一处理（见 6.7） |

---

## 9. 一句话现状

> **v0.2.0 已发布并托管于 GitHub（SyntaxEntity/MemStack），CI 绿、作者已归一、82 项测试全绿。代码结构清晰、约束明确。下一步是方案第三版（移动 / 社区 / 插件），或补上「桌面端导出入口」等已知缺口。动手前先跑 `cargo test --workspace` 确认环境。**
