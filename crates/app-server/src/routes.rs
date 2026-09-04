//! Session and settings endpoints.

use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};

use crate::{auth, state::AppState};

#[derive(Serialize)]
pub struct SessionResponse {
    /// Whether a password is configured at all. The client uses this to decide
    /// between showing a login screen and going straight to the library.
    auth_required: bool,
    authenticated: bool,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    /// Accepted and ignored: single-user server. Present so the existing login form
    /// can post unchanged.
    #[serde(default)]
    #[allow(dead_code)]
    username: Option<String>,
    password: String,
}

/// Requests arriving over TLS get a `Secure` cookie. Behind Caddy the hop to the
/// app is plain HTTP, so trust `X-Forwarded-Proto` for this - it only ever adds a
/// restriction, so a spoofed header cannot weaken the cookie.
fn is_secure_request(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
}

pub fn has_valid_session(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(auth::extract_cookie)
        .is_some_and(|token| state.verify_token(token))
}

pub async fn session(State(state): State<AppState>, headers: HeaderMap) -> Json<SessionResponse> {
    let auth_required = state.is_auth_required();

    Json(SessionResponse {
        auth_required,
        // With no password configured every caller is authenticated by definition.
        authenticated: !auth_required || has_valid_session(&state, &headers),
    })
}

pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginRequest>,
) -> Response {
    if !state.is_auth_required() {
        // Nothing to log in to; report success so the UI moves on.
        return Json(SessionResponse {
            auth_required: false,
            authenticated: true,
        })
        .into_response();
    }

    if !state.verify_password(&body.password) {
        tracing::warn!("Rejected a login attempt with an incorrect password.");

        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "invalid_credentials" })),
        )
            .into_response();
    }

    let cookie = auth::set_cookie_header(&state.issue_token(), is_secure_request(&headers));

    (
        [(header::SET_COOKIE, cookie)],
        Json(SessionResponse {
            auth_required: true,
            authenticated: true,
        }),
    )
        .into_response()
}

pub async fn logout() -> Response {
    (
        [(header::SET_COOKIE, auth::clear_cookie_header())],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

pub async fn get_meta(State(state): State<AppState>) -> Response {
    match state.read_meta() {
        Ok(meta) => Json(serde_json::Value::Object(meta)).into_response(),
        Err(err) => {
            tracing::error!("Failed to read settings: {err}");
            internal_error()
        }
    }
}

pub async fn put_meta(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    let Some(updates) = body.as_object() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "expected_object" })),
        )
            .into_response();
    };

    match state.write_meta(updates) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(err) => {
            tracing::error!("Failed to write settings: {err}");
            internal_error()
        }
    }
}

fn internal_error() -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": "internal_error" })),
    )
        .into_response()
}
