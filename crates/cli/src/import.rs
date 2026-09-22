//! 卡片导入：Markdown 文件/目录，或 Anki `.apkg`。

use chrono::Utc;
use memstack_core::{Card, CardState, CardType, NoteContent, Rating, ReviewLog};
use memstack_storage::{CardSeed, NewNote, Storage};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

pub async fn run(
    db: &Storage,
    path: &Path,
    deck_override: Option<&str>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let is_apkg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("apkg") || e.eq_ignore_ascii_case("colpkg"));

    if is_apkg {
        return run_apkg(db, path, deck_override, dry_run).await;
    }
    run_markdown(db, path, deck_override, dry_run).await
}

async fn run_markdown(
    db: &Storage,
    path: &Path,
    deck_override: Option<&str>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = path.to_string_lossy();
    let drafts = if path.is_dir() {
        memstack_import::import_markdown_dir(&path_str, deck_override)?
    } else {
        memstack_import::import_markdown_file(&path_str, deck_override)?
    };

    if drafts.is_empty() {
        println!("没有解析到卡片。");
        return Ok(());
    }

    if dry_run {
        println!("将导入 {} 条笔记：\n", drafts.len());
        for (i, d) in drafts.iter().enumerate() {
            println!("{}. [{}] {}", i + 1, d.deck, summary(&d.content));
            if !d.tags.is_empty() {
                println!(
                    "   tags: {}",
                    d.tags.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            if let Some(src) = &d.source {
                println!("   from: {src}");
            }
        }
        return Ok(());
    }

    let now = Utc::now();
    let mut total_cards = 0usize;
    let mut total_notes = 0usize;
    let mut decks = std::collections::HashMap::new();

    for draft in drafts {
        let deck_id = match decks.get(&draft.deck) {
            Some(id) => *id,
            None => {
                let deck = db.deck_ensure(&draft.deck, now).await?;
                decks.insert(draft.deck.clone(), deck.id);
                deck.id
            }
        };
        let (_, cards) = db
            .add_note(
                NewNote {
                    deck_id,
                    content: draft.content,
                    tags: draft.tags,
                    source: draft.source,
                },
                now,
            )
            .await?;
        total_notes += 1;
        total_cards += cards.len();
    }

    println!(
        "已导入 {total_notes} 条笔记 → {total_cards} 张卡（{} 个牌组）",
        decks.len()
    );
    Ok(())
}

async fn run_apkg(
    db: &Storage,
    path: &Path,
    deck_override: Option<&str>,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let path_str = path.to_string_lossy();
    let pack = memstack_import::parse_apkg(&path_str)?;

    if dry_run {
        println!(
            "将导入 {} 条笔记 → {} 张卡，{} 条复习历史：\n",
            pack.notes.len(),
            pack.card_count(),
            pack.reviews.len()
        );
        for (i, n) in pack.notes.iter().enumerate() {
            println!(
                "{}. [{}] {} ({} 张卡, {} 条历史)",
                i + 1,
                n.deck_name,
                summary(&n.content),
                n.cards.len(),
                pack.reviews
                    .iter()
                    .filter(|r| n.cards.iter().any(|c| c.anki_id == r.anki_card_id))
                    .count()
            );
            if !n.tags.is_empty() {
                println!(
                    "   tags: {}",
                    n.tags.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        }
        return Ok(());
    }

    let now = Utc::now();
    let mut decks: HashMap<String, memstack_core::DeckId> = HashMap::new();
    let mut card_map: HashMap<i64, Card> = HashMap::new();
    let mut total_notes = 0usize;
    let mut total_cards = 0usize;

    for n in &pack.notes {
        let deck_name = deck_override.unwrap_or(&n.deck_name).to_string();
        let deck_id = match decks.get(&deck_name) {
            Some(id) => *id,
            None => {
                let deck = db.deck_ensure(&deck_name, now).await?;
                decks.insert(deck_name.clone(), deck.id);
                deck.id
            }
        };

        let seeds: Vec<CardSeed> = n
            .cards
            .iter()
            .map(|c| CardSeed {
                ordinal: c.ordinal,
                card_type: c.card_type,
                front: c.front.clone(),
                back: c.back.clone(),
                state: c.state,
                step: c.step,
                stability: c.stability,
                difficulty: c.difficulty,
                due: c.due,
                last_review: c.last_review,
                reps: c.reps,
                lapses: c.lapses,
                suspended: c.suspended,
            })
            .collect();

        let (_, cards) = db
            .add_note_with_cards(
                NewNote {
                    deck_id,
                    content: n.content.clone(),
                    tags: n.tags.clone(),
                    source: Some(format!("apkg:{path_str}#{}", n.anki_id)),
                },
                seeds,
                now,
            )
            .await?;

        for (seed, card) in n.cards.iter().zip(cards.iter()) {
            card_map.insert(seed.anki_id, card.clone());
        }
        total_notes += 1;
        total_cards += cards.len();
    }

    // 复习历史
    let mut logs = Vec::new();
    for r in &pack.reviews {
        let Some(card) = card_map.get(&r.anki_card_id) else {
            continue;
        };
        logs.push(ReviewLog {
            id: None,
            card_id: card.id,
            reviewed_at: r.reviewed_at,
            rating: r.rating,
            state_before: r.state_before,
            step_before: r.step_before,
            stability_before: card.stability,
            difficulty_before: card.difficulty,
            interval_days: r.interval_days,
            elapsed_days: r.elapsed_days,
            duration_ms: r.duration_ms,
        });
    }
    let log_count = db.append_review_logs(&logs).await?;

    println!(
        "已导入 {total_notes} 条笔记 → {total_cards} 张卡，{log_count} 条复习历史（{} 个牌组）",
        decks.len()
    );
    Ok(())
}

fn summary(content: &NoteContent) -> String {
    match content {
        NoteContent::QuestionAnswer { front, .. } => {
            format!("问答 {}", first_line(front))
        }
        NoteContent::Cloze { text, .. } => format!("填空 {}", first_line(text)),
        NoteContent::CodeCloze {
            code, language, ..
        } => {
            let lang = if language.is_empty() {
                "?"
            } else {
                language.as_str()
            };
            format!("代码填空[{lang}] {}", first_line(code))
        }
    }
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("").trim()
}

// 以下类型在 dry-run 摘要里间接用到，保留引用避免未使用告警
#[allow(dead_code)]
fn _ty(_: BTreeSet<String>, _: CardState, _: CardType, _: Rating) {}
