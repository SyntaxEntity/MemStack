//! 用合成 .apkg 覆盖解析：牌组、标签、挖空、调度、revlog。

use crate::anki::parse_apkg;
use memstack_core::{CardState, NoteContent, Rating};
use rusqlite::Connection;
use std::io::Write;

const SCHEMA: &str = r#"
CREATE TABLE col (id integer primary key, crt integer, mod integer, scm integer, ver integer,
  dty integer, usn integer, ls integer, conf text, models text, decks text, dconf text, tags text);
CREATE TABLE notes (id integer primary key, guid text, mid integer, mod integer, usn integer,
  tags text, flds text, sfld text, csum integer, flags integer, data text);
CREATE TABLE cards (id integer primary key, nid integer, did integer, ord integer, mod integer,
  usn integer, type integer, queue integer, due integer, ivl integer, factor integer,
  reps integer, lapses integer, left integer, odue integer, odid integer, flags integer, data text);
CREATE TABLE revlog (id integer primary key, cid integer, usn integer, ease integer, ivl integer,
  lastIvl integer, factor integer, time integer, type integer);
CREATE TABLE graves (usn integer, oid integer, type integer);
"#;

fn sample_models() -> String {
    r#"{
      "100": {
        "id": 100,
        "name": "Basic",
        "type": 0,
        "flds": [{"name":"Front","ord":0},{"name":"Back","ord":1}],
        "tmpls": [{"name":"Card 1","ord":0,"qfmt":"{{Front}}","afmt":"{{FrontSide}}<hr id=answer>{{Back}}"}]
      },
      "200": {
        "id": 200,
        "name": "Cloze",
        "type": 1,
        "flds": [{"name":"Text","ord":0},{"name":"Extra","ord":1}],
        "tmpls": [{"name":"Cloze","ord":0,"qfmt":"{{cloze:Text}}","afmt":"{{cloze:Text}}<br>{{Extra}}"}]
      }
    }"#
    .to_string()
}

fn sample_decks() -> String {
    r#"{
      "1": {"id":1, "name":"算法"},
      "2": {"id":2, "name":"地理::城市"}
    }"#
    .to_string()
}

