//! Anki `.apkg` 导入。
//!
//! 解包 zip → 读 collection.anki2/anki21 → 映射牌组、标签、挖空、调度状态与复习历史。

mod map;
mod reader;
mod types;

#[cfg(test)]
mod tests;

pub use types::{AnkiImport, ImportedCard, ImportedNote, ImportedReview};

use crate::error::Result;

/// 解析 .apkg 文件。只读，不写库。
pub fn parse_apkg(path: &str) -> Result<AnkiImport> {
    let raw = reader::read_collection(path)?;
    map::build_import(&raw)
}
