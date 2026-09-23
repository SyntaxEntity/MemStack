use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("数据库错误：{0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("JSON 错误")]
    Json(#[from] serde_json::Error),

    #[error("用户名或密码错误")]
    BadCredentials,

    #[error("用户名已存在")]
    UsernameTaken,

    #[error("未认证")]
    Unauthorized,

    #[error("请求无效：{0}")]
    BadRequest(String),

    #[error("密码哈希错误")]
    PasswordHash,
}
