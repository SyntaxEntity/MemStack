//! 解析产物：可直接入库的笔记 / 卡片 / 复习记录。

use chrono::{DateTime, Utc};
use memstack_core::{CardState, CardType, NoteContent, Rating};
use std::collections::BTreeSet;

/// 整包导入内容。
#[derive(Debug, Clone, Default)]
pub struct AnkiImport {
    pub notes: Vec<ImportedNote>,
    pub reviews: Vec<ImportedReview>,
}

impl AnkiImport {
    pub fn card_count(&self) -> usize {
        self.notes.iter().map(|n| n.cards.len()).sum()
    }
}

/// 一条笔记及其全部卡（保留 Anki 调度状态）。
#[derive(Debug, Clone)]
pub struct ImportedNote {
    /// Anki note id，调试用
    pub anki_id: i64,
    pub deck_name: String,
    pub content: NoteContent,
    pub tags: BTreeSet<String>,
    pub cards: Vec<ImportedCard>,
    pub created_at: DateTime<Utc>,
}

/// 已渲染卡面 + Anki 调度状态。
#[derive(Debug, Clone)]
pub struct ImportedCard {
    pub anki_id: i64,
    pub ordinal: u16,
    pub card_type: CardType,
    pub front: String,
    pub back: String,
    pub state: CardState,
    pub step: u8,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub due: DateTime<Utc>,
    pub last_review: Option<DateTime<Utc>>,
    pub reps: u32,
    pub lapses: u32,
    pub suspended: bool,
}

/// 一条复习历史，`anki_card_id` 用来对上新卡 id。
#[derive(Debug, Clone)]
pub struct ImportedReview {
    pub anki_card_id: i64,
    pub reviewed_at: DateTime<Utc>,
    pub rating: Rating,
    pub state_before: CardState,
    pub step_before: u8,
    pub interval_days: f32,
    pub elapsed_days: u32,
    pub duration_ms: u32,
}
