//! 把 Markdown 卡片文件拆成笔记草稿。

use memstack_core::NoteContent;
use std::collections::BTreeSet;

use crate::error::{Error, Result};

/// 待入库的笔记草稿。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft {
    pub deck: String,
    pub content: NoteContent,
    pub tags: BTreeSet<String>,
    pub source: Option<String>,
}

/// 解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFile {
    pub notes: Vec<NoteDraft>,
}

/// frontmatter / 卡片头里认得的字段。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Meta {
    deck: Option<String>,
    tags: BTreeSet<String>,
    kind: Option<Kind>,
    language: Option<String>,
    extra: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Qa,
    Cloze,
    Code,
}

/// 解析整份 Markdown 卡片文件。
pub fn parse_markdown_cards(text: &str, default_deck: Option<&str>) -> Result<ParsedFile> {
    let text = strip_bom(text);
    let (file_meta, body) = split_frontmatter(text)?;

    let mut notes = Vec::new();
    let chunks = split_cards(body);

    for (index, chunk) in chunks.iter().enumerate() {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        // 每张卡可再带一小段 frontmatter，覆盖文件级
        let (card_meta, card_body) = split_frontmatter(chunk)?;
        let meta = merge_meta(&file_meta, &card_meta);
        let deck = default_deck
            .map(str::to_string)
            .or_else(|| meta.deck.clone())
            .ok_or(Error::MissingDeck { index })?;

        let content = build_content(&meta, card_body, index)?;
        notes.push(NoteDraft {
            deck,
            content,
            tags: meta.tags.clone(),
            source: None,
        });
    }

    if notes.is_empty() {
        return Err(Error::InvalidCard {
            index: 0,
            reason: "文件里没有卡片".into(),
        });
    }
    Ok(ParsedFile { notes })
}

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// 取文件（或单卡）开头的 `---` frontmatter。
fn split_frontmatter(text: &str) -> Result<(Meta, &str)> {
    let trimmed = text.trim_start_matches('\n');
    if !trimmed.starts_with("---") {
        return Ok((Meta::default(), text));
    }

    // 第一行是 ---，找到收尾的 ---
    let after = &trimmed[3..];
    let after = after.strip_prefix('\r').unwrap_or(after);
    let after = after.strip_prefix('\n').unwrap_or(after);

    let mut rest = after;
    let mut meta_text = String::new();
    loop {
        match rest.find('\n') {
            None => {
                // 没有闭合 frontmatter，整段当正文
                return Ok((Meta::default(), text));
            }
            Some(nl) => {
                let line = rest[..nl].trim_end_matches('\r');
                if line.trim() == "---" {
                    rest = rest[nl + 1..].strip_prefix('\n').unwrap_or(&rest[nl + 1..]);
                    // 也吃掉后面的空行，但保留正文
                    let meta = parse_meta(&meta_text)?;
                    return Ok((meta, rest));
                }
                meta_text.push_str(line);
                meta_text.push('\n');
                rest = &rest[nl + 1..];
            }
        }
    }
}

fn parse_meta(text: &str) -> Result<Meta> {
    let mut meta = Meta::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();

        match key.as_str() {
            "deck" | "牌组" => meta.deck = Some(unquote(value).to_string()),
            "tags" | "标签" => meta.tags = parse_tags(value),
            "type" | "kind" | "题型" => meta.kind = parse_kind(value),
            "language" | "lang" | "语言" => meta.language = Some(unquote(value).to_string()),
            "extra" | "补充" => meta.extra = Some(unquote(value).to_string()),
            _ => {}
        }
    }
    Ok(meta)
}

pub(crate) fn parse_kind(value: &str) -> Option<Kind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "qa" | "basic" | "问答" => Some(Kind::Qa),
        "cloze" | "填空" => Some(Kind::Cloze),
        "code" | "code_cloze" | "代码" | "代码填空" => Some(Kind::Code),
        _ => None,
    }
}

