//! Lanobe — a light-novel-only reader server.
//!
//! Derived from Manatan (MIT), with the manga/anime/OCR halves and the embedded
//! Suwayomi Java runtime removed. This binary is always headless: it assembles the
//! API routers, serves the embedded React PWA, and nothing else.

use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::anyhow;
use axum::{
    Router,
    http::{HeaderMap, HeaderValue, Method, StatusCode, Uri, header},
    response::IntoResponse,
    routing::any,
};
use clap::{Parser, Subcommand};
use directories::ProjectDirs;
use rust_embed::RustEmbed;
use serde::Serialize;
use tower_http::{
    compression::{
        CompressionLayer,
        predicate::{DefaultPredicate, NotForContentType, Predicate},
    },
    cors::{AllowOrigin, CorsLayer},
    set_header::SetResponseHeaderLayer,
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const APP_NAME: &str = "Lanobe";

/// The React build is embedded at compile time. `make webui` (or the Docker build)
/// populates this directory; a placeholder is committed so a bare `cargo build` works.
#[derive(RustEmbed)]
#[folder = "resources/webui"]
struct FrontendAssets;

#[derive(Serialize)]
struct VersionResponse {
    version: String,
    variant: String,
}

#[derive(Parser, Debug, Clone)]
#[command(author, version, about = "Light-novel reader server", long_about = None)]
struct Cli {
    /// IP address to bind to
    #[arg(long, default_value = "0.0.0.0", env = "LANOBE_HOST")]
    host: Ipv4Addr,

    /// Port to bind to
    #[arg(long, default_value_t = 4567, env = "LANOBE_PORT")]
    port: u16,

    /// Data directory (databases, imported dictionaries, fonts)
    #[arg(long, env = "LANOBE_DATA_DIR")]
    data_dir: Option<PathBuf>,

    /// Light novel library directory (absolute, or relative to the data dir)
    #[arg(long, env = "LANOBE_LIBRARY_PATH")]
    library_path: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand, Debug, Clone)]
enum Command {
    /// Hash a password for LANOBE_PASSWORD_HASH.
    ///
    /// Reads from stdin so the password does not end up in your shell history:
    ///     echo -n 'my password' | lanobe hash-password
    HashPassword,
}

fn run_hash_password() -> anyhow::Result<()> {
    use std::io::Read;

    let mut password = String::new();
    std::io::stdin().read_to_string(&mut password)?;

    let password = password.trim_end_matches(['\n', '\r']);
    if password.is_empty() {
        return Err(anyhow!("no password on stdin"));
    }

    println!("{}", lanobe_app_server::hash_password(password)?);
    Ok(())
}

fn resolve_data_dir(override_path: Option<&PathBuf>) -> PathBuf {
    if let Some(path) = override_path {
        return path.clone();
    }

    ProjectDirs::from("", "", APP_NAME)
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("./data"))
}

fn resolve_relative(option: Option<&PathBuf>, data_dir: &Path, default_relative: &str) -> PathBuf {
    match option {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => data_dir.join(path),
        None => data_dir.join(default_relative),
    }
}

