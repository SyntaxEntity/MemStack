//! 忆栈 MemStack 核心：领域模型与 FSRS 调度。
//!
//! 本 crate 不做任何 IO——不读文件、不碰数据库、不依赖 GUI。

mod cloze;
mod error;
mod model;
mod scheduler;
mod template;

pub use cloze::{cloze_faces, cloze_ids, generate_cards, parse_segments, CardDraft, Segment};
pub use error::{Error, Result};
pub use model::{
    Card, CardId, CardState, CardType, Deck, DeckId, DeckParams, Note, NoteContent, NoteId,
    NoteType, Rating, ReviewLog, ReviewLogId,
};
pub use scheduler::{ButtonPreview, ReviewOutcome, SchedulePreview, Scheduler};
pub use template::{template, NoteTemplate, TemplateKind, TEMPLATES};
