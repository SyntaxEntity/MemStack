//! CSV 卡片导入。
//!
//! 两种写法：
//!
//! **有表头**（推荐，列名大小写不敏感）：
//!
//! ```text
//! front,back,tags,deck
//! 什么是二分？,有序数组上对半查找,"leetcode,二分",算法
//! ```
//!
//! 扩展列全部可省：`type`(`qa`/`cloze`/`code`)、`deck`、`tags`、`front`、`back`、
//! `text`（填空正文）、`code`、`language`、`extra`。
//!
//! **无表头**（Anki 习惯）：
//!
//! ```text
//! 正面,背面[,标签]
//! ```
//!
//! 规则：
//! - `--deck` 覆盖行内 `deck`
//! - 题型：`code` 列非空 → 代码填空；`text`/`front` 含 `{{cN::` → 填空；否则问答
//! - 标签按逗号 / 顿号 / 空白分隔

use memstack_core::NoteContent;
use std::path::Path;

use crate::error::{Error, Result};
use crate::parse::{parse_csv_tags, parse_kind, Kind, NoteDraft};

/// 解析 CSV 文本为笔记草稿。`default_deck` 覆盖行内 deck。
pub fn parse_csv_cards(text: &str, default_deck: Option<&str>) -> Result<Vec<NoteDraft>> {
    parse_csv_cards_with_delim(text, default_deck, b',')
}

/// 同上，可指定分隔符（`b','` 或 `b'\t'`）。
pub fn parse_csv_cards_with_delim(
    text: &str,
    default_deck: Option<&str>,
    delimiter: u8,
) -> Result<Vec<NoteDraft>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::Fields)
        .from_reader(text.as_bytes());

    let mut rows: Vec<csv::StringRecord> = Vec::new();
    for row in reader.records() {
        rows.push(row?);
    }
    if rows.is_empty() {
        return Err(Error::BadCsvRow {
            index: 0,
            reason: "文件是空的".into(),
        });
    }

    let header = detect_header(&rows[0]);
    let (cols, data_start) = match header {
        Some(cols) => (cols, 1),
        None => (Cols::legacy(), 0),
    };

    let mut notes = Vec::new();
    for (i, row) in rows[data_start..].iter().enumerate() {
        let index = data_start + i + 1; // 1-based，含表头
        if row.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        notes.push(row_to_draft(row, &cols, default_deck, index)?);
    }

    if notes.is_empty() {
        return Err(Error::BadCsvRow {
            index: 0,
            reason: "没有解析到卡片".into(),
        });
    }
    Ok(notes)
}

/// 读文件并解析。`.tsv` 用制表符分隔。
pub fn import_csv_file(path: &str, default_deck: Option<&str>) -> Result<Vec<NoteDraft>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_string(),
        source,
    })?;
    let delim = if Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("tsv"))
    {
        b'\t'
    } else {
        b','
    };
    let mut notes = parse_csv_cards_with_delim(&text, default_deck, delim)?;
    for n in &mut notes {
        n.source = Some(path.to_string());
    }
    Ok(notes)
}

/// 列位置。未知列忽略。
#[derive(Debug, Clone, Copy, Default)]
struct Cols {
    front: Option<usize>,
    back: Option<usize>,
    text: Option<usize>,
    code: Option<usize>,
    kind: Option<usize>,
    deck: Option<usize>,
    tags: Option<usize>,
    language: Option<usize>,
    extra: Option<usize>,
}

impl Cols {
    /// 无表头：`front,back[,tags[,deck]]`
    fn legacy() -> Self {
        Self {
            front: Some(0),
            back: Some(1),
            tags: Some(2),
            deck: Some(3),
            ..Self::default()
        }
    }
}

