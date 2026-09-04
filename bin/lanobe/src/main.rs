//! Lanobe — a light-novel-only reader server.
//!
//! Derived from Manatan (MIT), with the manga/anime/OCR halves and the embedded
//! Suwayomi Java runtime removed. This binary is always headless: it assembles the
//! API routers, serves the embedded React PWA, and nothing else.

use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

use anyhow::anyhow;
use axum::{
    Router,
    http::{StatusCode, Uri},
    response::IntoResponse,
    routing::any,
};
use clap::Parser;
use directories::ProjectDirs;
use rust_embed::RustEmbed;
use serde::Serialize;
use tower_http::cors::{AllowOrigin, CorsLayer};
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

    let yomitan_router = lanobe_yomitan_server::create_router(data_dir.to_path_buf());
    let audio_router = lanobe_audio_server::create_router(data_dir.to_path_buf());
    let novel_router =
        lanobe_novel_server::create_router(data_dir.to_path_buf(), library_path.to_path_buf());
    let system_router = Router::new().route("/version", any(current_version_handler));

    // Same-origin in production (Caddy fronts everything); mirroring the request origin
    // keeps `yarn dev` against a remote server working.
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::mirror_request())
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    let app = Router::new()
        .nest("/api/audio", audio_router)
        .nest("/api/novel", novel_router)
        .nest("/api/system", system_router)
        .nest("/api/yomitan", yomitan_router)
        // Unknown /api routes must 404 as JSON. Without this they fall through to
        // the SPA fallback below and return 200 with an HTML body, which turns a
        // missing endpoint into a confusing JSON parse error on the client.
        .route("/api/{*rest}", any(unknown_api_route))
        .fallback(serve_frontend)
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

async fn serve_frontend(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if !path.is_empty()
        && let Some(content) = FrontendAssets::get(path)
    {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
            content.data,
        )
            .into_response();
    }

    // SPA fallback: every unknown path renders index.html so client-side routing works.
    if let Some(index) = FrontendAssets::get("index.html")
        && let Ok(html) = std::str::from_utf8(index.data.as_ref())
    {
        let with_base = html.replace("<head>", "<head><base href=\"/\" />");
        return (
            [(axum::http::header::CONTENT_TYPE, "text/html")],
            with_base,
        )
            .into_response();
    }

    (
        StatusCode::NOT_FOUND,
        "WebUI assets are missing from this build.",
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
