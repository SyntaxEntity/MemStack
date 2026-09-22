//! 挖空解析与出卡。
//!
//! 语法兼容 Anki：`{{c1::答案}}` / `{{c1::答案::提示}}`。
//! 同一序号可在文中出现多次，出卡时一起挖掉。

use crate::error::{Error, Result};
use crate::model::{CardType, NoteContent};

/// 解析出的片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Segment {
    Text(String),
    Cloze {
        /// `{{cN::...}}` 中的 N
        id: u32,
        answer: String,
        hint: Option<String>,
    },
}

/// 一张生成出来的卡面草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardDraft {
    pub card_type: CardType,
    /// 0 起，按挖空编号首次出现顺序
    pub ordinal: u16,
    pub front: String,
    pub back: String,
}

/// 把文本拆成普通片段与挖空片段。
pub fn parse_segments(text: &str) -> Result<Vec<Segment>> {
    let mut segments = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find("{{") {
        if start > 0 {
            segments.push(Segment::Text(rest[..start].to_string()));
        }

        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or_else(|| Error::UnclosedCloze {
            context: truncate_for_error(rest, 40),
        })?;

        segments.push(parse_cloze_body(&after[..end])?);
        rest = &after[end + 2..];
    }

    if !rest.is_empty() {
        segments.push(Segment::Text(rest.to_string()));
    }

    Ok(segments)
}

/// 按笔记内容生成全部卡面。问答出 1 张，填空按编号出 N 张。
pub fn generate_cards(content: &NoteContent) -> Result<Vec<CardDraft>> {
    match content {
        NoteContent::QuestionAnswer { front, back } => Ok(vec![CardDraft {
            card_type: CardType::QuestionAnswer,
            ordinal: 0,
            front: front.clone(),
            back: back.clone(),
        }]),
        NoteContent::Cloze { text, extra } => {
            let drafts = cloze_cards(text, CardType::Cloze)?;
            Ok(attach_extra(drafts, extra))
        }
        NoteContent::CodeCloze {
            code,
            language: _,
            extra,
        } => {
            let drafts = cloze_cards(code, CardType::CodeCloze)?;
            Ok(attach_extra(drafts, extra))
        }
    }
}

fn attach_extra(mut drafts: Vec<CardDraft>, extra: &str) -> Vec<CardDraft> {
    if extra.is_empty() {
        return drafts;
    }
    for draft in &mut drafts {
        draft.back.push_str("\n\n---\n\n");
        draft.back.push_str(extra);
    }
    drafts
}

fn cloze_cards(text: &str, card_type: CardType) -> Result<Vec<CardDraft>> {
    let segments = parse_segments(text)?;
    let mut ids: Vec<u32> = Vec::new();
    for segment in &segments {
        if let Segment::Cloze { id, .. } = segment {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
    }
    ids.sort_unstable();
    if ids.is_empty() {
        return Err(Error::NoClozeMarkers);
    }

    let mut drafts = Vec::with_capacity(ids.len());
    for (ordinal, id) in ids.into_iter().enumerate() {
        let front = render(&segments, Some(id), true);
        let back = render(&segments, Some(id), false);
        drafts.push(CardDraft {
            card_type,
            ordinal: ordinal as u16,
            front,
            back,
        });
    }
    Ok(drafts)
}

/// 按挖空编号渲染正反面。`id` 为 `cN` 的 N。
///
/// - 正面：当前序号挖成 `[提示]` / `[...]`，其余序号露出答案
/// - 背面：全部露出答案
pub fn cloze_faces(text: &str, id: u32) -> Result<(String, String)> {
    let segments = parse_segments(text)?;
    Ok((
        render(&segments, Some(id), true),
        render(&segments, Some(id), false),
    ))
}

/// 列出文中出现过的挖空编号（升序去重）。
pub fn cloze_ids(text: &str) -> Result<Vec<u32>> {
    let segments = parse_segments(text)?;
    let mut ids: Vec<u32> = segments
        .iter()
        .filter_map(|s| match s {
            Segment::Cloze { id, .. } => Some(*id),
            Segment::Text(_) => None,
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    Ok(ids)
}

fn render(segments: &[Segment], current: Option<u32>, hide_current: bool) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            Segment::Text(t) => out.push_str(t),
            Segment::Cloze { id, answer, hint } => {
                let hide = hide_current && current == Some(*id);
                if hide {
                    out.push('[');
                    out.push_str(hint.as_deref().filter(|h| !h.is_empty()).unwrap_or("..."));
                    out.push(']');
                } else {
                    out.push_str(answer);
                }
            }
        }
    }
    out
}

