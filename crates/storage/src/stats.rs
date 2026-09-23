//! 统计：摘要、复习热力图、到期预测。

use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::codec::state_from_str;
use crate::error::Result;
use crate::Storage;

/// 卡片状态计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardCounts {
    pub new: u64,
    pub learning: u64,
    pub review: u64,
    pub relearning: u64,
}

impl CardCounts {
    pub fn total(&self) -> u64 {
        self.new + self.learning + self.review + self.relearning
    }
}

/// 一日复习量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DayCount {
    /// 本地日期 YYYY-MM-DD
    pub date: String,
    pub count: u64,
}

/// 一日到期量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DueDay {
    pub date: String,
    pub due: u64,
}

/// 学习概况。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatsSummary {
    pub decks: u64,
    pub notes: u64,
    pub cards: u64,
    pub counts: CardCounts,
    pub due_now: u64,
    pub total_reviews: u64,
    pub reviews_30d: u64,
    /// 近 30 天非 Again 占比（0–1），无记录为 None。
    pub accuracy_30d: Option<f32>,
    pub streak_days: u32,
}

impl Storage {
    pub async fn stats_summary(&self, now: DateTime<Utc>) -> Result<StatsSummary> {
        let decks: i64 = sqlx::query("SELECT COUNT(*) AS n FROM decks")
            .fetch_one(&self.pool)
            .await?
            .try_get("n")?;
        let notes: i64 = sqlx::query("SELECT COUNT(*) AS n FROM notes")
            .fetch_one(&self.pool)
            .await?
            .try_get("n")?;
        let cards: i64 = sqlx::query("SELECT COUNT(*) AS n FROM cards")
            .fetch_one(&self.pool)
            .await?
            .try_get("n")?;

        let mut counts = CardCounts::default();
        let rows = sqlx::query("SELECT state, COUNT(*) AS n FROM cards GROUP BY state")
            .fetch_all(&self.pool)
            .await?;
        for row in rows {
            let s: String = row.try_get("state")?;
            let n: i64 = row.try_get("n")?;
            match state_from_str(&s)? {
                memstack_core::CardState::New => counts.new += n as u64,
                memstack_core::CardState::Learning => counts.learning += n as u64,
                memstack_core::CardState::Review => counts.review += n as u64,
                memstack_core::CardState::Relearning => counts.relearning += n as u64,
            }
        }

        let now_ms = now.timestamp_millis();
        let due_now: i64 =
            sqlx::query("SELECT COUNT(*) AS n FROM cards WHERE suspended = 0 AND due <= ?1")
                .bind(now_ms)
                .fetch_one(&self.pool)
                .await?
                .try_get("n")?;

        let total_reviews: i64 = sqlx::query("SELECT COUNT(*) AS n FROM review_logs")
            .fetch_one(&self.pool)
            .await?
            .try_get("n")?;

        let since30 = (now - Duration::days(30)).timestamp_millis();
        let row = sqlx::query(
            "SELECT COUNT(*) AS n,
                    SUM(CASE WHEN rating = 1 THEN 1 ELSE 0 END) AS agains
             FROM review_logs WHERE reviewed_at >= ?1",
        )
        .bind(since30)
        .fetch_one(&self.pool)
        .await?;
        let reviews_30d: i64 = row.try_get::<i64, _>("n")?;
        let agains: i64 = row.try_get::<Option<i64>, _>("agains")?.unwrap_or(0);
        let accuracy_30d = if reviews_30d > 0 {
            Some((reviews_30d - agains) as f32 / reviews_30d as f32)
        } else {
            None
        };

        let streak_days = self.streak_days(now).await?;

        Ok(StatsSummary {
            decks: decks as u64,
            notes: notes as u64,
            cards: cards as u64,
            counts,
            due_now: due_now as u64,
            total_reviews: total_reviews as u64,
            reviews_30d: reviews_30d as u64,
            accuracy_30d,
            streak_days,
        })
    }

