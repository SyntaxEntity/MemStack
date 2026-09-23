//! 本地 UUID 映射与同步状态（存在用户 SQLite 里）。

use sqlx::sqlite::SqlitePool;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{Error, Result};

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS sync_ids (
    kind     TEXT    NOT NULL,
    uuid     TEXT    NOT NULL,
    local_id INTEGER NOT NULL,
    PRIMARY KEY (kind, uuid)
);
CREATE INDEX IF NOT EXISTS idx_sync_ids_local ON sync_ids (kind, local_id);

CREATE TABLE IF NOT EXISTS sync_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Deck,
    Note,
    Card,
    ReviewLog,
    DeckParams,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Deck => "deck",
            Kind::Note => "note",
            Kind::Card => "card",
            Kind::ReviewLog => "review_log",
            Kind::DeckParams => "deck_params",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "deck" => Kind::Deck,
            "note" => Kind::Note,
            "card" => Kind::Card,
            "review_log" => Kind::ReviewLog,
            "deck_params" => Kind::DeckParams,
            other => return Err(Error::BadKind(other.to_string())),
        })
    }
}

pub struct Mapping {
    pool: SqlitePool,
}

impl Mapping {
    pub async fn attach(pool: SqlitePool) -> Result<Self> {
        sqlx::raw_sql(SCHEMA).execute(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn uuid_for(&self, kind: Kind, local_id: i64) -> Result<String> {
        let row =
            sqlx::query("SELECT uuid FROM sync_ids WHERE kind = ?1 AND local_id = ?2 LIMIT 1")
                .bind(kind.as_str())
                .bind(local_id)
                .fetch_optional(&self.pool)
                .await?;
        if let Some(row) = row {
            return Ok(row.try_get("uuid")?);
        }
        let uuid = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO sync_ids (kind, uuid, local_id) VALUES (?1, ?2, ?3)")
            .bind(kind.as_str())
            .bind(&uuid)
            .bind(local_id)
            .execute(&self.pool)
            .await?;
        Ok(uuid)
    }

    /// 已有映射则返回本地 id；没有则分配 `new_local_id` 并记住。
    pub async fn local_for_or_assign(
        &self,
        kind: Kind,
        uuid: &str,
        new_local_id: i64,
    ) -> Result<i64> {
        if let Some(id) = self.local_for(kind, uuid).await? {
            return Ok(id);
        }
        sqlx::query("INSERT INTO sync_ids (kind, uuid, local_id) VALUES (?1, ?2, ?3)")
            .bind(kind.as_str())
            .bind(uuid)
            .bind(new_local_id)
            .execute(&self.pool)
            .await?;
        Ok(new_local_id)
    }

    pub async fn local_for(&self, kind: Kind, uuid: &str) -> Result<Option<i64>> {
        let row = sqlx::query("SELECT local_id FROM sync_ids WHERE kind = ?1 AND uuid = ?2")
            .bind(kind.as_str())
            .bind(uuid)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.try_get("local_id")).transpose()?)
    }

    pub async fn remember(&self, kind: Kind, uuid: &str, local_id: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_ids (kind, uuid, local_id) VALUES (?1, ?2, ?3)
             ON CONFLICT (kind, uuid) DO UPDATE SET local_id = excluded.local_id",
        )
        .bind(kind.as_str())
        .bind(uuid)
        .bind(local_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM sync_meta WHERE key = ?1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.try_get("value")).transpose()?)
    }

    pub async fn set_meta(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT INTO sync_meta (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
