//! 复习闭环：取卡 → 翻面 → 评分 → 落库。支持撤销与每日上限。

use chrono::{Datelike, Duration, TimeZone, Utc};
use memstack_core::{Card, CardState, CardType, Rating, ReviewLog, Scheduler};
use memstack_storage::Storage;

use crate::term::{self, Key};

pub async fn run(
    db: &Storage,
    deck_name: Option<&str>,
    limit: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let deck_id = match deck_name {
        Some(name) => Some(db.get_deck_by_name(name).await?.ok_or("牌组不存在")?.id),
        None => None,
    };

    let now = Utc::now();
    let params = match deck_id {
        Some(id) => db.deck_params_or_default(id).await?,
        None => match db.list_decks().await?.first() {
            Some(d) => db.deck_params_or_default(d.id).await?,
            None => Default::default(),
        },
    };
    let scheduler = Scheduler::new(&params)?;

    let day_start = local_day_start(now);
    let mut usage = db.daily_usage(now, day_start).await?;
    // reviews_done 含新卡；复习额度只管 Review 态
    let pure_reviews = usage.reviews_done.saturating_sub(usage.new_reviewed);
    let mut new_remaining = params.new_cards_per_day.saturating_sub(usage.new_reviewed);
    let mut review_remaining = params.reviews_per_day.saturating_sub(pure_reviews);

    let mut queue = db
        .due_queue(now, deck_id, new_remaining, review_remaining, limit)
        .await?;

    let _raw = term::RawGuard::enter()?;
    let mut undo_stack: Vec<(Card, ReviewLog)> = Vec::new();
    let mut reviewed = 0u32;

    loop {
        let now = Utc::now();
        if queue.is_empty() {
            let pure = usage.reviews_done.saturating_sub(usage.new_reviewed);
            new_remaining = params.new_cards_per_day.saturating_sub(usage.new_reviewed);
            review_remaining = params.reviews_per_day.saturating_sub(pure);

            let refill = db
                .due_queue(now, deck_id, new_remaining, review_remaining, limit)
                .await?;
            queue = refill
                .into_iter()
                .filter(|c| !undo_stack.iter().any(|(prev, _)| prev.id == c.id))
                .collect();
            if queue.is_empty() {
                if reviewed == 0 {
                    draw_empty(deck_name, &usage, &params);
                } else {
                    println!("\n本批完成：{reviewed} 张。");
                }
                break;
            }
        }

        // 额度用尽时只跑学习卡
        if new_remaining == 0 && review_remaining == 0 {
            queue.retain(|c| c.state.is_in_steps());
            if queue.is_empty() {
                println!(
                    "\n今日额度已满。新卡 {}/{}, 复习 {}/{}",
                    usage.new_reviewed,
                    params.new_cards_per_day,
                    usage.reviews_done.saturating_sub(usage.new_reviewed),
                    params.reviews_per_day
                );
                break;
            }
        }

        let card = queue[0].clone();
        match show_card(db, &scheduler, &card, &mut undo_stack).await? {
            Flow::Continue => {
                queue.remove(0);
                reviewed += 1;
                if card.state == CardState::New {
                    new_remaining = new_remaining.saturating_sub(1);
                    usage.new_reviewed += 1;
                } else {
                    review_remaining = review_remaining.saturating_sub(1);
                }
                usage.reviews_done += 1;
            }
            Flow::Undone(prev) => {
                if prev.state == CardState::New {
                    usage.new_reviewed = usage.new_reviewed.saturating_sub(1);
                }
                usage.reviews_done = usage.reviews_done.saturating_sub(1);
                reviewed = reviewed.saturating_sub(1);
                queue.insert(0, prev);
            }
            Flow::Noop => {}
            Flow::Quit => {
                println!("\n已退出。本批 {reviewed} 张。");
                break;
            }
        }
    }

    Ok(())
}

enum Flow {
    Continue,
    Undone(Card),
    /// 撤销栈为空，原样重绘
    Noop,
    Quit,
}

