//! Saved vocabulary and kanji bookmarks — SPEC §5.4.
//!
//! Owns `study.db` and serves `/api/study/*`.

use axum::Router;
use std::path::Path;

mod apkg;
mod kanji;
mod routes;
mod state;
mod store;

pub use apkg::{ExportNote, build_apkg};
pub use kanji::extract_kanji;
pub use state::StudyState;
pub use store::{
    HighlightBuckets, HighlightIndex, LegacyHighlight, LegacyHighlightInput, NewTerm, SavedTerm,
    Status, bulk_delete_terms, bulk_set_kanji_status, bulk_set_term_status, delete_term,
    highlight_index, import_legacy_highlights, index_version, list_legacy_highlights,
    mark_legacy_promoted, save_term, set_kanji_status, set_term_status,
};

pub fn build_state(data_dir: &Path) -> anyhow::Result<StudyState> {
    StudyState::new(data_dir)
}

pub fn create_router(state: StudyState) -> Router {
    routes::router(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{
        LegacyHighlightInput, bulk_delete_terms, bulk_set_kanji_status, bulk_set_term_status,
        delete_term, highlight_index, import_legacy_highlights, index_version,
        list_legacy_highlights, mark_legacy_promoted, save_term, set_kanji_status, set_term_status,
    };
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Matches app-server's helper: nanosecond-stamped directories rather than a
    /// `tempfile` dependency the workspace does not carry.
    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("lanobe-study-server-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn term(name: &str, reading: &str) -> NewTerm {
        NewTerm {
            term: name.to_string(),
            reading: reading.to_string(),
            sentence: format!("これは{name}です。"),
            ..Default::default()
        }
    }

    fn with_state<T>(label: &str, body: impl FnOnce(&StudyState) -> T) -> T {
        let dir = temp_dir(label);
        let state = StudyState::new(&dir).expect("build state");
        let out = body(&state);
        drop(state);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    #[test]
    fn saving_a_term_indexes_every_kanji() {
        with_state("index-kanji", |state| {
            let mut conn = state.conn();
            let (id, created) = save_term(&mut conn, &term("図書館", "としょかん")).unwrap();
            assert!(created);

            let kanji_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM saved_kanji", [], |r| r.get(0))
                .unwrap();
            let link_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM term_kanji WHERE term_id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap();

            assert_eq!(kanji_count, 3);
            assert_eq!(link_count, 3);
        });
    }

    #[test]
    fn a_kana_only_term_indexes_no_kanji() {
        with_state("kana-only", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("ある", "ある")).unwrap();

            let kanji_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM saved_kanji", [], |r| r.get(0))
                .unwrap();
            assert_eq!(kanji_count, 0);
        });
    }

    #[test]
    fn saving_the_same_term_twice_updates_without_duplicating() {
        with_state("upsert", |state| {
            let mut conn = state.conn();
            let (first, created_first) = save_term(&mut conn, &term("勉強", "べんきょう")).unwrap();
            assert!(created_first);

            let mut again = term("勉強", "べんきょう");
            again.sentence = "毎日勉強します。".to_string();
            let (second, created_second) = save_term(&mut conn, &again).unwrap();

            assert_eq!(first, second, "the id must be stable across a re-save");
            assert!(!created_second);

            let terms: i64 = conn
                .query_row("SELECT COUNT(*) FROM saved_term", [], |r| r.get(0))
                .unwrap();
            let links: i64 = conn
                .query_row("SELECT COUNT(*) FROM term_kanji", [], |r| r.get(0))
                .unwrap();
            let sentence: String = conn
                .query_row(
                    "SELECT sentence FROM saved_term WHERE id = ?1",
                    [first],
                    |r| r.get(0),
                )
                .unwrap();

            assert_eq!(terms, 1);
            assert_eq!(links, 2);
            assert_eq!(sentence, "毎日勉強します。", "context should refresh");
        });
    }

    #[test]
    fn re_saving_without_context_does_not_erase_the_context_already_stored() {
        with_state("preserve-context", |state| {
            let mut conn = state.conn();

            // Saved while reading: full context.
            let mut first = term("図書館", "としょかん");
            first.book_id = Some("b1".into());
            first.book_title = Some("義妹生活５".into());
            first.chapter_index = Some(3);
            first.frequency = Some(900);
            first.gloss = vec![serde_json::json!("library")];
            let (id, _) = save_term(&mut conn, &first).unwrap();

            // Saved again from the manual dictionary page, which knows no book,
            // no chapter and no sentence.
            let bare = NewTerm {
                term: "図書館".into(),
                reading: "としょかん".into(),
                ..Default::default()
            };
            save_term(&mut conn, &bare).unwrap();

            let (book, title, chapter, freq, gloss, sentence): (
                Option<String>,
                Option<String>,
                Option<i64>,
                Option<i64>,
                String,
                String,
            ) = conn
                .query_row(
                    "SELECT book_id, book_title, chapter_index, frequency, gloss_json, sentence
                     FROM saved_term WHERE id = ?1",
                    [id],
                    |r| {
                        Ok((
                            r.get(0)?,
                            r.get(1)?,
                            r.get(2)?,
                            r.get(3)?,
                            r.get(4)?,
                            r.get(5)?,
                        ))
                    },
                )
                .unwrap();

            assert_eq!(book.as_deref(), Some("b1"));
            assert_eq!(title.as_deref(), Some("義妹生活５"));
            assert_eq!(chapter, Some(3));
            assert_eq!(freq, Some(900));
            assert_eq!(gloss, "[\"library\"]");
            assert_eq!(sentence, "これは図書館です。");
        });
    }

    #[test]
    fn re_saving_with_new_context_does_update_it() {
        with_state("update-context", |state| {
            let mut conn = state.conn();
            let mut first = term("電車", "でんしゃ");
            first.book_id = Some("b1".into());
            let (id, _) = save_term(&mut conn, &first).unwrap();

            let mut second = term("電車", "でんしゃ");
            second.book_id = Some("b2".into());
            second.sentence = "電車が来た。".into();
            save_term(&mut conn, &second).unwrap();

            let (book, sentence): (Option<String>, String) = conn
                .query_row(
                    "SELECT book_id, sentence FROM saved_term WHERE id = ?1",
                    [id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap();

            assert_eq!(book.as_deref(), Some("b2"), "a real new value must win");
            assert_eq!(sentence, "電車が来た。");
        });
    }

    #[test]
    fn the_same_spelling_with_a_different_reading_is_a_different_word() {
        with_state("homograph", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("表", "おもて")).unwrap();
            save_term(&mut conn, &term("表", "ひょう")).unwrap();

            let terms: i64 = conn
                .query_row("SELECT COUNT(*) FROM saved_term", [], |r| r.get(0))
                .unwrap();
            assert_eq!(terms, 2);
        });
    }

    #[test]
    fn known_status_keeps_the_row_but_leaves_the_highlight_index() {
        with_state("known", |state| {
            let mut conn = state.conn();
            let (id, _) = save_term(&mut conn, &term("電車", "でんしゃ")).unwrap();

            assert!(
                highlight_index(&conn)
                    .unwrap()
                    .terms
                    .unknown
                    .contains(&"電車".to_string())
            );

            assert!(set_term_status(&mut conn, id, Status::Known).unwrap());

            let index = highlight_index(&conn).unwrap();
            assert!(index.terms.unknown.is_empty());
            assert!(index.terms.learning.is_empty());

            // SPEC §4: removed from highlighting, not deleted.
            let still_there: i64 = conn
                .query_row("SELECT COUNT(*) FROM saved_term WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .unwrap();
            assert_eq!(still_there, 1);
        });
    }

    #[test]
    fn learning_terms_and_kanji_stay_in_their_own_bucket() {
        with_state("learning", |state| {
            let mut conn = state.conn();
            let (id, _) = save_term(&mut conn, &term("世界", "せかい")).unwrap();
            set_term_status(&mut conn, id, Status::Learning).unwrap();
            set_kanji_status(&mut conn, "世", Status::Learning).unwrap();

            let index = highlight_index(&conn).unwrap();
            assert_eq!(index.terms.learning, vec!["世界".to_string()]);
            assert!(index.terms.unknown.is_empty());
            assert_eq!(index.kanji.learning, "世");
            assert_eq!(index.kanji.unknown, "界");
        });
    }

    #[test]
    fn deleting_a_term_removes_only_its_orphaned_kanji() {
        with_state("orphans", |state| {
            let mut conn = state.conn();
            // 学 is shared; 校 and 生 are not.
            let (school, _) = save_term(&mut conn, &term("学校", "がっこう")).unwrap();
            save_term(&mut conn, &term("学生", "がくせい")).unwrap();

            assert!(delete_term(&mut conn, school).unwrap());

            let mut stmt = conn
                .prepare("SELECT char FROM saved_kanji ORDER BY char")
                .unwrap();
            let remaining: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();

            assert!(
                remaining.contains(&"学".to_string()),
                "shared kanji must survive"
            );
            assert!(remaining.contains(&"生".to_string()));
            assert!(!remaining.contains(&"校".to_string()), "orphan must go");
        });
    }

    #[test]
    fn index_version_bumps_on_writes_that_change_highlighting() {
        with_state("version", |state| {
            let mut conn = state.conn();
            let start = index_version(&conn).unwrap();

            let (id, _) = save_term(&mut conn, &term("時間", "じかん")).unwrap();
            let after_save = index_version(&conn).unwrap();
            assert!(after_save > start, "saving must invalidate the index");

            set_term_status(&mut conn, id, Status::Learning).unwrap();
            let after_status = index_version(&conn).unwrap();
            assert!(after_status > after_save);

            delete_term(&mut conn, id).unwrap();
            assert!(index_version(&conn).unwrap() > after_status);
        });
    }

    #[test]
    fn a_no_op_write_does_not_bump_the_version() {
        with_state("noop-version", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("場所", "ばしょ")).unwrap();
            let before = index_version(&conn).unwrap();

            // Nothing matches these, so no client's cached index went stale.
            assert!(!set_term_status(&mut conn, 9999, Status::Known).unwrap());
            assert!(!delete_term(&mut conn, 9999).unwrap());
            assert!(!set_kanji_status(&mut conn, "猫", Status::Known).unwrap());

            assert_eq!(index_version(&conn).unwrap(), before);
        });
    }

    #[test]
    fn a_bulk_status_change_bumps_the_version_once_not_per_row() {
        with_state("bulk-version", |state| {
            let mut conn = state.conn();
            let ids: Vec<i64> = ["学校", "学生", "先生", "時間"]
                .iter()
                .map(|t| save_term(&mut conn, &term(t, "")).unwrap().0)
                .collect();

            let before = index_version(&conn).unwrap();
            let affected = bulk_set_term_status(&mut conn, &ids, Status::Known).unwrap();

            assert_eq!(affected, 4);
            // One user action, one invalidation. Bumping per row would make every
            // client refetch the highlight index four times over.
            assert_eq!(index_version(&conn).unwrap(), before + 1);

            let index = highlight_index(&conn).unwrap();
            assert!(index.terms.unknown.is_empty());
        });
    }

    #[test]
    fn a_bulk_delete_prunes_orphans_but_keeps_shared_kanji() {
        with_state("bulk-delete", |state| {
            let mut conn = state.conn();
            // 学 is in both deleted terms and in the survivor; 校 and 生 are not.
            let a = save_term(&mut conn, &term("学校", "がっこう")).unwrap().0;
            let b = save_term(&mut conn, &term("学生", "がくせい")).unwrap().0;
            save_term(&mut conn, &term("大学", "だいがく")).unwrap();

            let before = index_version(&conn).unwrap();
            assert_eq!(bulk_delete_terms(&mut conn, &[a, b]).unwrap(), 2);
            assert_eq!(index_version(&conn).unwrap(), before + 1);

            let kanji = highlight_index(&conn).unwrap().kanji.unknown;
            assert!(kanji.contains('学'), "shared with 大学, must survive");
            assert!(kanji.contains('大'));
            assert!(!kanji.contains('校'));
            assert!(!kanji.contains('生'));
        });
    }

    #[test]
    fn an_empty_bulk_request_changes_nothing() {
        with_state("bulk-empty", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("世界", "せかい")).unwrap();
            let before = index_version(&conn).unwrap();

            assert_eq!(
                bulk_set_term_status(&mut conn, &[], Status::Known).unwrap(),
                0
            );
            assert_eq!(bulk_delete_terms(&mut conn, &[]).unwrap(), 0);
            assert_eq!(
                bulk_set_kanji_status(&mut conn, &[], Status::Known).unwrap(),
                0
            );

            assert_eq!(index_version(&conn).unwrap(), before);
        });
    }

    #[test]
    fn bulk_kanji_status_moves_only_the_named_characters() {
        with_state("bulk-kanji", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("図書館", "としょかん")).unwrap();

            let affected = bulk_set_kanji_status(
                &mut conn,
                &["図".to_string(), "書".to_string()],
                Status::Known,
            )
            .unwrap();
            assert_eq!(affected, 2);

            let index = highlight_index(&conn).unwrap();
            assert_eq!(index.kanji.unknown, "館", "the two marked known drop out");
        });
    }

    fn legacy(id: &str, text: &str) -> LegacyHighlightInput {
        LegacyHighlightInput {
            id: id.to_string(),
            chapter_index: 2,
            block_id: "ch2-b7".to_string(),
            text: text.to_string(),
            start_offset: 10,
            end_offset: 10 + text.chars().count() as i64,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn importing_legacy_highlights_is_idempotent() {
        with_state("legacy-idempotent", |state| {
            let mut conn = state.conn();
            let batch = [
                legacy("hl-1", "とても長い文章です"),
                legacy("hl-2", "もう一つ"),
            ];

            let first =
                import_legacy_highlights(&mut conn, "b1", Some("義妹生活５"), &batch).unwrap();
            assert_eq!(first, 2);

            // The client pushes on every reader mount, so a second identical
            // push must add nothing rather than duplicate the whole book.
            let second =
                import_legacy_highlights(&mut conn, "b1", Some("義妹生活５"), &batch).unwrap();
            assert_eq!(second, 0);

            let (items, total) = list_legacy_highlights(&conn, None, 100, 0).unwrap();
            assert_eq!(total, 2);
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].book_title.as_deref(), Some("義妹生活５"));
        });
    }

    #[test]
    fn a_title_arriving_on_a_later_push_backfills_rows_that_lack_one() {
        with_state("legacy-backfill", |state| {
            let mut conn = state.conn();
            let batch = [legacy("hl-1", "文章")];

            // First push lands before book metadata has loaded.
            import_legacy_highlights(&mut conn, "b1", None, &batch).unwrap();
            let (items, _) = list_legacy_highlights(&conn, Some("b1"), 10, 0).unwrap();
            assert_eq!(items[0].book_title, None);

            // Second push carries the title. OR IGNORE alone would drop it and
            // the Saved screen would show a raw book id forever.
            import_legacy_highlights(&mut conn, "b1", Some("義妹生活５"), &batch).unwrap();
            let (items, total) = list_legacy_highlights(&conn, Some("b1"), 10, 0).unwrap();
            assert_eq!(total, 1, "still not duplicated");
            assert_eq!(items[0].book_title.as_deref(), Some("義妹生活５"));

            // And a later push without a title must not erase it.
            import_legacy_highlights(&mut conn, "b1", None, &batch).unwrap();
            let (items, _) = list_legacy_highlights(&conn, Some("b1"), 10, 0).unwrap();
            assert_eq!(items[0].book_title.as_deref(), Some("義妹生活５"));
        });
    }

    #[test]
    fn legacy_highlights_never_enter_the_study_matcher() {
        with_state("legacy-not-indexed", |state| {
            let mut conn = state.conn();
            let before = index_version(&conn).unwrap();

            import_legacy_highlights(&mut conn, "b1", None, &[legacy("hl-1", "文章")]).unwrap();

            // They are whole phrases with no reading, in a different offset
            // space, so they must not appear in the highlight index -- nor
            // invalidate every client's cached copy.
            let index = highlight_index(&conn).unwrap();
            assert!(index.terms.unknown.is_empty());
            assert!(index.kanji.unknown.is_empty());
            assert_eq!(index_version(&conn).unwrap(), before);

            let saved: i64 = conn
                .query_row("SELECT COUNT(*) FROM saved_term", [], |r| r.get(0))
                .unwrap();
            assert_eq!(saved, 0);
        });
    }

    #[test]
    fn legacy_highlights_can_be_filtered_by_book_and_promoted() {
        with_state("legacy-filter", |state| {
            let mut conn = state.conn();
            import_legacy_highlights(&mut conn, "b1", None, &[legacy("a", "one")]).unwrap();
            import_legacy_highlights(&mut conn, "b2", None, &[legacy("b", "two")]).unwrap();

            let (only_b2, total) = list_legacy_highlights(&conn, Some("b2"), 100, 0).unwrap();
            assert_eq!(total, 1);
            assert_eq!(only_b2[0].id, "b");

            let (term_id, _) = save_term(&mut conn, &term("文章", "ぶんしょう")).unwrap();
            assert!(mark_legacy_promoted(&conn, "a", term_id).unwrap());
            assert!(!mark_legacy_promoted(&conn, "missing", term_id).unwrap());

            let (all, _) = list_legacy_highlights(&conn, Some("b1"), 100, 0).unwrap();
            assert_eq!(all[0].promoted_term_id, Some(term_id));
        });
    }

    #[test]
    fn an_empty_legacy_import_is_a_no_op() {
        with_state("legacy-empty", |state| {
            let mut conn = state.conn();
            assert_eq!(
                import_legacy_highlights(&mut conn, "b1", None, &[]).unwrap(),
                0
            );
            let (_, total) = list_legacy_highlights(&conn, None, 100, 0).unwrap();
            assert_eq!(total, 0);
        });
    }

    #[test]
    fn the_highlight_index_reports_the_current_version() {
        with_state("index-version-field", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("音楽", "おんがく")).unwrap();

            let index = highlight_index(&conn).unwrap();
            assert_eq!(index.version, index_version(&conn).unwrap());
            assert_eq!(index.kanji.unknown, "楽音", "sorted by codepoint");
        });
    }

    #[test]
    fn surrogate_pair_kanji_survives_a_round_trip() {
        with_state("extension-b", |state| {
            let mut conn = state.conn();
            save_term(&mut conn, &term("𠮟る", "しかる")).unwrap();

            let index = highlight_index(&conn).unwrap();
            assert_eq!(index.kanji.unknown, "\u{20B9F}");
            assert_eq!(index.kanji.unknown.chars().count(), 1);
        });
    }
}
