//! 复习会话：队列、额度、撤销栈。一次只渲染一张卡。

use chrono::{DateTime, Datelike, TimeZone, Utc};
use memstack_core::{Card, CardState, DeckId, DeckParams, ReviewLog, Scheduler};
use memstack_storage::Storage;

use crate::dto::{button_dto, card_dto, rating_from_id, ReviewCardDto, ReviewDoneDto};
use crate::error::{CmdError, Result};

pub struct ReviewSession {
    pub deck_id: Option<DeckId>,
    pub params: DeckParams,
    pub scheduler: Scheduler,
    pub queue: Vec<Card>,
    pub undo_stack: Vec<(Card, ReviewLog)>,
    pub reviewed: u32,
    pub new_used: u32,
    pub review_used: u32,
    pub flipped_at: Option<DateTime<Utc>>,
    pub limit: u32,
}

impl ReviewSession {
    pub fn done(&self) -> ReviewDoneDto {
        ReviewDoneDto {
            reviewed: self.reviewed,
            new_used: self.new_used,
            new_cap: self.params.new_cards_per_day,
            review_used: self.review_used,
            review_cap: self.params.reviews_per_day,
        }
    }
}

pub async fn start(
    db: &Storage,
    deck_id: Option<DeckId>,
    limit: u32,
) -> Result<(ReviewSession, Option<ReviewCardDto>)> {
    let now = Utc::now();
    let params = match deck_id {
        Some(id) => db.deck_params_or_default(id).await?,
        None => match db.list_decks().await?.first() {
            Some(d) => db.deck_params_or_default(d.id).await?,
            None => DeckParams::default(),
        },
    };
    let scheduler = Scheduler::new(&params)?;

    let day_start = local_day_start(now);
    let usage = db.daily_usage(now, day_start).await?;
    let pure_reviews = usage.reviews_done.saturating_sub(usage.new_reviewed);
    let new_used = usage.new_reviewed.min(params.new_cards_per_day);
    let review_used = pure_reviews.min(params.reviews_per_day);
    let new_remaining = params.new_cards_per_day.saturating_sub(new_used);
    let review_remaining = params.reviews_per_day.saturating_sub(review_used);

    let queue = db
        .due_queue(now, deck_id, new_remaining, review_remaining, limit)
        .await?;

    let mut session = ReviewSession {
        deck_id,
        params,
        scheduler,
        queue,
        undo_stack: Vec::new(),
        reviewed: 0,
        new_used,
        review_used,
        flipped_at: None,
        limit,
    };
    let card = next_card(db, &mut session).await?;
    Ok((session, card))
}

async fn refill(db: &Storage, session: &mut ReviewSession) -> Result<()> {
    let now = Utc::now();
    let new_remaining = session
        .params
        .new_cards_per_day
        .saturating_sub(session.new_used);
    let review_remaining = session
        .params
        .reviews_per_day
        .saturating_sub(session.review_used);
    let fresh = db
        .due_queue(
            now,
            session.deck_id,
            new_remaining,
            review_remaining,
            session.limit,
        )
        .await?;
    session.queue = fresh
        .into_iter()
        .filter(|c| !session.undo_stack.iter().any(|(prev, _)| prev.id == c.id))
        .collect();
    if new_remaining == 0 && review_remaining == 0 {
        session.queue.retain(|c| c.state.is_in_steps());
    }
    Ok(())
}

async fn next_card(db: &Storage, session: &mut ReviewSession) -> Result<Option<ReviewCardDto>> {
    if session.queue.is_empty() {
        refill(db, session).await?;
        if session.queue.is_empty() {
            return Ok(None);
        }
    }
    let card = session.queue[0].clone();
    let now = Utc::now();
    let preview = session.scheduler.preview(&card, now)?;
    session.flipped_at = Some(now);
    Ok(Some(ReviewCardDto {
        card: card_dto(&card),
        buttons: vec![
            button_dto(&preview.again),
            button_dto(&preview.hard),
            button_dto(&preview.good),
            button_dto(&preview.easy),
        ],
        remaining: session.queue.len(),
        new_used: session.new_used,
        new_cap: session.params.new_cards_per_day,
        review_used: session.review_used,
        review_cap: session.params.reviews_per_day,
    }))
}

/// 评分当前卡，返回下一张（或 None = 本批完成）。
pub async fn rate(
    db: &Storage,
    session: &mut ReviewSession,
    rating_id: &str,
    duration_ms: u32,
) -> Result<Option<ReviewCardDto>> {
    let Some(card) = session.queue.first().cloned() else {
        return Ok(None);
    };
    let Some(rating) = rating_from_id(rating_id) else {
        return Err(CmdError(format!("无效评分：{rating_id}")));
    };
    let now = Utc::now();
    let duration_ms = if duration_ms == 0 {
        session
            .flipped_at
            .map(|t| (now - t).num_milliseconds().clamp(0, i64::from(u32::MAX)) as u32)
            .unwrap_or(0)
    } else {
        duration_ms
    };

    let outcome = session.scheduler.review(&card, rating, now, duration_ms)?;
    let saved = db.save_review(&outcome.card, &outcome.log).await?;
    session.undo_stack.push((outcome.previous, saved));
    session.queue.remove(0);
    session.reviewed += 1;
    if card.state == CardState::New {
        session.new_used = session.new_used.saturating_add(1);
    } else {
        session.review_used = session.review_used.saturating_add(1);
    }
    session.flipped_at = None;

    next_card(db, session).await
}

/// 撤销最近一次评分，返回恢复后的那张卡。
pub async fn undo(db: &Storage, session: &mut ReviewSession) -> Result<Option<ReviewCardDto>> {
    let Some((prev, log)) = session.undo_stack.pop() else {
        return Err("没有可撤销的评分".into());
    };
    let log_id = log.id.ok_or("日志缺少 id，无法撤销")?;
    db.undo_review(&prev, log_id).await?;
    session.reviewed = session.reviewed.saturating_sub(1);
    if prev.state == CardState::New {
        session.new_used = session.new_used.saturating_sub(1);
    } else {
        session.review_used = session.review_used.saturating_sub(1);
    }
    session.queue.insert(0, prev.clone());
    session.flipped_at = Some(Utc::now());

    let now = Utc::now();
    let preview = session.scheduler.preview(&prev, now)?;
    Ok(Some(ReviewCardDto {
        card: card_dto(&prev),
        buttons: vec![
            button_dto(&preview.again),
            button_dto(&preview.hard),
            button_dto(&preview.good),
            button_dto(&preview.easy),
        ],
        remaining: session.queue.len(),
        new_used: session.new_used,
        new_cap: session.params.new_cards_per_day,
        review_used: session.review_used,
        review_cap: session.params.reviews_per_day,
    }))
}

/// 翻面时记录时刻，方便算答题时长。
pub fn flip(session: &mut ReviewSession) {
    session.flipped_at = Some(Utc::now());
}

fn local_day_start(now: DateTime<Utc>) -> DateTime<Utc> {
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
