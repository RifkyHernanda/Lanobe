//! Reads and writes over `study.db`.
//!
//! Every write that changes what gets highlighted bumps `index_version` inside
//! the same transaction, so the ETag can never claim a state the data is not in.

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::kanji::extract_kanji;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Unknown,
    Learning,
    Known,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Unknown => "unknown",
            Status::Learning => "learning",
            Status::Known => "known",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(Status::Unknown),
            "learning" => Some(Status::Learning),
            "known" => Some(Status::Known),
            _ => None,
        }
    }
}

/// What the lookup popup sends when you save a word.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTerm {
    pub term: String,
    #[serde(default)]
    pub reading: String,
    #[serde(default)]
    pub surface: Option<String>,
    #[serde(default)]
    pub gloss: Vec<serde_json::Value>,
    #[serde(default)]
    pub frequency: Option<i64>,
    #[serde(default)]
    pub book_id: Option<String>,
    #[serde(default)]
    pub book_title: Option<String>,
    #[serde(default)]
    pub chapter_index: Option<i64>,
    #[serde(default)]
    pub sentence: String,
    #[serde(default)]
    pub sentence_offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTerm {
    pub id: i64,
    pub term: String,
    pub reading: String,
    pub surface: Option<String>,
    pub gloss: serde_json::Value,
    pub frequency: Option<i64>,
    pub book_id: Option<String>,
    pub book_title: Option<String>,
    pub chapter_index: Option<i64>,
    pub sentence: String,
    pub created_at: i64,
    pub status: Status,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

/// Bumps the highlight-index version. Call inside the write's transaction, and
/// only for writes that actually change highlighting — editing a sentence does
/// not, so it should not invalidate every client's cached index.
fn bump_index_version(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute(
        "UPDATE study_meta SET value = CAST(CAST(value AS INTEGER) + 1 AS TEXT)
         WHERE key = 'index_version'",
        [],
    )?;
    Ok(())
}

pub fn index_version(conn: &Connection) -> rusqlite::Result<i64> {
    let raw: String = conn.query_row(
        "SELECT value FROM study_meta WHERE key = 'index_version'",
        [],
        |row| row.get(0),
    )?;
    Ok(raw.parse().unwrap_or(0))
}

/// Saves a term and indexes its kanji. Returns `(id, created)`; `created` is
/// false when an existing `(term, reading)` row was refreshed instead.
pub fn save_term(conn: &mut Connection, new: &NewTerm) -> rusqlite::Result<(i64, bool)> {
    let tx = conn.transaction()?;
    let timestamp = now();
    let gloss = serde_json::to_string(&new.gloss).unwrap_or_else(|_| "[]".to_string());

    let existing: Option<i64> = tx
        .query_row(
            "SELECT id FROM saved_term WHERE term = ?1 AND reading = ?2",
            params![new.term, new.reading],
            |row| row.get(0),
        )
        .optional()?;

    let (id, created) = match existing {
        // Re-saving from a new sentence refreshes the context but keeps the id,
        // so term_kanji and any P4 export record stay pointed at the same row.
        Some(id) => {
            tx.execute(
                "UPDATE saved_term
                 SET surface = ?1, gloss_json = ?2, frequency = COALESCE(?3, frequency),
                     book_id = ?4, book_title = ?5, chapter_index = ?6,
                     sentence = ?7, sentence_offset = ?8, updated_at = ?9
                 WHERE id = ?10",
                params![
                    new.surface,
                    gloss,
                    new.frequency,
                    new.book_id,
                    new.book_title,
                    new.chapter_index,
                    new.sentence,
                    new.sentence_offset,
                    timestamp,
                    id
                ],
            )?;
            (id, false)
        }
        None => {
            tx.execute(
                "INSERT INTO saved_term
                   (term, reading, surface, gloss_json, frequency, book_id, book_title,
                    chapter_index, sentence, sentence_offset, created_at, updated_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
                params![
                    new.term,
                    new.reading,
                    new.surface,
                    gloss,
                    new.frequency,
                    new.book_id,
                    new.book_title,
                    new.chapter_index,
                    new.sentence,
                    new.sentence_offset,
                    timestamp
                ],
            )?;
            (tx.last_insert_rowid(), true)
        }
    };

    // Index on the headword, not the surface form: the surface is one inflection
    // and its kanji are a subset.
    for c in extract_kanji(&new.term) {
        let ch = c.to_string();
        // OR IGNORE keeps first_term_id pointing at whatever introduced it, and
        // preserves a status the user has already set on that character.
        tx.execute(
            "INSERT OR IGNORE INTO saved_kanji (char, first_term_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![ch, id, timestamp],
        )?;
        tx.execute(
            "INSERT OR IGNORE INTO term_kanji (term_id, char) VALUES (?1, ?2)",
            params![id, ch],
        )?;
    }

    bump_index_version(&tx)?;
    tx.commit()?;
    Ok((id, created))
}

pub fn set_term_status(conn: &mut Connection, id: i64, status: Status) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    let changed = tx.execute(
        "UPDATE saved_term SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status.as_str(), now(), id],
    )?;

    if changed > 0 {
        bump_index_version(&tx)?;
    }
    tx.commit()?;
    Ok(changed > 0)
}

pub fn set_kanji_status(conn: &mut Connection, ch: &str, status: Status) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    let changed = tx.execute(
        "UPDATE saved_kanji SET status = ?1 WHERE char = ?2",
        params![status.as_str(), ch],
    )?;

    if changed > 0 {
        bump_index_version(&tx)?;
    }
    tx.commit()?;
    Ok(changed > 0)
}