    /// 连续有复习的天数（含今天或昨天起算）。
    async fn streak_days(&self, now: DateTime<Utc>) -> Result<u32> {
        let rows = sqlx::query(
            "SELECT DISTINCT date(reviewed_at / 1000, 'unixepoch', 'localtime') AS d
             FROM review_logs
             WHERE reviewed_at >= ?1
             ORDER BY d DESC",
        )
        .bind((now - Duration::days(400)).timestamp_millis())
        .fetch_all(&self.pool)
        .await?;
        let mut days = Vec::new();
        for row in rows {
            days.push(row.try_get::<String, _>("d")?);
        }
        if days.is_empty() {
            return Ok(0);
        }
        let local_today = now.with_timezone(&chrono::Local).date_naive();
        let today = local_today.format("%Y-%m-%d").to_string();
        let yesterday = (local_today - Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        if days[0] != today && days[0] != yesterday {
            return Ok(0);
        }
        let mut streak = 0u32;
        let mut expect = days[0].clone();
        for d in days.drain(..) {
            if d != expect {
                break;
            }
            streak += 1;
            if let Ok(nd) = NaiveDate::parse_from_str(&expect, "%Y-%m-%d") {
                expect = (nd - Duration::days(1)).format("%Y-%m-%d").to_string();
            } else {
                break;
            }
        }
        Ok(streak)
    }

    /// 近 `days` 天每日复习量（含无复习的日期补 0，升序）。
    pub async fn review_heatmap(&self, now: DateTime<Utc>, days: u32) -> Result<Vec<DayCount>> {
        let days = days.max(1);
        let start =
            now.with_timezone(&chrono::Local).date_naive() - Duration::days(i64::from(days) - 1);
        let start_ms = Utc
            .from_local_datetime(&start.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|d| d.timestamp_millis())
            .unwrap_or_else(|| (now - Duration::days(i64::from(days))).timestamp_millis());

        let rows = sqlx::query(
            "SELECT date(reviewed_at / 1000, 'unixepoch', 'localtime') AS d, COUNT(*) AS n
             FROM review_logs
             WHERE reviewed_at >= ?1
             GROUP BY d",
        )
        .bind(start_ms)
        .fetch_all(&self.pool)
        .await?;
        let mut map = std::collections::BTreeMap::new();
        for row in rows {
            let d: String = row.try_get("d")?;
            let n: i64 = row.try_get("n")?;
            map.insert(d, n as u64);
        }

        let mut out = Vec::with_capacity(days as usize);
        for i in 0..days {
            let date = start + Duration::days(i64::from(i));
            let key = date.format("%Y-%m-%d").to_string();
            let count = map.get(&key).copied().unwrap_or(0);
            out.push(DayCount { date: key, count });
        }
        Ok(out)
    }

    /// 未来 `days` 天到期量（按当前 `cards.due` 分桶，升序，不含已暂停）。
    pub async fn due_forecast(&self, now: DateTime<Utc>, days: u32) -> Result<Vec<DueDay>> {
        let days = days.max(1);
        let local_today = now.with_timezone(&chrono::Local).date_naive();
        let end = local_today + Duration::days(i64::from(days));
        let start_ms = Utc
            .from_local_datetime(&local_today.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|d| d.timestamp_millis())
            .unwrap_or(now.timestamp_millis());
        let end_ms = Utc
            .from_local_datetime(&end.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|d| d.timestamp_millis())
            .unwrap_or(start_ms);

        let rows = sqlx::query(
            "SELECT date(due / 1000, 'unixepoch', 'localtime') AS d, COUNT(*) AS n
             FROM cards
             WHERE suspended = 0 AND due >= ?1 AND due < ?2
             GROUP BY d",
        )
        .bind(start_ms)
        .bind(end_ms)
        .fetch_all(&self.pool)
        .await?;
        let mut map = std::collections::BTreeMap::new();
        for row in rows {
            let d: String = row.try_get("d")?;
            let n: i64 = row.try_get("n")?;
            map.insert(d, n as u64);
        }

        let mut out = Vec::with_capacity(days as usize);
        for i in 0..days {
            let date = local_today + Duration::days(i64::from(i));
            let key = date.format("%Y-%m-%d").to_string();
            let due = map.get(&key).copied().unwrap_or(0);
            out.push(DueDay { date: key, due });
        }
        Ok(out)
    }
}

/// 热力图 ASCII 色阶（浅→深）。
pub fn heat_level(count: u64) -> u8 {
    match count {
        0 => 0,
        1..=4 => 1,
        5..=14 => 2,
        15..=34 => 3,
        _ => 4,
    }
}
