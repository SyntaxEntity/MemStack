//! Tauri 命令：浏览 / 新建 / 编辑 / 复习。

use chrono::Utc;
use memstack_core::{DeckId, NoteId};
use memstack_storage::{NewNote, Storage};
use tauri::State;
use tokio::sync::Mutex;

use crate::dto::{
    card_dto, content_to_form, note_dto, parse_note_input, tags_from, CardDto, DeckDto,
    NoteDetailDto, NoteDto, NoteInput, ReviewCardDto, ReviewDoneDto,
};
use crate::error::Result;
use crate::session::{self, ReviewSession};

pub struct Db(pub Storage);

pub struct ReviewState(pub Mutex<Option<ReviewSession>>);

fn as_deck_id(id: i64) -> DeckId {
    DeckId(id)
}

fn as_note_id(id: i64) -> NoteId {
    NoteId(id)
}

#[tauri::command]
pub async fn list_decks(db: State<'_, Db>) -> Result<Vec<DeckDto>> {
    list_decks_impl(&db.0).await
}

pub async fn list_decks_impl(db: &Storage) -> Result<Vec<DeckDto>> {
    let now = Utc::now();
    let mut out = Vec::new();
    for deck in db.list_decks().await? {
        let counts = db.queue_counts(now, Some(deck.id)).await?;
        out.push(DeckDto {
            id: deck.id.0,
            name: deck.name,
            learning: counts.learning,
            new: counts.new,
            review: counts.review,
        });
    }
    Ok(out)
}

#[tauri::command]
pub async fn ensure_deck(db: State<'_, Db>, name: String) -> Result<DeckDto> {
    let deck = db.0.deck_ensure(name.trim(), Utc::now()).await?;
    let counts = db.0.queue_counts(Utc::now(), Some(deck.id)).await?;
    Ok(DeckDto {
        id: deck.id.0,
        name: deck.name,
        learning: counts.learning,
        new: counts.new,
        review: counts.review,
    })
}

#[tauri::command]
pub async fn list_notes(
    db: State<'_, Db>,
    deck_id: i64,
    limit: Option<u32>,
) -> Result<Vec<NoteDto>> {
    list_notes_impl(&db.0, deck_id, limit.unwrap_or(500)).await
}

pub async fn list_notes_impl(db: &Storage, deck_id: i64, limit: u32) -> Result<Vec<NoteDto>> {
    let notes = db.list_notes(as_deck_id(deck_id), limit).await?;
    let mut out = Vec::with_capacity(notes.len());
    for note in &notes {
        let cards = db.list_cards_for_note(note.id).await?;
        out.push(note_dto(note, cards.len()));
    }
    Ok(out)
}

#[tauri::command]
pub async fn get_note(db: State<'_, Db>, note_id: i64) -> Result<NoteDetailDto> {
    get_note_impl(&db.0, note_id).await
}

pub async fn get_note_impl(db: &Storage, note_id: i64) -> Result<NoteDetailDto> {
    let note = db
        .get_note(as_note_id(note_id))
        .await?
        .ok_or("笔记不存在")?;
    let deck = db.get_deck(note.deck_id).await?.ok_or("牌组不存在")?;
    let cards = db.list_cards_for_note(note.id).await?;
    let (kind, front, back, text, code, language, extra) = content_to_form(&note.content);
    Ok(NoteDetailDto {
        id: note.id.0,
        deck_id: note.deck_id.0,
        deck_name: deck.name,
        kind,
        front,
        back,
        text,
        code,
        language,
        extra,
        tags: note.tags.iter().cloned().collect(),
        cards: cards.iter().map(card_dto).collect(),
        updated_at: note.updated_at.to_rfc3339(),
    })
}

#[tauri::command]
pub async fn create_note(db: State<'_, Db>, input: NoteInput) -> Result<NoteDetailDto> {
    create_note_impl(&db.0, input).await
}

pub async fn create_note_impl(db: &Storage, input: NoteInput) -> Result<NoteDetailDto> {
    let content = parse_note_input(&input)?;
    let tags = tags_from(&input);
    let deck = db.deck_ensure(input.deck.trim(), Utc::now()).await?;
    let (note, _) = db
        .add_note(
            NewNote {
                deck_id: deck.id,
                content,
                tags,
                source: None,
            },
            Utc::now(),
        )
        .await?;
    get_note_impl(db, note.id.0).await
}

#[tauri::command]
pub async fn update_note(
    db: State<'_, Db>,
    note_id: i64,
    input: NoteInput,
) -> Result<NoteDetailDto> {
    update_note_impl(&db.0, note_id, input).await
}

pub async fn update_note_impl(
    db: &Storage,
    note_id: i64,
    input: NoteInput,
) -> Result<NoteDetailDto> {
    let content = parse_note_input(&input)?;
    let tags = tags_from(&input);
    let deck = db.deck_ensure(input.deck.trim(), Utc::now()).await?;
    db.update_note(as_note_id(note_id), content, tags, deck.id, Utc::now())
        .await?;
    get_note_impl(db, note_id).await
}

#[tauri::command]
pub async fn list_note_cards(db: State<'_, Db>, note_id: i64) -> Result<Vec<CardDto>> {
    let cards = db.0.list_cards_for_note(as_note_id(note_id)).await?;
    Ok(cards.iter().map(card_dto).collect())
}