async fn show_card(
    db: &Storage,
    scheduler: &Scheduler,
    card: &Card,
    undo_stack: &mut Vec<(Card, ReviewLog)>,
) -> Result<Flow, Box<dyn std::error::Error>> {
    let now = Utc::now();
    let preview = scheduler.preview(card, now)?;
    let state_label = match card.state {
        CardState::New => "新卡",
        CardState::Learning => "学习",
        CardState::Review => "复习",
        CardState::Relearning => "再学习",
    };
    let type_label = match card.card_type {
        CardType::QuestionAnswer => "问答",
        CardType::Cloze => "填空",
        CardType::CodeCloze => "代码填空",
    };

    // 正面
    let flipped_at = loop {
        term::clear();
        term::print_banner(&format!("{state_label} · {type_label}"));
        println!();
        println!("{}", card.front);
        println!();
        println!("{}", term::dim("—— 空格显示答案 · u 撤销 · q 退出 ——"));
        println!();

        match term::read_key()? {
            Key::Quit => return Ok(Flow::Quit),
            Key::Undo => return try_undo(db, undo_stack).await,
            Key::Flip => break Utc::now(),
            Key::Rate(_) | Key::Ignore => continue,
        }
    };

    // 背面 + 四键
    loop {
        term::clear();
        term::print_banner(&format!("{state_label} · {type_label}"));
        println!();
        println!("{}", card.front);
        println!();
        println!("{}", term::dim("────────"));
        println!();
        println!("{}", card.back);
        println!();
        println!(
            "  1=Again {}\t2=Hard {}\t3=Good {}\t4=Easy {}",
            fmt_in(preview.again.due_in),
            fmt_in(preview.hard.due_in),
            fmt_in(preview.good.due_in),
            fmt_in(preview.easy.due_in),
        );
        println!("{}", term::dim("  u=撤销  q=退出"));
        println!();

        match term::read_key()? {
            Key::Quit => return Ok(Flow::Quit),
            Key::Undo => return try_undo(db, undo_stack).await,
            Key::Flip | Key::Ignore => continue,
            Key::Rate(n) => {
                let rating = match n {
                    1 => Rating::Again,
                    2 => Rating::Hard,
                    3 => Rating::Good,
                    _ => Rating::Easy,
                };
                let duration_ms = (Utc::now() - flipped_at)
                    .num_milliseconds()
                    .clamp(0, i64::from(u32::MAX)) as u32;
                let outcome = scheduler.review(card, rating, Utc::now(), duration_ms)?;
                let saved = db.save_review(&outcome.card, &outcome.log).await?;
                undo_stack.push((outcome.previous, saved));
                return Ok(Flow::Continue);
            }
        }
    }
}

async fn try_undo(
    db: &Storage,
    undo_stack: &mut Vec<(Card, ReviewLog)>,
) -> Result<Flow, Box<dyn std::error::Error>> {
    let Some((prev, log)) = undo_stack.pop() else {
        return Ok(Flow::Noop);
    };
    let log_id = log.id.ok_or("日志缺少 id，无法撤销")?;
    db.undo_review(&prev, log_id).await?;
    Ok(Flow::Undone(prev))
}

fn draw_empty(
    deck_name: Option<&str>,
    usage: &memstack_storage::DailyUsage,
    params: &memstack_core::DeckParams,
) {
    term::clear();
    match deck_name {
        Some(d) => term::print_banner(&format!("牌组 {d}：没有到期卡片")),
        None => term::print_banner("没有到期卡片"),
    }
    println!();
    println!(
        "{}",
        term::dim(&format!(
            "今日新卡 {}/{}, 复习 {}/{}",
            usage.new_reviewed,
            params.new_cards_per_day,
            usage.reviews_done.saturating_sub(usage.new_reviewed),
            params.reviews_per_day
        ))
    );
    println!("{}", term::dim("可以先 `memstack add` 造几张，或明天再来。"));
}

fn fmt_in(d: Duration) -> String {
    let mins = d.num_minutes();
    if mins < 1 {
        "<1分".into()
    } else if mins < 60 {
        format!("{mins}分")
    } else if mins < 60 * 24 {
        format!("{}时", mins / 60)
    } else {
        format!("{}天", mins / (60 * 24))
    }
}

/// 本地时区的今天 00:00（UTC）。
fn local_day_start(now: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
    let local = now.with_timezone(&chrono::Local);
    let naive = local.date_naive().and_hms_opt(0, 0, 0).unwrap();
    chrono::Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| {
            Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
                .unwrap()
        })
}
