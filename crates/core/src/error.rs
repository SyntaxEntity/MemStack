use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("挖空标记未闭合：{context}")]
    UnclosedCloze { context: String },

    #[error("无效挖空标记：{0}")]
    InvalidCloze(String),

    #[error("笔记中没有挖空标记，无法生成填空卡")]
    NoClozeMarkers,

    #[error("学习步骤为空，无法安排间隔")]
    EmptySteps,

    #[error("FSRS 参数无效")]
    InvalidParameters {
        #[source]
        source: fsrs::FSRSError,
    },

    #[error("FSRS 计算失败")]
    Fsrs {
        #[source]
        source: fsrs::FSRSError,
    },
}

impl From<fsrs::FSRSError> for Error {
    fn from(source: fsrs::FSRSError) -> Self {
        match source {
            fsrs::FSRSError::InvalidParameters => Error::InvalidParameters { source },
            other => Error::Fsrs { source: other },
        }
    }
}