fn write_fake_apkg(path: &str) {
    let tmp_db = format!("{path}.anki2");
    let conn = Connection::open(&tmp_db).unwrap();
    conn.execute_batch(SCHEMA).unwrap();

    let crt = 1_700_000_000i64; // 某天 0 点附近
    conn.execute(
        "INSERT INTO col (id, crt, mod, scm, ver, dty, usn, ls, conf, models, decks, dconf, tags)
         VALUES (1, ?1, ?2, ?2, 11, 0, 0, 0, '{}', ?3, ?4, '{}', '{}')",
        rusqlite::params![crt, 1_700_000_000, sample_models(), sample_decks()],
    )
    .unwrap();

    // Basic note: Front / Back
    conn.execute(
        "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data)
         VALUES (11, 'g1', 100, 1700000000, 0, ' leetcode 二分 ', '二分不变式是什么？' || char(31) || '区间 [l,r) 收敛', '二分', 0, 0, '')",
        [],
    )
    .unwrap();
    // Cloze note: c1 + c2 + Extra
    conn.execute(
        "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data)
         VALUES (12, 'g2', 200, 1700000000, 0, ' 地理 ',
           '{{c1::北京}}是{{c2::政治}}中心' || char(31) || '补充说明', '北京', 0, 0, '')",
        [],
    )
    .unwrap();

    // cards: basic new; cloze two cards (ord 0/1), one review with history
    // card 21: basic, new
    conn.execute(
        "INSERT INTO cards (id,nid,did,ord,mod,usn,type,queue,due,ivl,factor,reps,lapses,left,odue,odid,flags,data)
         VALUES (21, 11, 1, 0, 1700000000, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, '')",
        [],
    )
    .unwrap();
    // card 22: cloze c1 (ord 0), review, ivl=10, factor=2500, due day=20
    conn.execute(
        "INSERT INTO cards (id,nid,did,ord,mod,usn,type,queue,due,ivl,factor,reps,lapses,left,odue,odid,flags,data)
         VALUES (22, 12, 2, 0, 1700000100, 0, 2, 2, 20, 10, 2500, 5, 1, 0, 0, 0, 0, '')",
        [],
    )
    .unwrap();
    // card 23: cloze c2 (ord 1), learning, due in ms
    let learn_due = 1_700_100_000_000i64;
    conn.execute(
        "INSERT INTO cards (id,nid,did,ord,mod,usn,type,queue,due,ivl,factor,reps,lapses,left,odue,odid,flags,data)
         VALUES (23, 12, 2, 1, 1700000200, 0, 1, 1, ?1, 1, 2500, 2, 0, 65536, 0, 0, 0, '')",
        [learn_due],
    )
    .unwrap();

    // revlog for card 22: two reviews (id = ms timestamp)
    conn.execute(
        "INSERT INTO revlog (id,cid,usn,ease,ivl,lastIvl,factor,time,type)
         VALUES (1699900000000, 22, 0, 1, 0, 0, 0, 5000, 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO revlog (id,cid,usn,ease,ivl,lastIvl,factor,time,type)
         VALUES (1700000000000, 22, 0, 3, 10, 0, 2500, 3200, 1)",
        [],
    )
    .unwrap();

    drop(conn);

    // zip 成 apkg
    let file = std::fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("collection.anki2", opts).unwrap();
    let bytes = std::fs::read(&tmp_db).unwrap();
    zip.write_all(&bytes).unwrap();
    zip.finish().unwrap();
    let _ = std::fs::remove_file(&tmp_db);
}

#[test]
fn parse_synthetic_apkg() {
    let dir = std::env::temp_dir().join(format!("memstack-apkg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sample.apkg");
    write_fake_apkg(&path.to_string_lossy());

    let pack = parse_apkg(&path.to_string_lossy()).expect("parse apkg");
    assert_eq!(pack.notes.len(), 2);
    assert_eq!(pack.card_count(), 3);
    assert_eq!(pack.reviews.len(), 2);

    // Basic
    let basic = pack.notes.iter().find(|n| n.anki_id == 11).unwrap();
    assert_eq!(basic.deck_name, "算法");
    assert!(basic.tags.contains("leetcode"));
    assert!(basic.tags.contains("二分"));
    match &basic.content {
        NoteContent::QuestionAnswer { front, back } => {
            assert!(front.contains("二分不变式"));
            assert!(back.contains("收敛"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(basic.cards.len(), 1);
    assert_eq!(basic.cards[0].state, CardState::New);

    // Cloze：拆成 c1/c2 两张，保留调度
    let cloze = pack.notes.iter().find(|n| n.anki_id == 12).unwrap();
    assert_eq!(cloze.deck_name, "地理::城市");
    assert!(cloze.tags.contains("地理"));
    match &cloze.content {
        NoteContent::Cloze { text, extra } => {
            assert!(text.contains("{{c1::北京}}"));
            assert!(extra.contains("补充说明"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(cloze.cards.len(), 2);

    let c1 = cloze.cards.iter().find(|c| c.ordinal == 0).unwrap();
    assert_eq!(c1.state, CardState::Review);
    assert_eq!(c1.lapses, 1);
    assert!(c1.stability.unwrap() > 0.0);
    assert!(c1.front.contains('[') && !c1.front.contains("北京"));
    assert!(c1.back.contains("北京"));

    let c2 = cloze.cards.iter().find(|c| c.ordinal == 1).unwrap();
    assert_eq!(c2.state, CardState::Learning);

    // revlog
    let r0 = &pack.reviews[0];
    let r1 = &pack.reviews[1];
    assert_eq!(r0.anki_card_id, 22);
    assert_eq!(r0.rating, Rating::Again);
    assert_eq!(r0.duration_ms, 5000);
    assert_eq!(r1.rating, Rating::Good);
    assert_eq!(r1.interval_days, 10.0);
    assert_eq!(r1.elapsed_days, 1);

    let _ = std::fs::remove_dir_all(&dir);
}
