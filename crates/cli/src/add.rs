//! 造卡。

use chrono::Utc;
use memstack_core::NoteContent;
use memstack_storage::{NewNote, Storage};
use std::collections::BTreeSet;

use crate::AddKind;

pub async fn run(
    db: &Storage,
    deck_name: &str,
    tags: &[String],
    kind: AddKind,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let deck = db.deck_ensure(deck_name, now).await?;

    let content = match kind {
        AddKind::Qa { front, back } => NoteContent::QuestionAnswer { front, back },
        AddKind::Cloze { text, extra } => NoteContent::Cloze { text, extra },
        AddKind::Code {
            code,
            language,
            extra,
        } => NoteContent::CodeCloze {
            code,
            language,
            extra,
        },
    };

    let tags: BTreeSet<String> = tags.iter().cloned().collect();
    let (note, cards) = db
        .add_note(
            NewNote {
                deck_id: deck.id,
                content,
                tags,
                source: None,
            },
            now,
        )
        .await?;

    println!(
        "已添加笔记 {} → {} 张卡（{}）",
        note.id,
        cards.len(),
        deck.name
    );
    for card in &cards {
        println!("  [{}] {}", card.id, first_line(&card.front));
    }
    Ok(())
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or("")
}
