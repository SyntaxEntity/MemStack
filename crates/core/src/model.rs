use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub i64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<i64> for $name {
            fn from(value: i64) -> Self {
                Self(value)
            }
        }
    };
}

id_type!(DeckId);
id_type!(NoteId);
id_type!(CardId);
id_type!(ReviewLogId);

/// 评分。键位 1-4 对应 Again / Hard / Good / Easy。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub const ALL: [Rating; 4] = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy];

    pub fn from_keystroke(key: char) -> Option<Self> {
        match key {
            '1' => Some(Rating::Again),
            '2' => Some(Rating::Hard),
            '3' => Some(Rating::Good),
            '4' => Some(Rating::Easy),
            _ => None,
        }
    }

    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// FSRS 评分码：1=Again, 2=Hard, 3=Good, 4=Easy
    pub fn as_fsrs_rating(self) -> u32 {
        self as u32
    }
}

impl fmt::Display for Rating {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Rating::Again => "Again",
            Rating::Hard => "Hard",
            Rating::Good => "Good",
            Rating::Easy => "Easy",
        })
    }
}

/// 卡片调度状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    New,
    Learning,
    Review,
    Relearning,
}

impl CardState {
    /// 是否处于学习/再学习步骤中（按分钟间隔重见）。
    pub fn is_in_steps(self) -> bool {
        matches!(self, CardState::Learning | CardState::Relearning)
    }
}

/// 题型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    /// 问答：正面 / 背面
    QuestionAnswer,
    /// 填空：文本挖空，按挖空序号出卡
    Cloze,
    /// 代码填空：代码挖洞，补全后与标准答案 diff
    CodeCloze,
}

/// 笔记正文。一条笔记可生成多张卡（填空）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "content", rename_all = "snake_case")]
pub enum NoteContent {
    QuestionAnswer {
        front: String,
        back: String,
    },
    Cloze {
        text: String,
        #[serde(default)]
        extra: String,
    },
    CodeCloze {
        code: String,
        /// 高亮用语言标识，如 rust / sql
        #[serde(default)]
        language: String,
        #[serde(default)]
        extra: String,
    },
}

impl NoteContent {
    pub fn note_type(&self) -> NoteType {
        match self {
            NoteContent::QuestionAnswer { .. } => NoteType::QuestionAnswer,
            NoteContent::Cloze { .. } => NoteType::Cloze,
            NoteContent::CodeCloze { .. } => NoteType::CodeCloze,
        }
    }

    pub fn card_type(&self) -> CardType {
        match self {
            NoteContent::QuestionAnswer { .. } => CardType::QuestionAnswer,
            NoteContent::Cloze { .. } => CardType::Cloze,
            NoteContent::CodeCloze { .. } => CardType::CodeCloze,
        }
    }
}

/// 笔记类型（与 [`NoteContent`] 变体一一对应）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoteType {
    QuestionAnswer,
    Cloze,
    CodeCloze,
}

