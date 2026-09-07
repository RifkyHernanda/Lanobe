//! Study state: saved vocabulary, the kanji index, and the highlight-index cache.

use rusqlite::Connection;
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct StudyState {
    /// One connection behind a mutex, matching app-server. This database sees a
    /// write when you save a word and a read when a book opens; a pool would be
    /// ceremony for that.
    conn: Arc<Mutex<Connection>>,
}

impl StudyState {
    pub fn new(data_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;

        let conn = Connection::open(data_dir.join("study.db"))?;
        conn.execute_batch(SCHEMA)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Panics only if a previous holder panicked mid-write, which would mean the
    /// database is already suspect.
    pub fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("study.db mutex poisoned")
    }
}

/// `cache_size` is 2 MB, not the 64 MB SPEC §8 budgets for `yomitan.db`. This
/// database is a few hundred KB of personal vocabulary and shares a 1 GB box
/// with the dictionary, which is the one that actually needs page cache.
const SCHEMA: &str = "
PRAGMA journal_mode = WAL;
PRAGMA synchronous  = NORMAL;
PRAGMA foreign_keys = ON;
PRAGMA cache_size   = -2000;
PRAGMA temp_store   = MEMORY;

CREATE TABLE IF NOT EXISTS saved_term (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    term            TEXT    NOT NULL,
    reading         TEXT    NOT NULL DEFAULT '',
    -- The inflected form as it appeared in the book. P4 needs it for cloze
    -- sentences, and it is the obvious second trie key if dictionary-form-only
    -- highlighting proves too narrow.
    surface         TEXT,
    gloss_json      TEXT    NOT NULL DEFAULT '[]',
    -- Captured at save time. There is no frequency source afterwards, and
    -- SPEC §5.4 wants the Saved screen to sort by it.
    frequency       INTEGER,
    book_id         TEXT,
    -- Denormalised so the Saved screen's book filter does not have to join
    -- across to sled, which owns book metadata.
    book_title      TEXT,
    chapter_index   INTEGER,
    sentence        TEXT    NOT NULL DEFAULT '',
    sentence_offset INTEGER,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL,
    status          TEXT    NOT NULL DEFAULT 'unknown'
                    CHECK (status IN ('unknown','learning','known'))
);

-- Saving the same word twice updates rather than duplicating. Reading is part
-- of the key because 表 (おもて) and 表 (ひょう) are different words.
CREATE UNIQUE INDEX IF NOT EXISTS saved_term_key     ON saved_term(term, reading);
CREATE INDEX        IF NOT EXISTS saved_term_book    ON saved_term(book_id);
CREATE INDEX        IF NOT EXISTS saved_term_status  ON saved_term(status);
CREATE INDEX        IF NOT EXISTS saved_term_created ON saved_term(created_at DESC);

CREATE TABLE IF NOT EXISTS saved_kanji (
    char          TEXT PRIMARY KEY,
    -- The term that introduced this character. SET NULL rather than CASCADE:
    -- deleting that term must not silently drop the kanji from the index.
    first_term_id INTEGER REFERENCES saved_term(id) ON DELETE SET NULL,
    created_at    INTEGER NOT NULL,
    status        TEXT NOT NULL DEFAULT 'unknown'
                  CHECK (status IN ('unknown','learning','known'))
);
CREATE INDEX IF NOT EXISTS saved_kanji_status ON saved_kanji(status);

CREATE TABLE IF NOT EXISTS term_kanji (
    term_id INTEGER NOT NULL REFERENCES saved_term(id)    ON DELETE CASCADE,
    char    TEXT    NOT NULL REFERENCES saved_kanji(char) ON DELETE CASCADE,
    PRIMARY KEY (term_id, char)
);
-- Drives 'which saved words contain this kanji', which is both the kanji
-- frequency sort and the Saved screen's drill-down.
CREATE INDEX IF NOT EXISTS term_kanji_char ON term_kanji(char);

-- P4 writes here so a second export of the same book skips already-exported
-- terms. Shape kept as SPEC §4 specifies it.
CREATE TABLE IF NOT EXISTS export_log (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    book_id       TEXT    NOT NULL,
    exported_at   INTEGER NOT NULL,
    term_ids_json TEXT    NOT NULL
);
CREATE INDEX IF NOT EXISTS export_log_book ON export_log(book_id);

-- Free-text selections from the pre-P3 highlight system, copied here so they
-- survive and can be reviewed across books. They are NOT saved_term rows: they
-- are whole phrases with no reading or glossary, and putting a 60-character
-- sentence into the term trie would blow the matcher's O(n·k) bound.
--
-- Their offsets are furigana-INCLUSIVE (produced by Range.toString().length),
-- unlike saved_term, so they must not be fed to the study matcher.
CREATE TABLE IF NOT EXISTS legacy_highlight (
    id            TEXT PRIMARY KEY,
    book_id       TEXT    NOT NULL,
    book_title    TEXT,
    chapter_index INTEGER NOT NULL,
    block_id      TEXT    NOT NULL,
    text          TEXT    NOT NULL,
    start_offset  INTEGER NOT NULL,
    end_offset    INTEGER NOT NULL,
    created_at    INTEGER NOT NULL,
    -- Set when the user promotes one into real vocabulary.
    promoted_term_id INTEGER REFERENCES saved_term(id) ON DELETE SET NULL
);
CREATE INDEX IF NOT EXISTS legacy_highlight_book ON legacy_highlight(book_id);

CREATE TABLE IF NOT EXISTS study_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT OR IGNORE INTO study_meta (key, value) VALUES ('index_version', '0');
";
