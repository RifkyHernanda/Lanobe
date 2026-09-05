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
            // Every field COALESCEs over its stored value, so a re-save only
            // updates what it actually carries. Assigning directly would let
            // saving the same word from the manual dictionary page -- which has
            // no book, chapter or sentence -- erase the context captured when it
            // was first saved while reading, and that context is the whole point
            // of the record.
            tx.execute(
                "UPDATE saved_term
                 SET surface       = COALESCE(?1, surface),
                     gloss_json    = CASE WHEN ?2 IN ('[]', '') THEN gloss_json ELSE ?2 END,
                     frequency     = COALESCE(?3, frequency),
                     book_id       = COALESCE(?4, book_id),
                     book_title    = COALESCE(?5, book_title),
                     chapter_index = COALESCE(?6, chapter_index),
                     sentence      = COALESCE(NULLIF(?7, ''), sentence),
                     sentence_offset = COALESCE(?8, sentence_offset),
                     updated_at    = ?9
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

/// Removes kanji no term references any more. Call inside the deleting
/// transaction; a kanji shared with a surviving term must stay indexed.
fn prune_orphaned_kanji(tx: &Transaction<'_>) -> rusqlite::Result<()> {
    tx.execute(
        "DELETE FROM saved_kanji WHERE char NOT IN (SELECT char FROM term_kanji)",
        [],
    )?;
    Ok(())
}

/// Bulk status change. One transaction and **one** version bump for the whole
/// batch — bumping per row would invalidate every client's cached highlight
/// index N times for a single user action.
pub fn bulk_set_term_status(
    conn: &mut Connection,
    ids: &[i64],
    status: Status,
) -> rusqlite::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    let timestamp = now();
    let mut affected = 0;
    {
        let mut stmt =
            tx.prepare("UPDATE saved_term SET status = ?1, updated_at = ?2 WHERE id = ?3")?;
        for id in ids {
            affected += stmt.execute(params![status.as_str(), timestamp, id])?;
        }
    }

    if affected > 0 {
        bump_index_version(&tx)?;
    }
    tx.commit()?;
    Ok(affected)
}

pub fn bulk_delete_terms(conn: &mut Connection, ids: &[i64]) -> rusqlite::Result<usize> {
    if ids.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    let mut affected = 0;
    {
        let mut stmt = tx.prepare("DELETE FROM saved_term WHERE id = ?1")?;
        for id in ids {
            affected += stmt.execute(params![id])?;
        }
    }

    if affected > 0 {
        prune_orphaned_kanji(&tx)?;
        bump_index_version(&tx)?;
    }
    tx.commit()?;
    Ok(affected)
}

pub fn bulk_set_kanji_status(
    conn: &mut Connection,
    chars: &[String],
    status: Status,
) -> rusqlite::Result<usize> {
    if chars.is_empty() {
        return Ok(0);
    }

    let tx = conn.transaction()?;
    let mut affected = 0;
    {
        let mut stmt = tx.prepare("UPDATE saved_kanji SET status = ?1 WHERE char = ?2")?;
        for ch in chars {
            affected += stmt.execute(params![status.as_str(), ch])?;
        }
    }

    if affected > 0 {
        bump_index_version(&tx)?;
    }
    tx.commit()?;
    Ok(affected)
}

