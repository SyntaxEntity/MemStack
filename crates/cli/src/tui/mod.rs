//! 终端界面：牌组 → 卡片列表 → 复习。

use chrono::{Datelike, TimeZone, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use memstack_core::{Card, CardState, CardType, Deck, DeckParams};
use memstack_storage::{DailyUsage, Storage};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use std::io;

mod session;

use session::ReviewSession;

/// 启动 TUI。占用终端，退出时恢复。
pub async fn run(db: &Storage) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = ratatui::init();
    let mut app = App::load(db).await?;
    let result = app.run(db, &mut terminal).await;
    ratatui::restore();
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Decks,
    Cards,
    Review,
    Detail,
}

struct DeckRow {
    deck: Deck,
    new: usize,
    learning: usize,
    review: usize,
}

struct App {
    screen: Screen,
    prev: Screen,
    decks: Vec<DeckRow>,
    deck_state: ListState,
    cards: Vec<Card>,
    card_state: ListState,
    review: Option<ReviewSession>,
    status: String,
    params: DeckParams,
    usage: DailyUsage,
}

impl App {
    async fn load(db: &Storage) -> Result<Self, Box<dyn std::error::Error>> {
        let now = Utc::now();
        let decks_raw = db.list_decks().await?;
        let mut decks = Vec::with_capacity(decks_raw.len());
        for deck in decks_raw {
            let counts = db.queue_counts(now, Some(deck.id)).await?;
            decks.push(DeckRow {
                deck,
                new: counts.new,
                learning: counts.learning,
                review: counts.review,
            });
        }
        let mut deck_state = ListState::default();
        if !decks.is_empty() {
            deck_state.select(Some(0));
        }
        let day_start = local_day_start(now);
        let usage = db.daily_usage(now, day_start).await?;
        Ok(Self {
            screen: Screen::Decks,
            prev: Screen::Decks,
            decks,
            deck_state,
            cards: Vec::new(),
            card_state: ListState::default(),
            review: None,
            status: String::new(),
            params: DeckParams::default(),
            usage,
        })
    }

    fn selected_deck(&self) -> Option<&DeckRow> {
        self.deck_state.selected().and_then(|i| self.decks.get(i))
    }

    fn selected_card(&self) -> Option<&Card> {
        self.card_state.selected().and_then(|i| self.cards.get(i))
    }

    async fn refresh_decks(&mut self, db: &Storage) -> Result<(), Box<dyn std::error::Error>> {
        let now = Utc::now();
        let decks_raw = db.list_decks().await?;
        let mut decks = Vec::with_capacity(decks_raw.len());
        for deck in decks_raw {
            let counts = db.queue_counts(now, Some(deck.id)).await?;
            decks.push(DeckRow {
                deck,
                new: counts.new,
                learning: counts.learning,
                review: counts.review,
            });
        }
        let selected = self.deck_state.selected().unwrap_or(0);
        self.decks = decks;
        if self.decks.is_empty() {
            self.deck_state.select(None);
        } else {
            self.deck_state
                .select(Some(selected.min(self.decks.len() - 1)));
        }
        let day_start = local_day_start(now);
        self.usage = db.daily_usage(now, day_start).await?;
        Ok(())
    }

    async fn open_deck(&mut self, db: &Storage) -> Result<(), Box<dyn std::error::Error>> {
        let Some(row) = self.selected_deck() else {
            self.status = "没有牌组。先 `memstack add` 或 `memstack import`。".into();
            return Ok(());
        };
        let deck_id = row.deck.id;
        self.params = db.deck_params_or_default(deck_id).await?;
        self.cards = db.list_cards(deck_id, 500).await?;
        let mut st = ListState::default();
        if !self.cards.is_empty() {
            st.select(Some(0));
        }
        self.card_state = st;
        self.screen = Screen::Cards;
        Ok(())
    }

    async fn start_review(&mut self, db: &Storage) -> Result<(), Box<dyn std::error::Error>> {
        let deck_id = self
            .selected_deck()
            .map(|r| r.deck.id)
            .or_else(|| self.cards.first().map(|c| c.deck_id));
        let Some(deck_id) = deck_id else {
            self.status = "没有可复习的牌组。".into();
            return Ok(());
        };

        self.params = db.deck_params_or_default(deck_id).await?;
        let session = ReviewSession::start(db, Some(deck_id), 50, self.params.clone()).await?;
        if session.is_empty() {
            self.status = "没有到期卡片（或今日额度已满）。".into();
            return Ok(());
        }
        self.review = Some(session);
        self.prev = self.screen;
        self.screen = Screen::Review;
        Ok(())
    }

