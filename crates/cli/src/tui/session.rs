//! 复习会话状态（TUI 驱动，非阻塞）。

use chrono::Utc;
use crossterm::event::KeyCode;
use memstack_core::{Card, CardState, CardType, DeckParams, Rating, ReviewLog, Scheduler};
use memstack_storage::Storage;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

pub enum ReviewFlow {
    Continue,
    Done,
    Quit,
}

pub struct ReviewSession {
    queue: Vec<Card>,
    flipped: bool,
    undo_stack: Vec<(Card, ReviewLog)>,
    scheduler: Scheduler,
    deck_id: Option<memstack_core::DeckId>,
    params: DeckParams,
    reviewed: u32,
    /// 评分后短暂提示
    flash: Option<String>,
}

impl ReviewSession {
    pub async fn start(
        db: &Storage,
        deck_id: Option<memstack_core::DeckId>,
        limit: u32,
        params: DeckParams,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let now = Utc::now();
        let day_start = super::local_day_start(now);
        let usage = db.daily_usage(now, day_start).await?;
        let pure_reviews = usage.reviews_done.saturating_sub(usage.new_reviewed);
        let new_remaining = params.new_cards_per_day.saturating_sub(usage.new_reviewed);
        let review_remaining = params.reviews_per_day.saturating_sub(pure_reviews);
        let queue = db
            .due_queue(now, deck_id, new_remaining, review_remaining, limit)
            .await?;
        let scheduler = Scheduler::new(&params)?;
        Ok(Self {
            queue,
            flipped: false,
            undo_stack: Vec::new(),
            scheduler,
            deck_id,
            params,
            reviewed: 0,
            flash: None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub async fn handle_key(
        &mut self,
        db: &Storage,
        code: KeyCode,
    ) -> Result<ReviewFlow, Box<dyn std::error::Error>> {
        // 先清掉提示
        self.flash = None;

        match code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(ReviewFlow::Quit),
            KeyCode::Char('u') | KeyCode::Char('U') => {
                self.undo(db).await?;
                return Ok(ReviewFlow::Continue);
            }
            _ => {}
        }

        if self.queue.is_empty() {
            // 等待补队列
            self.refill(db).await?;
            if self.queue.is_empty() {
                return Ok(ReviewFlow::Done);
            }
        }

        if !self.flipped {
            match code {
                KeyCode::Char(' ') | KeyCode::Enter | KeyCode::Char('j') => self.flipped = true,
                _ => {}
            }
            return Ok(ReviewFlow::Continue);
        }

        let rating = match code {
            KeyCode::Char('1') => Some(Rating::Again),
            KeyCode::Char('2') => Some(Rating::Hard),
            KeyCode::Char('3') => Some(Rating::Good),
            KeyCode::Char('4') => Some(Rating::Easy),
            _ => None,
        };
        let Some(rating) = rating else {
            return Ok(ReviewFlow::Continue);
        };

        let card = self.queue[0].clone();
        let outcome = self.scheduler.review(&card, rating, Utc::now(), 0)?;
        let saved = db.save_review(&outcome.card, &outcome.log).await?;
        self.undo_stack.push((outcome.previous, saved));
        self.queue.remove(0);
        self.flipped = false;
        self.reviewed += 1;
        self.flash = Some(format!(
            "✓ {rating} → {}",
            fmt_interval(outcome.log.interval_days)
        ));

        if self.queue.is_empty() {
            self.refill(db).await?;
            if self.queue.is_empty() {
                return Ok(ReviewFlow::Done);
            }
        }
        Ok(ReviewFlow::Continue)
    }

    async fn undo(&mut self, db: &Storage) -> Result<(), Box<dyn std::error::Error>> {
        let Some((prev, log)) = self.undo_stack.pop() else {
            self.flash = Some("没有可撤销的复习".into());
            return Ok(());
        };
        let log_id = log.id.ok_or("日志缺少 id")?;
        db.undo_review(&prev, log_id).await?;
        self.queue.insert(0, prev);
        self.flipped = false;
        self.reviewed = self.reviewed.saturating_sub(1);
        self.flash = Some("已撤销".into());
        Ok(())
    }

    async fn refill(&mut self, db: &Storage) -> Result<(), Box<dyn std::error::Error>> {
        let now = Utc::now();
        let day_start = super::local_day_start(now);
        let usage = db.daily_usage(now, day_start).await?;
        let pure_reviews = usage.reviews_done.saturating_sub(usage.new_reviewed);
        let new_remaining = self
            .params
            .new_cards_per_day
            .saturating_sub(usage.new_reviewed);
        let review_remaining = self.params.reviews_per_day.saturating_sub(pure_reviews);
        let more = db
            .due_queue(now, self.deck_id, new_remaining, review_remaining, 50)
            .await?;
        for c in more {
            if !self.undo_stack.iter().any(|(p, _)| p.id == c.id)
                && !self.queue.iter().any(|q| q.id == c.id)
            {
                self.queue.push(c);
            }
        }
        Ok(())
    }

    pub fn draw(&self, f: &mut ratatui::Frame, area: Rect) {
        let Some(card) = self.queue.first() else {
            f.render_widget(
                Paragraph::new("本批完成。")
                    .block(Block::default().borders(Borders::ALL).title("复习")),
                area,
            );
            return;
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(5),
                Constraint::Length(3),
            ])
            .split(area);

        let state = match card.state {
            CardState::New => "新卡",
            CardState::Learning => "学习",
            CardState::Review => "复习",
            CardState::Relearning => "再学习",
        };
        let kind = match card.card_type {
            CardType::QuestionAnswer => "问答",
            CardType::Cloze => "填空",
            CardType::CodeCloze => "代码填空",
        };
        f.render_widget(
            Paragraph::new(format!(
                "{state} · {kind} · 剩 {} · 已评 {}",
                self.queue.len(),
                self.reviewed
            ))
            .block(Block::default().borders(Borders::ALL).title("复习")),
            chunks[0],
        );

        let body = if self.flipped {
            format!("{}\n\n--------\n\n{}", card.front, card.back)
        } else {
            format!("{}\n\n\n[空格显示答案]", card.front)
        };
        f.render_widget(
            Paragraph::new(body)
                .wrap(Wrap { trim: false })
                .block(Block::default().borders(Borders::ALL).title("卡片")),
            chunks[1],
        );

        let help = match &self.flash {
            Some(msg) => format!("  {msg}"),
            None => {
                if self.flipped {
                    "  1=Again  2=Hard  3=Good  4=Easy    u 撤销  q 退出".into()
                } else {
                    "  空格/Enter 翻面    u 撤销  q 退出".into()
                }
            }
        };
        f.render_widget(
            Paragraph::new(help).block(Block::default().borders(Borders::ALL)),
            chunks[2],
        );
    }
}

fn fmt_interval(days: f32) -> String {
    if days < 1.0 {
        let mins = (days * 1440.0).round().max(1.0);
        format!("{mins:.0} 分")
    } else if days < 30.0 {
        format!("{days:.1} 天")
    } else {
        format!("{:.0} 天", days)
    }
}
