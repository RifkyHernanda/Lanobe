//! Writes an Anki `.apkg` by hand.
//!
//! `genanki-rs` would be the obvious choice, but its latest release pins
//! `libsqlite3-sys 0.22` while this workspace is on 0.28 via `rusqlite 0.31`,
//! and only one crate in a build may link `sqlite3`. So the format is built
//! directly with the `rusqlite` already in the tree.
//!
//! An `.apkg` is a zip holding:
//!   - `collection.anki2` — a SQLite database in Anki's **schema 11**
//!   - `media` — a JSON map of zip entry name to original filename, `{}` when
//!     there is no media
//!
//! Schema 11 rather than anything newer on purpose: it is the format every Anki
//! version since 2.1 can import, and newer schemas gain nothing for a deck this
//! plain.

use rusqlite::{Connection, params};
use serde_json::json;
use sha1::{Digest, Sha1};
use std::{
    io::{Cursor, Write},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// Unique scratch path per call. `now_ms` is injected for reproducible note ids,
/// so it is deliberately *not* unique -- two exports with the same timestamp
/// must not collide on the temp file, and `VACUUM INTO` refuses to overwrite.
fn scratch_path(label: &str) -> std::path::PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("lanobe-{label}-{nanos}-{seq}.anki2"))
}

/// One row of the exported deck.
#[derive(Debug, Clone)]
pub struct ExportNote {
    pub expression: String,
    pub reading: String,
    pub glossary: String,
    pub sentence: String,
    pub source_book: String,
    /// Left empty for now: word audio resolves to external URLs, so embedding
    /// it would mean the server downloading one file per word at export time.
    pub audio: String,
}

/// Anki separates a note's fields with 0x1f, so a field may not contain one.
const FIELD_SEPARATOR: char = '\u{1f}';

/// Anki's duplicate check reads the first 8 hex digits of the sort field's sha1
/// as an integer. A wrong value does not corrupt anything, it just stops Anki
/// noticing duplicates -- so it is worth getting right rather than stubbing.
fn field_checksum(text: &str) -> i64 {
    let digest = Sha1::digest(text.as_bytes());
    let hex = format!("{digest:x}");
    i64::from_str_radix(&hex[..8], 16).unwrap_or(0)
}

fn strip_separator(value: &str) -> String {
    value.replace(FIELD_SEPARATOR, " ")
}

/// The note type. Field order here is the order Anki shows on import, and it
/// matches SPEC §5.5.
fn model_json(model_id: i64, deck_id: i64) -> serde_json::Value {
    let field = |ord: usize, name: &str| {
        json!({
            "name": name, "ord": ord, "sticky": false, "rtl": false,
            "font": "Arial", "size": 20, "media": [], "description": ""
        })
    };

    json!({
        model_id.to_string(): {
            "id": model_id,
            "name": "Lanobe Vocabulary",
            "type": 0,
            "mod": 0,
            "usn": -1,
            "sortf": 0,
            "did": deck_id,
            "tmpls": [{
                "name": "Recognition",
                "ord": 0,
                "qfmt": "{{Expression}}",
                "afmt": "{{FrontSide}}\n\n<hr id=answer>\n\n\
                         <div class=reading>{{Reading}}</div>\n\
                         <div class=glossary>{{Glossary}}</div>\n\
                         <div class=sentence>{{Sentence}}</div>\n\
                         <div class=source>{{SourceBook}}</div>\n\
                         {{Audio}}",
                "did": null, "bqfmt": "", "bafmt": "", "bfont": "", "bsize": 0
            }],
            "flds": [
                field(0, "Expression"), field(1, "Reading"), field(2, "Glossary"),
                field(3, "Sentence"), field(4, "SourceBook"), field(5, "Audio")
            ],
            "css": ".card { font-family: serif; font-size: 28px; text-align: center; }\n\
                    .reading { font-size: 20px; color: #888; }\n\
                    .glossary { font-size: 18px; margin-top: 12px; }\n\
                    .sentence { font-size: 18px; margin-top: 12px; }\n\
                    .source { font-size: 14px; color: #888; margin-top: 12px; }",
            "latexPre": "", "latexPost": "", "latexsvg": false,
            "req": [[0, "any", [0]]],
            "tags": [], "vers": []
        }
    })
}

