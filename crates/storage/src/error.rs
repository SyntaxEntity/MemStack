use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("数据库错误：{0}")]
    Sqlx(#[from] sqlx::Error),

    #[error("JSON 序列化错误")]
    Json(#[from] serde_json::Error),

    #[error("时间戳无效：{0}")]
    Timestamp(i64),

    #[error("未知状态字符串：{0}")]
    UnknownState(String),

    #[error("未知题型字符串：{0}")]
    UnknownCardType(String),

    #[error("评分无效：{0}")]
    UnknownRating(i64),

    #[error("实体不存在")]
    NotFound,

    #[error("核心层错误")]
    Core(#[from] memstack_core::Error),
}
