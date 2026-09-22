//! 时间戳与领域枚举的数据库编解码。统一用 UTC 毫秒整数。

use chrono::{DateTime, TimeZone, Utc};
use memstack_core::{CardState, CardType, Rating};

use crate::error::{Error, Result};

pub fn to_ms(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

pub fn from_ms(ms: i64) -> Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .ok_or(Error::Timestamp(ms))
}

pub fn from_ms_opt(ms: Option<i64>) -> Result<Option<DateTime<Utc>>> {
    ms.map(from_ms).transpose()
}

pub fn to_ms_opt(dt: Option<DateTime<Utc>>) -> Option<i64> {
    dt.map(to_ms)
}

pub fn state_to_str(state: CardState) -> &'static str {
    match state {
        CardState::New => "new",
        CardState::Learning => "learning",
        CardState::Review => "review",
        CardState::Relearning => "relearning",
    }
}

pub fn state_from_str(s: &str) -> Result<CardState> {
    match s {
        "new" => Ok(CardState::New),
        "learning" => Ok(CardState::Learning),
        "review" => Ok(CardState::Review),
        "relearning" => Ok(CardState::Relearning),
        other => Err(Error::UnknownState(other.to_string())),
    }
}

pub fn card_type_to_str(t: CardType) -> &'static str {
    match t {
        CardType::QuestionAnswer => "question_answer",
        CardType::Cloze => "cloze",
        CardType::CodeCloze => "code_cloze",
    }
}

pub fn card_type_from_str(s: &str) -> Result<CardType> {
    match s {
        "question_answer" => Ok(CardType::QuestionAnswer),
        "cloze" => Ok(CardType::Cloze),
        "code_cloze" => Ok(CardType::CodeCloze),
        other => Err(Error::UnknownCardType(other.to_string())),
    }
}

pub fn rating_from_i64(v: i64) -> Result<Rating> {
    match v {
        1 => Ok(Rating::Again),
        2 => Ok(Rating::Hard),
        3 => Ok(Rating::Good),
        4 => Ok(Rating::Easy),
        other => Err(Error::UnknownRating(other)),
    }
}

pub fn int_to_bool(v: i64) -> bool {
    v != 0
}