/// 牌组。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deck {
    pub id: DeckId,
    pub name: String,
    pub parent_id: Option<DeckId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 笔记：内容源，不含调度状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteId,
    pub deck_id: DeckId,
    pub content: NoteContent,
    #[serde(default)]
    pub tags: BTreeSet<String>,
    /// 来源文件路径（Markdown 卡片导入时记录）
    #[serde(default)]
    pub source: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 卡片：实际被复习的条目。填空笔记按挖空序号生成多张。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Card {
    pub id: CardId,
    pub note_id: NoteId,
    pub deck_id: DeckId,
    pub card_type: CardType,
    /// 填空序号 = `cN - 1`（与 Anki `ord` 一致，可有空洞）。问答卡恒为 0。
    pub ordinal: u16,
    pub front: String,
    pub back: String,
    pub state: CardState,
    /// 学习 / 再学习步骤下标
    pub step: u8,
    /// FSRS 稳定度。未进入 Review 前为 None。
    pub stability: Option<f32>,
    /// FSRS 难度。未进入 Review 前为 None。
    pub difficulty: Option<f32>,
    pub due: DateTime<Utc>,
    pub last_review: Option<DateTime<Utc>>,
    pub reps: u32,
    pub lapses: u32,
    pub suspended: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Card {
    pub fn memory(&self) -> Option<fsrs::MemoryState> {
        match (self.stability, self.difficulty) {
            (Some(stability), Some(difficulty)) => Some(fsrs::MemoryState {
                stability,
                difficulty,
            }),
            _ => None,
        }
    }

    pub fn set_memory(&mut self, memory: fsrs::MemoryState) {
        self.stability = Some(memory.stability);
        self.difficulty = Some(memory.difficulty);
    }

    pub fn is_due(&self, now: DateTime<Utc>) -> bool {
        !self.suspended && self.due <= now
    }

    /// 距上次复习的整数天数，新卡为 0。供 FSRS 使用。
    pub fn elapsed_days(&self, now: DateTime<Utc>) -> u32 {
        match self.last_review {
            None => 0,
            Some(last) => {
                let days = (now - last).num_days();
                if days <= 0 {
                    0
                } else {
                    days as u32
                }
            }
        }
    }
}

/// 复习记录，只增不改。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewLog {
    pub id: Option<ReviewLogId>,
    pub card_id: CardId,
    pub reviewed_at: DateTime<Utc>,
    pub rating: Rating,
    pub state_before: CardState,
    pub step_before: u8,
    pub stability_before: Option<f32>,
    pub difficulty_before: Option<f32>,
    /// 本次调度到的间隔（天）。学习步骤中为分钟折算的天数。
    pub interval_days: f32,
    pub elapsed_days: u32,
    pub duration_ms: u32,
}

/// 牌组调度参数。可看可调，落在 `deck_params` 表。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckParams {
    /// FSRS 参数。空表示用官方默认值。
    pub fsrs_params: Vec<f32>,
    /// 目标记忆留存率，默认 0.9
    pub desired_retention: f32,
    /// 最大间隔（天）
    pub max_interval_days: u32,
    /// 学习步骤（分钟），如 [1, 10]
    pub learning_steps: Vec<u32>,
    /// 再学习步骤（分钟），如 [10]
    pub relearning_steps: Vec<u32>,
    /// 每日新卡上限
    pub new_cards_per_day: u32,
    /// 每日复习上限
    pub reviews_per_day: u32,
}

impl Default for DeckParams {
    fn default() -> Self {
        Self {
            fsrs_params: Vec::new(),
            desired_retention: 0.9,
            max_interval_days: 36_500,
            learning_steps: vec![1, 10],
            relearning_steps: vec![10],
            new_cards_per_day: 20,
            reviews_per_day: 200,
        }
    }
}

impl DeckParams {
    pub fn learning_steps_or_default(&self) -> Vec<u32> {
        if self.learning_steps.is_empty() {
            vec![1, 10]
        } else {
            self.learning_steps.clone()
        }
    }

    pub fn relearning_steps_or_default(&self) -> Vec<u32> {
        if self.relearning_steps.is_empty() {
            vec![10]
        } else {
            self.relearning_steps.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rating_from_keystroke() {
        assert_eq!(Rating::from_keystroke('1'), Some(Rating::Again));
        assert_eq!(Rating::from_keystroke('2'), Some(Rating::Hard));
        assert_eq!(Rating::from_keystroke('3'), Some(Rating::Good));
        assert_eq!(Rating::from_keystroke('4'), Some(Rating::Easy));
        assert_eq!(Rating::from_keystroke('5'), None);
    }

    #[test]
    fn elapsed_days_counts_whole_days() {
        let now = Utc::now();
        let mut card = Card {
            id: CardId(1),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            card_type: CardType::QuestionAnswer,
            ordinal: 0,
            front: String::new(),
            back: String::new(),
            state: CardState::Review,
            step: 0,
            stability: Some(1.0),
            difficulty: Some(5.0),
            due: now,
            last_review: Some(now - chrono::Duration::days(3)),
            reps: 1,
            lapses: 0,
            suspended: false,
            created_at: now,
            updated_at: now,
        };
        assert_eq!(card.elapsed_days(now), 3);
        card.last_review = Some(now + chrono::Duration::hours(2));
        assert_eq!(card.elapsed_days(now), 0);
    }
}
