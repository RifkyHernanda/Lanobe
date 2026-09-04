//! Session auth and cross-device settings.
//!
//! Two jobs. It owns the single-user login (argon2 password, signed cookie), and it
//! serves the global settings map that replaced Suwayomi's `meta/global` - the
//! thing that makes a theme change on the laptop show up on the tablet.
//!
//! Auth is opt-in: with no password configured the server is open, which is what
//! you want on a laptop or a LAN. Set `LANOBE_PASSWORD_HASH` before putting it on
//! the internet.

use axum::{
    Router,
    extract::{Request, State},
    http::StatusCode,
    middleware::{Next, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use std::path::Path;

pub mod auth;
mod routes;
mod state;

pub use auth::hash_password;
pub use state::AppState;

pub fn build_state(data_dir: &Path) -> anyhow::Result<AppState> {
    AppState::new(data_dir)
}

/// Mounted at `/api/app`.
///
/// `session` and `login` stay public even when auth is on: the client has to be
/// able to ask whether a password is needed, and to answer with one.
pub fn create_router(state: AppState) -> Router {
    let guarded = Router::new()
        .route("/meta", get(routes::get_meta).put(routes::put_meta))
        .route_layer(from_fn_with_state(state.clone(), require_auth));

    Router::new()
        .route("/session", get(routes::session))
        .route("/login", post(routes::login))
        .route("/logout", post(routes::logout))
        .merge(guarded)
        .with_state(state)
}

/// Rejects unauthenticated requests with a 401.
///
/// Apply to every API router that exposes library content. When no password is
/// configured this is a pass-through, so the open-by-default mode costs nothing.
pub async fn require_auth(State(state): State<AppState>, request: Request, next: Next) -> Response {
    if !state.is_auth_required() || routes::has_valid_session(&state, request.headers()) {
        return next.run(request).await;
    }

    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "error": "unauthorized" })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);

        let dir = std::env::temp_dir().join(format!("lanobe-app-server-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn meta_round_trips_and_merges() {
        let dir = temp_dir("meta");
        let state = AppState::new(&dir).expect("state");

        let mut first = serde_json::Map::new();
        first.insert("themeMode".into(), serde_json::json!("dark"));
        first.insert("mangaGridItemWidth".into(), serde_json::json!(300));
        state.write_meta(&first).expect("write");

        // A second write touching one key must not drop the others.
        let mut second = serde_json::Map::new();
        second.insert("themeMode".into(), serde_json::json!("light"));
        state.write_meta(&second).expect("write");

        let meta = state.read_meta().expect("read");
        assert_eq!(meta.get("themeMode"), Some(&serde_json::json!("light")));
        assert_eq!(
            meta.get("mangaGridItemWidth"),
            Some(&serde_json::json!(300))
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stores_structured_values_without_flattening_them() {
        let dir = temp_dir("structured");
        let state = AppState::new(&dir).expect("state");

        let mut updates = serde_json::Map::new();
        updates.insert(
            "customThemes".into(),
            serde_json::json!({ "mine": { "hue": 210 } }),
        );
        state.write_meta(&updates).expect("write");

        let meta = state.read_meta().expect("read");
        assert_eq!(
            meta.get("customThemes"),
            Some(&serde_json::json!({ "mine": { "hue": 210 } })),
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn session_secret_survives_a_restart() {
        let dir = temp_dir("secret");

        let token = {
            let state = AppState::new(&dir).expect("state");
            state.issue_token()
        };

        // A fresh state over the same data dir stands in for a server restart.
        let restarted = AppState::new(&dir).expect("state");
        assert!(
            restarted.verify_token(&token),
            "restarting must not log existing devices out",
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn is_open_when_no_password_is_configured() {
        let dir = temp_dir("open");
        let state = AppState::new(&dir).expect("state");

        assert!(!state.is_auth_required());
        assert!(
            !state.verify_password("anything"),
            "no password means no password matches"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
}
