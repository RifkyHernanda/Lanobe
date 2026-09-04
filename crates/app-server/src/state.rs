//! App state: the settings table, and the two pieces of auth configuration.

use rand::RngCore;
use rusqlite::{Connection, OptionalExtension};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use crate::auth;

/// Environment variables read at startup.
const ENV_PASSWORD_HASH: &str = "LANOBE_PASSWORD_HASH";
const ENV_PASSWORD: &str = "LANOBE_PASSWORD";
const ENV_SECRET: &str = "LANOBE_SECRET";

const SECRET_KEY: &str = "session_secret";

#[derive(Clone)]
pub struct AppState {
    /// One connection behind a mutex. This table sees a handful of reads per page
    /// load and a write when a setting changes - a pool would be ceremony.
    conn: Arc<Mutex<Connection>>,
    /// Argon2 PHC hash of the password. `None` means the server is unauthenticated,
    /// which is the right default for `docker compose up` on a laptop.
    password_hash: Option<Arc<String>>,
    session_secret: Arc<Vec<u8>>,
}

impl AppState {
    pub fn new(data_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(data_dir)?;

        let conn = Connection::open(data_dir.join("app.db"))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;

             CREATE TABLE IF NOT EXISTS meta (
                key        TEXT PRIMARY KEY,
                value      TEXT NOT NULL,
                updated_at INTEGER NOT NULL
             );

             CREATE TABLE IF NOT EXISTS secrets (
                key   TEXT PRIMARY KEY,
                value BLOB NOT NULL
             );",
        )?;

        let password_hash = Self::resolve_password_hash();
        let session_secret = Self::resolve_session_secret(&conn)?;

        match &password_hash {
            Some(_) => tracing::info!("Authentication enabled: a password is required."),
            None => tracing::warn!(
                "No password configured ({ENV_PASSWORD_HASH} / {ENV_PASSWORD} unset): \
                 anyone who can reach this server can read your library. Set one before \
                 exposing it to the internet."
            ),
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            password_hash: password_hash.map(Arc::new),
            session_secret: Arc::new(session_secret),
        })
    }

    fn resolve_password_hash() -> Option<String> {
        if let Ok(hash) = std::env::var(ENV_PASSWORD_HASH)
            && !hash.trim().is_empty()
        {
            return Some(hash);
        }

        // Convenience for self-hosters who would rather put a password in their
        // compose file than run `lanobe hash-password`. Hashed here so the plaintext
        // never reaches disk, though it is still visible in the process environment.
        if let Ok(password) = std::env::var(ENV_PASSWORD)
            && !password.trim().is_empty()
        {
            tracing::warn!(
                "Using {ENV_PASSWORD}. Prefer {ENV_PASSWORD_HASH} with the output of \
                 `lanobe hash-password` so the plaintext is not in your environment."
            );

            return match auth::hash_password(&password) {
                Ok(hash) => Some(hash),
                Err(err) => {
                    tracing::error!("Failed to hash {ENV_PASSWORD}: {err}");
                    None
                }
            };
        }

        None
    }

    /// A secret from the environment wins; otherwise one is generated once and kept
    /// in the database, so restarts do not log every device out.
    fn resolve_session_secret(conn: &Connection) -> anyhow::Result<Vec<u8>> {
        if let Ok(secret) = std::env::var(ENV_SECRET)
            && !secret.trim().is_empty()
        {
            return Ok(secret.into_bytes());
        }

        let stored: Option<Vec<u8>> = conn
            .query_row(
                "SELECT value FROM secrets WHERE key = ?1",
                [SECRET_KEY],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(secret) = stored {
            return Ok(secret);
        }

        let mut secret = vec![0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        conn.execute(
            "INSERT INTO secrets (key, value) VALUES (?1, ?2)",
            rusqlite::params![SECRET_KEY, secret],
        )?;

        Ok(secret)
    }

    pub fn is_auth_required(&self) -> bool {
        self.password_hash.is_some()
    }

    pub fn verify_password(&self, password: &str) -> bool {
        self.password_hash
            .as_ref()
            .is_some_and(|hash| auth::verify_password(password, hash))
    }

    pub fn issue_token(&self) -> String {
        auth::issue_token(&self.session_secret)
    }

    pub fn verify_token(&self, token: &str) -> bool {
        auth::verify_token(&self.session_secret, token)
    }

    /// All settings as a JSON object. Values were stored as JSON, so a row that
    /// somehow is not valid JSON degrades to a plain string rather than failing the
    /// whole request.
    pub fn read_meta(&self) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("meta lock poisoned"))?;
        let mut statement = conn.prepare("SELECT key, value FROM meta")?;

        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        let mut map = serde_json::Map::new();
        for row in rows {
            let (key, raw) = row?;
            let value = serde_json::from_str(&raw)
                .unwrap_or_else(|_| serde_json::Value::String(raw.clone()));
            map.insert(key, value);
        }

        Ok(map)
    }

    /// Merge keys into the settings table. Absent keys are left alone, so a client
    /// can write one setting without having to send the whole object back.
    pub fn write_meta(
        &self,
        updates: &serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|_| anyhow::anyhow!("meta lock poisoned"))?;
        let transaction = conn.transaction()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0) as i64;

        for (key, value) in updates {
            transaction.execute(
                "INSERT INTO meta (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                rusqlite::params![key, value.to_string(), now],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }
}
