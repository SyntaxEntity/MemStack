//! 前端 DTO。字段名 camelCase，时间用 RFC3339。

use memstack_core::{ButtonPreview, Card, CardType, Note, NoteContent, Rating};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckDto {
    pub id: i64,
    pub name: String,
    pub learning: usize,
    pub new: usize,
    pub review: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDto {
    pub id: i64,
    pub deck_id: i64,
    pub kind: &'static str,
    pub preview: String,
    pub tags: Vec<String>,
    pub card_count: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDto {
    pub id: i64,
    pub note_id: i64,
    pub ordinal: u16,
    pub card_type: &'static str,
    pub state: &'static str,
    pub front: String,
    pub back: String,
    pub reps: u32,
    pub lapses: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDetailDto {
    pub id: i64,
    pub deck_id: i64,
    pub deck_name: String,
    pub kind: &'static str,
    pub front: String,
    pub back: String,
    pub text: String,
    pub code: String,
    pub language: String,
    pub extra: String,
    pub tags: Vec<String>,
    pub cards: Vec<CardDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteInput {
    pub deck: String,
    pub kind: String,
    #[serde(default)]
    pub front: String,
    #[serde(default)]
    pub back: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub extra: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ButtonDto {
    pub id: &'static str,
    pub label: &'static str,
    pub key: &'static str,
    pub due_in: String,
    pub interval_days: Option<f32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCardDto {
    pub card: CardDto,
    pub buttons: Vec<ButtonDto>,
    pub remaining: usize,
    pub new_used: u32,
    pub new_cap: u32,
    pub review_used: u32,
    pub review_cap: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDoneDto {
    pub reviewed: u32,
    pub new_used: u32,
    pub new_cap: u32,
    pub review_used: u32,
    pub review_cap: u32,
}

pub fn card_type_str(t: CardType) -> &'static str {
    match t {
        CardType::QuestionAnswer => "qa",
        CardType::Cloze => "cloze",
        CardType::CodeCloze => "code",
    }
}

pub fn state_str(s: memstack_core::CardState) -> &'static str {
    match s {
        memstack_core::CardState::New => "new",
        memstack_core::CardState::Learning => "learning",
        memstack_core::CardState::Review => "review",
        memstack_core::CardState::Relearning => "relearning",
    }
}

pub fn kind_str(c: &NoteContent) -> &'static str {
    match c {
        NoteContent::QuestionAnswer { .. } => "qa",
        NoteContent::Cloze { .. } => "cloze",
        NoteContent::CodeCloze { .. } => "code",
    }
}

pub fn note_preview(content: &NoteContent) -> String {
    let raw = match content {
        NoteContent::QuestionAnswer { front, .. } => front.as_str(),
        NoteContent::Cloze { text, .. } => text.as_str(),
        NoteContent::CodeCloze { code, .. } => code.as_str(),
    };
    first_line(raw).chars().take(80).collect()
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

pub fn card_dto(card: &Card) -> CardDto {
    CardDto {
        id: card.id.0,
        note_id: card.note_id.0,
        ordinal: card.ordinal,
        card_type: card_type_str(card.card_type),
        state: state_str(card.state),
        front: card.front.clone(),
        back: card.back.clone(),
        reps: card.reps,
        lapses: card.lapses,
    }
}

pub fn note_dto(note: &Note, card_count: usize) -> NoteDto {
    NoteDto {
        id: note.id.0,
        deck_id: note.deck_id.0,
        kind: kind_str(&note.content),
        preview: note_preview(&note.content),
        tags: note.tags.iter().cloned().collect(),
        card_count,
        updated_at: note.updated_at.to_rfc3339(),
    }
}

pub fn button_dto(preview: &ButtonPreview) -> ButtonDto {
    let (id, label, key) = match preview.rating {
        Rating::Again => ("again", "Again", "1"),
        Rating::Hard => ("hard", "Hard", "2"),
        Rating::Good => ("good", "Good", "3"),
        Rating::Easy => ("easy", "Easy", "4"),
    };
    ButtonDto {
        id,
        label,
        key,
        due_in: fmt_duration(preview.due_in),
        interval_days: preview.interval_days,
    }
}

fn fmt_duration(d: chrono::Duration) -> String {
    let mins = d.num_minutes();
    if mins < 1 {
        "<1分".into()
    } else if mins < 60 {
        format!("{mins}分")
    } else if mins < 60 * 24 {
        format!("{}时", mins / 60)
    } else {
        format!("{}天", mins / (60 * 24))
    }
}

pub fn rating_from_id(id: &str) -> Option<Rating> {
    match id {
        "again" | "1" => Some(Rating::Again),
        "hard" | "2" => Some(Rating::Hard),
        "good" | "3" => Some(Rating::Good),
        "easy" | "4" => Some(Rating::Easy),
        _ => None,
    }
}

pub fn parse_note_input(input: &NoteInput) -> crate::error::Result<NoteContent> {
    match input.kind.as_str() {
        "qa" => {
            if input.front.trim().is_empty() || input.back.trim().is_empty() {
                return Err("问答卡需要正面和背面".into());
            }
            Ok(NoteContent::QuestionAnswer {
                front: input.front.trim().to_string(),
                back: input.back.trim().to_string(),
            })
        }
        "cloze" => {
            let text = input.text.trim();
            if text.is_empty() {
                return Err("填空卡需要正文".into());
            }
            if !text.contains("{{c") {
                return Err("填空卡必须包含 {{c1::答案}} 挖空标记".into());
            }
            Ok(NoteContent::Cloze {
                text: text.to_string(),
                extra: input.extra.trim().to_string(),
            })
        }
        "code" => {
            let code = input.code.trim();
            if code.is_empty() {
                return Err("代码填空需要代码".into());
            }
            if !code.contains("{{c") {
                return Err("代码填空必须包含 {{c1::答案}} 挖空标记".into());
            }
            Ok(NoteContent::CodeCloze {
                code: code.to_string(),
                language: input.language.trim().to_string(),
                extra: input.extra.trim().to_string(),
            })
        }
        other => Err(format!("未知题型：{other}").into()),
    }
}

pub fn tags_from(input: &NoteInput) -> std::collections::BTreeSet<String> {
    input
        .tags
        .iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// 把笔记内容拆回表单字段。
pub fn content_to_form(
    content: &NoteContent,
) -> (&'static str, String, String, String, String, String, String) {
    match content {
        NoteContent::QuestionAnswer { front, back } => (
            "qa",
            front.clone(),
            back.clone(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ),
        NoteContent::Cloze { text, extra } => (
            "cloze",
            String::new(),
            String::new(),
            text.clone(),
            String::new(),
            String::new(),
            extra.clone(),
        ),
        NoteContent::CodeCloze {
            code,
            language,
            extra,
        } => (
            "code",
            String::new(),
            String::new(),
            String::new(),
            code.clone(),
            language.clone(),
            extra.clone(),
        ),
    }
}