fn parse_cloze_body(body: &str) -> Result<Segment> {
    let rest = body
        .strip_prefix('c')
        .ok_or_else(|| Error::InvalidCloze(body.to_string()))?;

    let split = rest
        .find("::")
        .ok_or_else(|| Error::InvalidCloze(body.to_string()))?;
    let id: u32 = rest[..split]
        .parse()
        .map_err(|_| Error::InvalidCloze(body.to_string()))?;
    let payload = &rest[split + 2..];

    // Anki 语义：cN 之后第一个 `::` 分隔答案与提示；提示里可以再有 `::`
    let (answer, hint) = match payload.find("::") {
        Some(i) => (payload[..i].to_string(), Some(payload[i + 2..].to_string())),
        None => (payload.to_string(), None),
    };

    Ok(Segment::Cloze { id, answer, hint })
}

fn truncate_for_error(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}...")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drafts(text: &str) -> Vec<CardDraft> {
        cloze_cards(text, CardType::Cloze).expect("should parse")
    }

    #[test]
    fn qa_generates_single_card() {
        let cards = generate_cards(&NoteContent::QuestionAnswer {
            front: "二分左边界？".into(),
            back: "循环不变式".into(),
        })
        .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "二分左边界？");
        assert_eq!(cards[0].back, "循环不变式");
    }

    #[test]
    fn single_cloze() {
        let cards = drafts("首都是{{c1::北京}}。");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "首都是[...]。");
        assert_eq!(cards[0].back, "首都是北京。");
    }

    #[test]
    fn cloze_with_hint() {
        let cards = drafts("{{c1::北京::中国}}");
        assert_eq!(cards[0].front, "[中国]");
        assert_eq!(cards[0].back, "北京");
    }

    #[test]
    fn multi_cloze_each_gets_a_card() {
        let cards = drafts("{{c1::TCP}} 在 {{c2::传输层}}。");
        assert_eq!(cards.len(), 2);
        // 按编号升序：c1 → ordinal 0，c2 → ordinal 1
        assert_eq!(cards[0].ordinal, 0);
        assert_eq!(cards[0].front, "[...] 在 传输层。");
        assert_eq!(cards[0].back, "TCP 在 传输层。");
        assert_eq!(cards[1].ordinal, 1);
        assert_eq!(cards[1].front, "TCP 在 [...]。");
        assert_eq!(cards[1].back, "TCP 在 传输层。");
    }

    #[test]
    fn same_id_hidden_together() {
        let cards = drafts("{{c1::a}} 和 {{c1::b}}");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].front, "[...] 和 [...]");
        assert_eq!(cards[0].back, "a 和 b");
    }

    #[test]
    fn code_cloze_writes_code_back() {
        let cards = generate_cards(&NoteContent::CodeCloze {
            code: "let m = l + ({{c1::r - l}}) / 2;".into(),
            language: "rust".into(),
            extra: String::new(),
        })
        .unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].card_type, CardType::CodeCloze);
        assert_eq!(cards[0].front, "let m = l + ([...]) / 2;");
        assert_eq!(cards[0].back, "let m = l + (r - l) / 2;");
    }

    #[test]
    fn hint_allows_colons_in_hint_only() {
        // 第一个 :: 切开答案与提示，提示里允许再出现 ::
        let cards = drafts("{{c1::fn::签名::示例}}");
        assert_eq!(cards[0].front, "[签名::示例]");
        assert_eq!(cards[0].back, "fn");
    }

    #[test]
    fn missing_cloze_errors() {
        assert!(matches!(
            cloze_cards("没有挖空", CardType::Cloze),
            Err(Error::NoClozeMarkers)
        ));
    }

    #[test]
    fn unclosed_cloze_errors() {
        assert!(matches!(
            parse_segments("{{c1::abc"),
            Err(Error::UnclosedCloze { .. })
        ));
    }
}
