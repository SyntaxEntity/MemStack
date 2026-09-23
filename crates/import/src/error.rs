use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("第 {index} 张卡缺少正面或背面")]
    MissingSide { index: usize },

    #[error("第 {index} 张卡：{reason}")]
    InvalidCard { index: usize, reason: String },

    #[error("第 {index} 张卡：牌组未指定（frontmatter 写 deck:，或用 --deck）")]
    MissingDeck { index: usize },

    #[error("CSV 第 {index} 行：{reason}")]
    BadCsvRow { index: usize, reason: String },

    #[error("CSV 解析失败：{0}")]
    Csv(#[from] csv::Error),

    #[error("核心层错误")]
    Core(#[from] memstack_core::Error),

    #[error("读文件失败：{path}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("不是合法的 .apkg：{0}")]
    BadApkg(String),

    #[error("apkg 里找不到 collection.anki2 / collection.anki21")]
    NoCollection,

    #[error("Anki 库读取失败：{0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("Anki 元数据 JSON 无效")]
    Json(#[from] serde_json::Error),

    #[error("笔记 {id} 的模板渲染失败：{reason}")]
    Template { id: i64, reason: String },
}
