//! 导出 / 应用回环（不经 HTTP）。

use std::collections::BTreeSet;

use memstack_core::{CardState, NoteContent, Rating, ReviewLog};
use memstack_storage::{NewNote, Storage};

use memstack_sync::{apply_items, export_items, Kind, Mapping, WireItem};

#[tokio::test]
async fn export_then_apply_roundtrip() {
    let src = Storage::open_in_memory().await.unwrap();
    let map_src = Mapping::attach(src.pool().clone()).await.unwrap();
    let t = chrono::Utc::now();
    let deck = src.deck_ensure("算法", t).await.unwrap();
    let (note, cards) = src
        .add_note(
            NewNote {
                deck_id: deck.id,
                content: NoteContent::QuestionAnswer {
                    front: "LIFO?".into(),
                    back: "栈".into(),
                },
                tags: BTreeSet::from(["t1".into()]),
                source: None,
            },
            t,
        )
        .await
        .unwrap();
    let log = ReviewLog {
        id: None,
        card_id: cards[0].id,
        reviewed_at: t,
        rating: Rating::Good,
        state_before: CardState::New,
        step_before: 0,
        stability_before: None,
        difficulty_before: None,
        interval_days: 1.0,
        elapsed_days: 0,
        duration_ms: 5,
    };
    let mut reviewed = cards[0].clone();
    reviewed.state = CardState::Review;
    reviewed.reps = 1;
    reviewed.stability = Some(2.5);
    reviewed.difficulty = Some(5.0);
    reviewed.updated_at = t;
    src.save_review(&reviewed, &log).await.unwrap();

    let items = export_items(&src, &map_src).await.unwrap();
    assert!(items.iter().any(|i| i.kind == "deck"));
    assert!(items.iter().any(|i| i.kind == "note"));
    assert!(items.iter().any(|i| i.kind == "card"));
    assert!(items.iter().any(|i| i.kind == "review_log"));

    // 另一端
    let dst = Storage::open_in_memory().await.unwrap();
    let map_dst = Mapping::attach(dst.pool().clone()).await.unwrap();
    // 去掉 uuid 以外身份，模拟远端
    let wire: Vec<WireItem> = items
        .into_iter()
        .map(|mut i| {
            i.seq = None;
            i
        })
        .collect();
    let n = apply_items(&dst, &map_dst, &wire).await.unwrap();
    assert!(n >= 4, "applied={n}");

    let decks = dst.list_decks().await.unwrap();
    assert_eq!(decks.len(), 1);
    assert_eq!(decks[0].name, "算法");
    let notes = dst.list_all_notes(10).await.unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].id, note.id); // 不同库 id 可不同，这里只断言有内容
    assert_eq!(
        notes[0].content,
        NoteContent::QuestionAnswer {
            front: "LIFO?".into(),
            back: "栈".into()
        }
    );
    let dst_cards = dst.list_all_cards(10).await.unwrap();
    assert_eq!(dst_cards.len(), 1);
    assert!(dst_cards[0].reps >= 1 || dst_cards[0].state != CardState::New);
    let logs = dst.list_all_review_logs(10).await.unwrap();
    assert_eq!(logs.len(), 1);
}

#[tokio::test]
async fn review_log_apply_is_idempotent() {
    let src = Storage::open_in_memory().await.unwrap();
    let map_src = Mapping::attach(src.pool().clone()).await.unwrap();
    let t = chrono::Utc::now();
    let deck = src.deck_ensure("g", t).await.unwrap();
    let (_, cards) = src
        .add_note(
            NewNote {
                deck_id: deck.id,
                content: NoteContent::QuestionAnswer {
                    front: "q".into(),
                    back: "a".into(),
                },
                tags: BTreeSet::new(),
                source: None,
            },
            t,
        )
        .await
        .unwrap();
    let log = ReviewLog {
        id: None,
        card_id: cards[0].id,
        reviewed_at: t,
        rating: Rating::Easy,
        state_before: CardState::New,
        step_before: 0,
        stability_before: None,
        difficulty_before: None,
        interval_days: 3.0,
        elapsed_days: 0,
        duration_ms: 1,
    };
    src.save_review(&cards[0], &log).await.unwrap();
    let items = export_items(&src, &map_src).await.unwrap();

    let dst = Storage::open_in_memory().await.unwrap();
    let map_dst = Mapping::attach(dst.pool().clone()).await.unwrap();
    let n1 = apply_items(&dst, &map_dst, &items).await.unwrap();
    let n2 = apply_items(&dst, &map_dst, &items).await.unwrap();
    assert!(n2 < n1, "二次 apply 应跳过 review_log：{n1} vs {n2}");
    assert_eq!(dst.list_all_review_logs(10).await.unwrap().len(), 1);
}

#[tokio::test]
async fn kind_roundtrip() {
    assert_eq!(Kind::parse("review_log").unwrap(), Kind::ReviewLog);
    assert_eq!(Kind::Deck.as_str(), "deck");
}