fn deck_json(deck_id: i64, name: &str) -> serde_json::Value {
    json!({
        deck_id.to_string(): {
            "id": deck_id,
            "name": name,
            "mod": 0, "usn": -1,
            "desc": "Exported from Lanobe",
            "dyn": 0, "collapsed": false, "browserCollapsed": false,
            "extendNew": 10, "extendRev": 50,
            "conf": 1,
            "newToday": [0, 0], "revToday": [0, 0], "lrnToday": [0, 0], "timeToday": [0, 0]
        }
    })
}

/// Schema 11. Column lists are exact — Anki reads these by position in places.
const SCHEMA: &str = "
CREATE TABLE col (
    id integer PRIMARY KEY, crt integer NOT NULL, mod integer NOT NULL,
    scm integer NOT NULL, ver integer NOT NULL, dty integer NOT NULL,
    usn integer NOT NULL, ls integer NOT NULL, conf text NOT NULL,
    models text NOT NULL, decks text NOT NULL, dconf text NOT NULL, tags text NOT NULL
);
CREATE TABLE notes (
    id integer PRIMARY KEY, guid text NOT NULL, mid integer NOT NULL,
    mod integer NOT NULL, usn integer NOT NULL, tags text NOT NULL,
    flds text NOT NULL, sfld integer NOT NULL, csum integer NOT NULL,
    flags integer NOT NULL, data text NOT NULL
);
CREATE TABLE cards (
    id integer PRIMARY KEY, nid integer NOT NULL, did integer NOT NULL,
    ord integer NOT NULL, mod integer NOT NULL, usn integer NOT NULL,
    type integer NOT NULL, queue integer NOT NULL, due integer NOT NULL,
    ivl integer NOT NULL, factor integer NOT NULL, reps integer NOT NULL,
    lapses integer NOT NULL, left integer NOT NULL, odue integer NOT NULL,
    odid integer NOT NULL, flags integer NOT NULL, data text NOT NULL
);
CREATE TABLE revlog (
    id integer PRIMARY KEY, cid integer NOT NULL, usn integer NOT NULL,
    ease integer NOT NULL, ivl integer NOT NULL, lastIvl integer NOT NULL,
    factor integer NOT NULL, time integer NOT NULL, type integer NOT NULL
);
CREATE TABLE graves (usn integer NOT NULL, oid integer NOT NULL, type integer NOT NULL);
CREATE INDEX ix_notes_usn ON notes (usn);
CREATE INDEX ix_cards_usn ON cards (usn);
CREATE INDEX ix_revlog_usn ON revlog (usn);
CREATE INDEX ix_cards_nid ON cards (nid);
CREATE INDEX ix_cards_sched ON cards (did, queue, due);
CREATE INDEX ix_revlog_cid ON revlog (cid);
CREATE INDEX ix_notes_csum ON notes (csum);
";

