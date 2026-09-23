//! 同步服务库：账号、令牌、用户数据变更。

use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::path::Path;

use crate::auth;
use crate::error::{Error, Result};

const SCHEMA: &str = include_str!("../migrations/0001_init.sql");

/// 变更实体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Deck,
    Note,
    Card,
    ReviewLog,
    DeckParams,
}

impl Kind {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "deck" => Kind::Deck,
            "note" => Kind::Note,
            "card" => Kind::Card,
            "review_log" => Kind::ReviewLog,
            "deck_params" => Kind::DeckParams,
            other => return Err(Error::BadRequest(format!("未知类型：{other}"))),
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Deck => "deck",
            Kind::Note => "note",
            Kind::Card => "card",
            Kind::ReviewLog => "review_log",
            Kind::DeckParams => "deck_params",
        }
    }
}

/// 一条同步变更（出站）。
#[derive(Debug, Clone)]
pub struct SyncItem {
    pub item_id: String,
    pub kind: Kind,
    pub deleted: bool,
    pub updated_at: i64,
    pub seq: i64,
    pub payload: String,
}

/// push 单条入站。
#[derive(Debug, Clone)]
pub struct IncomingItem {
    pub item_id: String,
    pub kind: Kind,
    pub deleted: bool,
    pub updated_at: i64,
    pub payload: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Apply {
    Inserted,
    Updated,
    /// 按冲突规则丢弃（本地已更新或 review_log 已存在）.
    Skipped,
}

#[derive(Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Self { pool })
    }

    /// 内存库，测试用。
    #[cfg(test)]
    pub async fn open_in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Self { pool })
    }

    // ---------- 账号 ----------

    pub async fn create_user(
        &self,
        username: &str,
        password: &str,
        now: DateTime<Utc>,
    ) -> Result<i64> {
        if username.trim().is_empty() || password.is_empty() {
            return Err(Error::BadRequest("用户名和密码不能为空".into()));
        }
        if username.len() > 64 || password.len() > 256 {
            return Err(Error::BadRequest("用户名或密码过长".into()));
        }
        let hash = auth::hash_password(password)?;
        let ms = now.timestamp_millis();
        let result = sqlx::query(
            "INSERT INTO users (username, password_hash, created_at) VALUES (?1, ?2, ?3)",
        )
        .bind(username.trim())
        .bind(hash)
        .bind(ms)
        .execute(&self.pool)
        .await;
        match result {
            Ok(r) => Ok(r.last_insert_rowid()),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(Error::UsernameTaken),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn user_id_by_login(&self, username: &str, password: &str) -> Result<i64> {
        let row = sqlx::query("SELECT id, password_hash FROM users WHERE username = ?1")
            .bind(username.trim())
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Err(Error::BadCredentials);
        };
        let id: i64 = row.try_get("id")?;
        let hash: String = row.try_get("password_hash")?;
        if auth::verify_password(password, &hash)? {
            Ok(id)
        } else {
            Err(Error::BadCredentials)
        }
    }

    pub async fn issue_token(
        &self,
        user_id: i64,
        now: DateTime<Utc>,
        ttl: Option<i64>,
    ) -> Result<String> {
        let token = auth::new_token();
        let ms = now.timestamp_millis();
        sqlx::query(
            "INSERT INTO tokens (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(auth::token_hash(&token))
        .bind(user_id)
        .bind(ms)
        .bind(ttl.map(|s| ms + s * 1000))
        .execute(&self.pool)
        .await?;
        Ok(token)
    }

    pub async fn user_id_by_token(&self, token: &str, now: DateTime<Utc>) -> Result<i64> {
        let row = sqlx::query("SELECT user_id, expires_at FROM tokens WHERE token_hash = ?1")
            .bind(auth::token_hash(token))
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Err(Error::Unauthorized);
        };
        let user_id: i64 = row.try_get("user_id")?;
        let expires_at: Option<i64> = row.try_get("expires_at")?;
        if let Some(exp) = expires_at {
            if now.timestamp_millis() > exp {
                return Err(Error::Unauthorized);
            }
        }
        Ok(user_id)
    }

    // ---------- 同步 ----------

    pub async fn current_cursor(&self, user_id: i64) -> Result<i64> {
        let row =
            sqlx::query("SELECT COALESCE(MAX(seq), 0) AS m FROM sync_items WHERE user_id = ?1")
                .bind(user_id)
                .fetch_one(&self.pool)
                .await?;
        Ok(row.try_get::<i64, _>("m")?)
    }

    /// 增量拉取：`seq > since`。
    pub async fn pull(&self, user_id: i64, since: i64, limit: u32) -> Result<Vec<SyncItem>> {
        let rows = sqlx::query(
            "SELECT item_id, kind, deleted, updated_at, seq, payload
             FROM sync_items
             WHERE user_id = ?1 AND seq > ?2
             ORDER BY seq ASC
             LIMIT ?3",
        )
        .bind(user_id)
        .bind(since)
        .bind(i64::from(limit.min(10_000)))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let kind: String = row.try_get("kind")?;
                Ok(SyncItem {
                    item_id: row.try_get("item_id")?,
                    kind: Kind::parse(&kind)?,
                    deleted: row.try_get::<i64, _>("deleted")? != 0,
                    updated_at: row.try_get("updated_at")?,
                    seq: row.try_get("seq")?,
                    payload: row.try_get("payload")?,
                })
            })
            .collect()
    }

    /// 推送一批变更。冲突：`review_log` 按 id 去重只增；其余 LWW（`updated_at` 大者胜，相等保留已有）。
    pub async fn push(&self, user_id: i64, items: &[IncomingItem]) -> Result<(i64, Vec<Apply>)> {
        let mut tx = self.pool.begin().await?;
        let mut cursor =
            sqlx::query("SELECT COALESCE(MAX(seq), 0) AS m FROM sync_items WHERE user_id = ?1")
                .bind(user_id)
                .fetch_one(&mut *tx)
                .await?
                .try_get::<i64, _>("m")?;
        let mut applied = Vec::with_capacity(items.len());

        for item in items {
            if item.item_id.trim().is_empty() || item.item_id.len() > 128 {
                return Err(Error::BadRequest("item_id 无效".into()));
            }
            let existing = sqlx::query(
                "SELECT deleted, updated_at, seq FROM sync_items WHERE user_id = ?1 AND item_id = ?2",
            )
            .bind(user_id)
            .bind(&item.item_id)
            .fetch_optional(&mut *tx)
            .await?;

            let action = match existing {
                None => {
                    cursor += 1;
                    sqlx::query(
                        "INSERT INTO sync_items (user_id, item_id, kind, deleted, updated_at, seq, payload)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    )
                    .bind(user_id)
                    .bind(&item.item_id)
                    .bind(item.kind.as_str())
                    .bind(i64::from(item.deleted))
                    .bind(item.updated_at)
                    .bind(cursor)
                    .bind(&item.payload)
                    .execute(&mut *tx)
                    .await?;
                    Apply::Inserted
                }
                Some(row) => {
                    // review_log 只增不改
                    if item.kind == Kind::ReviewLog {
                        Apply::Skipped
                    } else {
                        let updated: i64 = row.try_get("updated_at")?;
                        if item.updated_at > updated {
                            cursor += 1;
                            sqlx::query(
                                "UPDATE sync_items
                                 SET kind = ?1, deleted = ?2, updated_at = ?3, seq = ?4, payload = ?5
                                 WHERE user_id = ?6 AND item_id = ?7",
                            )
                            .bind(item.kind.as_str())
                            .bind(i64::from(item.deleted))
                            .bind(item.updated_at)
                            .bind(cursor)
                            .bind(&item.payload)
                            .bind(user_id)
                            .bind(&item.item_id)
                            .execute(&mut *tx)
                            .await?;
                            Apply::Updated
                        } else {
                            Apply::Skipped
                        }
                    }
                }
            };
            applied.push(action);
        }

        tx.commit().await?;
        Ok((cursor, applied))
    }
}