fn detect_header(row: &csv::StringRecord) -> Option<Cols> {
    let mut cols = Cols::default();
    let mut known = 0usize;
    for (i, field) in row.iter().enumerate() {
        let key = field.trim().to_ascii_lowercase();
        match key.as_str() {
            "front" | "正面" | "question" | "问" => {
                cols.front = Some(i);
                known += 1;
            }
            "back" | "背面" | "answer" | "答" => {
                cols.back = Some(i);
                known += 1;
            }
            "text" | "正文" | "cloze" => {
                cols.text = Some(i);
                known += 1;
            }
            "code" | "代码" => {
                cols.code = Some(i);
                known += 1;
            }
            "type" | "kind" | "题型" => {
                cols.kind = Some(i);
                known += 1;
            }
            "deck" | "牌组" => {
                cols.deck = Some(i);
                known += 1;
            }
            "tags" | "tag" | "标签" => {
                cols.tags = Some(i);
                known += 1;
            }
            "language" | "lang" | "语言" => {
                cols.language = Some(i);
                known += 1;
            }
            "extra" | "补充" | "note" | "备注" => {
                cols.extra = Some(i);
                known += 1;
            }
            _ => {}
        }
    }
    // 至少认得 2 个有名列才算表头；否则当数据行（无表头模式）
    if known >= 2 {
        Some(cols)
    } else {
        None
    }
}

fn field(row: &csv::StringRecord, col: Option<usize>) -> &str {
    col.and_then(|i| row.get(i)).unwrap_or("").trim()
}

