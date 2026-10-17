//! `memstack export`：把库导出成 JSON（无损）或 Markdown（可再导入）。
//!
//! 两种格式定位不同：
//!
//! - **JSON**：牌组、牌组参数、笔记、卡片、复习记录全量。卡片带 FSRS 稳定度 / 难度 /
//!   到期时间，复习记录带每次评分与前后状态，足以离线复算调度。
//! - **Markdown**：每张笔记一个文件，格式与 [`memstack_import`] 读入的一致，
//!   可被 `memstack import` 再次读回。**不含调度状态**——那是 JSON 的职责。
//!
//! `extra`（补充说明）字段的编码：单行或空走 frontmatter 的 `extra:`；多行时
//! 填空卡改用正文末尾 `---` 补充块（解析器认得），代码填空卡无此通道，退化为
//! 单行（换行折成空格）。

use chrono::{DateTime, Utc};
use memstack_core::{Card, CardId, DeckId, DeckParams, Note, NoteContent, ReviewLog};
use memstack_storage::Storage;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 导出格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    /// JSON：全量含调度状态与复习记录，可复算 FSRS
    Json,
    /// Markdown：每张笔记一个文件，可被 `import` 再读入
    Md,
}

/// JSON 导出包。
#[derive(Debug, Serialize)]
struct Bundle {
    format: &'static str,
    version: u32,
    generated_at: DateTime<Utc>,
    decks: Vec<DeckOut>,
    notes: Vec<Note>,
    cards: Vec<Card>,
    review_logs: Vec<ReviewLog>,
}

/// 牌组条目（附带调度参数）。
#[derive(Debug, Serialize)]
struct DeckOut {
    id: DeckId,
    name: String,
    parent_id: Option<DeckId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    params: DeckParams,
}

/// JSON 包的格式标记，供下游识别。
const BUNDLE_FORMAT: &str = "memstack-export";
/// 包结构版本。字段增删时递增。
const BUNDLE_VERSION: u32 = 1;

/// Markdown 默认输出目录。
const DEFAULT_MD_DIR: &str = "memstack-export";

pub async fn run(
    db: &Storage,
    deck: Option<&str>,
    format: Format,
    out: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let (decks, notes, cards, logs) = collect(db, deck).await?;
    let deck_names: HashMap<DeckId, String> =
        decks.iter().map(|d| (d.id, d.name.clone())).collect();

    match format {
        Format::Json => {
            let bundle = Bundle {
                format: BUNDLE_FORMAT,
                version: BUNDLE_VERSION,
                generated_at: now,
                decks,
                notes,
                cards,
                review_logs: logs,
            };
            let text = serde_json::to_string_pretty(&bundle)?;
            match out {
                Some(path) => {
                    write_file(path, &text)?;
                    println!(
                        "已导出 {}（{} 牌组 / {} 笔记 / {} 卡片 / {} 复习记录）",
                        path.display(),
                        bundle.decks.len(),
                        bundle.notes.len(),
                        bundle.cards.len(),
                        bundle.review_logs.len()
                    );
                }
                None => println!("{text}"),
            }
        }
        Format::Md => {
            let dir = out
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from(DEFAULT_MD_DIR));
            let written = write_markdown(&dir, &notes, &deck_names)?;
            println!(
                "已导出 {} 个 Markdown 文件到 {}（不含调度状态；完整数据请用 --format json）",
                written,
                dir.display()
            );
        }
    }
    Ok(())
}