    async fn run(
        &mut self,
        db: &Storage,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            terminal.draw(|f| self.draw(f))?;

            if !event::poll(std::time::Duration::from_millis(200))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL)
                && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
            {
                return Ok(());
            }

            if self.handle_key(db, key.code).await? {
                return Ok(());
            }
        }
    }

    /// 返回 true 表示退出。
    async fn handle_key(
        &mut self,
        db: &Storage,
        code: KeyCode,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match self.screen {
            Screen::Decks => self.handle_decks(db, code).await,
            Screen::Cards => self.handle_cards(db, code).await,
            Screen::Review => self.handle_review(db, code).await,
            Screen::Detail => self.handle_detail(code),
        }
    }

    async fn handle_decks(
        &mut self,
        db: &Storage,
        code: KeyCode,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
            KeyCode::Char('j') | KeyCode::Down => self.deck_next(),
            KeyCode::Char('k') | KeyCode::Up => self.deck_prev(),
            KeyCode::Enter | KeyCode::Char('l') => self.open_deck(db).await?,
            KeyCode::Char('r') => self.start_review(db).await?,
            KeyCode::Char('g') => self.refresh_decks(db).await?,
            _ => {}
        }
        Ok(false)
    }

    async fn handle_cards(
        &mut self,
        db: &Storage,
        code: KeyCode,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.screen = Screen::Decks;
                self.refresh_decks(db).await?;
            }
            KeyCode::Char('j') | KeyCode::Down => self.card_next(),
            KeyCode::Char('k') | KeyCode::Up => self.card_prev(),
            KeyCode::Enter | KeyCode::Char(' ') => {
                if self.selected_card().is_some() {
                    self.prev = Screen::Cards;
                    self.screen = Screen::Detail;
                }
            }
            KeyCode::Char('r') => self.start_review(db).await?,
            _ => {}
        }
        Ok(false)
    }

    fn card_next(&mut self) {
        if self.cards.is_empty() {
            return;
        }
        let i = self.card_state.selected().unwrap_or(0);
        self.card_state.select(Some((i + 1).min(self.cards.len() - 1)));
    }

    fn card_prev(&mut self) {
        if self.cards.is_empty() {
            return;
        }
        let i = self.card_state.selected().unwrap_or(0);
        self.card_state.select(Some(i.saturating_sub(1)));
    }

    fn deck_next(&mut self) {
        if self.decks.is_empty() {
            return;
        }
        let i = self.deck_state.selected().unwrap_or(0);
        self.deck_state
            .select(Some((i + 1).min(self.decks.len() - 1)));
    }

    fn deck_prev(&mut self) {
        if self.decks.is_empty() {
            return;
        }
        let i = self.deck_state.selected().unwrap_or(0);
        self.deck_state.select(Some(i.saturating_sub(1)));
    }

    fn handle_detail(&mut self, code: KeyCode) -> Result<bool, Box<dyn std::error::Error>> {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.screen = self.prev,
            _ => {}
        }
        Ok(false)
    }

    async fn handle_review(
        &mut self,
        db: &Storage,
        code: KeyCode,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let Some(session) = self.review.as_mut() else {
            self.screen = Screen::Decks;
            return Ok(false);
        };
        match session.handle_key(db, code).await? {
            session::ReviewFlow::Continue => {}
            session::ReviewFlow::Done => {
                self.review = None;
                self.screen = self.prev;
                if self.screen == Screen::Cards {
                    self.open_deck(db).await?;
                } else {
                    self.refresh_decks(db).await?;
                }
                self.status = "复习完成。".into();
            }
            session::ReviewFlow::Quit => {
                self.review = None;
                self.screen = self.prev;
                self.refresh_decks(db).await?;
            }
        }
        Ok(false)
    }

    // ---------- drawing ----------

    fn draw(&mut self, f: &mut Frame) {
        let area = f.area();
        match self.screen {
            Screen::Decks => self.draw_decks(f, area),
            Screen::Cards => self.draw_cards(f, area),
            Screen::Review => self.draw_review(f, area),
            Screen::Detail => {
                self.draw_cards(f, area);
                self.draw_detail(f, area);
            }
        }
    }

    fn draw_decks(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let title = format!(
            "忆栈 MemStack — 牌组    今日新卡 {}/{}  复习 {}/{}",
            self.usage.new_reviewed,
            self.params.new_cards_per_day,
            self.usage.reviews_done.saturating_sub(self.usage.new_reviewed),
            self.params.reviews_per_day
        );
        f.render_widget(
            Paragraph::new(title).block(Block::default().borders(Borders::ALL).title("牌组")),
            chunks[0],
        );

        let items: Vec<ListItem> = self
            .decks
            .iter()
            .map(|d| {
                let line = format!(
                    "{:<28}  新 {:>3}  学 {:>3}  复 {:>3}  共 {:>3}",
                    d.deck.name,
                    d.new,
                    d.learning,
                    d.review,
                    d.new + d.learning + d.review
                );
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("牌组 ↑↓ 选择"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("▸ ");
        f.render_stateful_widget(list, chunks[1], &mut self.deck_state);

        let help = if self.status.is_empty() {
            "Enter 打开  r 复习  g 刷新  q 退出"
        } else {
            self.status.as_str()
        };
        f.render_widget(
            Paragraph::new(help).block(Block::default().borders(Borders::ALL)),
            chunks[2],
        );
    }

    fn draw_cards(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(3),
                Constraint::Length(2),
            ])
            .split(area);

        let deck_name = self
            .selected_deck()
            .map(|d| d.deck.name.clone())
            .unwrap_or_else(|| "?".into());
        f.render_widget(
            Paragraph::new(format!("{deck_name} — {} 张卡", self.cards.len()))
                .block(Block::default().borders(Borders::ALL).title("卡片")),
            chunks[0],
        );

        let items: Vec<ListItem> = self
            .cards
            .iter()
            .map(|c| {
                let state = match c.state {
                    CardState::New => "新  ",
                    CardState::Learning => "学  ",
                    CardState::Review => "复  ",
                    CardState::Relearning => "再  ",
                };
                let kind = match c.card_type {
                    CardType::QuestionAnswer => "问答",
                    CardType::Cloze => "填空",
                    CardType::CodeCloze => "代码",
                };
                let flag = if c.suspended { "⏸ " } else { "" };
                let line = format!(
                    "{flag}{state} {kind}  {}",
                    one_line(&c.front)
                );
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("↑↓ 选择"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("▸ ");
        f.render_stateful_widget(list, chunks[1], &mut self.card_state);

        let help = if self.status.is_empty() {
            "Enter 详情  r 复习本牌组  Esc 返回"
        } else {
            self.status.as_str()
        };
        f.render_widget(
            Paragraph::new(help).block(Block::default().borders(Borders::ALL)),
            chunks[2],
        );
    }

    fn draw_detail(&self, f: &mut Frame, area: Rect) {
        let Some(card) = self.selected_card() else {
            return;
        };
        let popup = centered(area, 80, 70);
        f.render_widget(Clear, popup);
        let text = format!(
            "[{}]  {}\n\n{}\n\n--------\n\n{}\n\n\n{}",
            match card.card_type {
                CardType::QuestionAnswer => "问答",
                CardType::Cloze => "填空",
                CardType::CodeCloze => "代码填空",
            },
            match card.state {
                CardState::New => "新卡",
                CardState::Learning => "学习",
                CardState::Review => "复习",
                CardState::Relearning => "再学习",
            },
            card.front,
            card.back,
            "Esc / q 关闭"
        );
        f.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("卡片详情")),
            popup,
        );
    }

    fn draw_review(&self, f: &mut Frame, area: Rect) {
        let Some(session) = &self.review else {
            return;
        };
        session.draw(f, area);
    }
}

fn one_line(s: &str) -> String {
    let t = s.lines().next().unwrap_or("").trim();
    if t.chars().count() > 40 {
        t.chars().take(40).collect::<String>() + "…"
    } else {
        t.to_string()
    }
}

fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    let w = area.width * pct_x / 100;
    let h = area.height * pct_y / 100;
    let x = (area.width.saturating_sub(w)) / 2;
    let y = (area.height.saturating_sub(h)) / 2;
    Rect::new(area.x + x, area.y + y, w, h)
}

pub fn local_day_start(now: chrono::DateTime<Utc>) -> chrono::DateTime<Utc> {
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
