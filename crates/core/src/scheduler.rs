//! FSRS 调度。
//!
//! 学习 / 再学习走步骤（分钟），毕业或 Review 态走 FSRS。
//! 评分 Again 从 Review 跌入 Relearning，并计入 lapses。

use chrono::{DateTime, Duration, Utc};
use fsrs::{ItemState, FSRS};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::{Card, CardState, DeckParams, Rating, ReviewLog};

/// 四个评分按钮的预览。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulePreview {
    pub again: ButtonPreview,
    pub hard: ButtonPreview,
    pub good: ButtonPreview,
    pub easy: ButtonPreview,
}

impl SchedulePreview {
    pub fn get(&self, rating: Rating) -> &ButtonPreview {
        match rating {
            Rating::Again => &self.again,
            Rating::Hard => &self.hard,
            Rating::Good => &self.good,
            Rating::Easy => &self.easy,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ButtonPreview {
    pub rating: Rating,
    pub state: CardState,
    /// 下次见到这张卡之前要等多久
    pub due_in: Duration,
    /// 毕业后的 FSRS 间隔（天），学习步骤中为 None
    pub interval_days: Option<f32>,
}

/// 一次复习的结果。`previous` 用于撤销。
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewOutcome {
    pub card: Card,
    pub log: ReviewLog,
    pub previous: Card,
}

/// 调度器。参数来自 [`DeckParams`]，可看可调。
pub struct Scheduler {
    fsrs: FSRS,
    desired_retention: f32,
    max_interval_days: u32,
    learning_steps: Vec<u32>,
    relearning_steps: Vec<u32>,
}

impl Scheduler {
    pub fn new(params: &DeckParams) -> Result<Self> {
        let fsrs = FSRS::new(&params.fsrs_params)?;
        Ok(Self {
            fsrs,
            desired_retention: params.desired_retention.clamp(0.7, 0.97),
            max_interval_days: params.max_interval_days.max(1),
            learning_steps: params.learning_steps_or_default(),
            relearning_steps: params.relearning_steps_or_default(),
        })
    }

    /// 预览四个按钮各自会把卡调度到哪里。不产生副作用。
    pub fn preview(&self, card: &Card, now: DateTime<Utc>) -> Result<SchedulePreview> {
        Ok(SchedulePreview {
            again: self.preview_rating(card, Rating::Again, now)?,
            hard: self.preview_rating(card, Rating::Hard, now)?,
            good: self.preview_rating(card, Rating::Good, now)?,
            easy: self.preview_rating(card, Rating::Easy, now)?,
        })
    }

    /// 对卡片评分。返回新卡状态与一条只增不改的复习记录。
    pub fn review(
        &self,
        card: &Card,
        rating: Rating,
        now: DateTime<Utc>,
        duration_ms: u32,
    ) -> Result<ReviewOutcome> {
        let previous = card.clone();
        let mut next = card.clone();

        let state_before = card.state;
        let step_before = card.step;
        let stability_before = card.stability;
        let difficulty_before = card.difficulty;
        let elapsed_days = card.elapsed_days(now);

        let interval_days = match card.state {
            CardState::New | CardState::Learning | CardState::Relearning => {
                self.schedule_steps(&mut next, rating, now)?
            }
            CardState::Review => self.schedule_review(&mut next, rating, elapsed_days, now)?,
        };

        next.reps += 1;
        next.last_review = Some(now);
        next.updated_at = now;

        let log = ReviewLog {
            id: None,
            card_id: card.id,
            reviewed_at: now,
            rating,
            state_before,
            step_before,
            stability_before,
            difficulty_before,
            interval_days,
            elapsed_days,
            duration_ms,
        };

        Ok(ReviewOutcome {
            card: next,
            log,
            previous,
        })
    }

    fn preview_rating(
        &self,
        card: &Card,
        rating: Rating,
        now: DateTime<Utc>,
    ) -> Result<ButtonPreview> {
        let outcome = self.review(card, rating, now, 0)?;
        let next = &outcome.card;
        let due_in = next.due - now;
        Ok(ButtonPreview {
            rating,
            state: next.state,
            due_in,
            interval_days: if next.state == CardState::Review {
                Some(interval_from_due(next.due, now))
            } else {
                None
            },
        })
    }

    /// 学习 / 再学习 / 新卡的步骤调度。返回 interval_days（步骤内用分钟折算）。
    fn schedule_steps(&self, card: &mut Card, rating: Rating, now: DateTime<Utc>) -> Result<f32> {
        let relearning = card.state == CardState::Relearning;
        let steps = if relearning {
            self.relearning_steps.clone()
        } else {
            self.learning_steps.clone()
        };
        if steps.is_empty() {
            return Err(Error::EmptySteps);
        }

        let step = usize::from(card.step);

        match rating {
            Rating::Again => {
                card.state = if relearning {
                    CardState::Relearning
                } else {
                    CardState::Learning
                };
                card.step = 0;
                card.due = now + Duration::minutes(i64::from(steps[0]));
                Ok(steps[0] as f32 / 1_440.0)
            }
            Rating::Hard => {
                // 卡在当前步骤，间隔略拉长；两步时取中点，贴近 Anki
                let minutes = if steps.len() >= 2 && step == 0 {
                    (steps[0] + steps[1]) / 2
                } else {
                    let base = steps.get(step).copied().unwrap_or(steps[0]);
                    base + base / 2
                };
                card.due = now + Duration::minutes(i64::from(minutes.max(1)));
                Ok(minutes as f32 / 1_440.0)
            }
            Rating::Good => {
                let next_step = step + 1;
                if next_step >= steps.len() {
                    self.graduate(card, Rating::Good, now)
                } else {
                    card.step = next_step as u8;
                    card.state = if relearning {
                        CardState::Relearning
                    } else {
                        CardState::Learning
                    };
                    let minutes = steps[next_step];
                    card.due = now + Duration::minutes(i64::from(minutes));
                    Ok(minutes as f32 / 1_440.0)
                }
            }
            Rating::Easy => self.graduate(card, Rating::Easy, now),
        }
    }

    /// Review 态：Again 记一次遗忘并进入再学习，其余走 FSRS。
    fn schedule_review(
        &self,
        card: &mut Card,
        rating: Rating,
        elapsed_days: u32,
        now: DateTime<Utc>,
    ) -> Result<f32> {
        if rating == Rating::Again {
            // 忘了：FSRS 状态仍按 Again 更新，再进入再学习步骤
            let item = self.next_item(card.memory(), elapsed_days, Rating::Again)?;
            card.set_memory(item.memory);
            card.lapses += 1;
            card.state = CardState::Relearning;
            card.step = 0;
            let minutes = self.relearning_steps[0];
            card.due = now + Duration::minutes(i64::from(minutes));
            // 以 FSRS 给的间隔作为记录上的间隔参考
            Ok(self.clamp_interval(item.interval))
        } else {
            let item = self.next_item(card.memory(), elapsed_days, rating)?;
            card.set_memory(item.memory);
            card.state = CardState::Review;
            card.step = 0;
            let days = self.clamp_interval(item.interval);
            let whole_days = days.round().max(1.0) as i64;
            card.due = now + Duration::days(whole_days);
            Ok(days)
        }
    }

    /// 毕业：进入 Review，用 FSRS 定间隔。
    fn graduate(&self, card: &mut Card, rating: Rating, now: DateTime<Utc>) -> Result<f32> {
        let elapsed_days = card.elapsed_days(now);
        let item = self.next_item(card.memory(), elapsed_days, rating)?;
        card.set_memory(item.memory);
        card.state = CardState::Review;
        card.step = 0;
        let days = self.clamp_interval(item.interval);
        let whole_days = days.round().max(1.0) as i64;
        card.due = now + Duration::days(whole_days);
        Ok(days)
    }

    fn next_item(
        &self,
        memory: Option<fsrs::MemoryState>,
        elapsed_days: u32,
        rating: Rating,
    ) -> Result<ItemState> {
        let states = self
            .fsrs
            .next_states(memory, self.desired_retention, elapsed_days)?;
        Ok(match rating {
            Rating::Again => states.again,
            Rating::Hard => states.hard,
            Rating::Good => states.good,
            Rating::Easy => states.easy,
        })
    }

    fn clamp_interval(&self, interval: f32) -> f32 {
        interval.clamp(0.0, self.max_interval_days as f32)
    }
}

fn interval_from_due(due: DateTime<Utc>, now: DateTime<Utc>) -> f32 {
    let seconds = (due - now).num_seconds().max(0);
    seconds as f32 / 86_400.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{CardId, CardType, DeckId, NoteId};

    fn t0() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn new_card() -> Card {
        let now = t0();
        Card {
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
            due: now,
            last_review: None,
            reps: 0,
            lapses: 0,
            suspended: false,
            created_at: now,
            updated_at: now,
        }
    }

    fn scheduler() -> Scheduler {
        Scheduler::new(&DeckParams::default()).unwrap()
    }

    #[test]
    fn again_resets_learning_step() {
        let s = scheduler();
        let now = t0();
        let out = s.review(&new_card(), Rating::Again, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Learning);
        assert_eq!(out.card.step, 0);
        assert_eq!(out.card.due, now + Duration::minutes(1));
        assert!(out.card.stability.is_none());
        assert_eq!(out.log.rating, Rating::Again);
    }

    #[test]
    fn good_advances_learning_step() {
        let s = scheduler();
        let now = t0();
        let out = s.review(&new_card(), Rating::Good, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Learning);
        assert_eq!(out.card.step, 1);
        assert_eq!(out.card.due, now + Duration::minutes(10));
    }

    #[test]
    fn good_on_last_step_graduates() {
        let s = scheduler();
        let now = t0();
        let mut card = new_card();
        card.state = CardState::Learning;
        card.step = 1;
        let out = s.review(&card, Rating::Good, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Review);
        assert!(out.card.stability.is_some());
        assert!(out.card.difficulty.is_some());
        assert!(out.card.due > now);
    }

    #[test]
    fn easy_graduates_immediately() {
        let s = scheduler();
        let now = t0();
        let out = s.review(&new_card(), Rating::Easy, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Review);
        let good = s.review(&new_card(), Rating::Good, now, 0).unwrap();
        // Easy 从 New 直接毕业；Good 先走一步学习
        assert_eq!(good.card.state, CardState::Learning);
        let easy_days = out.log.interval_days;
        assert!(easy_days >= 1.0);
    }

    #[test]
    fn review_good_schedules_fsrs_interval() {
        let s = scheduler();
        let now = t0();
        let mut card = new_card();
        card.state = CardState::Review;
        card.stability = Some(2.0);
        card.difficulty = Some(5.0);
        card.last_review = Some(now - Duration::days(1));
        card.reps = 1;

        let out = s.review(&card, Rating::Good, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Review);
        assert!(out.card.lapses == 0);
        assert!(out.card.stability.unwrap() >= 2.0);
        assert_eq!(out.log.elapsed_days, 1);
        let days = (out.card.due - now).num_days();
        assert!(days >= 1);
    }

    #[test]
    fn review_again_lapses_to_relearning() {
        let s = scheduler();
        let now = t0();
        let mut card = new_card();
        card.state = CardState::Review;
        card.stability = Some(5.0);
        card.difficulty = Some(5.0);
        card.last_review = Some(now - Duration::days(10));
        card.reps = 3;

        let out = s.review(&card, Rating::Again, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Relearning);
        assert_eq!(out.card.lapses, 1);
        assert_eq!(out.card.step, 0);
        assert_eq!(out.card.due, now + Duration::minutes(10));
        // FSRS 仍更新记忆状态
        assert!(out.card.stability.is_some());
    }

    #[test]
    fn relearning_good_graduates_again() {
        let s = scheduler();
        let now = t0();
        let mut card = new_card();
        card.state = CardState::Relearning;
        card.step = 0;
        card.stability = Some(0.5);
        card.difficulty = Some(7.0);
        card.last_review = Some(now - Duration::minutes(10));
        card.lapses = 1;
        card.reps = 2;

        // 只有 1 个再学习步骤 [10]，Good 即毕业
        let out = s.review(&card, Rating::Good, now, 0).unwrap();
        assert_eq!(out.card.state, CardState::Review);
        assert_eq!(out.card.lapses, 1);
    }

    #[test]
    fn preview_has_four_buttons() {
        let s = scheduler();
        let now = t0();
        let p = s.preview(&new_card(), now).unwrap();
        assert_eq!(p.again.due_in, Duration::minutes(1));
        assert_eq!(p.good.due_in, Duration::minutes(10));
        assert_eq!(p.easy.state, CardState::Review);
        assert_eq!(p.get(Rating::Again).rating, Rating::Again);
    }

    #[test]
    fn undo_snapshot_is_pre_review() {
        let s = scheduler();
        let now = t0();
        let card = new_card();
        let out = s.review(&card, Rating::Good, now, 500).unwrap();
        assert_eq!(out.previous, card);
        assert_eq!(out.log.duration_ms, 500);
        assert_eq!(out.log.state_before, CardState::New);
    }

    #[test]
    fn custom_params_roundtrip() {
        let params = DeckParams {
            fsrs_params: fsrs::DEFAULT_PARAMETERS.to_vec(),
            learning_steps: vec![5],
            max_interval_days: 30,
            ..DeckParams::default()
        };
        let s = Scheduler::new(&params).unwrap();
        let now = t0();
        let mut card = new_card();
        card.state = CardState::Review;
        card.stability = Some(100.0);
        card.difficulty = Some(5.0);
        card.last_review = Some(now - Duration::days(1));
        let out = s.review(&card, Rating::Easy, now, 0).unwrap();
        let days = (out.card.due - now).num_days();
        assert!(days <= 31, "max_interval_days should clamp, got {days}");
    }
}
