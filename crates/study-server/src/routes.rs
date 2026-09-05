//! `/api/study/*`.
//!
//! Mounted inside the binary's `protected` router, so everything here already
//! sits behind the session check — saved vocabulary is library content.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;

use crate::{
    state::StudyState,
    store::{
        self, KanjiSort, ListQuery, NewTerm, Status, TermSort, bulk_delete_terms,
        bulk_set_kanji_status, bulk_set_term_status, delete_term, highlight_index, index_version,
        list_kanji, list_terms, save_term, set_kanji_status, set_term_status, stats,
    },
};

type ApiResult<T> = Result<T, (StatusCode, Json<serde_json::Value>)>;

fn db_error(err: rusqlite::Error) -> (StatusCode, Json<serde_json::Value>) {
    tracing::error!("study.db: {err}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": "database", "message": err.to_string() })),
    )
}

fn bad_request(message: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({ "error": "bad_request", "message": message })),
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    q: Option<String>,
    book: Option<String>,
    status: Option<String>,
    sort: Option<String>,
    dir: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

impl ListParams {
    fn to_query(&self) -> ApiResult<ListQuery> {
        let status = match self.status.as_deref() {
            None | Some("") | Some("all") => None,
            Some(raw) => Some(Status::parse(raw).ok_or_else(|| bad_request("unknown status"))?),
        };

        Ok(ListQuery {
            // Empty string means "no filter", not "match the empty string";
            // a cleared search box sends `?q=`.
            q: self.q.as_ref().filter(|s| !s.is_empty()).cloned(),
            book: self.book.as_ref().filter(|s| !s.is_empty()).cloned(),
            status,
            descending: !matches!(self.dir.as_deref(), Some("asc")),
            // Clamped so a hand-written request cannot ask the server to
            // serialise the entire table into memory at once.
            limit: self.limit.unwrap_or(100).clamp(1, 500),
            offset: self.offset.unwrap_or(0).max(0),
        })
    }
}

async fn create_term(
    State(state): State<StudyState>,
    Json(body): Json<NewTerm>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.term.trim().is_empty() {
        return Err(bad_request("term is required"));
    }

    let mut conn = state.conn();
    let (id, created) = save_term(&mut conn, &body).map_err(db_error)?;
    let version = index_version(&conn).map_err(db_error)?;

    Ok(Json(
        json!({ "id": id, "created": created, "indexVersion": version }),
    ))
}

async fn get_terms(
    State(state): State<StudyState>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let sort = match params.sort.as_deref() {
        None | Some("") | Some("created") => TermSort::Created,
        Some("frequency") => TermSort::Frequency,
        Some("term") => TermSort::Term,
        Some(_) => return Err(bad_request("unknown sort")),
    };

    let query = params.to_query()?;
    let conn = state.conn();
    let (items, total) = list_terms(&conn, &query, sort).map_err(db_error)?;

    Ok(Json(json!({ "items": items, "total": total })))
}

#[derive(Debug, Deserialize)]
struct StatusBody {
    status: String,
}

async fn patch_term(
    State(state): State<StudyState>,
    Path(id): Path<i64>,
    Json(body): Json<StatusBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let status = Status::parse(&body.status).ok_or_else(|| bad_request("unknown status"))?;

    let mut conn = state.conn();
    let ok = set_term_status(&mut conn, id, status).map_err(db_error)?;
    if !ok {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "id": id })),
        ));
    }

    let version = index_version(&conn).map_err(db_error)?;
    Ok(Json(json!({ "ok": true, "indexVersion": version })))
}