pub(crate) fn parse_tags(value: &str) -> BTreeSet<String> {
    let value = value.trim();
    let inner = value
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(value);
    inner
        .split([',', '、'])
        .map(|s| unquote(s.trim()).to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// CSV 标签：逗号 / 顿号 / 空白都可分隔（兼容 Anki 导出）。
pub(crate) fn parse_csv_tags(value: &str) -> BTreeSet<String> {
    value
        .trim()
        .split([',', '、', ' ', '\t'])
        .map(|s| {
            s.trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn unquote(s: &str) -> &str {
    s.trim().trim_matches('"').trim_matches('\'').trim()
}

fn merge_meta(base: &Meta, over: &Meta) -> Meta {
    Meta {
        deck: over.deck.clone().or_else(|| base.deck.clone()),
        // 卡片自带 tags 就用卡片的，否则继承文件级
        tags: if over.tags.is_empty() {
            base.tags.clone()
        } else {
            over.tags.clone()
        },
        kind: over.kind.or(base.kind),
        language: over.language.clone().or_else(|| base.language.clone()),
        extra: over.extra.clone().or_else(|| base.extra.clone()),
    }
}

/// 按 `***` 单独成行切卡片。`---` 是正反面分隔，不切卡。
fn split_cards(body: &str) -> Vec<String> {
    let mut cards = Vec::new();
    let mut current = String::new();

    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').trim();
        if trimmed == "***" {
            if !current.trim().is_empty() {
                cards.push(current.clone());
            }
            current.clear();
        } else {
            current.push_str(line);
        }
    }
    if !current.trim().is_empty() {
        cards.push(current);
    }
    cards
}

fn build_content(meta: &Meta, body: &str, index: usize) -> Result<NoteContent> {
    let body = body.trim();
    let has_cloze = body.contains("{{c");

    let kind = match meta.kind {
        Some(k) => k,
        None => detect_kind(body, has_cloze),
    };

    match kind {
        Kind::Qa => {
            let (front, back) = split_qa(body, index)?;
            Ok(NoteContent::QuestionAnswer { front, back })
        }
        Kind::Cloze => {
            if !has_cloze {
                return Err(Error::InvalidCard {
                    index,
                    reason: "填空卡必须包含 {{c1::答案}} 挖空标记".into(),
                });
            }
            let (text, extra) = split_cloze_extra(body, meta);
            Ok(NoteContent::Cloze { text, extra })
        }
        Kind::Code => {
            let (code, fence_lang) = extract_code(body);
            if !code.contains("{{c") {
                return Err(Error::InvalidCard {
                    index,
                    reason: "代码填空必须包含 {{c1::答案}} 挖空标记".into(),
                });
            }
            let language = meta
                .language
                .clone()
                .filter(|s| !s.is_empty())
                .or(fence_lang)
                .unwrap_or_default();
            let extra = meta.extra.clone().unwrap_or_default();
            Ok(NoteContent::CodeCloze {
                code,
                language,
                extra,
            })
        }
    }
}

fn detect_kind(body: &str, has_cloze: bool) -> Kind {
    if has_cloze {
        // 正文是代码围栏就当代码填空
        if body.trim_start().starts_with("```") {
            Kind::Code
        } else {
            Kind::Cloze
        }
    } else {
        Kind::Qa
    }
}

/// 问答：`---` 分正反面；也认 `正面：` / `背面：`。
fn split_qa(body: &str, index: usize) -> Result<(String, String)> {
    // 1) 正面：/背面：
    if let Some(pair) = split_labeled(body) {
        return Ok(pair);
    }

    // 2) 第一处顶格 --- 切开
    let mut lines = body.split_inclusive('\n');
    let mut front = String::new();
    let mut back = String::new();
    let mut seen_sep = false;
    let mut buf = String::new();
    for line in lines.by_ref() {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').trim();
        if trimmed == "---" && !seen_sep {
            seen_sep = true;
            front = buf.clone();
            buf.clear();
            continue;
        }
        buf.push_str(line);
    }
    if seen_sep {
        back = buf;
    }

    let front = front.trim().to_string();
    let back = back.trim().to_string();
    if front.is_empty() || back.is_empty() {
        return Err(Error::MissingSide { index });
    }
    Ok((front, back))
}

fn split_labeled(body: &str) -> Option<(String, String)> {
    // 支持：
    //   正面：……\n\n背面：……
    //   正面:\n……\n背面:\n……
    let (_front_start, after_front) = find_label(body, &["正面：", "正面:"])?;
    let (back_start_rel, after_back_rel) = find_label(&body[after_front..], &["背面：", "背面:"])?;
    let back_start = after_front + back_start_rel;
    let after_back = after_front + after_back_rel;

    let front = body[after_front..back_start].trim().to_string();
    let back = body[after_back..].trim().to_string();
    if front.is_empty() || back.is_empty() {
        return None;
    }
    Some((front, back))
}

/// 找到 `正面：` 这类标注，返回 (标签起点, 标签之后的字节偏移)。
fn find_label(body: &str, keys: &[&str]) -> Option<(usize, usize)> {
    for k in keys {
        if let Some(pos) = body.find(k) {
            return Some((pos, pos + k.len()));
        }
    }
    None
}

/// 填空：正文里若末尾有 `---` 补充区，拆成 text / extra。
fn split_cloze_extra(body: &str, meta: &Meta) -> (String, String) {
    if let Some(extra) = meta.extra.clone() {
        return (body.to_string(), extra);
    }
    // 末尾 --- 补充
    if let Some(pos) = find_trailing_extra(body) {
        let text = body[..pos].trim().to_string();
        let extra = body[pos + 3..].trim().to_string();
        return (text, extra);
    }
    (body.to_string(), String::new())
}

fn find_trailing_extra(body: &str) -> Option<usize> {
    // 只在最后一段 `---` 之后还有内容时算补充
    let mut last = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r').trim();
        if trimmed == "---" {
            last = Some(offset);
        }
        offset += line.len();
    }
    let pos = last?;
    let after = body[pos + 3..].trim();
    if after.is_empty() {
        None
    } else {
        Some(pos)
    }
}

/// 代码填空：剥掉围栏，取语言标识。
fn extract_code(body: &str) -> (String, Option<String>) {
    let trimmed = body.trim();
    if !trimmed.starts_with("```") {
        return (trimmed.to_string(), None);
    }
    let rest = &trimmed[3..];
    let nl = rest.find('\n');
    let Some(nl) = nl else {
        return (String::new(), None);
    };
    let info = rest[..nl].trim();
    let lang = if info.is_empty() {
        None
    } else {
        Some(info.split_whitespace().next().unwrap_or(info).to_string())
    };
    let code_and_maybe_close = &rest[nl + 1..];
    // 去掉收尾 ```
    let code = code_and_maybe_close
        .rsplit_once("```")
        .map(|(c, _)| c)
        .unwrap_or(code_and_maybe_close);
    (code.trim_end_matches('\n').to_string(), lang)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> NoteDraft {
        parse_markdown_cards(text, None)
            .unwrap()
            .notes
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn design_example_qa() {
        // docs/方案.md 里的示例卡
        let text = r#"---
deck: 算法
tags: [leetcode, 二分]
---

正面：二分查找左边界模板，循环不变式是什么？

```rust
fn lower_bound(arr: &[i32], x: i32) -> usize {
    let (mut l, mut r) = (0, arr.len());
    while l < r {
        let m = l + (r - l) / 2;
        if arr[m] < x { l = m + 1 } else { r = m }
    }
    l
}
```

背面：区间 [l, r) 内 arr[l..r] 全部可能 >= x，l == r 时收敛。
"#;
        let note = one(text);
        assert_eq!(note.deck, "算法");
        assert!(note.tags.contains("leetcode"));
        assert!(note.tags.contains("二分"));
        match note.content {
            NoteContent::QuestionAnswer { front, back } => {
                assert!(front.contains("循环不变式"));
                assert!(front.contains("fn lower_bound"));
                assert!(back.contains("l == r"));
            }
            other => panic!("expected QA, got {other:?}"),
        }
    }

    #[test]
    fn qa_with_dash_separator() {
        let text = "---\ndeck: t\n---\n正面内容\n\n---\n\n背面内容\n";
        let note = one(text);
        match note.content {
            NoteContent::QuestionAnswer { front, back } => {
                assert_eq!(front, "正面内容");
                assert_eq!(back, "背面内容");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cloze_note() {
        let text = "---\ndeck: 地理\ntype: cloze\n---\n首都是{{c1::北京}}，是{{c2::政治}}中心\n";
        let note = one(text);
        assert_eq!(note.deck, "地理");
        match note.content {
            NoteContent::Cloze { text, .. } => {
                assert!(text.contains("{{c1::北京}}"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cloze_auto_detect_with_code_fence_is_code() {
        let text = "---\ndeck: 算法\n---\n```rust\nlet m = l + ({{c1::r - l}}) / 2;\n```\n";
        let note = one(text);
        match note.content {
            NoteContent::CodeCloze {
                code,
                language,
                extra: _,
            } => {
                assert!(code.contains("{{c1::r - l}}"));
                assert_eq!(language, "rust");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn code_type_without_fence() {
        let text = "---\ndeck: sql\ntype: code\nlanguage: sql\n---\nSELECT {{c1::id}} FROM t;\n";
        let note = one(text);
        match note.content {
            NoteContent::CodeCloze { code, language, .. } => {
                assert_eq!(code, "SELECT {{c1::id}} FROM t;");
                assert_eq!(language, "sql");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn batch_three_cards() {
        let text = r#"---
deck: 批量
tags: [t1]
---
问1
---
答1

***

问2
---
答2

***

---
tags: [t2]
---
{{c1::填空}}内容
"#;
        let file = parse_markdown_cards(text, None).unwrap();
        assert_eq!(file.notes.len(), 3);
        assert_eq!(file.notes[0].deck, "批量");
        assert!(file.notes[0].tags.contains("t1"));
        assert!(matches!(
            file.notes[0].content,
            NoteContent::QuestionAnswer { .. }
        ));
        assert!(matches!(
            file.notes[1].content,
            NoteContent::QuestionAnswer { .. }
        ));
        assert!(matches!(file.notes[2].content, NoteContent::Cloze { .. }));
        assert!(file.notes[2].tags.contains("t2"));
        // 卡片自带 tags 时不再合并文件级
        assert!(!file.notes[2].tags.contains("t1"));
    }

    #[test]
    fn default_deck_cli_override() {
        let text = "---\ndeck: 原牌组\n---\n问\n---\n答\n";
        let file = parse_markdown_cards(text, Some("覆盖")).unwrap();
        assert_eq!(file.notes[0].deck, "覆盖");
    }

    #[test]
    fn missing_deck_errors() {
        let text = "问\n---\n答\n";
        assert!(matches!(
            parse_markdown_cards(text, None),
            Err(Error::MissingDeck { .. })
        ));
    }

    #[test]
    fn cloze_with_trailing_extra() {
        let text = "---\ndeck: t\ntype: cloze\n---\n{{c1::北京}}是首都\n\n---\n\n补充说明在这里\n";
        let note = one(text);
        match note.content {
            NoteContent::Cloze { text, extra } => {
                assert!(text.contains("{{c1::北京}}"));
                assert_eq!(extra, "补充说明在这里");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_file_errors() {
        assert!(parse_markdown_cards("", None).is_err());
    }
}
