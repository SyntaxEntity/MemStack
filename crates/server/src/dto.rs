//! HTTP 报文。

use serde::{Deserialize, Serialize};

use crate::db::{Apply, Kind, SyncItem};

#[derive(Debug, Deserialize)]
pub struct RegisterReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginReq {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthRes {
    pub token: String,
    pub user_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct PushItem {
    pub item_id: String,
    pub kind: String,
    #[serde(default)]
    pub deleted: bool,
    pub updated_at: i64,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct PushReq {
    pub items: Vec<PushItem>,
}

#[derive(Debug, Serialize)]
pub struct PushRes {
    pub cursor: i64,
    pub applied: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct PullItem {
    pub item_id: String,
    pub kind: String,
    pub deleted: bool,
    pub updated_at: i64,
    pub seq: i64,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct PullRes {
    pub items: Vec<PullItem>,
    pub cursor: i64,
    pub has_more: bool,
}

#[derive(Debug, Serialize)]
pub struct ErrorRes {
    pub error: String,
}

impl From<&SyncItem> for PullItem {
    fn from(s: &SyncItem) -> Self {
        PullItem {
            item_id: s.item_id.clone(),
            kind: s.kind.as_str().to_string(),
            deleted: s.deleted,
            updated_at: s.updated_at,
            seq: s.seq,
            payload: serde_json::from_str(&s.payload).unwrap_or(serde_json::Value::Null),
        }
    }
}

impl From<Apply> for &'static str {
    fn from(a: Apply) -> Self {
        match a {
            Apply::Inserted => "inserted",
            Apply::Updated => "updated",
            Apply::Skipped => "skipped",
        }
    }
}

pub fn parse_incoming(items: &[PushItem]) -> crate::error::Result<Vec<crate::db::IncomingItem>> {
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        out.push(crate::db::IncomingItem {
            item_id: it.item_id.clone(),
            kind: Kind::parse(&it.kind)?,
            deleted: it.deleted,
            updated_at: it.updated_at,
            payload: serde_json::to_string(&it.payload)?,
        });
    }
    Ok(out)
}
