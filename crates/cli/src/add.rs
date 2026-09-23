//! 造卡与模板。

use chrono::Utc;
use memstack_core::{template, NoteContent, NoteTemplate};
use memstack_storage::{NewNote, Storage};
use std::collections::BTreeSet;

use crate::AddKind;

pub async fn list_templates() {
    println!("{:<16} {:<8} 说明", "ID", "题型");
    for t in memstack_core::TEMPLATES {
        println!("{:<16} {:<8} {}", t.id, t.kind.as_str(), t.summary);
    }
}

pub async fn show_template(id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let t = template(id).ok_or(format!("未知模板：{id}"))?;
    println!("{} · {}（{}）", t.name, t.id, t.kind.as_str());
    println!("{}", t.summary);
    println!();
    print_fields(t);
    Ok(())
}

fn print_fields(t: &NoteTemplate) {
    match t.kind {
        memstack_core::TemplateKind::Qa => {
            println!("正面：\n{}\n", t.front);
            println!("背面：\n{}", t.back);
        }
        memstack_core::TemplateKind::Cloze => {
            println!("正文：\n{}", t.text);
            if !t.extra.is_empty() {
                println!("\n补充：\n{}", t.extra);
            }
        }
        memstack_core::TemplateKind::Code => {
            println!("语言：{}", t.language);
            println!("代码：\n{}", t.code);
            if !t.extra.is_empty() {
                println!("\n补充：\n{}", t.extra);
            }
        }
    }
    if !t.tags.is_empty() {
        println!("\n默认标签：{}", t.tags.join(", "));
    }
}

pub async fn run(
    db: &Storage,
    deck_name: &str,
    tags: &[String],
    template_id: Option<&str>,
    kind: Option<AddKind>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (content, tags_out) = if let Some(tid) = template_id {
        let t = template(tid).ok_or(format!("未知模板：{tid}"))?;
        let mut set: BTreeSet<String> = t.tags.iter().map(|s| s.to_string()).collect();
        set.extend(tags.iter().cloned());
        (t.content(), set)
    } else {
        let kind = kind.ok_or("需要指定题型子命令，或使用 --template")?;
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
        (content, tags.iter().cloned().collect())
    };

    let now = Utc::now();
    let deck = db.deck_ensure(deck_name, now).await?;
    let (note, cards) = db
        .add_note(
            NewNote {
                deck_id: deck.id,
                content,
                tags: tags_out,
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

/// 编辑器集成：JSON → 笔记。字段与 desktop `NoteInput` 对齐。
#[derive(Debug, serde::Deserialize)]
struct JsonNote {
    kind: String,
    #[serde(default)]
    deck: Option<String>,
    #[serde(default)]
    front: String,
    #[serde(default)]
    back: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    language: String,
    #[serde(default)]
    extra: String,
    #[serde(default)]
    tags: Vec<String>,
}

pub async fn run_json(
    db: &Storage,
    deck_name: &str,
    tags: &[String],
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let raw = if path.as_os_str() == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(path)?
    };
    let item: JsonNote = serde_json::from_str(&raw)?;
    let content = match item.kind.as_str() {
        "qa" => {
            if item.front.trim().is_empty() || item.back.trim().is_empty() {
                return Err("问答卡需要 front/back".into());
            }
            NoteContent::QuestionAnswer {
                front: item.front.trim().to_string(),
                back: item.back.trim().to_string(),
            }
        }
        "cloze" => {
            if !item.text.contains("{{c") {
                return Err("填空卡需要 {{c1::答案}}".into());
            }
            NoteContent::Cloze {
                text: item.text,
                extra: item.extra,
            }
        }
        "code" => {
            if !item.code.contains("{{c") {
                return Err("代码填空需要 {{c1::答案}}".into());
            }
            NoteContent::CodeCloze {
                code: item.code,
                language: item.language,
                extra: item.extra,
            }
        }
        other => return Err(format!("未知 kind：{other}").into()),
    };
    let mut tags_out: BTreeSet<String> = tags.iter().cloned().collect();
    tags_out.extend(item.tags.iter().cloned());
    let deck_name = item.deck.as_deref().unwrap_or(deck_name);
    let now = Utc::now();
    let deck = db.deck_ensure(deck_name, now).await?;
    let (note, cards) = db
        .add_note(
            NewNote {
                deck_id: deck.id,
                content,
                tags: tags_out,
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