#[tauri::command]
pub async fn start_review(
    db: State<'_, Db>,
    review: State<'_, ReviewState>,
    deck_id: Option<i64>,
    limit: Option<u32>,
) -> Result<Option<ReviewCardDto>> {
    let (session, card) = session::start(&db.0, deck_id.map(DeckId), limit.unwrap_or(50)).await?;
    *review.0.lock().await = Some(session);
    Ok(card)
}

#[tauri::command]
pub async fn rate_card(
    db: State<'_, Db>,
    review: State<'_, ReviewState>,
    rating: String,
    duration_ms: Option<u32>,
) -> Result<Option<ReviewCardDto>> {
    let mut guard = review.0.lock().await;
    let Some(session) = guard.as_mut() else {
        return Err("没有进行中的复习".into());
    };
    session::rate(&db.0, session, &rating, duration_ms.unwrap_or(0)).await
}

#[tauri::command]
pub async fn undo_review(
    db: State<'_, Db>,
    review: State<'_, ReviewState>,
) -> Result<Option<ReviewCardDto>> {
    let mut guard = review.0.lock().await;
    let Some(session) = guard.as_mut() else {
        return Err("没有进行中的复习".into());
    };
    session::undo(&db.0, session).await
}

#[tauri::command]
pub async fn flip_card(review: State<'_, ReviewState>) -> Result<()> {
    let mut guard = review.0.lock().await;
    if let Some(session) = guard.as_mut() {
        session::flip(session);
    }
    Ok(())
}

#[tauri::command]
pub async fn review_done(review: State<'_, ReviewState>) -> Result<ReviewDoneDto> {
    let mut guard = review.0.lock().await;
    match guard.take() {
        Some(session) => Ok(session.done()),
        None => Ok(ReviewDoneDto {
            reviewed: 0,
            new_used: 0,
            new_cap: 0,
            review_used: 0,
            review_cap: 0,
        }),
    }
}

#[tauri::command]
pub async fn queue_summary(db: State<'_, Db>, deck_id: Option<i64>) -> Result<DeckDto> {
    let now = Utc::now();
    let counts = db.0.queue_counts(now, deck_id.map(DeckId)).await?;
    let name = match deck_id {
        Some(id) => {
            db.0.get_deck(DeckId(id))
                .await?
                .map(|d| d.name)
                .unwrap_or_default()
        }
        None => "全部".into(),
    };
    Ok(DeckDto {
        id: deck_id.unwrap_or(0),
        name,
        learning: counts.learning,
        new: counts.new,
        review: counts.review,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::NoteInput;

    fn input(kind: &str) -> NoteInput {
        NoteInput {
            deck: "算法".into(),
            kind: kind.into(),
            front: "什么是二分？".into(),
            back: "对半查找".into(),
            text: "首都是{{c1::北京}}".into(),
            code: "let x = {{c1::1}};".into(),
            language: "rust".into(),
            extra: String::new(),
            tags: vec!["t1".into()],
        }
    }

    #[tokio::test]
    async fn browse_create_edit_review() {
        let db = Storage::open_in_memory().await.unwrap();

        let decks = list_decks_impl(&db).await.unwrap();
        assert!(decks.is_empty());

        let created = create_note_impl(&db, input("qa")).await.unwrap();
        assert_eq!(created.kind, "qa");
        assert_eq!(created.cards.len(), 1);

        let decks = list_decks_impl(&db).await.unwrap();
        assert_eq!(decks.len(), 1);
        assert_eq!(decks[0].new, 1);

        let notes = list_notes_impl(&db, decks[0].id, 10).await.unwrap();
        assert_eq!(notes.len(), 1);

        let mut edit = input("qa");
        edit.front = "二分左边界？".into();
        edit.back = "循环不变式".into();
        let updated = update_note_impl(&db, created.id, edit).await.unwrap();
        assert!(updated.front.contains("二分"));

        let (mut session, card) = session::start(&db, Some(DeckId(decks[0].id)), 10)
            .await
            .unwrap();
        let card = card.expect("有新卡");
        assert_eq!(card.card.front, "二分左边界？");

        session::flip(&mut session);
        let next = session::rate(&db, &mut session, "good", 100).await.unwrap();
        assert!(next.is_none(), "本批只有一张");

        let done = session.done();
        assert_eq!(done.reviewed, 1);
        assert_eq!(done.new_used, 1);
    }

    #[tokio::test]
    async fn undo_restores_card() {
        let db = Storage::open_in_memory().await.unwrap();
        let created = create_note_impl(&db, input("qa")).await.unwrap();
        let deck_id = created.deck_id;

        let (mut session, card) = session::start(&db, Some(DeckId(deck_id)), 10)
            .await
            .unwrap();
        assert!(card.is_some());
        session::rate(&db, &mut session, "again", 10).await.unwrap();
        assert_eq!(session.reviewed, 1);

        let back = session::undo(&db, &mut session).await.unwrap().unwrap();
        assert_eq!(back.card.id, created.cards[0].id);
        assert_eq!(session.reviewed, 0);
        assert_eq!(back.card.state, "new");
    }
}
