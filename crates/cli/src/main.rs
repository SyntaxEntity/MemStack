use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod add;
mod import;
mod review;
mod term;
mod tui;

#[derive(Parser, Debug)]
#[command(name = "memstack", version, about = "忆栈 MemStack — 开源间隔重复记忆卡")]
struct Cli {
    /// SQLite 数据库路径
    #[arg(long, global = true, default_value = "memstack.sqlite")]
    db: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 造卡：问答 / 填空 / 代码填空
    Add {
        /// 牌组名
        #[arg(long)]
        deck: String,
        /// 标签，逗号分隔
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        #[command(subcommand)]
        kind: AddKind,
    },
    /// 列出牌组
    Decks,
    /// 复习：空格翻面，1-4 评分，u 撤销，q 退出
    Review {
        /// 只复习指定牌组
        #[arg(long)]
        deck: Option<String>,
        /// 本次最多复习多少张（默认 20）
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// 按状态统计卡片数
    Stats {
        #[arg(long)]
        deck: Option<String>,
    },
    /// 从 Markdown 导入：单文件或目录（*.md）。`***` 切多张卡。
    Import {
        /// .md 文件、目录，或 .apkg
        path: PathBuf,
        /// 覆盖 frontmatter 里的 deck
        #[arg(long)]
        deck: Option<String>,
        /// 干跑，只打印将导入的内容
        #[arg(long)]
        dry_run: bool,
    },
    /// 交互浏览：牌组 → 卡片列表 → 复习
    Tui,
}

#[derive(Subcommand, Debug)]
enum AddKind {
    /// 问答卡
    Qa {
        #[arg(long)]
        front: String,
        #[arg(long)]
        back: String,
    },
    /// 填空卡：正文里写 {{c1::答案}}
    Cloze {
        #[arg(long)]
        text: String,
        /// 补充说明
        #[arg(long, default_value = "")]
        extra: String,
    },
    /// 代码填空：代码里挖洞
    Code {
        #[arg(long)]
        code: String,
        /// 高亮语言，如 rust / sql
        #[arg(long, default_value = "")]
        language: String,
        #[arg(long, default_value = "")]
        extra: String,
    },
}

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("错误：{err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let db = memstack_storage::Storage::open(&cli.db).await?;

    match cli.command {
        Command::Add { deck, tags, kind } => add::run(&db, &deck, &tags, kind).await?,
        Command::Decks => {
            for deck in db.list_decks().await? {
                println!("{}", deck.name);
            }
        }
        Command::Review { deck, limit } => review::run(&db, deck.as_deref(), limit).await?,
        Command::Import {
            path,
            deck,
            dry_run,
        } => import::run(&db, &path, deck.as_deref(), dry_run).await?,
        Command::Tui => tui::run(&db).await?,
        Command::Stats { deck } => {
            let deck_id = match deck.as_deref() {
                Some(name) => db
                    .get_deck_by_name(name)
                    .await?
                    .map(|d| d.id),
                None => None,
            };
            if deck.is_some() && deck_id.is_none() {
                return Err("牌组不存在".into());
            }
            let now = chrono::Utc::now();
            let counts = db.queue_counts(now, deck_id).await?;
            println!("学习中 {}", counts.learning);
            println!("新卡   {}", counts.new);
            println!("待复习 {}", counts.review);
            println!("合计   {}", counts.total());
        }
    }
    Ok(())
}
