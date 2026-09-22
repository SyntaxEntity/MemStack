//! Anki 原始行 → 忆栈领域模型。

use super::reader::{RawCard, RawCollection};
use super::types::{AnkiImport, ImportedCard, ImportedNote, ImportedReview};
use crate::error::{Error, Result};
use chrono::{DateTime, TimeZone, Utc};
use memstack_core::{
    cloze_faces, cloze_ids, parse_segments, CardState, CardType, NoteContent, Rating,
};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

const FIELD_SEP: char = '\x1f';

pub fn build_import(raw: &RawCollection) -> Result<AnkiImport> {
    let models: Value = serde_json::from_str(&raw.models_json)?;
    let decks: Value = serde_json::from_str(&raw.decks_json)?;

    let deck_names = parse_deck_names(&decks);
    let model_info = parse_models(&models);

    let mut notes = Vec::with_capacity(raw.notes.len());
    let mut anki_cards: HashMap<i64, (usize, usize)> = HashMap::new(); // anki_card_id → (note_idx, card_idx)

    for rn in &raw.notes {
        let model = model_info.get(&rn.mid).ok_or_else(|| Error::Template {
            id: rn.id,
            reason: format!("找不到模型 {}", rn.mid),
        })?;

        let cards_for_note: Vec<&RawCard> = raw.cards.iter().filter(|c| c.nid == rn.id).collect();
        let deck_name = cards_for_note
            .first()
            .and_then(|c| deck_names.get(&c.did))
            .cloned()
            .unwrap_or_else(|| "Default".to_string());

        let fields: Vec<&str> = rn.flds.split(FIELD_SEP).collect();
        let (content, rendered) = render_note(model, rn.id, &fields)?;

        let mut cards = Vec::new();
        for rc in &cards_for_note {
            let (front, back) = rendered
                .get(rc.ord as usize)
                .cloned()
                .or_else(|| rendered.first().cloned())
                .unwrap_or_else(|| (String::new(), String::new()));

            let imported = map_card(rc, &deck_name, content.card_type(), front, back, raw.crt)?;
            let note_idx = notes.len();
            let card_idx = cards.len();
            anki_cards.insert(rc.id, (note_idx, card_idx));
            cards.push(imported);
        }

        notes.push(ImportedNote {
            anki_id: rn.id,
            deck_name,
            content,
            tags: parse_tags(&rn.tags),
            cards,
            created_at: ms_to_utc(rn.mod_ms),
        });
    }

    let mut reviews = Vec::new();
    let mut last_per_card: HashMap<i64, DateTime<Utc>> = HashMap::new();
    for rv in &raw.revlogs {
        if !anki_cards.contains_key(&rv.cid) {
            continue;
        }
        let reviewed_at = revlog_time(rv.id);
        let elapsed_days = match last_per_card.get(&rv.cid) {
            Some(prev) => {
                let days = (reviewed_at - *prev).num_days();
                days.clamp(0, i32::MAX as i64) as u32
            }
            None => 0,
        };
        last_per_card.insert(rv.cid, reviewed_at);

        reviews.push(ImportedReview {
            anki_card_id: rv.cid,
            reviewed_at,
            rating: map_rating(rv.ease)?,
            state_before: map_revlog_state(rv.rtype),
            step_before: 0,
            interval_days: ivl_to_days(rv.ivl),
            elapsed_days,
            duration_ms: rv.time_ms.clamp(0, u32::MAX as i64) as u32,
        });
    }

    Ok(AnkiImport { notes, reviews })
}

// ---------- models / decks ----------

struct ModelInfo {
    #[allow(dead_code)]
    name: String,
    /// 0=标准 1=挖空
    is_cloze: bool,
    field_names: Vec<String>,
    /// 每个模板 (qfmt, afmt)
    templates: Vec<(String, String)>,
}

fn parse_deck_names(decks: &Value) -> HashMap<i64, String> {
    let mut map = HashMap::new();
    let Some(obj) = decks.as_object() else {
        return map;
    };
    for (_k, v) in obj {
        let id = v.get("id").and_then(Value::as_i64);
        let name = v.get("name").and_then(Value::as_str).unwrap_or("Default");
        if let Some(id) = id {
            map.insert(id, name.to_string());
        }
    }
    map
}

