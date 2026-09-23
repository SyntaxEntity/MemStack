# memstack-server

忆栈同步服务。账号、多端增量同步、冲突处理。**自建免费，体验和官方托管一致。**

> **当前状态**：v1 已实现——账号（注册/登录/Bearer 令牌）、增量同步（pull/push）、冲突合并（`review_log` 只增去重，其余 LWW）。客户端对接与 Docker 镜像待做。下面是部署目标形态。

## 许可证：AGPL-3.0-only

本 crate 单独用 **GNU AGPL v3**，和客户端不一样。

| 部分 | 许可 |
|---|---|
| core / storage / markdown / import / cli / desktop | MIT OR Apache-2.0 |
| **server（本目录）** | **AGPL-3.0-only** |

完整正文见仓库根目录 [LICENSE-AGPL](../../LICENSE-AGPL)。

### 为什么服务端是 AGPL

客户端随便嵌、随便商用，Rust 生态惯例，没顾虑。

服务端不一样：同步服务是跑在网络上给人用的。若只用 MIT/Apache，把代码改一改挂个域名就能开闭源 SaaS，不用把改动开源。AGPL 的网络条款堵掉这条路：

- **改了 server 并向别人提供服务**（哪怕没分发二进制），必须向使用者提供对应源码，含你的修改
- 自己机房自己用、不对外提供服务，没有额外义务
- 只是拿客户端连自己的服务器，不影响客户端的 MIT/Apache

一句话：自建随便用，套壳闭源 SaaS 不行。这不是技术封锁——代码就在这，谁都拿得走——是让「拿它开收费服务的人」也把改动亮出来。

### 集成前请确认

如果你的产品要内置或托管本同步服务，且**不打算按 AGPL 开源服务端**，请先换协议或另写服务端，不要直接拿这套代码上线。合规问题请找律师，本文不是法律意见。

## 自托管部署

目标：一条命令起服务，数据在自己的盘上。

### 你需要

- Docker（或兼容的容器运行时）
- 一台能暴露 80/443 的机器（家里 NAS、VPS 都行）
- 域名 + TLS 证书（有 HTTPS 才安全；测试可先裸跑）
- 客户端版本 ≥ 支持同步的那版（第一版 CLI/TUI 走本地 SQLite，用不上本服务）

### 数据在哪

同步服务自己的库（账号、同步元数据）挂在数据卷里。用户笔记本体是各自的 SQLite；服务只做增量同步与冲突处理，见方案「复习记录只增不改，最坏情况也不丢数据」。

备份 = 备份数据卷。删容器不删卷，数据还在。

### 部署（服务就绪后）

#### Docker 一条命令

```bash
docker run -d --name memstack-server \
  --restart unless-stopped \
  -p 8080:8080 \
  -v memstack-data:/data \
  -e MEMSTACK_DATA=/data \
  -e MEMSTACK_BIND=0.0.0.0:8080 \
  ghcr.io/memstack/memstack-server:latest
```

或 docker compose：

```yaml
# compose.yaml
services:
  memstack:
    image: ghcr.io/memstack/memstack-server:latest
    restart: unless-stopped
    ports:
      - "8080:8080"
    volumes:
      - memstack-data:/data
    environment:
      MEMSTACK_DATA: /data
      MEMSTACK_BIND: 0.0.0.0:8080

volumes:
  memstack-data:
```

```bash
docker compose up -d
```

### 反向代理 + HTTPS

生产环境不要裸跑 8080。以 Caddy 为例（自动签证书）：

```
sync.example.com {
    reverse_proxy 127.0.0.1:8080
}
```

Nginx 要点：`proxy_pass` 到上游，带上 `Upgrade` / `Connection` 头（若用 WebSocket），`client_max_body_size` 调到能传 apkg。

### 环境变量

| 变量 | 默认 | 说明 |
|---|---|---|
| `MEMSTACK_BIND` | `0.0.0.0:8080` | 监听地址 |
| `MEMSTACK_DATA` | `/data` | 数据目录（挂卷） |
| `MEMSTACK_LOG` | `info` | 日志级别 |

具体以实现落地后的 `--help` 为准。

### 客户端指向自托管

```bash
memstack --db memstack.sqlite
# 同步服务实现后：在客户端设置同步地址
memstack config set sync https://sync.example.com
```

### 升级与备份

```bash
docker pull ghcr.io/memstack/memstack-server:latest
docker compose up -d

# 备份数据卷
docker run --rm -v memstack-data:/data -v "$(pwd):/backup" \
  alpine tar czf /backup/memstack-data.tgz -C /data .
```

## 开发

```bash
cargo build -p memstack-server
cargo test -p memstack-server
./target/debug/memstack-server --bind 127.0.0.1:8080 --data ./data
```

### HTTP API（v1）

| 方法 | 路径 | 说明 |
|---|---|---|
| GET | `/health` | 健康检查 |
| POST | `/v1/auth/register` | `{"username","password"}` → `{"token","user_id"}` |
| POST | `/v1/auth/login` | 同上 |
| GET | `/v1/sync/pull?since=SEQ&limit=N` | 增量拉取 `seq > since` → `{"items","cursor","has_more"}` |
| POST | `/v1/sync/push` | `{"items":[{item_id,kind,deleted,updated_at,payload}]}` → `{"cursor","applied"}` |

`kind`：`deck` / `note` / `card` / `review_log` / `deck_params`。`item_id` 由客户端生成（建议 UUID）。

冲突规则：

- `review_log`：按 `item_id` 只增不改，重复推送跳过
- 其余：LWW，`updated_at` 大者胜；相等保留已有
- `deleted: true` 为墓碑，供其他端删本地副本

技术选型见方案：axum + tokio + SQLite；服务端自用库与客户端五张业务表分离，同步以变更条目为粒度。

## 和「核心免费」的关系

方案承诺：核心功能永远免费，导出永远不设限；收入靠官方托管同步的订阅。**自建同步免费，体验一样。** 没有广告，不卖词库，不切付费墙。