/// Builds the `.apkg` bytes for `notes` under `Lanobe::{book}`.
///
/// `now_ms` is injected rather than read from the clock so the output is
/// reproducible and the tests can assert exact ids.
pub fn build_apkg(deck_name: &str, notes: &[ExportNote], now_ms: i64) -> anyhow::Result<Vec<u8>> {
    // Anki keys models and decks by id, and treats a collision as the same
    // object. Deriving them from the export time keeps repeated exports of the
    // same book merging into one deck rather than piling up duplicates.
    let deck_id = now_ms;
    let model_id = now_ms + 1;

    let conn = Connection::open_in_memory()?;
    conn.execute_batch(SCHEMA)?;

    conn.execute(
        "INSERT INTO col (id, crt, mod, scm, ver, dty, usn, ls, conf, models, decks, dconf, tags)
         VALUES (1, ?1, ?2, ?2, 11, 0, 0, 0, ?3, ?4, ?5, ?6, '{}')",
        params![
            now_ms / 1000,
            now_ms,
            json!({
                "nextPos": 1, "estTimes": true, "activeDecks": [1], "sortType": "noteFld",
                "timeLim": 0, "sortBackwards": false, "addToCur": true, "curDeck": 1,
                "newBury": true, "newSpread": 0, "dueCounts": true, "curModel": model_id,
                "collapseTime": 1200
            })
            .to_string(),
            model_json(model_id, deck_id).to_string(),
            deck_json(deck_id, deck_name).to_string(),
            json!({
                "1": {
                    "id": 1, "name": "Default", "mod": 0, "usn": 0, "maxTaken": 60,
                    "autoplay": true, "timer": 0, "replayq": true,
                    "new": { "bury": true, "delays": [1.0, 10.0], "initialFactor": 2500,
                             "ints": [1, 4, 7], "order": 1, "perDay": 20, "separate": true },
                    "rev": { "bury": true, "ease4": 1.3, "fuzz": 0.05, "ivlFct": 1.0,
                             "maxIvl": 36500, "minSpace": 1, "perDay": 200 },
                    "lapse": { "delays": [10.0], "leechAction": 0, "leechFails": 8,
                               "minInt": 1, "mult": 0.0 }
                }
            })
            .to_string(),
        ],
    )?;

    for (position, note) in notes.iter().enumerate() {
        // Ids must be unique and are conventionally epoch-ms; spacing by
        // position keeps them unique without needing the clock.
        let note_id = now_ms + 1000 + position as i64;
        let card_id = now_ms + 500_000 + position as i64;

        let fields = [
            &note.expression,
            &note.reading,
            &note.glossary,
            &note.sentence,
            &note.source_book,
            &note.audio,
        ]
        .iter()
        .map(|f| strip_separator(f))
        .collect::<Vec<_>>()
        .join(&FIELD_SEPARATOR.to_string());

        let sort_field = strip_separator(&note.expression);

        conn.execute(
            "INSERT INTO notes (id, guid, mid, mod, usn, tags, flds, sfld, csum, flags, data)
             VALUES (?1, ?2, ?3, ?4, -1, '', ?5, ?6, ?7, 0, '')",
            params![
                note_id,
                // Anki dedupes across imports on guid, so it must be stable for
                // the same word: derived from the expression and reading rather
                // than random, or re-exporting a book would duplicate every card.
                format!(
                    "lanobe-{:x}",
                    field_checksum(&format!("{}\u{1f}{}", note.expression, note.reading))
                ),
                model_id,
                now_ms / 1000,
                fields,
                sort_field,
                field_checksum(&sort_field),
            ],
        )?;

        conn.execute(
            "INSERT INTO cards (id, nid, did, ord, mod, usn, type, queue, due, ivl,
                                factor, reps, lapses, left, odue, odid, flags, data)
             VALUES (?1, ?2, ?3, 0, ?4, -1, 0, 0, ?5, 0, 0, 0, 0, 0, 0, 0, 0, '')",
            params![
                card_id,
                note_id,
                deck_id,
                now_ms / 1000,
                position as i64 + 1
            ],
        )?;
    }

    // rusqlite cannot hand back the bytes of an in-memory database, so the
    // collection is serialised to a temp file and read back.
    let temp = scratch_path("apkg");
    conn.execute("VACUUM INTO ?1", params![temp.to_string_lossy()])?;
    drop(conn);
    let collection = std::fs::read(&temp)?;
    let _ = std::fs::remove_file(&temp);

    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

        zip.start_file("collection.anki2", options)?;
        zip.write_all(&collection)?;

        // Required even when empty: Anki rejects an archive without it.
        zip.start_file("media", options)?;
        zip.write_all(b"{}")?;

        zip.finish()?;
    }

    Ok(buffer.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn note(expression: &str, reading: &str) -> ExportNote {
        ExportNote {
            expression: expression.to_string(),
            reading: reading.to_string(),
            glossary: "to study".to_string(),
            sentence: format!("これは{expression}です。"),
            source_book: "義妹生活５".to_string(),
            audio: String::new(),
        }
    }

    fn open_collection(apkg: &[u8]) -> (Connection, Vec<String>) {
        let mut archive = zip::ZipArchive::new(Cursor::new(apkg.to_vec())).expect("valid zip");
        let names: Vec<String> = archive.file_names().map(str::to_string).collect();

        let mut bytes = Vec::new();
        archive
            .by_name("collection.anki2")
            .expect("collection present")
            .read_to_end(&mut bytes)
            .expect("readable");

        let path = scratch_path("apkg-test");
        std::fs::write(&path, &bytes).expect("write temp");
        let conn = Connection::open(&path).expect("open collection");
        (conn, names)
    }

    #[test]
    fn the_archive_holds_a_collection_and_a_media_manifest() {
        let apkg = build_apkg(
            "Lanobe::Test",
            &[note("勉強", "べんきょう")],
            1_700_000_000_000,
        )
        .expect("build");
        let (_conn, names) = open_collection(&apkg);

        assert!(names.contains(&"collection.anki2".to_string()));
        // Anki rejects an archive with no media manifest, even an empty one.
        assert!(names.contains(&"media".to_string()));
    }

    #[test]
    fn the_collection_declares_schema_11() {
        let apkg = build_apkg(
            "Lanobe::Test",
            &[note("勉強", "べんきょう")],
            1_700_000_000_000,
        )
        .expect("build");
        let (conn, _) = open_collection(&apkg);

        let ver: i64 = conn
            .query_row("SELECT ver FROM col", [], |r| r.get(0))
            .expect("col row");
        assert_eq!(ver, 11, "schema 11 is what every Anki since 2.1 imports");
    }

    #[test]
    fn every_note_gets_exactly_one_card() {
        let notes = [
            note("勉強", "べんきょう"),
            note("図書館", "としょかん"),
            note("電車", "でんしゃ"),
        ];
        let apkg = build_apkg("Lanobe::Test", &notes, 1_700_000_000_000).expect("build");
        let (conn, _) = open_collection(&apkg);

        let note_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        let card_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM cards", [], |r| r.get(0))
            .unwrap();
        let orphans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cards WHERE nid NOT IN (SELECT id FROM notes)",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(note_count, 3);
        assert_eq!(card_count, 3);
        assert_eq!(orphans, 0, "a card pointing at no note would not import");
    }

    #[test]
    fn fields_are_separated_by_0x1f_in_declared_order() {
        let apkg = build_apkg(
            "Lanobe::Test",
            &[note("勉強", "べんきょう")],
            1_700_000_000_000,
        )
        .expect("build");
        let (conn, _) = open_collection(&apkg);

        let flds: String = conn
            .query_row("SELECT flds FROM notes", [], |r| r.get(0))
            .unwrap();
        let parts: Vec<&str> = flds.split('\u{1f}').collect();

        assert_eq!(parts.len(), 6, "Expression..Audio");
        assert_eq!(parts[0], "勉強");
        assert_eq!(parts[1], "べんきょう");
        assert_eq!(parts[2], "to study");
        assert_eq!(parts[4], "義妹生活５");
        assert_eq!(parts[5], "", "audio is deliberately empty for now");
    }

    #[test]
    fn a_separator_inside_a_field_cannot_corrupt_the_row() {
        // 0x1f in a glossary would otherwise silently create a seventh field
        // and shift everything after it.
        let mut n = note("勉強", "べんきょう");
        n.glossary = "one\u{1f}two".to_string();

        let apkg = build_apkg("Lanobe::Test", &[n], 1_700_000_000_000).expect("build");
        let (conn, _) = open_collection(&apkg);

        let flds: String = conn
            .query_row("SELECT flds FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(flds.split('\u{1f}').count(), 6);
        assert!(flds.contains("one two"));
    }

    #[test]
    fn the_sort_field_and_checksum_match_the_expression() {
        let apkg = build_apkg(
            "Lanobe::Test",
            &[note("勉強", "べんきょう")],
            1_700_000_000_000,
        )
        .expect("build");
        let (conn, _) = open_collection(&apkg);

        let (sfld, csum): (String, i64) = conn
            .query_row("SELECT sfld, csum FROM notes", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();

        assert_eq!(sfld, "勉強");
        assert_eq!(csum, field_checksum("勉強"), "Anki dedupes on this");
    }

    #[test]
    fn the_deck_carries_the_requested_name() {
        let apkg = build_apkg(
            "Lanobe::義妹生活５",
            &[note("勉強", "べんきょう")],
            1_700_000_000_000,
        )
        .expect("build");
        let (conn, _) = open_collection(&apkg);

        let decks: String = conn
            .query_row("SELECT decks FROM col", [], |r| r.get(0))
            .unwrap();
        assert!(decks.contains("Lanobe::義妹生活５"), "got {decks}");
    }

    #[test]
    fn the_guid_is_stable_for_the_same_word_across_exports() {
        // Anki dedupes across imports on guid. A random one would duplicate
        // every card on a second export of the same book.
        let first = build_apkg(
            "Lanobe::A",
            &[note("勉強", "べんきょう")],
            1_700_000_000_000,
        )
        .unwrap();
        let second = build_apkg(
            "Lanobe::A",
            &[note("勉強", "べんきょう")],
            1_900_000_000_000,
        )
        .unwrap();

        let guid_of = |apkg: &[u8]| -> String {
            let (conn, _) = open_collection(apkg);
            conn.query_row("SELECT guid FROM notes", [], |r| r.get(0))
                .unwrap()
        };

        assert_eq!(guid_of(&first), guid_of(&second));
    }

    #[test]
    fn an_empty_deck_still_produces_a_valid_archive() {
        let apkg = build_apkg("Lanobe::Empty", &[], 1_700_000_000_000).expect("build");
        let (conn, names) = open_collection(&apkg);

        assert!(names.contains(&"collection.anki2".to_string()));
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM notes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