fn parse_models(models: &Value) -> HashMap<i64, ModelInfo> {
    let mut map = HashMap::new();
    let Some(obj) = models.as_object() else {
        return map;
    };
    for (_k, v) in obj {
        let id = v
            .get("id")
            .and_then(Value::as_i64)
            .or_else(|| _k.parse::<i64>().ok());
        let Some(id) = id else { continue };

        let name = v
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Basic")
            .to_string();
        let is_cloze = v.get("type").and_then(Value::as_i64) == Some(1);

        let field_names = v
            .get("flds")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|f| {
                        f.get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("?")
                            .to_string()
                    })
                    .collect()
            })
            .unwrap_or_default();

        let templates = v
            .get("tmpls")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|t| {
                        let q = t
                            .get("qfmt")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let a = t
                            .get("afmt")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        (q, a)
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![("{{Front}}".into(), "{{Front}}<hr id=answer>{{Back}}".into())]
            });

        map.insert(
            id,
            ModelInfo {
                name,
                is_cloze,
                field_names,
                templates,
            },
        );
    }
    map
}

// ---------- note rendering ----------

/// 产出笔记内容 + 每个模板序号上的 (front, back)。
fn render_note(
    model: &ModelInfo,
    note_id: i64,
    fields: &[&str],
) -> Result<(NoteContent, Vec<(String, String)>)> {
    let named: Vec<(String, String)> = model
        .field_names
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let v = fields.get(i).copied().unwrap_or("");
            (name.clone(), html_to_text(v))
        })
        .collect();

    if model.is_cloze {
        render_cloze_note(&named)
    } else {
        render_qa_note(model, note_id, &named)
    }
}

fn render_cloze_note(named: &[(String, String)]) -> Result<(NoteContent, Vec<(String, String)>)> {
    // 找出带挖空的字段，拼成一段 text（通常就一个 Text 字段）
    let mut cloze_src = String::new();
    let mut extra_parts: Vec<String> = Vec::new();
    let mut language = String::new();

    for (name, value) in named {
        if value.contains("{{c") {
            if !cloze_src.is_empty() {
                cloze_src.push_str("\n\n");
            }
            cloze_src.push_str(value);
        } else if name.eq_ignore_ascii_case("language") || name.eq_ignore_ascii_case("lang") {
            language = value.clone();
        } else if !value.trim().is_empty() {
            extra_parts.push(format!("{name}: {value}"));
        }
    }

    if cloze_src.trim().is_empty() {
        // 兜底：把第一个字段当挖空正文
        cloze_src = named.first().map(|(_, v)| v.clone()).unwrap_or_default();
    }

    let ids = cloze_ids(&cloze_src)?;
    let mut rendered = Vec::new();
    // Anki 的 ord = cloze_number - 1；编号有空洞时也按 N-1 对齐
    let max_ord = ids.iter().map(|id| id.saturating_sub(1)).max().unwrap_or(0);
    let mut faces = vec![(String::new(), String::new()); (max_ord as usize) + 1];
    for id in ids {
        let (front, back) = cloze_faces(&cloze_src, id)?;
        faces[(id - 1) as usize] = (front, back);
    }
    rendered.extend(faces);

    let extra = extra_parts.join("\n");
    let content = if !language.is_empty() {
        NoteContent::CodeCloze {
            code: cloze_src,
            language,
            extra,
        }
    } else {
        NoteContent::Cloze {
            text: cloze_src,
            extra,
        }
    };
    Ok((content, rendered))
}

