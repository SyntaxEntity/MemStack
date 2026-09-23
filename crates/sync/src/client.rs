//! 同步服务 HTTP 客户端。

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRes {
    pub token: String,
    pub user_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireItem {
    pub item_id: String,
    pub kind: String,
    #[serde(default)]
    pub deleted: bool,
    pub updated_at: i64,
    #[serde(default)]
    pub seq: Option<i64>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushReq {
    pub items: Vec<WireItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRes {
    pub cursor: i64,
    pub applied: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRes {
    pub items: Vec<WireItem>,
    pub cursor: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone)]
pub struct Client {
    base: String,
    token: Option<String>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(base: impl Into<String>) -> Self {
        let base = base.into().trim_end_matches('/').to_string();
        Self {
            base,
            token: None,
            http: reqwest::Client::new(),
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    fn auth(&self) -> Result<&str> {
        self.token.as_deref().ok_or(Error::NotConfigured)
    }

    pub async fn register(&mut self, username: &str, password: &str) -> Result<AuthRes> {
        let res: AuthRes = self
            .http
            .post(format!("{}/v1/auth/register", self.base))
            .json(&serde_json::json!({"username": username, "password": password}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        self.token = Some(res.token.clone());
        Ok(res)
    }

    pub async fn login(&mut self, username: &str, password: &str) -> Result<AuthRes> {
        let res: AuthRes = self
            .http
            .post(format!("{}/v1/auth/login", self.base))
            .json(&serde_json::json!({"username": username, "password": password}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        self.token = Some(res.token.clone());
        Ok(res)
    }

    pub async fn push(&self, items: Vec<WireItem>) -> Result<PushRes> {
        let token = self.auth()?;
        let res = self
            .http
            .post(format!("{}/v1/sync/push", self.base))
            .bearer_auth(token)
            .json(&PushReq { items })
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await?;
        if !status.is_success() {
            return Err(Error::Server(body));
        }
        Ok(serde_json::from_str(&body)?)
    }

    pub async fn pull(&self, since: i64, limit: u32) -> Result<PullRes> {
        let token = self.auth()?;
        let res = self
            .http
            .get(format!("{}/v1/sync/pull", self.base))
            .bearer_auth(token)
            .query(&[("since", since.to_string()), ("limit", limit.to_string())])
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await?;
        if !status.is_success() {
            return Err(Error::Server(body));
        }
        Ok(serde_json::from_str(&body)?)
    }
}