fn main() -> anyhow::Result<()> {
    let args = Cli::parse();

    let rust_log = std::env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default();
    let env_filter = if rust_log.is_empty() {
        EnvFilter::builder().parse_lossy("info")
    } else {
        EnvFilter::builder().parse_lossy(rust_log)
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    if let Some(Command::HashPassword) = args.command {
        return run_hash_password();
    }

    let data_dir = resolve_data_dir(args.data_dir.as_ref());
    let library_path = resolve_relative(args.library_path.as_ref(), &data_dir, "library");

    let runtime = tokio::runtime::Runtime::new()
        .map_err(|err| anyhow!("Failed to create Tokio runtime: {err}"))?;

    runtime.block_on(async move {
        if let Err(err) = run_server(&data_dir, &library_path, args.host, args.port).await {
            error!("Server stopped with an error: {err}");
            return Err(err);
        }
        Ok(())
    })
}

async fn run_server(
    data_dir: &Path,
    library_path: &Path,
    host: Ipv4Addr,
    port: u16,
) -> anyhow::Result<()> {
    info!("{APP_NAME} v{APP_VERSION}");
    info!("Data directory:    {}", data_dir.display());
    info!("Library directory: {}", library_path.display());

    for dir in [data_dir, library_path] {
        fs::create_dir_all(dir)
            .map_err(|err| anyhow!("Failed to create {}: {err}", dir.display()))?;
    }

    let app_state = lanobe_app_server::build_state(data_dir)
        .map_err(|err| anyhow!("Failed to initialise app state: {err}"))?;

    let yomitan_router = lanobe_yomitan_server::create_router(data_dir.to_path_buf());
    let audio_router = lanobe_audio_server::create_router(data_dir.to_path_buf());
    let novel_router =
        lanobe_novel_server::create_router(data_dir.to_path_buf(), library_path.to_path_buf());
    let app_router = lanobe_app_server::create_router(app_state.clone());
    let system_router = Router::new().route("/version", any(current_version_handler));

    // Everything that exposes library content sits behind the session check. The
    // version probe and the login endpoints stay open so the client can discover
    // that a password is needed and then supply one.
    let protected = Router::new()
        .nest("/api/audio", audio_router)
        .nest("/api/novel", novel_router)
        .nest("/api/yomitan", yomitan_router)
        .layer(axum::middleware::from_fn_with_state(
            app_state,
            lanobe_app_server::require_auth,
        ));

    // Same-origin in production (Caddy fronts everything); mirroring the request
    // origin keeps `yarn dev` against a remote server working. Credentials are
    // required so the session cookie survives that cross-origin dev setup, and a
    // credentialed response may not use wildcards - hence the explicit lists.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE, header::ACCEPT]);

    // Nothing was compressed before this. Dictionary lookups are 5-34 KB of JSON
    // that gzip to under 3 KB, and the app bundle is 897 KB that gzips to 280 KB.
    //
    // The octet-stream exclusion is the important one: `get_epub` returns a bare
    // Vec<u8>, which axum types as application/octet-stream, and an EPUB is a zip
    // container that is already deflated. DefaultPredicate would happily spend CPU
    // recompressing many MB of it for no gain. Fonts (woff2 is brotli inside) and
    // audio/video are excluded for the same reason.
    let compression = CompressionLayer::new().gzip(true).compress_when(
        DefaultPredicate::new()
            .and(NotForContentType::const_new("application/octet-stream"))
            .and(NotForContentType::const_new("application/zip"))
            .and(NotForContentType::const_new("application/epub+zip"))
            .and(NotForContentType::const_new("font/"))
            .and(NotForContentType::const_new("audio/"))
            .and(NotForContentType::const_new("video/")),
    );

    // CompressionLayer sets `vary: accept-encoding` only on responses it actually
    // compressed -- verified: the EPUB and 304s come back without it. Those are
    // exactly the responses a shared cache could then hand to a client with a
    // different Accept-Encoding, so tag every response here instead.
    //
    // Appending, not overriding: CORS already emits `vary: origin, ...` and
    // clobbering that would let a cache serve one origin's response to another.
    // `if_not_present` would be wrong for the same reason -- Vary is already set,
    // so it would never fire. The cost is a duplicated `accept-encoding` on
    // compressed responses, which is harmless: HTTP folds repeated Vary lines into
    // one list and a repeated entry is a no-op.
    let vary_accept_encoding = SetResponseHeaderLayer::appending(
        header::VARY,
        HeaderValue::from_static("accept-encoding"),
    );

    let app = Router::new()
        .merge(protected)
        .nest("/api/app", app_router)
        .nest("/api/system", system_router)
        // Unknown /api routes must 404 as JSON. Without this they fall through to
        // the SPA fallback below and return 200 with an HTML body, which turns a
        // missing endpoint into a confusing JSON parse error on the client.
        .route("/api/{*rest}", any(unknown_api_route))
        .fallback(serve_frontend)
        // Layers apply outermost-last, so this reads inside-out: compression wraps
        // the routes (including the SPA fallback, which is what makes the bundle
        // compressible), and CORS stays outermost so it can short-circuit OPTIONS
        // preflights without them ever entering the compressor.
        .layer(compression)
        .layer(vary_accept_encoding)
        .layer(cors);

    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}"))
        .await
        .map_err(|err| anyhow!("Failed to bind {host}:{port}: {err}"))?;

    info!("Listening on http://{host}:{port}");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|err| anyhow!("Server error: {err}"))?;

    info!("Shutdown complete.");
    Ok(())
}

