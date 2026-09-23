//! 忆栈 MemStack 桌面壳（Tauri 2）。
//!
//! 业务在 `commands`，UI 在 `frontend/`。core 不碰 GUI。

mod commands;
mod dto;
mod error;
mod session;

use std::path::PathBuf;

use commands::{Db, ReviewState};
use memstack_storage::Storage;
use tauri::Manager;
use tokio::sync::Mutex;

fn default_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("MEMSTACK_DB") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".memstack/memstack.db")
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let path = default_db_path();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let handle = app.handle().clone();
            let path2 = path.clone();
            // Storage::open 是 async，在阻塞线程里起一个 runtime 跑一次
            let storage = std::thread::spawn(move || {
                tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                    .expect("tokio runtime")
                    .block_on(Storage::open(&path2))
            })
            .join()
            .expect("open db thread")
            .expect("open sqlite");

            handle.manage(Db(storage));
            handle.manage(ReviewState(Mutex::new(None)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_decks,
            commands::ensure_deck,
            commands::list_notes,
            commands::get_note,
            commands::create_note,
            commands::update_note,
            commands::list_note_cards,
            commands::start_review,
            commands::rate_card,
            commands::undo_review,
            commands::flip_card,
            commands::review_done,
            commands::queue_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
