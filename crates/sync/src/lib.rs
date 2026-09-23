//! 客户端同步适配：把本地 SQLite 映射为服务端变更条目。
//!
//! 身份：客户端生成 UUID（`sync_ids`），本地 i64 主键不外泄。
//! 冲突：与服务端一致——`review_log` 只增，其余 LWW。

mod client;
mod error;
mod mapping;

pub use client::{AuthRes, Client, PullRes, PushRes, WireItem};
pub use error::{Error, Result};
pub use mapping::{Kind, Mapping};

use chrono::{DateTime, TimeZone, Utc};
use memstack_core::{Card, CardState, CardType, DeckParams, Note, NoteContent, Rating, ReviewLog};
use memstack_storage::Storage;
use serde::{Deserialize, Serialize};

fn to_ms(dt: DateTime<Utc>) -> i64 {
    dt.timestamp_millis()
}

fn from_ms(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckPayload {
    pub name: String,
    pub parent_uuid: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotePayload {
    pub deck_uuid: String,
    pub content: NoteContent,
    pub tags: Vec<String>,
    pub source: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardPayload {
    pub note_uuid: String,
    pub deck_uuid: String,
    pub card_type: CardType,
    pub ordinal: u16,
    pub front: String,
    pub back: String,
    pub state: CardState,
    pub step: u8,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub due: i64,
    pub last_review: Option<i64>,
    pub reps: u32,
    pub lapses: u32,
    pub suspended: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewLogPayload {
    pub card_uuid: String,
    pub reviewed_at: i64,
    pub rating: Rating,
    pub state_before: CardState,
    pub step_before: u8,
    pub stability_before: Option<f32>,
    pub difficulty_before: Option<f32>,
    pub interval_days: f32,
    pub elapsed_days: u32,
    pub duration_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeckParamsPayload {
    pub deck_uuid: String,
    #[serde(flatten)]
    pub params: DeckParams,
    pub updated_at: i64,
}

/// 本地 → 服务端变更列表。
pub async fn export_items(storage: &Storage, map: &Mapping) -> Result<Vec<WireItem>> {
    let mut items = Vec::new();

    for deck in storage.list_decks().await? {
        let uuid = map.uuid_for(Kind::Deck, deck.id.0).await?;
        let parent_uuid = match deck.parent_id {
            Some(p) => Some(map.uuid_for(Kind::Deck, p.0).await?),
            None => None,
        };
        let payload = DeckPayload {
            name: deck.name.clone(),
            parent_uuid,
            created_at: to_ms(deck.created_at),
            updated_at: to_ms(deck.updated_at),
        };
        items.push(WireItem {
            item_id: uuid,
            kind: Kind::Deck.as_str().into(),
            deleted: false,
            updated_at: payload.updated_at,
            seq: None,
            payload: serde_json::to_value(payload)?,
        });

        let params = storage.deck_params_or_default(deck.id).await?;
        let puuid = map.uuid_for(Kind::DeckParams, deck.id.0).await?;
        let pp = DeckParamsPayload {
            deck_uuid: map.uuid_for(Kind::Deck, deck.id.0).await?,
            params,
            updated_at: to_ms(deck.updated_at),
        };
        items.push(WireItem {
            item_id: puuid,
            kind: Kind::DeckParams.as_str().into(),
            deleted: false,
            updated_at: pp.updated_at,
            seq: None,
            payload: serde_json::to_value(pp)?,
        });
    }

    for note in storage.list_all_notes(100_000).await? {
        let uuid = map.uuid_for(Kind::Note, note.id.0).await?;
        let payload = NotePayload {
            deck_uuid: map.uuid_for(Kind::Deck, note.deck_id.0).await?,
            content: note.content.clone(),
            tags: note.tags.iter().cloned().collect(),
            source: note.source.clone(),
            created_at: to_ms(note.created_at),
            updated_at: to_ms(note.updated_at),
        };
        items.push(WireItem {
            item_id: uuid,
            kind: Kind::Note.as_str().into(),
            deleted: false,
            updated_at: payload.updated_at,
            seq: None,
            payload: serde_json::to_value(payload)?,
        });
    }

    for card in storage.list_all_cards(100_000).await? {
        let uuid = map.uuid_for(Kind::Card, card.id.0).await?;
        let payload = CardPayload {
            note_uuid: map.uuid_for(Kind::Note, card.note_id.0).await?,
            deck_uuid: map.uuid_for(Kind::Deck, card.deck_id.0).await?,
            card_type: card.card_type,
            ordinal: card.ordinal,
            front: card.front.clone(),
            back: card.back.clone(),
            state: card.state,
            step: card.step,
            stability: card.stability,
            difficulty: card.difficulty,
            due: to_ms(card.due),
            last_review: card.last_review.map(to_ms),
            reps: card.reps,
            lapses: card.lapses,
            suspended: card.suspended,
            created_at: to_ms(card.created_at),
            updated_at: to_ms(card.updated_at),
        };
        items.push(WireItem {
            item_id: uuid,
            kind: Kind::Card.as_str().into(),
            deleted: false,
            updated_at: payload.updated_at,
            seq: None,
            payload: serde_json::to_value(payload)?,
        });
    }

    for log in storage.list_all_review_logs(100_000).await? {
        let Some(log_id) = log.id else { continue };
        let uuid = map.uuid_for(Kind::ReviewLog, log_id.0).await?;
        let payload = ReviewLogPayload {
            card_uuid: map.uuid_for(Kind::Card, log.card_id.0).await?,
            reviewed_at: to_ms(log.reviewed_at),
            rating: log.rating,
            state_before: log.state_before,
            step_before: log.step_before,
            stability_before: log.stability_before,
            difficulty_before: log.difficulty_before,
            interval_days: log.interval_days,
            elapsed_days: log.elapsed_days,
            duration_ms: log.duration_ms,
        };
        items.push(WireItem {
            item_id: uuid,
            kind: Kind::ReviewLog.as_str().into(),
            deleted: false,
            updated_at: payload.reviewed_at,
            seq: None,
            payload: serde_json::to_value(payload)?,
        });
    }

    Ok(items)
}

/// 应用一批远端变更。返回落地条数。
pub async fn apply_items(storage: &Storage, map: &Mapping, items: &[WireItem]) -> Result<usize> {
    // 依赖顺序：deck → params → note → card → review_log
    let mut applied = 0;
    for kind in [
        Kind::Deck,
        Kind::DeckParams,
        Kind::Note,
        Kind::Card,
        Kind::ReviewLog,
    ] {
        for item in items
            .iter()
            .filter(|i| Kind::parse(&i.kind).ok() == Some(kind))
        {
            if item.deleted {
                // v1：墓碑只记账，不删本地（避免级联误删）。
                continue;
            }
            match kind {
                Kind::Deck => {
                    let p: DeckPayload = serde_json::from_value(item.payload.clone())?;
                    storage
                        .put_deck_by_name(
                            &p.name,
                            None,
                            from_ms(p.created_at),
                            from_ms(p.updated_at),
                        )
                        .await?;
                    if let Some(d) = storage.get_deck_by_name(&p.name).await? {
                        map.remember(Kind::Deck, &item.item_id, d.id.0).await?;
                    }
                    applied += 1;
                }
                Kind::DeckParams => {
                    let p: DeckParamsPayload = serde_json::from_value(item.payload.clone())?;
                    let Some(deck_local) = map.local_for(Kind::Deck, &p.deck_uuid).await? else {
                        continue;
                    };
                    storage
                        .put_deck_params(memstack_core::DeckId(deck_local), &p.params)
                        .await?;
                    applied += 1;
                }
                Kind::Note => {
                    let p: NotePayload = serde_json::from_value(item.payload.clone())?;
                    let Some(deck_local) = map.local_for(Kind::Deck, &p.deck_uuid).await? else {
                        continue;
                    };
                    let mut note = Note {
                        id: memstack_core::NoteId(0),
                        deck_id: memstack_core::DeckId(deck_local),
                        content: p.content,
                        tags: p.tags.into_iter().collect(),
                        source: p.source,
                        created_at: from_ms(p.created_at),
                        updated_at: from_ms(p.updated_at),
                    };
                    if let Some(local_id) = map.local_for(Kind::Note, &item.item_id).await? {
                        note.id = memstack_core::NoteId(local_id);
                        if let Some(existing) = storage.get_note(note.id).await? {
                            if existing.updated_at >= note.updated_at {
                                continue;
                            }
                        }
                        storage.update_note_full(&note).await?;
                    } else {
                        let id = storage.insert_note_full(&note).await?;
                        map.remember(Kind::Note, &item.item_id, id.0).await?;
                    }
                    applied += 1;
                }
                Kind::Card => {
                    let p: CardPayload = serde_json::from_value(item.payload.clone())?;
                    let Some(note_local) = map.local_for(Kind::Note, &p.note_uuid).await? else {
                        continue;
                    };
                    let Some(deck_local) = map.local_for(Kind::Deck, &p.deck_uuid).await? else {
                        continue;
                    };
                    let mut card = Card {
                        id: memstack_core::CardId(0),
                        note_id: memstack_core::NoteId(note_local),
                        deck_id: memstack_core::DeckId(deck_local),
                        card_type: p.card_type,
                        ordinal: p.ordinal,
                        front: p.front,
                        back: p.back,
                        state: p.state,
                        step: p.step,
                        stability: p.stability,
                        difficulty: p.difficulty,
                        due: from_ms(p.due),
                        last_review: p.last_review.map(from_ms),
                        reps: p.reps,
                        lapses: p.lapses,
                        suspended: p.suspended,
                        created_at: from_ms(p.created_at),
                        updated_at: from_ms(p.updated_at),
                    };
                    if let Some(local_id) = map.local_for(Kind::Card, &item.item_id).await? {
                        card.id = memstack_core::CardId(local_id);
                        if let Some(existing) = storage.get_card(card.id).await? {
                            if existing.updated_at >= card.updated_at {
                                continue;
                            }
                        }
                        storage.update_card_full(&card).await?;
                    } else {
                        let id = storage.insert_card_full(&card).await?;
                        map.remember(Kind::Card, &item.item_id, id.0).await?;
                    }
                    applied += 1;
                }
                Kind::ReviewLog => {
                    // 只增：已见过就跳过
                    if map
                        .local_for(Kind::ReviewLog, &item.item_id)
                        .await?
                        .is_some()
                    {
                        continue;
                    }
                    let p: ReviewLogPayload = serde_json::from_value(item.payload.clone())?;
                    let Some(card_local) = map.local_for(Kind::Card, &p.card_uuid).await? else {
                        continue;
                    };
                    let log = ReviewLog {
                        id: None,
                        card_id: memstack_core::CardId(card_local),
                        reviewed_at: from_ms(p.reviewed_at),
                        rating: p.rating,
                        state_before: p.state_before,
                        step_before: p.step_before,
                        stability_before: p.stability_before,
                        difficulty_before: p.difficulty_before,
                        interval_days: p.interval_days,
                        elapsed_days: p.elapsed_days,
                        duration_ms: p.duration_ms,
                    };
                    storage.append_review_logs(&[log]).await?;
                    // 记下 uuid → 用「日志数」无法回填 id；改为按 uuid 占位 local_id=0 后更新
                    // 简化：写入后查最后一条不稳。用 local_for_or_assign(0) 即可去重。
                    map.remember(Kind::ReviewLog, &item.item_id, 0).await?;
                    applied += 1;
                }
            }
        }
    }
    Ok(applied)
}

/// 一次完整同步：先 push 本地，再 pull 增量。
pub async fn sync_once(
    storage: &Storage,
    map: &Mapping,
    client: &Client,
) -> Result<(usize, usize)> {
    let items = export_items(storage, map).await?;
    let push_res = client.push(items).await?;
    let since: i64 = map
        .get_meta("cursor")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut cursor = since;
    let mut applied = 0;
    loop {
        let page = client.pull(cursor, 500).await?;
        applied += apply_items(storage, map, &page.items).await?;
        cursor = page.cursor;
        if !page.has_more {
            break;
        }
    }
    map.set_meta("cursor", &cursor.to_string()).await?;
    Ok((push_res.applied.len(), applied))
}
