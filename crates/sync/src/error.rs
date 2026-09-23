use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("存储错误")]
    Storage(#[from] memstack_storage::Error),

    #[error("数据库错误：{0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("JSON 错误")]
    Json(#[from] serde_json::Error),

    #[error("HTTP 错误：{0}")]
    Http(#[from] reqwest::Error),

    #[error("同步服务返回错误：{0}")]
    Server(String),

    #[error("未登录（缺少服务地址或令牌）")]
    NotConfigured,

    #[error("未知同步类型：{0}")]
    BadKind(String),

    #[error("请求无效：{0}")]
    BadRequest(String),
}