/// Vite fingerprints everything it emits into `assets/` as `name-<8 chars>.ext`,
/// so those may be cached forever: changing a file changes its URL. Everything
/// else in the build keeps its name across builds and must be revalidated —
/// `sw.js` above all, because a year-long cache on the service worker would
/// leave the app permanently unable to update itself.
fn is_content_hashed(path: &str) -> bool {
    let Some(name) = path.strip_prefix("assets/") else {
        return false;
    };

    // Vite emits `assets/` flat. Anything nested came from `public/`, which is
    // copied verbatim and therefore unhashed.
    if name.contains('/') {
        return false;
    }

    let Some((stem, _)) = name.rsplit_once('.') else {
        return false;
    };

    // Require `-` plus 8 base64url characters, and at least one character of
    // name before it. Anchored at the end rather than split on the last `-`:
    // base64url hashes contain `-` themselves (e.g. `index-B1BY-SEW.js`), so
    // splitting on the last dash misclassifies a good fraction of real files.
    let bytes = stem.as_bytes();
    if bytes.len() < 10 {
        return false;
    }
    let (head, hash) = bytes.split_at(bytes.len() - 8);
    head[head.len() - 1] == b'-'
        && hash
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'-' || *b == b'_')
}

/// A **weak** validator, deliberately: it identifies the resource rather than one
/// specific content-coding, so it stays correct when the compression layer gzips
/// the body underneath us. A strong ETag would have to vary per encoding.
fn weak_etag(hash: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(20);
    out.push_str("W/\"");
    for byte in &hash[..8] {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out.push('"');
    out
}

fn if_none_match_matches(headers: &HeaderMap, etag: &str) -> bool {
    let Some(Ok(value)) = headers.get(header::IF_NONE_MATCH).map(|v| v.to_str()) else {
        return false;
    };

    value.split(',').any(|candidate| {
        let candidate = candidate.trim();
        candidate == "*" || candidate.trim_start_matches("W/") == etag.trim_start_matches("W/")
    })
}

/// `index.html` is rewritten and hashed identically on every request, and the SPA
/// fallback serves it for every unknown path, so do the work once rather than
/// reallocating the whole document per navigation.
fn index_html() -> Option<&'static (String, String)> {
    static INDEX: OnceLock<Option<(String, String)>> = OnceLock::new();

    INDEX
        .get_or_init(|| {
            let index = FrontendAssets::get("index.html")?;
            let html = std::str::from_utf8(index.data.as_ref()).ok()?;
            Some((
                html.replace("<head>", "<head><base href=\"/\" />"),
                // Hashed from the embedded bytes, which still uniquely identify
                // the rewritten output because the rewrite is deterministic.
                weak_etag(&index.metadata.sha256_hash()),
            ))
        })
        .as_ref()
}

async fn serve_frontend(headers: HeaderMap, uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty()
        && let Some(content) = FrontendAssets::get(path)
    {
        let etag = weak_etag(&content.metadata.sha256_hash());
        let cache_control = if is_content_hashed(path) {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };

        if if_none_match_matches(&headers, &etag) {
            return (
                StatusCode::NOT_MODIFIED,
                [
                    (header::ETAG, etag),
                    (header::CACHE_CONTROL, cache_control.to_owned()),
                ],
            )
                .into_response();
        }

        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [
                (header::CONTENT_TYPE, mime.as_ref().to_owned()),
                (header::ETAG, etag),
                (header::CACHE_CONTROL, cache_control.to_owned()),
            ],
            content.data,
        )
            .into_response();
    }

    // SPA fallback: every unknown path renders index.html so client-side routing works.
    let Some((html, etag)) = index_html() else {
        return (
            StatusCode::NOT_FOUND,
            "WebUI assets are missing from this build.",
        )
            .into_response();
    };

    if if_none_match_matches(&headers, etag) {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::ETAG, etag.clone()),
                (header::CACHE_CONTROL, "no-cache".to_owned()),
            ],
        )
            .into_response();
    }

    (
        [
            (header::CONTENT_TYPE, "text/html".to_owned()),
            (header::ETAG, etag.clone()),
            (header::CACHE_CONTROL, "no-cache".to_owned()),
        ],
        html.clone(),
    )
        .into_response()
}

async fn unknown_api_route(uri: Uri) -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({
            "error": "not_found",
            "path": uri.path(),
        })),
    )
}

