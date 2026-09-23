-- 同步服务库：账号、令牌、用户数据变更。
-- 实体 id 由客户端生成（UUID 字符串），跨端稳定。

CREATE TABLE IF NOT EXISTS users (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    username      TEXT    NOT NULL UNIQUE,
    password_hash TEXT    NOT NULL,
    created_at    INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS tokens (
    token_hash TEXT    PRIMARY KEY,
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    expires_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tokens_user ON tokens (user_id);

-- 当前状态 + 变更序号。pull 用 seq 增量拉。
CREATE TABLE IF NOT EXISTS sync_items (
    user_id    INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    item_id    TEXT    NOT NULL,
    kind       TEXT    NOT NULL,
    deleted    INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    payload    TEXT    NOT NULL DEFAULT '{}',
    PRIMARY KEY (user_id, item_id)
);

CREATE INDEX IF NOT EXISTS idx_sync_items_user_seq ON sync_items (user_id, seq);
CREATE INDEX IF NOT EXISTS idx_sync_items_kind ON sync_items (user_id, kind);