async fn remove_term(
    State(state): State<StudyState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<serde_json::Value>> {
    let mut conn = state.conn();
    let ok = delete_term(&mut conn, id).map_err(db_error)?;
    if !ok {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "id": id })),
        ));
    }

    let version = index_version(&conn).map_err(db_error)?;
    Ok(Json(json!({ "ok": true, "indexVersion": version })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkTermsBody {
    ids: Vec<i64>,
    /// "status" or "delete".
    action: String,
    status: Option<String>,
}

async fn bulk_terms(
    State(state): State<StudyState>,
    Json(body): Json<BulkTermsBody>,
) -> ApiResult<Json<serde_json::Value>> {
    // Bounded so one request cannot pin the write lock for an unbounded time.
    if body.ids.len() > 1000 {
        return Err(bad_request("at most 1000 ids per request"));
    }

    let mut conn = state.conn();
    let affected = match body.action.as_str() {
        "delete" => bulk_delete_terms(&mut conn, &body.ids).map_err(db_error)?,
        "status" => {
            let raw = body.status.as_deref().unwrap_or_default();
            let status = Status::parse(raw).ok_or_else(|| bad_request("unknown status"))?;
            bulk_set_term_status(&mut conn, &body.ids, status).map_err(db_error)?
        }
        _ => return Err(bad_request("action must be 'status' or 'delete'")),
    };

    let version = index_version(&conn).map_err(db_error)?;
    Ok(Json(
        json!({ "affected": affected, "indexVersion": version }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BulkKanjiBody {
    chars: Vec<String>,
    status: String,
}

async fn bulk_kanji(
    State(state): State<StudyState>,
    Json(body): Json<BulkKanjiBody>,
) -> ApiResult<Json<serde_json::Value>> {
    if body.chars.len() > 1000 {
        return Err(bad_request("at most 1000 characters per request"));
    }
    let status = Status::parse(&body.status).ok_or_else(|| bad_request("unknown status"))?;

    let mut conn = state.conn();
    let affected = bulk_set_kanji_status(&mut conn, &body.chars, status).map_err(db_error)?;
    let version = index_version(&conn).map_err(db_error)?;

    Ok(Json(
        json!({ "affected": affected, "indexVersion": version }),
    ))
}

async fn get_kanji(
    State(state): State<StudyState>,
    Query(params): Query<ListParams>,
) -> ApiResult<Json<serde_json::Value>> {
    let sort = match params.sort.as_deref() {
        None | Some("") | Some("created") => KanjiSort::Created,
        Some("count") => KanjiSort::Count,
        Some("char") => KanjiSort::Char,
        Some(_) => return Err(bad_request("unknown sort")),
    };

    let query = params.to_query()?;
    let conn = state.conn();
    let (items, total) = list_kanji(&conn, &query, sort).map_err(db_error)?;

    Ok(Json(json!({ "items": items, "total": total })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KanjiStatusBody {
    /// In the body rather than the path: a 4-byte CJK Extension B character
    /// percent-encoded into a URL path is the kind of thing that survives axum
    /// and then dies in a proxy.
    char: String,
    status: String,
}

async fn patch_kanji(
    State(state): State<StudyState>,
    Json(body): Json<KanjiStatusBody>,
) -> ApiResult<Json<serde_json::Value>> {
    let status = Status::parse(&body.status).ok_or_else(|| bad_request("unknown status"))?;

    let mut conn = state.conn();
    let ok = set_kanji_status(&mut conn, &body.char, status).map_err(db_error)?;
    if !ok {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "char": body.char })),
        ));
    }

    let version = index_version(&conn).map_err(db_error)?;
    Ok(Json(json!({ "ok": true, "indexVersion": version })))
}

/// The payload every open book fetches. Served with an ETag so a reader that
/// already has the current index pays one round trip and no body.
async fn get_highlight_index(
    State(state): State<StudyState>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let conn = state.conn();
    let index = highlight_index(&conn).map_err(db_error)?;
    drop(conn);

    // Derived from the version counter rather than hashing the body: the counter
    // is bumped in the same transaction as every write that changes highlighting,
    // so it is exact, and it costs nothing to compute.
    let etag = format!("\"sv{}\"", index.version);

    if let Some(Ok(candidate)) = headers.get(header::IF_NONE_MATCH).map(|v| v.to_str())
        && candidate
            .split(',')
            .any(|tag| tag.trim().trim_start_matches("W/") == etag)
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag),
                (header::CACHE_CONTROL, "no-cache".into()),
            ],
        )
            .into_response());
    }

    Ok((
        [
            (header::ETAG, etag),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        Json(index),
    )
        .into_response())
}

async fn get_stats(State(state): State<StudyState>) -> ApiResult<Json<store::Stats>> {
    let conn = state.conn();
    Ok(Json(stats(&conn).map_err(db_error)?))
}

pub fn router(state: StudyState) -> axum::Router {
    use axum::routing::{delete, get, patch, post};

    axum::Router::new()
        .route("/terms", post(create_term).get(get_terms))
        .route("/terms/{id}", patch(patch_term))
        .route("/terms/{id}", delete(remove_term))
        .route("/terms/bulk", post(bulk_terms))
        .route("/kanji", get(get_kanji))
        .route("/kanji", patch(patch_kanji))
        .route("/kanji/bulk", post(bulk_kanji))
        .route("/highlight-index", get(get_highlight_index))
        .route("/stats", get(get_stats))
        .with_state(state)
}
