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
        self.card_state
            .select(Some((i + 1).min(self.cards.len() - 1)));
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
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        let title = format!(
            "忆栈 MemStack — 牌组    今日新卡 {}/{}  复习 {}/{}",
            self.usage.new_reviewed,
            self.params.new_cards_per_day,
            self.usage
                .reviews_done
                .saturating_sub(self.usage.new_reviewed),
            self.params.reviews_per_day
        );
        f.render_widget(
            Paragraph::new(title)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("状态")),
            chunks[0],
        );

        let items: Vec<ListItem> = self
            .decks
            .iter()
            .map(|d| {
                let line = format!(
                    "{}  新 {:>3}  学 {:>3}  复 {:>3}  共 {:>3}",
                    pad_display(&d.deck.name, 24),
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
            // ASCII 选中标记，宽度稳定（▸ 在部分终端是模糊宽度）
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut self.deck_state);

        let help = if self.status.is_empty() {
            "Enter 打开  r 复习  g 刷新  q 退出"
        } else {
            self.status.as_str()
        };
        f.render_widget(
            Paragraph::new(help)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL)),
            chunks[2],
        );
    }

    fn draw_cards(&mut self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ])
            .split(area);

        let deck_name = self
            .selected_deck()
            .map(|d| d.deck.name.clone())
            .unwrap_or_else(|| "?".into());
        f.render_widget(
            Paragraph::new(format!("{deck_name} — {} 张卡", self.cards.len()))
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title("状态")),
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
                // 暂停标记固定占 2 列（CJK 一字 = 2 列）
                let flag = pad_display(if c.suspended { "停" } else { "" }, 2);
                let line = format!("{flag}{state} {kind}  {}", one_line(&c.front));
                ListItem::new(line)
            })
            .collect();
        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title("↑↓ 选择"))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
            .highlight_symbol("> ");
        f.render_stateful_widget(list, chunks[1], &mut self.card_state);

        let help = if self.status.is_empty() {
            "Enter 详情  r 复习本牌组  Esc 返回"
        } else {
            self.status.as_str()
        };
        f.render_widget(
            Paragraph::new(help)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL)),
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
            "[{}]  {}\n\n{}\n\n--------\n\n{}",
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
            card.front.trim_end(),
            card.back.trim_end(),
        );
        f.render_widget(
            Paragraph::new(text)
                // 保留代码缩进，不能 trim
                .wrap(Wrap { trim: false })
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("卡片详情")
                        .title_bottom(" Esc / q 关闭 "),
                ),
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
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    let t = s.lines().next().unwrap_or("").trim();
    // 按显示列宽截断，避免中文把行撑歪
    if t.width() <= 40 {
        return t.to_string();
    }
    let mut out = String::new();
    let mut w = 0;
    for ch in t.chars() {
        let cw = ch.width().unwrap_or(1);
        if w + cw > 39 {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push('…');
    out
}

/// 左对齐到指定显示列宽（CJK 记 2 列）。
fn pad_display(s: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    let w = s.width();
    if w >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

fn centered(area: Rect, pct_x: u16, pct_y: u16) -> Rect {
    // 至少留出边框 + 一行正文，避免矮终端弹层被压没
    let w = (area.width * pct_x / 100).max(20).min(area.width);
    let h = (area.height * pct_y / 100).max(5).min(area.height);
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn pad_display_cjk_counts_double() {
        // 「算法」= 4 列，补到 8 应再跟 4 个空格
        assert_eq!(
            pad_display("算法", 8).chars().filter(|c| *c == ' ').count(),
            4
        );
        assert_eq!(pad_display("abcd", 8), "abcd    ");
        assert_eq!(pad_display("abcdefghij", 4), "abcdefghij");
    }

    #[test]
    fn card_rows_align_regardless_of_suspend() {
        use unicode_width::UnicodeWidthStr;
        let mk = |suspended: bool, front: &str| memstack_core::Card {
            id: memstack_core::CardId(1),
            note_id: memstack_core::NoteId(1),
            deck_id: memstack_core::DeckId(1),
            card_type: memstack_core::CardType::QuestionAnswer,
            ordinal: 0,
            front: front.into(),
            back: "a".into(),
            state: memstack_core::CardState::New,
            step: 0,
            stability: None,
            difficulty: None,
            due: Utc::now(),
            last_review: None,
            reps: 0,
            lapses: 0,
            suspended,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let row = |c: &memstack_core::Card| {
            let flag = pad_display(if c.suspended { "停" } else { "" }, 2);
            format!("{flag}新   问答  {}", one_line(&c.front))
        };
        let a = row(&mk(false, "短"));
        let b = row(&mk(true, "短"));
        assert_eq!(a.width(), b.width(), "{a:?} vs {b:?}");
        let col = |s: &str| {
            let i = s.find("短").unwrap();
            s[..i].width()
        };
        assert_eq!(col(&a), col(&b), "「短」起始列应相同");
    }

    #[test]
    fn one_line_truncates_by_columns() {
        let s = "这是一个比较长的中文标题用来测试截断是否按显示列宽";
        let t = one_line(s);
        assert!(t.ends_with('…'));
        assert!(t.width() <= 40);
    }

    #[test]
    fn review_frame_borders_do_not_drift() {
        // 矮终端也应头/底各 3 行，边框等宽
        let backend = TestBackend::new(40, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let card = memstack_core::Card {
            id: memstack_core::CardId(1),
            note_id: memstack_core::NoteId(1),
            deck_id: memstack_core::DeckId(1),
            card_type: memstack_core::CardType::QuestionAnswer,
            ordinal: 0,
            front:
                "什么是{{c1::二分}}？这是一段比较长的正面内容，用来触发换行，检查边框是否还对齐。"
                    .into(),
            back: "对半查找\n```rust\nlet m = l + (r-l)/2;\n```".into(),
            state: memstack_core::CardState::New,
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
        let session = session::ReviewSession::from_cards_for_test(vec![card]);
        terminal.draw(|f| session.draw(f, f.area())).unwrap();

        let buf = terminal.backend().buffer();
        let area = *buf.area();
        // 宽字符占两格：跳过续格，统计显示列宽
        for y in 0..area.height {
            let mut w = 0usize;
            let mut x = 0u16;
            while x < area.width {
                let s = buf[(x, y)].symbol();
                if s.is_empty() {
                    x += 1;
                    continue;
                }
                let cw = unicode_width::UnicodeWidthStr::width(s).max(1);
                w += cw;
                x += cw as u16;
            }
            assert_eq!(w as u16, area.width, "row {y} width drifted: {w}");
        }
        // 顶边框与底边框都是 ──... 且等长
        let top: String = (0..area.width)
            .map(|x| buf[(x, 0)].symbol().chars().next().unwrap_or(' '))
            .collect();
        let bot: String = (0..area.width)
            .map(|x| {
                buf[(x, area.height - 1)]
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(top.starts_with('┌') && top.ends_with('┐'), "top={top:?}");
        assert!(bot.starts_with('└') && bot.ends_with('┘'), "bot={bot:?}");
    }
}