fn render_qa_note(
    model: &ModelInfo,
    note_id: i64,
    named: &[(String, String)],
) -> Result<(NoteContent, Vec<(String, String)>)> {
    let get = |name: &str| {
        named
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };

    let mut rendered = Vec::new();
    for (qfmt, afmt) in &model.templates {
        let front = expand_template(qfmt, named, note_id)?;
        // 答案侧可引用 {{FrontSide}}
        let front_side = front.clone();
        let mut back = expand_template(afmt, named, note_id)?;
        back = back.replace("{{FrontSide}}", &front_side);
        back = back
            .replace("<hr id=answer>", "\n\n--------\n\n")
            .replace("<hr>", "\n\n--------\n\n");
        rendered.push((html_to_text(&front), html_to_text(&back)));
    }

    // NoteContent 用最常见字段名，便于以后编辑
    let front_field = if !get("Front").is_empty() {
        get("Front")
    } else if !get("Question").is_empty() {
        get("Question")
    } else {
        named.first().map(|(_, v)| v.clone()).unwrap_or_default()
    };
    let back_field = if !get("Back").is_empty() {
        get("Back")
    } else if !get("Answer").is_empty() {
        get("Answer")
    } else {
        named
            .iter()
            .skip(1)
            .map(|(n, v)| {
                if v.is_empty() {
                    String::new()
                } else {
                    format!("{n}: {v}")
                }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };

    Ok((
        NoteContent::QuestionAnswer {
            front: front_field,
            back: back_field,
        },
        rendered,
    ))
}

/// 极简 Anki 模板展开：`{{Field}}`、`{{#Field}}…{{/Field}}`、`{{^Field}}…{{/Field}}`。
fn expand_template(fmt: &str, named: &[(String, String)], note_id: i64) -> Result<String> {
    let mut s = fmt.to_string();

    // 条件块（可嵌套不支持，够用）
    for (name, value) in named {
        let filled = !value.trim().is_empty();
        // {{#name}}...{{/name}}
        let open = format!("{{{{#{name}}}}}");
        let open_neg = format!("{{{{^{name}}}}}");
        let close = format!("{{{{/{name}}}}}");
        s = strip_or_keep_block(&s, &open, &close, filled);
        s = strip_or_keep_block(&s, &open_neg, &close, !filled);
    }

    // 字段替换
    for (name, value) in named {
        let pat = format!("{{{{{name}}}}}");
        s = s.replace(&pat, value);
    }

    // 去掉未知标签，避免库里残留 {{...}}
    // 但保留已解析的挖空 {{cN::...}}（挖空卡不走这里）
    if s.contains("{{") {
        s = remove_unknown_tags(&s, note_id)?;
    }
    Ok(s)
}

fn strip_or_keep_block(s: &str, open: &str, close: &str, keep: bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after = &rest[start + open.len()..];
        if let Some(end) = after.find(close) {
            if keep {
                out.push_str(&after[..end]);
            }
            rest = &after[end + close.len()..];
        } else {
            // 没闭合，原样保留
            out.push_str(open);
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

fn remove_unknown_tags(s: &str, note_id: i64) -> Result<String> {
    // 像挖空的留下；其它 {{...}} 可能是 FrontSide/杂讯
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("}}") {
            Some(end) => {
                let body = &after[..end];
                let is_cloze =
                    body.starts_with('c') && body[1..].starts_with(|c: char| c.is_ascii_digit());
                let is_front_side = body == "FrontSide";
                if is_cloze {
                    out.push_str("{{");
                    out.push_str(body);
                    out.push_str("}}");
                } else if !is_front_side && parse_segments(&format!("{{{{{body}}}}}")).is_err() {
                    // 丢弃无效标签
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str("{{");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    let _ = note_id;
    Ok(out)
}

fn html_to_text(s: &str) -> String {
    let mut s = s.to_string();
    for (from, to) in [
        ("<br>", "\n"),
        ("<br/>", "\n"),
        ("<br />", "\n"),
        ("<BR>", "\n"),
        ("</div>", "\n"),
        ("</p>", "\n"),
        ("<div>", ""),
        ("<p>", ""),
    ] {
        s = s.replace(from, to);
    }
    s.trim().to_string()
}

// ---------- card scheduling ----------

fn map_card(
    rc: &RawCard,
    _deck_name: &str,
    card_type: CardType,
    front: String,
    back: String,
    crt: i64,
) -> Result<ImportedCard> {
    let suspended = rc.queue == -1 || rc.queue == -2;
    let state = match rc.ctype {
        0 => CardState::New,
        1 => CardState::Learning,
        2 => CardState::Review,
        3 => CardState::Relearning,
        _ => CardState::Review,
    };

    let due = match state {
        CardState::New => Utc::now(),
        CardState::Learning | CardState::Relearning => learning_due(rc.due),
        CardState::Review => review_due(rc.due, crt),
    };

    // last_review：从 revlog 事后填；这里用 ivl 粗算
    let last_review = if state == CardState::Review && rc.reps > 0 {
        let days = rc.ivl.max(1);
        Some(Utc::now() - chrono::Duration::days(days))
    } else if rc.reps > 0 {
        Some(learning_due(rc.due).min(Utc::now()))
    } else {
        None
    };

    // FSRS 近似：stability≈间隔天数，difficulty 由 ease factor 映射
    let stability = if state == CardState::Review && rc.ivl > 0 {
        Some(rc.ivl as f32)
    } else {
        None
    };
    let difficulty = if rc.factor > 0 {
        // factor 是千分比（2500=250%）
        let ease = rc.factor as f32 / 1000.0;
        Some((13.0 - ease * 4.0).clamp(1.0, 10.0))
    } else {
        None
    };

    // 学习剩余步数放在 left 的高 16 bit（Anki 打包）
    let step = if state.is_in_steps() {
        ((rc.left >> 16) & 0xffff) as u8
    } else {
        0
    };

    Ok(ImportedCard {
        anki_id: rc.id,
        ordinal: rc.ord.clamp(0, u16::MAX as i64) as u16,
        card_type,
        front,
        back,
        state,
        step,
        stability,
        difficulty,
        due,
        last_review,
        reps: rc.reps.clamp(0, u32::MAX as i64) as u32,
        lapses: rc.lapses.clamp(0, u32::MAX as i64) as u32,
        suspended,
    })
}

/// 学习卡 due：时间戳。大于 1e12 当毫秒，大于 1e9 当秒。
fn learning_due(due: i64) -> DateTime<Utc> {
    if due > 1_000_000_000_000 {
        ms_to_utc(due)
    } else if due > 1_000_000_000 {
        secs_to_utc(due)
    } else {
        // 相对秒数（很少见）
        Utc::now() + chrono::Duration::seconds(due.max(0))
    }
}

/// 复习卡 due：从 col.crt 起的天序号。
fn review_due(due: i64, crt: i64) -> DateTime<Utc> {
    secs_to_utc(crt + due * 86_400)
}

fn ivl_to_days(ivl: i64) -> f32 {
    if ivl > 0 {
        ivl as f32
    } else if ivl < 0 {
        // 负值是秒（学习中）
        (-ivl) as f32 / 86_400.0
    } else {
        0.0
    }
}

fn map_rating(ease: i64) -> Result<Rating> {
    match ease {
        1 => Ok(Rating::Again),
        2 => Ok(Rating::Hard),
        3 => Ok(Rating::Good),
        4 => Ok(Rating::Easy),
        // ease 0 出现在某些 learn 记录里，按 Again 处理
        0 => Ok(Rating::Again),
        other => Err(Error::Template {
            id: 0,
            reason: format!("未知 ease {other}"),
        }),
    }
}

fn map_revlog_state(rtype: i64) -> CardState {
    match rtype {
        0 => CardState::Learning,
        1 => CardState::Review,
        2 => CardState::Relearning,
        _ => CardState::Review,
    }
}

fn parse_tags(raw: &str) -> BTreeSet<String> {
    raw.split_whitespace()
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

fn ms_to_utc(ms: i64) -> DateTime<Utc> {
    // Anki note.mod 是秒；revlog.id 是毫秒。两者都按数值粗分。
    if ms > 1_000_000_000_000 {
        DateTime::from_timestamp_millis(ms).unwrap_or_else(Utc::now)
    } else {
        DateTime::from_timestamp(ms, 0).unwrap_or_else(Utc::now)
    }
}

fn secs_to_utc(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).single().unwrap_or_else(Utc::now)
}

fn revlog_time(id: i64) -> DateTime<Utc> {
    ms_to_utc(id)
}