/// Deletes a term and any kanji it was the last term to reference.
pub fn delete_term(conn: &mut Connection, id: i64) -> rusqlite::Result<bool> {
    let tx = conn.transaction()?;
    // term_kanji rows go with it via ON DELETE CASCADE.
    let changed = tx.execute("DELETE FROM saved_term WHERE id = ?1", params![id])?;

    if changed > 0 {
        prune_orphaned_kanji(&tx)?;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedKanji {
    pub char: String,
    /// How many saved words contain this character. SPEC §5.4's "sort by
    /// frequency" for the Kanji tab means this, not corpus frequency.
    pub term_count: i64,
    pub created_at: i64,
    pub status: Status,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TermSort {
    Created,
    Frequency,
    Term,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KanjiSort {
    Created,
    Count,
    Char,
}

#[derive(Debug, Clone)]
pub struct ListQuery {
    pub q: Option<String>,
    pub book: Option<String>,
    pub status: Option<Status>,
    pub descending: bool,
    pub limit: i64,
    pub offset: i64,
}

impl Default for ListQuery {
    fn default() -> Self {
        Self {
            q: None,
            book: None,
            status: None,
            descending: true,
            limit: 100,
            offset: 0,
        }
    }
}

/// Sort columns are chosen from a fixed set here and never interpolated from
/// caller input -- `ORDER BY` cannot be a bound parameter, so the only safe way
/// to make it dynamic is to map an enum onto literals.
fn term_order(sort: TermSort, descending: bool) -> &'static str {
    match (sort, descending) {
        (TermSort::Created, true) => "created_at DESC, id DESC",
        (TermSort::Created, false) => "created_at ASC, id ASC",
        // NULLs last either way: an unranked word should not lead the list.
        (TermSort::Frequency, true) => "frequency IS NULL, frequency DESC, id DESC",
        (TermSort::Frequency, false) => "frequency IS NULL, frequency ASC, id ASC",
        (TermSort::Term, true) => "term DESC",
        (TermSort::Term, false) => "term ASC",
    }
}

fn kanji_order(sort: KanjiSort, descending: bool) -> &'static str {
    match (sort, descending) {
        (KanjiSort::Created, true) => "created_at DESC, char DESC",
        (KanjiSort::Created, false) => "created_at ASC, char ASC",
        (KanjiSort::Count, true) => "term_count DESC, char ASC",
        (KanjiSort::Count, false) => "term_count ASC, char ASC",
        (KanjiSort::Char, true) => "char DESC",
        (KanjiSort::Char, false) => "char ASC",
    }
}

fn row_to_term(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedTerm> {
    let gloss_json: String = row.get("gloss_json")?;
    let status: String = row.get("status")?;
    Ok(SavedTerm {
        id: row.get("id")?,
        term: row.get("term")?,
        reading: row.get("reading")?,
        surface: row.get("surface")?,
        gloss: serde_json::from_str(&gloss_json).unwrap_or(serde_json::Value::Null),
        frequency: row.get("frequency")?,
        book_id: row.get("book_id")?,
        book_title: row.get("book_title")?,
        chapter_index: row.get("chapter_index")?,
        sentence: row.get("sentence")?,
        created_at: row.get("created_at")?,
        status: Status::parse(&status).unwrap_or(Status::Unknown),
    })
}

pub fn list_terms(
    conn: &Connection,
    query: &ListQuery,
    sort: TermSort,
) -> rusqlite::Result<(Vec<SavedTerm>, i64)> {
    // `?1 IS NULL OR ...` keeps one prepared statement rather than assembling
    // SQL per filter combination.
    let filter = "WHERE (?1 IS NULL OR term LIKE '%' || ?1 || '%' OR reading LIKE '%' || ?1 || '%')
                    AND (?2 IS NULL OR book_id = ?2)
                    AND (?3 IS NULL OR status = ?3)";
    let status = query.status.map(Status::as_str);

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM saved_term {filter}"),
        params![query.q, query.book, status],
        |row| row.get(0),
    )?;

    let sql = format!(
        "SELECT * FROM saved_term {filter} ORDER BY {} LIMIT ?4 OFFSET ?5",
        term_order(sort, query.descending)
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(
            params![query.q, query.book, status, query.limit, query.offset],
            row_to_term,
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok((items, total))
}

pub fn list_kanji(
    conn: &Connection,
    query: &ListQuery,
    sort: KanjiSort,
) -> rusqlite::Result<(Vec<SavedKanji>, i64)> {
    let filter = "WHERE (?1 IS NULL OR k.char = ?1)
                    AND (?2 IS NULL OR k.status = ?2)";
    let status = query.status.map(Status::as_str);

    let total: i64 = conn.query_row(
        &format!("SELECT COUNT(*) FROM saved_kanji k {filter}"),
        params![query.q, status],
        |row| row.get(0),
    )?;

    let sql = format!(
        "SELECT k.char, k.created_at, k.status,
                (SELECT COUNT(*) FROM term_kanji tk WHERE tk.char = k.char) AS term_count
         FROM saved_kanji k {filter} ORDER BY {} LIMIT ?3 OFFSET ?4",
        kanji_order(sort, query.descending)
    );
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(params![query.q, status, query.limit, query.offset], |row| {
            let status: String = row.get("status")?;
            Ok(SavedKanji {
                char: row.get("char")?,
                term_count: row.get("term_count")?,
                created_at: row.get("created_at")?,
                status: Status::parse(&status).unwrap_or(Status::Unknown),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok((items, total))
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub unknown: i64,
    pub learning: i64,
    pub known: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BookCount {
    pub id: String,
    pub title: Option<String>,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub terms: StatusCounts,
    pub kanji: StatusCounts,
    pub books: Vec<BookCount>,
}

fn status_counts(conn: &Connection, table: &str) -> rusqlite::Result<StatusCounts> {
    let mut counts = StatusCounts::default();
    let mut stmt = conn.prepare(&format!(
        "SELECT status, COUNT(*) FROM {table} GROUP BY status"
    ))?;
    for row in stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (status, count) = row?;
        match Status::parse(&status) {
            Some(Status::Unknown) => counts.unknown = count,
            Some(Status::Learning) => counts.learning = count,
            Some(Status::Known) => counts.known = count,
            None => {}
        }
    }
    Ok(counts)
}

pub fn stats(conn: &Connection) -> rusqlite::Result<Stats> {
    let mut stmt = conn.prepare(
        "SELECT book_id, MAX(book_title), COUNT(*) FROM saved_term
         WHERE book_id IS NOT NULL GROUP BY book_id ORDER BY COUNT(*) DESC",
    )?;
    let books = stmt
        .query_map([], |row| {
            Ok(BookCount {
                id: row.get(0)?,
                title: row.get(1)?,
                count: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(Stats {
        terms: status_counts(conn, "saved_term")?,
        kanji: status_counts(conn, "saved_kanji")?,
        books,
    })
}