fn row_to_draft(
    row: &csv::StringRecord,
    cols: &Cols,
    default_deck: Option<&str>,
    index: usize,
) -> Result<NoteDraft> {
    let front = field(row, cols.front);
    let back = field(row, cols.back);
    let text = field(row, cols.text);
    let code = field(row, cols.code);
    let language = field(row, cols.language);
    let extra = field(row, cols.extra);
    let kind_raw = field(row, cols.kind);
    let tags_raw = field(row, cols.tags);
    let deck_raw = field(row, cols.deck);

    let deck = default_deck
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            let d = deck_raw.trim();
            if d.is_empty() {
                None
            } else {
                Some(d.to_string())
            }
        })
        .ok_or(Error::MissingDeck { index })?;

    let has_cloze = text.contains("{{c") || front.contains("{{c") || code.contains("{{c");
    let kind = parse_kind(kind_raw).unwrap_or(if !code.is_empty() {
        Kind::Code
    } else if has_cloze {
        Kind::Cloze
    } else {
        Kind::Qa
    });

    let content = match kind {
        Kind::Qa => {
            if front.is_empty() || back.is_empty() {
                return Err(Error::MissingSide { index });
            }
            NoteContent::QuestionAnswer {
                front: front.to_string(),
                back: back.to_string(),
            }
        }
        Kind::Cloze => {
            let body = if !text.is_empty() { text } else { front };
            if !body.contains("{{c") {
                return Err(Error::BadCsvRow {
                    index,
                    reason: "填空卡必须包含 {{c1::答案}} 挖空标记".into(),
                });
            }
            NoteContent::Cloze {
                text: body.to_string(),
                extra: extra.to_string(),
            }
        }
        Kind::Code => {
            let body = if !code.is_empty() { code } else { front };
            if !body.contains("{{c") {
                return Err(Error::BadCsvRow {
                    index,
                    reason: "代码填空必须包含 {{c1::答案}} 挖空标记".into(),
                });
            }
            NoteContent::CodeCloze {
                code: body.to_string(),
                language: language.to_string(),
                extra: extra.to_string(),
            }
        }
    };

    Ok(NoteDraft {
        deck,
        content,
        tags: parse_csv_tags(tags_raw),
        source: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(text: &str) -> Vec<NoteDraft> {
        parse_csv_cards(text, Some("默认")).unwrap()
    }

    #[test]
    fn headerless_qa() {
        let got = notes("什么是二分？,对半查找\n链表环？,快慢指针\n");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].deck, "默认");
        match &got[0].content {
            NoteContent::QuestionAnswer { front, back } => {
                assert_eq!(front, "什么是二分？");
                assert_eq!(back, "对半查找");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn headerless_with_tags() {
        let got = notes("Q1,A1,\"algo,二分\"\n");
        assert!(got[0].tags.contains("algo"));
        assert!(got[0].tags.contains("二分"));
    }

    #[test]
    fn header_qa_deck_override() {
        let text = "front,back,deck,tags\n问题,答案,行内牌组,t1\n";
        let got = parse_csv_cards(text, Some("命令行")).unwrap();
        assert_eq!(got[0].deck, "命令行");
        assert!(got[0].tags.contains("t1"));
    }

    #[test]
    fn header_qa_row_deck() {
        let text = "front,back,deck\n问题,答案,算法\n";
        let got = parse_csv_cards(text, None).unwrap();
        assert_eq!(got[0].deck, "算法");
    }

    #[test]
    fn cloze_auto_from_text() {
        let text = "text,deck,extra\n首都是{{c1::北京}},地理,补充说明\n";
        let got = parse_csv_cards(text, None).unwrap();
        match &got[0].content {
            NoteContent::Cloze { text, extra } => {
                assert!(text.contains("北京"));
                assert_eq!(extra, "补充说明");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cloze_from_front_column() {
        let text = "front,back\n{{c1::答案}}在这里,\n";
        let got = parse_csv_cards(text, Some("d")).unwrap();
        assert!(matches!(got[0].content, NoteContent::Cloze { .. }));
    }

    #[test]
    fn code_cloze() {
        let text = "type,code,language,deck\ncode,\"let m = l + ({{c1::r - l}}) / 2;\",rust,算法\n";
        let got = parse_csv_cards(text, None).unwrap();
        match &got[0].content {
            NoteContent::CodeCloze {
                code,
                language,
                extra,
            } => {
                assert!(code.contains("{{c1::"));
                assert_eq!(language, "rust");
                assert_eq!(extra, "");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn code_column_implies_code_cloze() {
        let text = "code,language\n\"let x = {{c1::1}};\",python\n";
        let got = parse_csv_cards(text, Some("d")).unwrap();
        assert!(matches!(got[0].content, NoteContent::CodeCloze { .. }));
    }

    #[test]
    fn quoted_commas_and_newlines() {
        let text = "front,back\n\"含,逗号\",\"多\n行\"\n";
        let got = notes(text);
        match &got[0].content {
            NoteContent::QuestionAnswer { front, back } => {
                assert_eq!(front, "含,逗号");
                assert_eq!(back, "多\n行");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn chinese_headers() {
        let text = "正面,背面,标签,牌组\n问,答,a b,地理\n";
        let got = parse_csv_cards(text, None).unwrap();
        assert_eq!(got[0].deck, "地理");
        assert!(got[0].tags.contains("a"));
        assert!(got[0].tags.contains("b"));
    }

    #[test]
    fn skips_blank_rows() {
        let got = notes("front,back\nQ,A\n,,\n\nQ2,A2\n");
        assert_eq!(got.len(), 2);
    }

    #[test]
    fn missing_deck_errors() {
        let err = parse_csv_cards("front,back\nQ,A\n", None).unwrap_err();
        assert!(matches!(err, Error::MissingDeck { index: 2 }));
    }

    #[test]
    fn missing_back_errors() {
        let err = parse_csv_cards("front,back\nQ,\n", Some("d")).unwrap_err();
        assert!(matches!(err, Error::MissingSide { index: 2 }));
    }

    #[test]
    fn bom_and_crlf() {
        let got = notes("\u{feff}front,back\r\nQ,A\r\n");
        match &got[0].content {
            NoteContent::QuestionAnswer { front, back } => {
                assert_eq!(front, "Q");
                assert_eq!(back, "A");
            }
            other => panic!("{other:?}"),
        }
    }
}
