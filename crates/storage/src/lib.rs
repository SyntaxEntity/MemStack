//! SQLite 存储。五张表：decks、notes、cards、review_logs、deck_params。
//!
//! 复习记录只增不改；撤销走删除最后一条，不是 UPDATE。

mod codec;
mod error;

pub use error::{Error, Result};

use chrono::{DateTime, Utc};
use codec::*;
use memstack_core::{
    generate_cards, Card, CardId, CardState, CardType, Deck, DeckId, DeckParams, Note, NoteContent,
    NoteId, ReviewLog, ReviewLogId, Scheduler,
};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions, SqliteRow};
use sqlx::Row;
use std::collections::BTreeSet;
use std::path::Path;

const SCHEMA: &str = include_str!("../migrations/0001_init.sql");

/// 新笔记（尚未入库）。
#[derive(Debug, Clone)]
pub struct NewNote {
    pub deck_id: DeckId,
    pub content: NoteContent,
    pub tags: BTreeSet<String>,
    pub source: Option<String>,
}

/// 带调度状态的卡片种子（apkg 导入用）。
#[derive(Debug, Clone)]
pub struct CardSeed {
    pub ordinal: u16,
    pub card_type: CardType,
    pub front: String,
    pub back: String,
    pub state: CardState,
    pub step: u8,
    pub stability: Option<f32>,
    pub difficulty: Option<f32>,
    pub due: DateTime<Utc>,
    pub last_review: Option<DateTime<Utc>>,
    pub reps: u32,
    pub lapses: u32,
    pub suspended: bool,
}

/// 今日额度使用情况。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DailyUsage {
    pub new_reviewed: u32,
    pub reviews_done: u32,
}

/// 待复习队列的分桶计数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueCounts {
    pub learning: usize,
    pub new: usize,
    pub review: usize,
}

impl QueueCounts {
    pub fn total(&self) -> usize {
        self.learning + self.new + self.review
    }
}

pub struct Storage {
    pool: SqlitePool,
}