async fn current_version_handler() -> impl IntoResponse {
    axum::Json(VersionResponse {
        version: APP_VERSION.to_string(),
        variant: "server".to_string(),
    })
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut sigterm = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = async {
                match sigterm.as_mut() {
                    Some(sigterm) => { sigterm.recv().await; },
                    None => std::future::pending::<()>().await,
                }
            } => {},
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }

    info!("Shutdown signal received.");
}

#[cfg(test)]
mod tests {
    use super::{if_none_match_matches, is_content_hashed, weak_etag};
    use axum::http::{HeaderMap, HeaderValue, header};

    #[test]
    fn vite_hashed_assets_are_immutable() {
        // Real filenames from a `make webui` build.
        assert!(is_content_hashed("assets/index-lkkwNXr7.js"));
        assert!(is_content_hashed("assets/index-CWLzD0OZ.css"));
        assert!(is_content_hashed("assets/polyfills-Be1mub0e.js"));
        // @vitejs/plugin-legacy adds an infix but keeps the hash.
        assert!(is_content_hashed("assets/index-legacy-CyFcTjgM.js"));
    }

    #[test]
    fn base64url_hashes_containing_a_dash_are_recognised() {
        // `index-B1BY-SEW.js` is a real emitted name. Splitting on the last `-`
        // would read the hash as "SEW" and wrongly mark the file unhashed.
        assert!(is_content_hashed("assets/index-B1BY-SEW.js"));
        assert!(is_content_hashed("assets/chunk-a_b-cdef.js"));
    }

    #[test]
    fn files_that_keep_their_name_across_builds_are_never_immutable() {
        // Freezing sw.js for a year would leave the app unable to update itself.
        assert!(!is_content_hashed("sw.js"));
        assert!(!is_content_hashed("registerSW.js"));
        assert!(!is_content_hashed("index.html"));
        assert!(!is_content_hashed("site.webmanifest"));
        assert!(!is_content_hashed("favicon.svg"));
        // Fetched at runtime by i18next, and unhashed because it comes from public/.
        assert!(!is_content_hashed("locales/en.json"));
    }

    #[test]
    fn unhashed_files_under_assets_are_not_immutable() {
        // Guards the day someone adds WebUI/public/assets/, which would be copied
        // verbatim into the same prefix without a fingerprint.
        assert!(!is_content_hashed("assets/logo.png"));
        assert!(!is_content_hashed("assets/some-file.png"));
        assert!(!is_content_hashed("assets/nested/index-lkkwNXr7.js"));
        assert!(!is_content_hashed("assets/noextension"));
        assert!(!is_content_hashed("assets/.gitkeep"));
    }

    #[test]
    fn weak_etag_is_stable_and_differs_per_content() {
        let a = weak_etag(&[0xab; 32]);
        let b = weak_etag(&[0xcd; 32]);

        assert_eq!(a, weak_etag(&[0xab; 32]));
        assert_ne!(a, b);
        assert!(a.starts_with("W/\""), "must be a weak validator: {a}");
        assert_eq!(a, "W/\"abababababababab\"");
    }

    fn headers_with(if_none_match: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::IF_NONE_MATCH,
            HeaderValue::from_str(if_none_match).expect("valid header"),
        );
        headers
    }

    #[test]
    fn if_none_match_recognises_its_own_etag() {
        let etag = weak_etag(&[0x01; 32]);

        assert!(if_none_match_matches(&headers_with(&etag), &etag));
        assert!(if_none_match_matches(&headers_with("*"), &etag));
        // Browsers may echo the tag back without the weak prefix.
        assert!(if_none_match_matches(
            &headers_with(etag.trim_start_matches("W/")),
            &etag
        ));
        // And may send several.
        assert!(if_none_match_matches(
            &headers_with(&format!("W/\"deadbeef\", {etag}")),
            &etag
        ));
    }

    #[test]
    fn if_none_match_rejects_a_different_or_absent_etag() {
        let etag = weak_etag(&[0x01; 32]);

        assert!(!if_none_match_matches(&HeaderMap::new(), &etag));
        assert!(!if_none_match_matches(
            &headers_with("W/\"deadbeef\""),
            &etag
        ));
        assert!(!if_none_match_matches(&headers_with(""), &etag));
    }
}