/// Deletes a term and any kanji it was the last term to reference.
pub fn delete_term(conn: &mut Connection, id: i64) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    // term_kanji rows go with it via ON DELETE CASCADE.
    let changed = tx.execute("DELETE FROM saved_term WHERE id = ?1", params![id])?;

    if changed > 0 {
        // A kanji shared with a surviving term must stay indexed; only ones left
        // with no referencing term are removed.
        tx.execute(
            "DELETE FROM saved_kanji
             WHERE char NOT IN (SELECT char FROM term_kanji)",
            [],
        )?;
        bump_index_version(&tx)?;
    }
    tx.commit()?;
    Ok(changed > 0)
}

/// The payload `/api/study/highlight-index` serves.
///
/// `known` rows are omitted entirely — that is how SPEC §4's "status = known
/// removes something from highlighting without deleting it" is implemented.
/// Both remaining buckets are sent so the client's only-unknown toggle is a
/// pure filter over one cached payload rather than a second request.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HighlightIndex {
    pub version: i64,
    /// Concatenated rather than an array: at 2500 kanji, JSON array punctuation
    /// costs more bytes than the characters themselves.
    pub kanji: HighlightBuckets<String>,
    pub terms: HighlightBuckets<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HighlightBuckets<T> {
    pub unknown: T,
    pub learning: T,
}

pub fn highlight_index(conn: &Connection) -> rusqlite::Result<HighlightIndex> {
    let mut kanji = HighlightBuckets {
        unknown: String::new(),
        learning: String::new(),
    };
    let mut stmt =
        conn.prepare("SELECT char, status FROM saved_kanji WHERE status != 'known' ORDER BY char")?;
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (ch, status) = row?;
        match Status::parse(&status) {
            Some(Status::Learning) => kanji.learning.push_str(&ch),
            _ => kanji.unknown.push_str(&ch),
        }
    }

    let mut terms = HighlightBuckets {
        unknown: Vec::new(),
        learning: Vec::new(),
    };
    let mut stmt =
        conn.prepare("SELECT term, status FROM saved_term WHERE status != 'known' ORDER BY term")?;
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })? {
        let (term, status) = row?;
        match Status::parse(&status) {
            Some(Status::Learning) => terms.learning.push(term),
            _ => terms.unknown.push(term),
        }
    }

    Ok(HighlightIndex {
        version: index_version(conn)?,
        kanji,
        terms,
    })
}