/// 按需过滤出本次要导出的四类数据。
async fn collect(
    db: &Storage,
    deck: Option<&str>,
) -> Result<(Vec<DeckOut>, Vec<Note>, Vec<Card>, Vec<ReviewLog>), Box<dyn std::error::Error>> {
    // u32::MAX ≈ 不限量；导出要的就是全量。
    let all_decks = db.list_decks().await?;
    let target = match deck {
        Some(name) => {
            let found = db
                .get_deck_by_name(name)
                .await?
                .ok_or_else(|| format!("牌组不存在：{name}"))?;
            Some(found.id)
        }
        None => None,
    };

    let mut decks = Vec::new();
    for d in &all_decks {
        if let Some(id) = target {
            if d.id != id {
                continue;
            }
        }
        decks.push(DeckOut {
            id: d.id,
            name: d.name.clone(),
            parent_id: d.parent_id,
            created_at: d.created_at,
            updated_at: d.updated_at,
            params: db.deck_params_or_default(d.id).await?,
        });
    }

    let notes: Vec<Note> = db
        .list_all_notes(u32::MAX)
        .await?
        .into_iter()
        .filter(|n| target.is_none_or(|id| n.deck_id == id))
        .collect();

    let cards: Vec<Card> = db
        .list_all_cards(u32::MAX)
        .await?
        .into_iter()
        .filter(|c| target.is_none_or(|id| c.deck_id == id))
        .collect();

    let card_ids: HashSet<CardId> = cards.iter().map(|c| c.id).collect();
    let logs: Vec<ReviewLog> = db
        .list_all_review_logs(u32::MAX)
        .await?
        .into_iter()
        .filter(|l| card_ids.contains(&l.card_id))
        .collect();

    Ok((decks, notes, cards, logs))
}

// ============================================================================
// Markdown
// ============================================================================

/// 把每条笔记渲染成一个 Markdown 文件，返回写出的文件数。
fn write_markdown(
    dir: &Path,
    notes: &[Note],
    deck_names: &HashMap<DeckId, String>,
) -> Result<usize, Box<dyn std::error::Error>> {
    let mut count = 0;
    for note in notes {
        let deck = deck_names
            .get(&note.deck_id)
            .map(String::as_str)
            .unwrap_or("未命名牌组");
        let sub = dir.join(safe_component(deck));
        std::fs::create_dir_all(&sub)?;
        let name = format!("{}-{}.md", note.id.0, safe_component(&note_slug(note)));
        write_file(&sub.join(name), &render_markdown(note, deck))?;
        count += 1;
    }
    Ok(count)
}

/// 渲染单条笔记为可被 `memstack import` 读回的 Markdown。
fn render_markdown(note: &Note, deck: &str) -> String {
    let mut fm = String::from("---\n");
    fm.push_str(&format!("deck: {}\n", single_line(deck)));
    if !note.tags.is_empty() {
        fm.push_str(&format!("tags: [{}]\n", tags_line(&note.tags)));
    }

    let mut body = String::new();
    let mut trailing_extra: Option<String> = None;

    match &note.content {
        NoteContent::QuestionAnswer { front, back } => {
            fm.push_str("type: qa\n");
            body.push_str(front.trim());
            body.push_str("\n\n---\n\n");
            body.push_str(back.trim());
        }
        NoteContent::Cloze { text, extra } => {
            fm.push_str("type: cloze\n");
            body.push_str(text.trim());
            push_extra(&mut fm, extra, &mut trailing_extra, false);
        }
        NoteContent::CodeCloze {
            code,
            language,
            extra,
        } => {
            fm.push_str("type: code\n");
            if !language.is_empty() {
                fm.push_str(&format!("language: {}\n", single_line(language)));
            }
            body.push_str("```");
            body.push_str(language);
            body.push('\n');
            body.push_str(code.trim_end());
            body.push_str("\n```\n");
            push_extra(&mut fm, extra, &mut trailing_extra, true);
        }
    }

    let mut out = fm;
    out.push_str("---\n\n");
    out.push_str(body.trim_end());
    out.push('\n');
    if let Some(extra) = trailing_extra {
        out.push_str("\n---\n\n");
        out.push_str(extra.trim());
        out.push('\n');
    }
    out
}

/// 补充说明写入 frontmatter；多行时填空卡改走正文末尾 `---` 块。
///
/// `flatten` 为真（代码填空）时不支持末尾块——解析器只从 frontmatter 读 `extra`，
/// 故折成单行，宁可少换行也不丢内容。
fn push_extra(fm: &mut String, extra: &str, trailing: &mut Option<String>, flatten: bool) {
    if extra.is_empty() {
        fm.push_str("extra: \n");
        return;
    }
    if !extra.contains('\n') {
        fm.push_str(&format!("extra: {}\n", extra.trim()));
        return;
    }
    if flatten {
        fm.push_str(&format!("extra: {}\n", single_line(extra)));
    } else {
        *trailing = Some(extra.to_string());
    }
}

fn tags_line(tags: &BTreeSet<String>) -> String {
    tags.iter()
        .map(|t| t.trim().replace(',', " "))
        .collect::<Vec<_>>()
        .join(", ")
}

