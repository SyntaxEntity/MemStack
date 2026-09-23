use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod add;
mod import;
mod review;
mod stats;
mod sync;
mod term;
mod tui;

#[derive(Parser, Debug)]
#[command(
    name = "memstack",
    version,
    about = "忆栈 MemStack — 开源间隔重复记忆卡"
)]
struct Cli {
    /// SQLite 数据库路径
    #[arg(long, global = true, default_value = "memstack.sqlite")]
    db: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// 造卡：问答 / 填空 / 代码填空，或 --template 套模板
    Add {
        /// 牌组名
        #[arg(long)]
        deck: String,
        /// 标签，逗号分隔
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// 内置模板 id（见 `memstack templates`）
        #[arg(long)]
        template: Option<String>,
        #[command(subcommand)]
        kind: Option<AddKind>,
    },
    /// 列出 / 查看造卡模板
    Templates {
        /// 模板 id；省略则列出全部
        id: Option<String>,
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
    /// 统计：概况、复习热力图、到期预测
    Stats {
        #[command(flatten)]
        args: stats::StatsArgs,
    },
    /// 从 JSON 造卡（编辑器集成）。`path` 为 `-` 时读 stdin。
    AddJson {
        /// 牌组名
        #[arg(long)]
        deck: String,
        /// 标签，逗号分隔
        #[arg(long, value_delimiter = ',')]
        tags: Vec<String>,
        /// JSON 文件路径，或 `-`
        path: PathBuf,
    },
    /// 从 Markdown / CSV / Anki 导入。Markdown 可单文件或目录（*.md）。
    Import {
        /// .md 文件、目录、.csv，或 .apkg
        path: PathBuf,
        /// 覆盖文件里的 deck
        #[arg(long)]
        deck: Option<String>,
        /// 干跑，只打印将导入的内容
        #[arg(long)]
        dry_run: bool,
    },
    /// 交互浏览：牌组 → 卡片列表 → 复习
    Tui,
    /// 同步：register / login / status / push / pull / run
    Sync {
        #[command(subcommand)]
        cmd: sync::SyncCmd,
    },
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
        Command::Add {
            deck,
            tags,
            template,
            kind,
        } => {
            add::run(&db, &deck, &tags, template.as_deref(), kind).await?;
        }
        Command::Templates { id } => match id.as_deref() {
            Some(id) => add::show_template(id).await?,
            None => add::list_templates().await,
        },
        Command::AddJson { deck, tags, path } => add::run_json(&db, &deck, &tags, &path).await?,
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
        Command::Sync { cmd } => sync::run(&db, cmd).await?,
        Command::Stats { args } => stats::run(&db, &args).await?,
    }
    Ok(())
}
