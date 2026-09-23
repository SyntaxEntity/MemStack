//! `memstack stats`：概况、热力图、到期预测。

use chrono::Utc;
use memstack_storage::{heat_level, Storage};

#[derive(Debug, Default, Clone, clap::Args)]
pub struct StatsArgs {
    /// 只看这个牌组的到期预测（概况仍是全库）
    #[arg(long)]
    pub deck: Option<String>,
    /// 热力图天数
    #[arg(long, default_value_t = 112)]
    pub heat_days: u32,
    /// 预测天数
    #[arg(long, default_value_t = 14)]
    pub forecast_days: u32,
    /// 输出 JSON
    #[arg(long)]
    pub json: bool,
}

pub async fn run(db: &Storage, args: &StatsArgs) -> Result<(), Box<dyn std::error::Error>> {
    let now = Utc::now();
    let summary = db.stats_summary(now).await?;
    let heat = db.review_heatmap(now, args.heat_days).await?;
    let forecast = db.due_forecast(now, args.forecast_days).await?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "summary": summary,
                "heatmap": heat,
                "forecast": forecast,
            }))?
        );
        return Ok(());
    }

    println!("牌组   {}", summary.decks);
    println!("笔记   {}", summary.notes);
    let c = summary.counts;
    println!(
        "卡片   {}（新 {} · 学习 {} · 复习 {} · 再学习 {}）",
        summary.cards,
        c.new,
        c.learning + c.relearning,
        c.review,
        c.relearning
    );
    println!("待复习 {}", summary.due_now);
    match summary.accuracy_30d {
        Some(a) => println!(
            "近30天复习 {} · 正确率 {:.0}%",
            summary.reviews_30d,
            a * 100.0
        ),
        None => println!("近30天复习 0"),
    }
    println!("连续天数 {}", summary.streak_days);

    println!();
    println!("复习热力图（近 {} 天）", heat.len());
    print_heatmap(&heat);

    println!();
    println!("到期预测（近 {} 天）", forecast.len());
    let max = forecast.iter().map(|d| d.due).max().unwrap_or(1).max(1);
    for day in &forecast {
        let bar_len = (day.due * 24 / max) as usize;
        let bar = "█".repeat(bar_len);
        let label = day.date.split('-').skip(1).collect::<Vec<_>>().join("-");
        println!("  {label}  {bar} {}", day.due);
    }
    Ok(())
}

/// 按周打印（每行一周，周一→周日）。
fn print_heatmap(days: &[memstack_storage::DayCount]) {
    const SHADE: [char; 5] = ['·', '░', '▒', '▓', '█'];
    if days.is_empty() {
        return;
    }
    use chrono::Datelike;
    let shift = chrono::NaiveDate::parse_from_str(&days[0].date, "%Y-%m-%d")
        .map(|d| d.weekday().num_days_from_monday())
        .unwrap_or(0);
    print!("       ");
    for _ in 0..shift {
        print!("  ");
    }
    for (i, day) in days.iter().enumerate() {
        let lvl = heat_level(day.count) as usize;
        print!("{}", SHADE[lvl.min(4)]);
        if (i as u32 + shift) % 7 == 6 {
            println!();
            print!("       ");
        } else {
            print!(" ");
        }
    }
    if (days.len() as u32 + shift) % 7 != 0 {
        println!();
    }
    println!("       浅 ·░▒▓█ 深");
}