/// 取一段用于文件名的短标题。
fn note_slug(note: &Note) -> String {
    let raw = match &note.content {
        NoteContent::QuestionAnswer { front, .. } => front.as_str(),
        NoteContent::Cloze { text, .. } => text.as_str(),
        NoteContent::CodeCloze { code, .. } => code.as_str(),
    };
    let first = raw
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("note");
    let clean = strip_cloze(first);
    let truncated: String = clean.chars().take(40).collect();
    let slug = safe_component(&truncated);
    if slug.is_empty() {
        "note".into()
    } else {
        slug
    }
}

/// 去掉挖空标记，只留答案文字（用于文件名）。
fn strip_cloze(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(pos) = rest.find("{{c") {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos + 3..];
        let Some(end) = tail.find("}}") else {
            // 未闭合：仍尽量剥掉 `1::` 前缀
            out.push_str(tail.split("::").nth(1).unwrap_or(tail));
            return out;
        };
        // inner 形如 `1::答案` 或 `1::答案::提示`
        let inner = &tail[..end];
        out.push_str(inner.split("::").nth(1).unwrap_or(""));
        rest = &tail[end + 2..];
    }
    out.push_str(rest);
    out
}

/// 文件名 / 目录名安全化：去掉路径分隔符与不可见字符。
fn safe_component(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => '_',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    cleaned.trim().trim_matches('.').trim().to_string()
}