impl Storage {
    /// 打开（或创建）单文件数据库并跑迁移。
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        Self::migrate(&pool).await?;
        Ok(Self { pool })
    }

    /// 内存库，测试用。
    pub async fn open_in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        Self::migrate(&pool).await?;
        Ok(Self { pool })
    }

    async fn migrate(pool: &SqlitePool) -> Result<()> {
        // 多条语句一次性执行
        sqlx::raw_sql(SCHEMA).execute(pool).await?;
        Ok(())
    }

    // ---------- decks ----------

    pub async fn create_deck(
        &self,
        name: &str,
        parent_id: Option<DeckId>,
        now: DateTime<Utc>,
    ) -> Result<Deck> {
        let ms = to_ms(now);
        let parent = parent_id.map(|p| p.0);
        let result = sqlx::query(
            "INSERT INTO decks (name, parent_id, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(name)
        .bind(parent)
        .bind(ms)
        .bind(ms)
        .execute(&self.pool)
        .await?;

        let id = DeckId(result.last_insert_rowid());
        Ok(Deck {
            id,
            name: name.to_string(),
            parent_id,
            created_at: now,
            updated_at: now,
        })
    }

    /// 按名字取牌组，没有就建一个（含默认 deck_params）。
    pub async fn deck_ensure(&self, name: &str, now: DateTime<Utc>) -> Result<Deck> {
        if let Some(deck) = self.get_deck_by_name(name).await? {
            return Ok(deck);
        }
        let deck = self.create_deck(name, None, now).await?;
        self.put_deck_params(deck.id, &DeckParams::default())
            .await?;
        Ok(deck)
    }

    pub async fn get_deck(&self, id: DeckId) -> Result<Option<Deck>> {
        let row = sqlx::query(
            "SELECT id, name, parent_id, created_at, updated_at FROM decks WHERE id = ?1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_deck).transpose()
    }

    pub async fn get_deck_by_name(&self, name: &str) -> Result<Option<Deck>> {
        let row = sqlx::query(
            "SELECT id, name, parent_id, created_at, updated_at FROM decks WHERE name = ?1",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(row_to_deck).transpose()
    }

    pub async fn list_decks(&self) -> Result<Vec<Deck>> {
        let rows = sqlx::query(
            "SELECT id, name, parent_id, created_at, updated_at FROM decks ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_deck).collect()
    }

    // ---------- notes & cards ----------

    /// 写入笔记并按题型展开卡片。填空会生成多张。
    pub async fn add_note(&self, new: NewNote, now: DateTime<Utc>) -> Result<(Note, Vec<Card>)> {
        let drafts = generate_cards(&new.content)?;
        let seeds: Vec<CardSeed> = drafts
            .iter()
            .map(|d| CardSeed {
                ordinal: d.ordinal,
                card_type: d.card_type,
                front: d.front.clone(),
                back: d.back.clone(),
                state: CardState::New,
                step: 0,
                stability: None,
                difficulty: None,
                due: now,
                last_review: None,
                reps: 0,
                lapses: 0,
                suspended: false,
            })
            .collect();
        self.add_note_with_cards(new, seeds, now).await
    }

    /// 写入笔记 + 预渲染卡片（保留外部调度状态，供 apkg 导入）。
    pub async fn add_note_with_cards(
        &self,
        new: NewNote,
        seeds: Vec<CardSeed>,
        now: DateTime<Utc>,
    ) -> Result<(Note, Vec<Card>)> {
        let ms = to_ms(now);
        let content_json = serde_json::to_string(&new.content)?;
        let tags_json = serde_json::to_string(&new.tags)?;

        let mut tx = self.pool.begin().await?;

        let note_result = sqlx::query(
            "INSERT INTO notes (deck_id, content, tags, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(new.deck_id.0)
        .bind(&content_json)
        .bind(&tags_json)
        .bind(&new.source)
        .bind(ms)
        .bind(ms)
        .execute(&mut *tx)
        .await?;

        let note_id = NoteId(note_result.last_insert_rowid());

        let mut cards = Vec::with_capacity(seeds.len());
        for seed in &seeds {
            let card_result = sqlx::query(
                "INSERT INTO cards
                 (note_id, deck_id, card_type, ordinal, front, back, state, step,
                  stability, difficulty, due, last_review, reps, lapses, suspended,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                         ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
            )
            .bind(note_id.0)
            .bind(new.deck_id.0)
            .bind(card_type_to_str(seed.card_type))
            .bind(i64::from(seed.ordinal))
            .bind(&seed.front)
            .bind(&seed.back)
            .bind(state_to_str(seed.state))
            .bind(i64::from(seed.step))
            .bind(seed.stability)
            .bind(seed.difficulty)
            .bind(to_ms(seed.due))
            .bind(to_ms_opt(seed.last_review))
            .bind(i64::from(seed.reps))
            .bind(i64::from(seed.lapses))
            .bind(i64::from(seed.suspended))
            .bind(ms)
            .bind(ms)
            .execute(&mut *tx)
            .await?;

            let card_id = CardId(card_result.last_insert_rowid());

            cards.push(Card {
                id: card_id,
                note_id,
                deck_id: new.deck_id,
                card_type: seed.card_type,
                ordinal: seed.ordinal,
                front: seed.front.clone(),
                back: seed.back.clone(),
                state: seed.state,
                step: seed.step,
                stability: seed.stability,
                difficulty: seed.difficulty,
                due: seed.due,
                last_review: seed.last_review,
                reps: seed.reps,
                lapses: seed.lapses,
                suspended: seed.suspended,
                created_at: now,
                updated_at: now,
            });
        }

        tx.commit().await?;

        let note = Note {
            id: note_id,
            deck_id: new.deck_id,
            content: new.content,
            tags: new.tags,
            source: new.source,
            created_at: now,
            updated_at: now,
        };
        Ok((note, cards))
    }

    /// 批量追加复习记录（apkg 历史导入）。只插不改。
    pub async fn append_review_logs(&self, logs: &[ReviewLog]) -> Result<usize> {
        let mut n = 0;
        let mut tx = self.pool.begin().await?;
        for log in logs {
            sqlx::query(
                "INSERT INTO review_logs
                 (card_id, reviewed_at, rating, state_before, step_before,
                  stability_before, difficulty_before, interval_days, elapsed_days, duration_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .bind(log.card_id.0)
            .bind(to_ms(log.reviewed_at))
            .bind(i64::from(log.rating.as_u8()))
            .bind(state_to_str(log.state_before))
            .bind(i64::from(log.step_before))
            .bind(log.stability_before)
            .bind(log.difficulty_before)
            .bind(log.interval_days)
            .bind(i64::from(log.elapsed_days))
            .bind(i64::from(log.duration_ms))
            .execute(&mut *tx)
            .await?;
            n += 1;
        }
        tx.commit().await?;
        Ok(n)
    }

    pub async fn get_card(&self, id: CardId) -> Result<Option<Card>> {
        let row = sqlx::query("SELECT * FROM cards WHERE id = ?1")
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map(row_to_card).transpose()
    }

    /// 列出牌组下的卡片（浏览用），按状态、due、id 排序。
    pub async fn list_cards(&self, deck_id: DeckId, limit: u32) -> Result<Vec<Card>> {
        let rows = sqlx::query(
            "SELECT * FROM cards
             WHERE deck_id = ?1
             ORDER BY
               CASE state
                 WHEN 'learning' THEN 0
                 WHEN 'relearning' THEN 0
                 WHEN 'new' THEN 1
                 ELSE 2
               END,
               due, id
             LIMIT ?2",
        )
        .bind(deck_id.0)
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_card).collect()
    }

    /// 到期队列：学习/再学习 → 新卡（受额度限制）→ 复习。
    pub async fn due_queue(
        &self,
        now: DateTime<Utc>,
        deck_id: Option<DeckId>,
        new_remaining: u32,
        review_remaining: u32,
        limit: u32,
    ) -> Result<Vec<Card>> {
        let ms = to_ms(now);
        let mut out = Vec::new();

        // 学习中优先，按 due
        let learning = if let Some(d) = deck_id {
            sqlx::query(
                "SELECT * FROM cards
                 WHERE deck_id = ?1 AND suspended = 0
                   AND state IN ('learning', 'relearning')
                   AND due <= ?2
                 ORDER BY due
                 LIMIT ?3",
            )
            .bind(d.0)
            .bind(ms)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT * FROM cards
                 WHERE suspended = 0
                   AND state IN ('learning', 'relearning')
                   AND due <= ?1
                 ORDER BY due
                 LIMIT ?2",
            )
            .bind(ms)
            .bind(i64::from(limit))
            .fetch_all(&self.pool)
            .await?
        };
        out.extend(
            learning
                .into_iter()
                .map(row_to_card)
                .collect::<Result<Vec<_>>>()?,
        );
        if out.len() >= limit as usize {
            return Ok(out);
        }

        // 新卡
        if new_remaining > 0 {
            let take = (limit as usize - out.len()).min(new_remaining as usize) as i64;
            let rows = if let Some(d) = deck_id {
                sqlx::query(
                    "SELECT * FROM cards
                     WHERE deck_id = ?1 AND suspended = 0 AND state = 'new'
                     ORDER BY created_at, id
                     LIMIT ?2",
                )
                .bind(d.0)
                .bind(take)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT * FROM cards
                     WHERE suspended = 0 AND state = 'new'
                     ORDER BY created_at, id
                     LIMIT ?1",
                )
                .bind(take)
                .fetch_all(&self.pool)
                .await?
            };
            out.extend(
                rows.into_iter()
                    .map(row_to_card)
                    .collect::<Result<Vec<_>>>()?,
            );
        }

        // 复习
        if review_remaining > 0 && out.len() < limit as usize {
            let take = (limit as usize - out.len()).min(review_remaining as usize) as i64;
            let rows = if let Some(d) = deck_id {
                sqlx::query(
                    "SELECT * FROM cards
                     WHERE deck_id = ?1 AND suspended = 0
                       AND state = 'review' AND due <= ?2
                     ORDER BY due
                     LIMIT ?3",
                )
                .bind(d.0)
                .bind(ms)
                .bind(take)
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT * FROM cards
                     WHERE suspended = 0
                       AND state = 'review' AND due <= ?1
                     ORDER BY due
                     LIMIT ?2",
                )
                .bind(ms)
                .bind(take)
                .fetch_all(&self.pool)
                .await?
            };
            out.extend(
                rows.into_iter()
                    .map(row_to_card)
                    .collect::<Result<Vec<_>>>()?,
            );
        }

        Ok(out)
    }

    /// 分桶计数。学习中含未到期步骤卡；待复习只算已到期。
    pub async fn queue_counts(
        &self,
        now: DateTime<Utc>,
        deck_id: Option<DeckId>,
    ) -> Result<QueueCounts> {
        let ms = to_ms(now);
        let count = async |sql: &'static str, binds: &[i64]| -> Result<i64> {
            let mut q = sqlx::query(sql);
            for b in binds {
                q = q.bind(*b);
            }
            let row = q.fetch_one(&self.pool).await?;
            Ok(row.try_get::<i64, _>(0)?)
        };

        let (learning, new, review) = match deck_id {
            Some(d) => {
                let learning = count(
                    "SELECT COUNT(*) FROM cards WHERE deck_id = ?1 AND suspended = 0
                     AND state IN ('learning','relearning')",
                    &[d.0],
                )
                .await?;
                let new = count(
                    "SELECT COUNT(*) FROM cards WHERE deck_id = ?1 AND suspended = 0 AND state = 'new'",
                    &[d.0],
                )
                .await?;
                let review = count(
                    "SELECT COUNT(*) FROM cards WHERE deck_id = ?1 AND suspended = 0
                     AND state = 'review' AND due <= ?2",
                    &[d.0, ms],
                )
                .await?;
                (learning, new, review)
            }
            None => {
                let learning = count(
                    "SELECT COUNT(*) FROM cards WHERE suspended = 0
                     AND state IN ('learning','relearning')",
                    &[],
                )
                .await?;
                let new = count(
                    "SELECT COUNT(*) FROM cards WHERE suspended = 0 AND state = 'new'",
                    &[],
                )
                .await?;
                let review = count(
                    "SELECT COUNT(*) FROM cards WHERE suspended = 0
                     AND state = 'review' AND due <= ?1",
                    &[ms],
                )
                .await?;
                (learning, new, review)
            }
        };

        Ok(QueueCounts {
            learning: learning as usize,
            new: new as usize,
            review: review as usize,
        })
    }

    // ---------- review ----------

    /// 落库一次复习：更新卡片 + 追加日志。同一事务。
    pub async fn save_review(&self, card: &Card, log: &ReviewLog) -> Result<ReviewLog> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "UPDATE cards SET
                state = ?1, step = ?2, stability = ?3, difficulty = ?4,
                due = ?5, last_review = ?6, reps = ?7, lapses = ?8,
                updated_at = ?9
             WHERE id = ?10",
        )
        .bind(state_to_str(card.state))
        .bind(i64::from(card.step))
        .bind(card.stability)
        .bind(card.difficulty)
        .bind(to_ms(card.due))
        .bind(to_ms_opt(card.last_review))
        .bind(i64::from(card.reps))
        .bind(i64::from(card.lapses))
        .bind(to_ms(card.updated_at))
        .bind(card.id.0)
        .execute(&mut *tx)
        .await?;

        let log_result = sqlx::query(
            "INSERT INTO review_logs
             (card_id, reviewed_at, rating, state_before, step_before,
              stability_before, difficulty_before, interval_days, elapsed_days, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )
        .bind(log.card_id.0)
        .bind(to_ms(log.reviewed_at))
        .bind(i64::from(log.rating.as_u8()))
        .bind(state_to_str(log.state_before))
        .bind(i64::from(log.step_before))
        .bind(log.stability_before)
        .bind(log.difficulty_before)
        .bind(log.interval_days)
        .bind(i64::from(log.elapsed_days))
        .bind(i64::from(log.duration_ms))
        .execute(&mut *tx)
        .await?;

        let id = ReviewLogId(log_result.last_insert_rowid());

        tx.commit().await?;

        let mut saved = log.clone();
        saved.id = Some(id);
        Ok(saved)
    }

    /// 撤销最后一次复习：恢复卡面快照，删掉那条日志。
    pub async fn undo_review(&self, previous: &Card, log_id: ReviewLogId) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "UPDATE cards SET
                state = ?1, step = ?2, stability = ?3, difficulty = ?4,
                due = ?5, last_review = ?6, reps = ?7, lapses = ?8,
                updated_at = ?9
             WHERE id = ?10",
        )
        .bind(state_to_str(previous.state))
        .bind(i64::from(previous.step))
        .bind(previous.stability)
        .bind(previous.difficulty)
        .bind(to_ms(previous.due))
        .bind(to_ms_opt(previous.last_review))
        .bind(i64::from(previous.reps))
        .bind(i64::from(previous.lapses))
        .bind(to_ms(previous.updated_at))
        .bind(previous.id.0)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM review_logs WHERE id = ?1")
            .bind(log_id.0)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn review_logs_for_card(&self, card_id: CardId) -> Result<Vec<ReviewLog>> {
        let rows =
            sqlx::query("SELECT * FROM review_logs WHERE card_id = ?1 ORDER BY reviewed_at, id")
                .bind(card_id.0)
                .fetch_all(&self.pool)
                .await?;
        rows.into_iter().map(row_to_review_log).collect()
    }

    /// 今日已用额度。
    pub async fn daily_usage(
        &self,
        now: DateTime<Utc>,
        day_start: DateTime<Utc>,
    ) -> Result<DailyUsage> {
        let row = sqlx::query(
            "SELECT
                COALESCE(SUM(CASE WHEN state_before = 'new' THEN 1 ELSE 0 END), 0),
                COUNT(*)
             FROM review_logs
             WHERE reviewed_at >= ?1 AND reviewed_at <= ?2",
        )
        .bind(to_ms(day_start))
        .bind(to_ms(now))
        .fetch_one(&self.pool)
        .await?;

        Ok(DailyUsage {
            new_reviewed: row.try_get::<i64, _>(0)? as u32,
            reviews_done: row.try_get::<i64, _>(1)? as u32,
        })
    }

    // ---------- deck_params ----------

    pub async fn get_deck_params(&self, deck_id: DeckId) -> Result<Option<DeckParams>> {
        let row = sqlx::query("SELECT * FROM deck_params WHERE deck_id = ?1")
            .bind(deck_id.0)
            .fetch_optional(&self.pool)
            .await?;
        row.map(row_to_params).transpose()
    }

    pub async fn put_deck_params(&self, deck_id: DeckId, params: &DeckParams) -> Result<()> {
        let fsrs_json = serde_json::to_string(&params.fsrs_params)?;
        let learning_json = serde_json::to_string(&params.learning_steps)?;
        let relearning_json = serde_json::to_string(&params.relearning_steps)?;

        sqlx::query(
            "INSERT INTO deck_params
             (deck_id, fsrs_params, desired_retention, max_interval_days,
              learning_steps, relearning_steps, new_cards_per_day, reviews_per_day)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT (deck_id) DO UPDATE SET
                fsrs_params = excluded.fsrs_params,
                desired_retention = excluded.desired_retention,
                max_interval_days = excluded.max_interval_days,
                learning_steps = excluded.learning_steps,
                relearning_steps = excluded.relearning_steps,
                new_cards_per_day = excluded.new_cards_per_day,
                reviews_per_day = excluded.reviews_per_day",
        )
        .bind(deck_id.0)
        .bind(&fsrs_json)
        .bind(f64::from(params.desired_retention))
        .bind(i64::from(params.max_interval_days))
        .bind(&learning_json)
        .bind(&relearning_json)
        .bind(i64::from(params.new_cards_per_day))
        .bind(i64::from(params.reviews_per_day))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 取牌组参数，缺省时写入默认值。
    pub async fn deck_params_or_default(&self, deck_id: DeckId) -> Result<DeckParams> {
        if let Some(p) = self.get_deck_params(deck_id).await? {
            return Ok(p);
        }
        let p = DeckParams::default();
        self.put_deck_params(deck_id, &p).await?;
        Ok(p)
    }

    /// 用牌组参数建调度器。
    pub async fn scheduler_for(&self, deck_id: DeckId) -> Result<Scheduler> {
        let params = self.deck_params_or_default(deck_id).await?;
        Ok(Scheduler::new(&params)?)
    }
}

// ---------- row 映射 ----------

fn row_to_deck(row: SqliteRow) -> Result<Deck> {
    Ok(Deck {
        id: DeckId(row.try_get("id")?),
        name: row.try_get("name")?,
        parent_id: row.try_get::<Option<i64>, _>("parent_id")?.map(DeckId),
        created_at: from_ms(row.try_get("created_at")?)?,
        updated_at: from_ms(row.try_get("updated_at")?)?,
    })
}

fn row_to_card(row: SqliteRow) -> Result<Card> {
    let card_type: String = row.try_get("card_type")?;
    let state: String = row.try_get("state")?;
    Ok(Card {
        id: CardId(row.try_get("id")?),
        note_id: NoteId(row.try_get("note_id")?),
        deck_id: DeckId(row.try_get("deck_id")?),
        card_type: card_type_from_str(&card_type)?,
        ordinal: row.try_get::<i64, _>("ordinal")? as u16,
        front: row.try_get("front")?,
        back: row.try_get("back")?,
        state: state_from_str(&state)?,
        step: row.try_get::<i64, _>("step")? as u8,
        stability: row.try_get("stability")?,
        difficulty: row.try_get("difficulty")?,
        due: from_ms(row.try_get("due")?)?,
        last_review: from_ms_opt(row.try_get("last_review")?)?,
        reps: row.try_get::<i64, _>("reps")? as u32,
        lapses: row.try_get::<i64, _>("lapses")? as u32,
        suspended: int_to_bool(row.try_get("suspended")?),
        created_at: from_ms(row.try_get("created_at")?)?,
        updated_at: from_ms(row.try_get("updated_at")?)?,
    })
}

fn row_to_review_log(row: SqliteRow) -> Result<ReviewLog> {
    let state_before: String = row.try_get("state_before")?;
    Ok(ReviewLog {
        id: Some(ReviewLogId(row.try_get("id")?)),
        card_id: CardId(row.try_get("card_id")?),
        reviewed_at: from_ms(row.try_get("reviewed_at")?)?,
        rating: rating_from_i64(row.try_get("rating")?)?,
        state_before: state_from_str(&state_before)?,
        step_before: row.try_get::<i64, _>("step_before")? as u8,
        stability_before: row.try_get("stability_before")?,
        difficulty_before: row.try_get("difficulty_before")?,
        interval_days: row.try_get("interval_days")?,
        elapsed_days: row.try_get::<i64, _>("elapsed_days")? as u32,
        duration_ms: row.try_get::<i64, _>("duration_ms")? as u32,
    })
}

fn row_to_params(row: SqliteRow) -> Result<DeckParams> {
    let fsrs_params: String = row.try_get("fsrs_params")?;
    let learning_steps: String = row.try_get("learning_steps")?;
    let relearning_steps: String = row.try_get("relearning_steps")?;
    Ok(DeckParams {
        fsrs_params: serde_json::from_str(&fsrs_params)?,
        desired_retention: row.try_get::<f64, _>("desired_retention")? as f32,
        max_interval_days: row.try_get::<i64, _>("max_interval_days")? as u32,
        learning_steps: serde_json::from_str(&learning_steps)?,
        relearning_steps: serde_json::from_str(&relearning_steps)?,
        new_cards_per_day: row.try_get::<i64, _>("new_cards_per_day")? as u32,
        reviews_per_day: row.try_get::<i64, _>("reviews_per_day")? as u32,
    })
}

// CardType / Rating 在 row 映射里经字符串与 i64 转换使用

#[cfg(test)]
mod tests {
    use super::*;
    use memstack_core::{CardState, Rating};

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn qa_note(deck: DeckId) -> NewNote {
        NewNote {
            deck_id: deck,
            content: NoteContent::QuestionAnswer {
                front: "问".into(),
                back: "答".into(),
            },
            tags: BTreeSet::from(["t1".to_string()]),
            source: None,
        }
    }

    #[tokio::test]
    async fn migrate_and_crud() {
        let db = Storage::open_in_memory().await.unwrap();
        let t = now();

        let deck = db.deck_ensure("算法", t).await.unwrap();
        assert_eq!(deck.name, "算法");

        let (note, cards) = db.add_note(qa_note(deck.id), t).await.unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].state, CardState::New);
        assert_eq!(note.tags.len(), 1);

        let loaded = db.get_card(cards[0].id).await.unwrap().unwrap();
        assert_eq!(loaded.front, "问");
    }

    #[tokio::test]
    async fn cloze_note_spawns_multiple_cards() {
        let db = Storage::open_in_memory().await.unwrap();
        let t = now();
        let deck = db.deck_ensure("地理", t).await.unwrap();

        let (note, cards) = db
            .add_note(
                NewNote {
                    deck_id: deck.id,
                    content: NoteContent::Cloze {
                        text: "{{c1::北京}}是{{c2::首都}}".into(),
                        extra: String::new(),
                    },
                    tags: BTreeSet::new(),
                    source: Some("a.md".into()),
                },
                t,
            )
            .await
            .unwrap();

        assert_eq!(cards.len(), 2);
        assert_eq!(note.source.as_deref(), Some("a.md"));
    }

    #[tokio::test]
    async fn review_and_undo() {
        let db = Storage::open_in_memory().await.unwrap();
        let t = now();
        let deck = db.deck_ensure("算法", t).await.unwrap();
        let (_, cards) = db.add_note(qa_note(deck.id), t).await.unwrap();
        let card = cards[0].clone();

        let mut next = card.clone();
        next.state = CardState::Learning;
        next.step = 1;
        next.due = t + chrono::Duration::minutes(10);
        next.reps = 1;
        next.last_review = Some(t);
        next.updated_at = t;

        let log = ReviewLog {
            id: None,
            card_id: card.id,
            reviewed_at: t,
            rating: Rating::Good,
            state_before: CardState::New,
            step_before: 0,
            stability_before: None,
            difficulty_before: None,
            interval_days: 0.01,
            elapsed_days: 0,
            duration_ms: 300,
        };

        let saved = db.save_review(&next, &log).await.unwrap();
        assert!(saved.id.is_some());

        let logs = db.review_logs_for_card(card.id).await.unwrap();
        assert_eq!(logs.len(), 1);

        let usage = db
            .daily_usage(t, t - chrono::Duration::hours(1))
            .await
            .unwrap();
        assert_eq!(usage.new_reviewed, 1);
        assert_eq!(usage.reviews_done, 1);

        db.undo_review(&card, saved.id.unwrap()).await.unwrap();
        let restored = db.get_card(card.id).await.unwrap().unwrap();
        assert_eq!(restored.state, CardState::New);
        assert_eq!(restored.reps, 0);
        assert!(db.review_logs_for_card(card.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn due_queue_respects_limits() {
        let db = Storage::open_in_memory().await.unwrap();
        let t = now();
        let deck = db.deck_ensure("算法", t).await.unwrap();

        for i in 0..5 {
            db.add_note(
                NewNote {
                    deck_id: deck.id,
                    content: NoteContent::QuestionAnswer {
                        front: format!("q{i}"),
                        back: "a".into(),
                    },
                    tags: BTreeSet::new(),
                    source: None,
                },
                t,
            )
            .await
            .unwrap();
        }

        let q = db.due_queue(t, None, 2, 100, 10).await.unwrap();
        assert_eq!(q.len(), 2, "new_remaining = 2");

        let q = db.due_queue(t, None, 100, 0, 3).await.unwrap();
        assert_eq!(q.len(), 3, "limit = 3");

        let counts = db.queue_counts(t, None).await.unwrap();
        assert_eq!(counts.new, 5);
    }

    #[tokio::test]
    async fn deck_params_roundtrip() {
        let db = Storage::open_in_memory().await.unwrap();
        let t = now();
        let deck = db.deck_ensure("算法", t).await.unwrap();

        let params = DeckParams {
            desired_retention: 0.85,
            learning_steps: vec![3, 15],
            fsrs_params: vec![0.1; 21],
            ..DeckParams::default()
        };
        db.put_deck_params(deck.id, &params).await.unwrap();

        let got = db.deck_params_or_default(deck.id).await.unwrap();
        assert!((got.desired_retention - 0.85).abs() < 1e-6);
        assert_eq!(got.learning_steps, vec![3, 15]);
        assert_eq!(got.fsrs_params.len(), 21);
    }
}
