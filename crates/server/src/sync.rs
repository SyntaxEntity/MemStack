//! 路由处理：注册 / 登录 / 增量同步。

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;

use crate::db::{Db, SyncItem};
use crate::dto::{
    self, AuthRes, ErrorRes, LoginReq, PullItem, PullRes, PushReq, PushRes, RegisterReq,
};
use crate::error::Error;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

pub struct ApiError(Error);

impl From<Error> for ApiError {
    fn from(e: Error) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            Error::Unauthorized | Error::BadCredentials => StatusCode::UNAUTHORIZED,
            Error::UsernameTaken => StatusCode::CONFLICT,
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (
            status,
            Json(ErrorRes {
                error: self.0.to_string(),
            }),
        )
            .into_response()
    }
}

type ApiResult<T> = std::result::Result<T, ApiError>;

pub async fn health() -> &'static str {
    "ok"
}

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterReq>,
) -> ApiResult<Json<AuthRes>> {
    let now = Utc::now();
    let user_id = state
        .db
        .create_user(&req.username, &req.password, now)
        .await?;
    let token = state.db.issue_token(user_id, now, None).await?;
    Ok(Json(AuthRes { token, user_id }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginReq>,
) -> ApiResult<Json<AuthRes>> {
    let now = Utc::now();
    let user_id = state
        .db
        .user_id_by_login(&req.username, &req.password)
        .await?;
    let token = state.db.issue_token(user_id, now, None).await?;
    Ok(Json(AuthRes { token, user_id }))
}

fn bearer(headers: &HeaderMap) -> Result<&str, Error> {
    let raw = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or(Error::Unauthorized)?;
    raw.strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .ok_or(Error::Unauthorized)
}

async fn auth_user(state: &AppState, headers: &HeaderMap) -> ApiResult<i64> {
    let token = bearer(headers)?;
    Ok(state.db.user_id_by_token(token, Utc::now()).await?)
}

#[derive(Debug, Deserialize)]
pub struct SinceQuery {
    #[serde(default)]
    pub since: i64,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_limit() -> u32 {
    500
}

pub async fn pull(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<SinceQuery>,
) -> ApiResult<Json<PullRes>> {
    let user_id = auth_user(&state, &headers).await?;
    let limit = q.limit.clamp(1, 5000);
    let items: Vec<SyncItem> = state.db.pull(user_id, q.since, limit).await?;
    let has_more = items.len() as u32 >= limit;
    let cursor = items.last().map(|s| s.seq).unwrap_or(q.since);
    // 全库游标：即使本页为空也要返回最新 seq
    let cursor = if items.is_empty() {
        state.db.current_cursor(user_id).await?
    } else {
        cursor
    };
    Ok(Json(PullRes {
        items: items.iter().map(PullItem::from).collect(),
        cursor,
        has_more,
    }))
}

pub async fn push(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PushReq>,
) -> ApiResult<Json<PushRes>> {
    if req.items.len() > 5000 {
        return Err(Error::BadRequest("单次最多 5000 条".into()).into());
    }
    let user_id = auth_user(&state, &headers).await?;
    let incoming = dto::parse_incoming(&req.items)?;
    let (cursor, applied) = state.db.push(user_id, &incoming).await?;
    Ok(Json(PushRes {
        cursor,
        applied: applied.into_iter().map(Into::into).collect(),
    }))
}
