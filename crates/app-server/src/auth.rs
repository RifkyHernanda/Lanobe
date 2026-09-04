//! Password verification and signed session cookies.
//!
//! Deliberately small: one user, one cookie, no refresh dance. The cookie carries
//! nothing but an expiry, signed with a per-install secret, so a stolen cookie is
//! worth exactly one session and forging one requires the secret.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

type HmacSha256 = Hmac<Sha256>;

pub const COOKIE_NAME: &str = "lanobe_session";

/// 90 days. Long on purpose: this is a personal reader opened from a phone on the
/// sofa, and being logged out every week is the kind of friction that gets an app
/// abandoned. Logout and rotating the secret both revoke immediately.
pub const SESSION_TTL_SECS: u64 = 90 * 24 * 60 * 60;

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("password hashing failed: {0}")]
    Hash(String),
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);

    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AuthError::Hash(err.to_string()))
}

pub fn verify_password(password: &str, phc_hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(phc_hash) else {
        tracing::error!("configured password hash is not a valid PHC string; refusing all logins");
        return false;
    };

    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sign(secret: &[u8], payload: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

/// Issue a token of the form `<expiry>.<signature>`.
pub fn issue_token(secret: &[u8]) -> String {
    let expires_at = now_secs() + SESSION_TTL_SECS;
    let payload = expires_at.to_string();
    let signature = sign(secret, &payload);

    format!("{payload}.{signature}")
}

pub fn verify_token(secret: &[u8], token: &str) -> bool {
    let Some((payload, signature)) = token.split_once('.') else {
        return false;
    };

    // Constant-time compare: `Mac::verify_slice` rather than `==` on the strings,
    // so signature checking does not leak a prefix match through timing.
    let Ok(provided) = URL_SAFE_NO_PAD.decode(signature) else {
        return false;
    };

    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    if mac.verify_slice(&provided).is_err() {
        return false;
    }

    // Signature is ours, so the expiry can be trusted.
    payload
        .parse::<u64>()
        .is_ok_and(|expires_at| expires_at > now_secs())
}

/// Pull our cookie out of a raw `Cookie` header. Hand-rolled to avoid pulling in a
/// cookie crate for one header.
pub fn extract_cookie(header: &str) -> Option<&str> {
    header.split(';').find_map(|pair| {
        let (name, value) = pair.trim().split_once('=')?;
        (name == COOKIE_NAME).then_some(value)
    })
}

pub fn set_cookie_header(token: &str, secure: bool) -> String {
    let secure_flag = if secure { "; Secure" } else { "" };

    format!(
        "{COOKIE_NAME}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={SESSION_TTL_SECS}{secure_flag}",
    )
}

pub fn clear_cookie_header() -> String {
    format!("{COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_password() {
        let hash = hash_password("correct horse").expect("hashing works");

        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong horse", &hash));
    }

    #[test]
    fn rejects_a_malformed_hash() {
        assert!(!verify_password("anything", "not-a-phc-string"));
    }

    #[test]
    fn accepts_its_own_token() {
        let secret = b"secret value";

        assert!(verify_token(secret, &issue_token(secret)));
    }

    #[test]
    fn rejects_a_token_signed_with_another_secret() {
        let token = issue_token(b"one secret");

        assert!(!verify_token(b"another secret", &token));
    }

    #[test]
    fn rejects_a_tampered_expiry() {
        let secret = b"secret value";
        let token = issue_token(secret);
        let (_, signature) = token.split_once('.').expect("token has a signature");

        let forged = format!("{}.{signature}", now_secs() + SESSION_TTL_SECS * 10);

        assert!(!verify_token(secret, &forged));
    }

    #[test]
    fn rejects_an_expired_token() {
        let secret = b"secret value";
        let payload = (now_secs() - 1).to_string();
        let expired = format!("{payload}.{}", sign(secret, &payload));

        assert!(!verify_token(secret, &expired));
    }

    #[test]
    fn rejects_garbage() {
        let secret = b"secret value";

        assert!(!verify_token(secret, ""));
        assert!(!verify_token(secret, "no-separator"));
        assert!(!verify_token(secret, "123.not-base64!!"));
    }

    #[test]
    fn finds_its_cookie_among_others() {
        let header = format!("theme=dark; {COOKIE_NAME}=abc.def; other=1");

        assert_eq!(extract_cookie(&header), Some("abc.def"));
        assert_eq!(extract_cookie("theme=dark; other=1"), None);
    }
}
