//! `memstack sync`：对接同步服务。

use clap::Subcommand;
use memstack_storage::Storage;
use memstack_sync::{Client, Mapping};

#[derive(Subcommand, Debug)]
pub enum SyncCmd {
    /// 注册并保存令牌
    Register {
        /// 同步服务地址，如 http://127.0.0.1:8080
        #[arg(long)]
        url: String,
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// 登录并保存令牌
    Login {
        #[arg(long)]
        url: String,
        #[arg(long)]
        username: String,
        #[arg(long)]
        password: String,
    },
    /// 查看当前同步配置
    Status,
    /// 推送本地变更
    Push,
    /// 拉取远端变更
    Pull,
    /// 推送 + 拉取
    Run,
}

pub async fn run(db: &Storage, cmd: SyncCmd) -> Result<(), Box<dyn std::error::Error>> {
    let map = Mapping::attach(db.pool().clone()).await?;
    match cmd {
        SyncCmd::Register {
            url,
            username,
            password,
        } => {
            let mut client = Client::new(&url);
            let auth = client.register(&username, &password).await?;
            map.set_meta("server_url", url.trim_end_matches('/'))
                .await?;
            map.set_meta("token", &auth.token).await?;
            println!("已注册 user_id={}，令牌已保存", auth.user_id);
        }
        SyncCmd::Login {
            url,
            username,
            password,
        } => {
            let mut client = Client::new(&url);
            let auth = client.login(&username, &password).await?;
            map.set_meta("server_url", url.trim_end_matches('/'))
                .await?;
            map.set_meta("token", &auth.token).await?;
            println!("已登录 user_id={}，令牌已保存", auth.user_id);
        }
        SyncCmd::Status => {
            let url = map.get_meta("server_url").await?;
            let token = map.get_meta("token").await?;
            let cursor = map.get_meta("cursor").await?;
            match url {
                Some(url) => {
                    println!("服务   {url}");
                    println!(
                        "令牌   {}",
                        if token.is_some() {
                            "已配置"
                        } else {
                            "未配置"
                        }
                    );
                    println!("游标   {}", cursor.unwrap_or_else(|| "0".into()));
                }
                None => println!("未配置同步服务。先 `memstack sync login --url ...`"),
            }
        }
        SyncCmd::Push => {
            let client = load_client(&map).await?;
            let items = memstack_sync::export_items(db, &map).await?;
            let n = items.len();
            let res = client.push(items).await?;
            println!("已推送 {n} 条（inserted/updated/skipped 服务端裁定）");
            let mut ins = 0;
            let mut upd = 0;
            let mut skip = 0;
            for a in &res.applied {
                match a.as_str() {
                    "inserted" => ins += 1,
                    "updated" => upd += 1,
                    _ => skip += 1,
                }
            }
            println!(
                "  inserted={ins} updated={upd} skipped={skip} cursor={}",
                res.cursor
            );
        }
        SyncCmd::Pull => {
            let client = load_client(&map).await?;
            let (n, cursor) = pull_all(db, &map, &client).await?;
            println!("已拉取并落地 {n} 条，cursor={cursor}");
        }
        SyncCmd::Run => {
            let client = load_client(&map).await?;
            let (pushed, applied) = memstack_sync::sync_once(db, &map, &client).await?;
            println!("同步完成：推送结果 {pushed} 条，落地远端 {applied} 条");
        }
    }
    Ok(())
}

async fn load_client(map: &Mapping) -> Result<Client, Box<dyn std::error::Error>> {
    let url = map
        .get_meta("server_url")
        .await?
        .ok_or("未配置服务地址，先 memstack sync login")?;
    let token = map
        .get_meta("token")
        .await?
        .ok_or("未配置令牌，先 memstack sync login")?;
    Ok(Client::new(url).with_token(token))
}

async fn pull_all(
    db: &Storage,
    map: &Mapping,
    client: &Client,
) -> Result<(usize, i64), Box<dyn std::error::Error>> {
    let since: i64 = map
        .get_meta("cursor")
        .await?
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut cursor = since;
    let mut applied = 0;
    loop {
        let page = client.pull(cursor, 500).await?;
        applied += memstack_sync::apply_items(db, map, &page.items).await?;
        cursor = page.cursor;
        if !page.has_more {
            break;
        }
    }
    map.set_meta("cursor", &cursor.to_string()).await?;
    Ok((applied, cursor))
}