/// 把多行文本压成一行，供 frontmatter 单值使用。
fn single_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn write_file(path: &Path, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, text)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use memstack_core::{CardState, CardType, NoteId};
    use memstack_storage::NewNote;

    /// 真库全链路：造卡 → 复习落库 → 导出四类数据都在。
    #[tokio::test]
    async fn collect_against_real_db_includes_review_logs() {
        let db = Storage::open_in_memory().await.expect("内存库");
        let now = Utc::now();
        let deck = db.deck_ensure("算法", now).await.expect("建牌组");

        let (_, cards) = db
            .add_note(
                NewNote {
                    deck_id: deck.id,
                    content: NoteContent::QuestionAnswer {
                        front: "f".into(),
                        back: "b".into(),
                    },
                    tags: ["leetcode".to_string()].into_iter().collect(),
                    source: None,
                },
                now,
            )
            .await
            .expect("加笔记");
        assert_eq!(cards.len(), 1);

        let mut card = cards[0].clone();
        card.state = CardState::Review;
        card.stability = Some(2.0);
        card.difficulty = Some(5.0);
        card.last_review = Some(now);
        card.due = now + chrono::Duration::days(2);
        card.reps = 1;
        db.save_review(
            &card,
            &ReviewLog {
                id: None,
                card_id: card.id,
                reviewed_at: now,
                rating: memstack_core::Rating::Good,
                state_before: CardState::New,
                step_before: 0,
                stability_before: None,
                difficulty_before: None,
                interval_days: 2.0,
                elapsed_days: 0,
                duration_ms: 1200,
            },
        )
        .await
        .expect("落复习记录");

        let (decks, notes, cards_out, logs) = collect(&db, None).await.expect("导出");
        assert_eq!(decks.len(), 1);
        assert_eq!(notes.len(), 1);
        assert_eq!(cards_out.len(), 1);
        assert_eq!(logs.len(), 1, "复习记录应被导出");
        assert_eq!(logs[0].card_id, card.id);
        assert!(cards_out[0].stability.is_some(), "调度状态应保留");

        let (d, n, c, l) = collect(&db, Some("算法")).await.expect("按牌组导出");
        assert_eq!((d.len(), n.len(), c.len(), l.len()), (1, 1, 1, 1));
        assert!(
            collect(&db, Some("不存在")).await.is_err(),
            "未知牌组应报错"
        );
    }

    fn note(content: NoteContent, tags: &[&str]) -> Note {
        Note {
            id: NoteId(7),
            deck_id: DeckId(1),
            content,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            source: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn roundtrip(n: &Note) -> memstack_import::NoteDraft {
        let text = render_markdown(n, "算法");
        let parsed = memstack_import::parse_markdown_cards(&text, None).expect("应能解析回来");
        assert_eq!(parsed.notes.len(), 1, "应只解析出一张卡：\n{text}");
        parsed.notes.into_iter().next().unwrap()
    }

    #[test]
    fn qa_roundtrip() {
        let n = note(
            NoteContent::QuestionAnswer {
                front: "二分左边界，循环不变式？".into(),
                back: "区间 [l,r) 收敛".into(),
            },
            &["leetcode", "二分"],
        );
        let d = roundtrip(&n);
        assert_eq!(d.deck, "算法");
        assert_eq!(d.content, n.content);
        assert_eq!(d.tags, n.tags);
    }

    #[test]
    fn cloze_roundtrip() {
        let n = note(
            NoteContent::Cloze {
                text: "首都是{{c1::北京}}".into(),
                extra: String::new(),
            },
            &[],
        );
        let d = roundtrip(&n);
        assert_eq!(d.content, n.content);
    }

    #[test]
    fn cloze_single_line_extra_roundtrip() {
        let n = note(
            NoteContent::Cloze {
                text: "首都是{{c1::北京}}".into(),
                extra: "中国首都".into(),
            },
            &[],
        );
        let d = roundtrip(&n);
        assert_eq!(d.content, n.content);
    }

    #[test]
    fn cloze_multiline_extra_roundtrip() {
        let n = note(
            NoteContent::Cloze {
                text: "{{c1::alpha}} 是什么".into(),
                extra: "第一行\n第二行".into(),
            },
            &[],
        );
        let d = roundtrip(&n);
        assert_eq!(d.content, n.content);
    }

    #[test]
    fn code_roundtrip() {
        let n = note(
            NoteContent::CodeCloze {
                code: "let m = l + ({{c1::r - l}}) / 2;".into(),
                language: "rust".into(),
                extra: String::new(),
            },
            &["算法"],
        );
        let d = roundtrip(&n);
        assert_eq!(d.content, n.content);
    }

    #[test]
    fn code_multiline_extra_flattens() {
        let n = note(
            NoteContent::CodeCloze {
                code: "x = {{c1::1}};".into(),
                language: "rust".into(),
                extra: "a\nb".into(),
            },
            &[],
        );
        match roundtrip(&n).content {
            NoteContent::CodeCloze { extra, .. } => assert_eq!(extra, "a b"),
            other => panic!("题型不符：{other:?}"),
        }
    }

    #[test]
    fn safe_component_strips_separators() {
        assert_eq!(safe_component("a/b\\c:d"), "a_b_c_d");
        assert_eq!(safe_component("  ..trim..  "), "trim");
        assert_eq!(safe_component(""), "");
    }

    #[test]
    fn strip_cloze_keeps_answers() {
        assert_eq!(strip_cloze("首都是{{c1::北京}}"), "首都是北京");
        assert_eq!(strip_cloze("{{c1::甲::提示}}与{{c2::乙}}"), "甲与乙");
        assert_eq!(strip_cloze("无挖空"), "无挖空");
        assert_eq!(strip_cloze("残缺 {{c1::未闭合"), "残缺 未闭合");
    }

    #[test]
    fn bundle_serializes_cards_and_logs() {
        let card = Card {
            id: CardId(1),
            note_id: NoteId(1),
            deck_id: DeckId(1),
            card_type: CardType::QuestionAnswer,
            ordinal: 0,
            front: "f".into(),
            back: "b".into(),
            state: CardState::New,
            step: 0,
            stability: None,
            difficulty: None,
            due: Utc::now(),
            last_review: None,
            reps: 0,
            lapses: 0,
            suspended: false,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let log = ReviewLog {
            id: None,
            card_id: CardId(1),
            reviewed_at: Utc::now(),
            rating: memstack_core::Rating::Good,
            state_before: CardState::New,
            step_before: 0,
            stability_before: None,
            difficulty_before: None,
            interval_days: 1.0,
            elapsed_days: 0,
            duration_ms: 900,
        };
        let bundle = Bundle {
            format: BUNDLE_FORMAT,
            version: BUNDLE_VERSION,
            generated_at: Utc::now(),
            decks: Vec::new(),
            notes: Vec::new(),
            cards: vec![card],
            review_logs: vec![log],
        };
        let json = serde_json::to_string(&bundle).expect("应可序列化");
        assert!(json.contains("\"review_logs\""));
        assert!(json.contains("\"card_type\":\"question_answer\""));
        assert!(json.contains("\"rating\":\"good\""));
    }
}
