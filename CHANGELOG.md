# 更新日志

## v0.2.0 — 2026-10-20

### 数据
- **导出**：`memstack export` —— JSON 全量（牌组参数、卡片调度状态、复习记录，可离线复算 FSRS）/ Markdown（每张笔记一个文件，可被 `import` 再读入）；支持 `--deck` 过滤

### 构建
- MSRV 由 `1.85` 修正为 **`1.94`**（实测：`sqlx 0.9` 硬要求 rustc 1.94；1.93 编译失败、1.94 通过）
- CI 增加 MSRV 钉住 job，防止声明随依赖升级漂移

### 文档
- README 按工作区骨架重排，补「已知限制」；修正此前关于搜索 / 暂停归档 / FSRS 参数可调的失实描述
- 根目录 `LICENSE` 置 MIT 主许可，修复 GitHub 将本项目识别为 AGPL-3.0 的问题

## v0.1.0 — 2026-09-24

首个版本：能造卡、能复习、能导入、能同步。

### 造卡与复习
- 三种题型：问答、填空（`{{c1::答案}}`）、代码填空（挖洞补全）
- 卡片模板：概念 / 命令 / 算法 / SQL / 填空 / 踩坑复盘
- FSRS 调度（Anki 同源），参数可调，复习记录完整落库
- 键盘复习：空格翻面，`1-4` 评分，`u` 撤销，每日上限

### 界面
- CLI + TUI（ratatui）
- 桌面端（Tauri 2）：浏览 / 新建 / 编辑 / 跑测 / 统计面板
- 划词造卡：Neovim（`:MemstackQa` 等）、VSCode（`Ctrl+Alt+M Q/C/M`）

### 数据
- 单文件 SQLite；导出不设限；`review_logs` 只增不改
- 导入：Markdown 批量、CSV/TSV、Anki `.apkg`（含调度与 revlog）
- 同步服务（AGPLv3）：账号、增量 push/pull、冲突合并（LWW + 日志只增）
- `memstack sync register|login|push|pull|run`，Docker 一键部署

### 统计
- 概况、近 30 天正确率、连续天数
- 复习热力图、到期预测（CLI `stats` 与桌面「统计」）

### 安装

```bash
cargo install --path crates/cli
cargo run -q --bin memstack-desktop
docker compose up -d   # 同步服务
```
