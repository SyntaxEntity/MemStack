//! 忆栈同步服务（AGPLv3）。
//!
//! 账号 + 增量同步 + 冲突合并。自建免费，见 README。

mod auth;
mod db;
mod dto;
mod error;
mod sync;

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::routing::{get, post};
use axum::Router;
use clap::Parser;
use tower_http::trace::TraceLayer;

use db::Db;
use sync::AppState;

#[derive(Parser, Debug)]
#[command(
    name = "memstack-server",
    version,
    about = "忆栈同步服务 — 账号、增量同步、冲突处理"
)]
struct Cli {
    /// 监听地址
    #[arg(long, env = "MEMSTACK_BIND", default_value = "0.0.0.0:8080")]
    bind: SocketAddr,

    /// 数据目录（存放 memstack-server.db）
    #[arg(long, env = "MEMSTACK_DATA", default_value = "/data")]
    data: PathBuf,
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/health", get(sync::health))
        .route("/v1/auth/register", post(sync::register))
        .route("/v1/auth/login", post(sync::login))
        .route("/v1/sync/pull", get(sync::pull))
        .route("/v1/sync/push", post(sync::push))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let _ = std::fs::create_dir_all(&cli.data);
    let db_path = cli.data.join("memstack-server.db");
    let db = Db::open(&db_path).await.expect("打开同步服务数据库失败");
    let state = AppState { db };
    let listener = tokio::net::TcpListener::bind(cli.bind)
        .await
        .expect("监听失败");
    eprintln!(
        "memstack-server listening on {} data={}",
        cli.bind,
        db_path.display()
    );
    axum::serve(listener, app(state)).await.expect("服务退出");
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::{json, Value};
    use tower::ServiceExt;

    async fn test_app() -> Router {
        app(AppState {
            db: Db::open_in_memory().await.unwrap(),
        })
    }

    async fn json_body(res: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(res.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn post_json(
        app: &Router,
        path: &str,
        token: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut req = Request::post(path)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        if let Some(t) = token {
            req.headers_mut()
                .insert("authorization", format!("Bearer {t}").parse().unwrap());
        }
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (status, json_body(res).await)
    }

    async fn get_json(app: &Router, path: &str, token: Option<&str>) -> (StatusCode, Value) {
        let mut req = Request::get(path).body(Body::empty()).unwrap();
        if let Some(t) = token {
            req.headers_mut()
                .insert("authorization", format!("Bearer {t}").parse().unwrap());
        }
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        (status, json_body(res).await)
    }

    #[tokio::test]
    async fn health_ok() {
        let app = test_app().await;
        let res = app
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn register_login_and_sync_roundtrip() {
        let app = test_app().await;

        let (st, auth) = post_json(
            &app,
            "/v1/auth/register",
            None,
            json!({"username": "ada", "password": "lovelace"}),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{auth}");
        let token = auth["token"].as_str().unwrap().to_string();

        // 重复注册
        let (st, _) = post_json(
            &app,
            "/v1/auth/register",
            None,
            json!({"username": "ada", "password": "x"}),
        )
        .await;
        assert_eq!(st, StatusCode::CONFLICT);

        // 登录
        let (st, login) = post_json(
            &app,
            "/v1/auth/login",
            None,
            json!({"username": "ada", "password": "lovelace"}),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(login["user_id"], auth["user_id"]);

        // 未认证 pull
        let (st, _) = get_json(&app, "/v1/sync/pull?since=0", None).await;
        assert_eq!(st, StatusCode::UNAUTHORIZED);

        // push 两条
        let (st, push) = post_json(
            &app,
            "/v1/sync/push",
            Some(&token),
            json!({
                "items": [
                    {"item_id": "note-1", "kind": "note", "updated_at": 100, "payload": {"front": "q"}},
                    {"item_id": "log-1", "kind": "review_log", "updated_at": 200, "payload": {"rating": 3}}
                ]
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{push}");
        assert_eq!(push["applied"], json!(["inserted", "inserted"]));
        assert_eq!(push["cursor"], 2);

        // pull 全量
        let (st, pull) = get_json(&app, "/v1/sync/pull?since=0", Some(&token)).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(pull["cursor"], 2);
        assert_eq!(pull["items"].as_array().unwrap().len(), 2);

        // LWW：更旧的更新应 skip
        let (st, push) = post_json(
            &app,
            "/v1/sync/push",
            Some(&token),
            json!({
                "items": [
                    {"item_id": "note-1", "kind": "note", "updated_at": 50, "payload": {"front": "old"}}
                ]
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(push["applied"], json!(["skipped"]));

        // 更新的覆盖
        let (st, push) = post_json(
            &app,
            "/v1/sync/push",
            Some(&token),
            json!({
                "items": [
                    {"item_id": "note-1", "kind": "note", "updated_at": 300, "payload": {"front": "new"}, "deleted": true}
                ]
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(push["applied"], json!(["updated"]));

        // review_log 已存在则 skip
        let (st, push) = post_json(
            &app,
            "/v1/sync/push",
            Some(&token),
            json!({
                "items": [
                    {"item_id": "log-1", "kind": "review_log", "updated_at": 999, "payload": {"rating": 1}}
                ]
            }),
        )
        .await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(push["applied"], json!(["skipped"]));

        // 增量 pull
        let (st, pull) = get_json(&app, "/v1/sync/pull?since=2", Some(&token)).await;
        assert_eq!(st, StatusCode::OK);
        let items = pull["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["item_id"], "note-1");
        assert_eq!(items[0]["deleted"], true);
        assert_eq!(pull["cursor"], 3);
    }

    #[tokio::test]
    async fn users_are_isolated() {
        let app = test_app().await;
        let (_, a) = post_json(
            &app,
            "/v1/auth/register",
            None,
            json!({"username": "a", "password": "1"}),
        )
        .await;
        let (_, b) = post_json(
            &app,
            "/v1/auth/register",
            None,
            json!({"username": "b", "password": "1"}),
        )
        .await;
        let ta = a["token"].as_str().unwrap();
        let tb = b["token"].as_str().unwrap();

        post_json(
            &app,
            "/v1/sync/push",
            Some(ta),
            json!({"items": [{"item_id": "n1", "kind": "note", "updated_at": 1, "payload": {}}]}),
        )
        .await;

        let (_, pull) = get_json(&app, "/v1/sync/pull?since=0", Some(tb)).await;
        assert_eq!(pull["items"].as_array().unwrap().len(), 0);
    }
}
