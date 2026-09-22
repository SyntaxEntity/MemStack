//! 从 .apkg 解出 collection SQLite，并读出原始表。

use crate::error::{Error, Result};
use rusqlite::{Connection, OpenFlags};
use std::io::Read;
use std::path::PathBuf;

/// collection 里的原始行。
pub struct RawCollection {
    pub crt: i64,
    pub models_json: String,
    pub decks_json: String,
    pub notes: Vec<RawNote>,
    pub cards: Vec<RawCard>,
    pub revlogs: Vec<RawRevlog>,
}

pub struct RawNote {
    pub id: i64,
    pub mid: i64,
    pub mod_ms: i64,
    /// 字段用 \x1f 分隔
    pub flds: String,
    /// " tag1 tag2 "
    pub tags: String,
}

pub struct RawCard {
    pub id: i64,
    pub nid: i64,
    pub did: i64,
    pub ord: i64,
    /// 0=new 1=learning 2=review 3=relearning
    pub ctype: i64,
    /// 0=new 1=learning 2=review 3=day learning 4=preview -1=suspended -2=buried
    pub queue: i64,
    pub due: i64,
    pub ivl: i64,
    pub factor: i64,
    pub reps: i64,
    pub lapses: i64,
    pub left: i64,
}

pub struct RawRevlog {
    pub id: i64,
    pub cid: i64,
    /// 1-4
    pub ease: i64,
    pub ivl: i64,
    #[allow(dead_code)]
    pub last_ivl: i64,
    #[allow(dead_code)]
    pub factor: i64,
    /// 毫秒
    pub time_ms: i64,
    /// 0=learn 1=review 2=relearn 3=cram 4=manual
    pub rtype: i64,
}

pub fn read_collection(apkg_path: &str) -> Result<RawCollection> {
    let file = std::fs::File::open(apkg_path).map_err(|source| Error::Io {
        path: apkg_path.to_string(),
        source,
    })?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| Error::BadApkg(e.to_string()))?;

    let mut db_bytes = None;
    for name in ["collection.anki21", "collection.anki2"] {
        if let Ok(mut entry) = zip.by_name(name) {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| Error::BadApkg(e.to_string()))?;
            db_bytes = Some(buf);
            break;
        }
    }
    let db_bytes = db_bytes.ok_or(Error::NoCollection)?;

    // rusqlite 要文件路径，落到临时文件
    let tmp: PathBuf = std::env::temp_dir().join(format!(
        "memstack-import-{}-{}.anki2",
        std::process::id(),
        apkg_path.len()
    ));
    std::fs::write(&tmp, &db_bytes).map_err(|source| Error::Io {
        path: tmp.display().to_string(),
        source,
    })?;

    let result = load_from_sqlite(&tmp);
    let _ = std::fs::remove_file(&tmp);
    result
}

fn load_from_sqlite(path: &std::path::Path) -> Result<RawCollection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;

    let (crt, models_json, decks_json): (i64, String, String) = conn
        .query_row("SELECT crt, models, decks FROM col LIMIT 1", [], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;

    let mut notes = Vec::new();
    {
        let mut stmt =
            conn.prepare("SELECT id, mid, mod, flds, tags FROM notes ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok(RawNote {
                id: row.get(0)?,
                mid: row.get(1)?,
                mod_ms: row.get(2)?,
                flds: row.get(3)?,
                tags: row.get(4)?,
            })
        })?;
        for row in rows {
            notes.push(row?);
        }
    }

    let mut cards = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, nid, did, ord, type, queue, due, ivl, factor, reps, lapses, left
             FROM cards ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RawCard {
                id: row.get(0)?,
                nid: row.get(1)?,
                did: row.get(2)?,
                ord: row.get(3)?,
                ctype: row.get(4)?,
                queue: row.get(5)?,
                due: row.get(6)?,
                ivl: row.get(7)?,
                factor: row.get(8)?,
                reps: row.get(9)?,
                lapses: row.get(10)?,
                left: row.get(11)?,
            })
        })?;
        for row in rows {
            cards.push(row?);
        }
    }

    let mut revlogs = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, cid, ease, ivl, lastIvl, factor, time, type FROM revlog ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RawRevlog {
                id: row.get(0)?,
                cid: row.get(1)?,
                ease: row.get(2)?,
                ivl: row.get(3)?,
                last_ivl: row.get(4)?,
                factor: row.get(5)?,
                time_ms: row.get(6)?,
                rtype: row.get(7)?,
            })
        })?;
        for row in rows {
            revlogs.push(row?);
        }
    }

    Ok(RawCollection {
        crt,
        models_json,
        decks_json,
        notes,
        cards,
        revlogs,
    })
}
