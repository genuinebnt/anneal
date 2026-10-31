//! anneal's HTTP API. Content comes from files (loaded once at start-up),
//! progress lives in Postgres, and code runs through [`anneal_runner`].

mod activity;
mod backup;
pub mod auth;
pub mod cli_updates;
mod dsa;
mod feed;
pub mod dsa_next;
mod error;
mod format;
pub mod lsp;
pub mod mock;
pub mod practice;
pub mod review;
pub mod preflight;
mod progress;
pub mod reviews;
mod routes;
pub mod settings;
pub mod calendar;
pub mod course;
pub mod planner;
pub mod solutions;
pub mod streaks;
pub mod statement;
mod store;
pub mod ai;
pub mod meaning;
pub mod sd;
pub mod system;
mod views;

use std::path::Path;
use std::sync::Arc;

use anneal_content::Catalog;
use anneal_runner::Runner;
use axum::Router;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use sqlx::PgPool;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;

pub use error::ApiError;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct AppState {
    pub catalog: Arc<Catalog>,
    pub runner: Arc<Runner>,
    pub db: PgPool,
    pub lsp: lsp::LspConfig,
    pub auth: auth::AuthConfig,
    pub courses: Arc<Vec<anneal_content::course::Course>>,
    /// Meaning vectors for the DSA problems, and the model that embeds a search ([`meaning`]).
    pub meaning: Arc<meaning::Meaning>,
    /// AI help, when a provider is configured ([`ai`]); `None` turns every AI endpoint off.
    pub ai: Option<ai::AiConfig>,
}

/// The API under `/api`. If `web_dist` is a built web app, it's served for every other path.
pub fn app(state: AppState, web_dist: Option<&Path>) -> Router {
    // Everything but health and the auth routes needs a session when auth is on.
    let protected = Router::new()
        .route("/activity", get(routes::activity))
        .route("/export", get(backup::export))
        .route("/feed", get(feed::get).post(feed::make))
        .route("/day/{date}", get(routes::day))
        .route("/week-summary", get(routes::week_summary))
        .route("/import", post(backup::import).layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024)))
        .route("/prefs", get(settings::get_prefs).post(settings::put_pref))
        .route("/progress", get(routes::progress))
        .route("/stats", get(routes::stats))
        .route("/reviews", get(routes::reviews))
        .route("/problems/{id}/resolve", post(routes::resolve))
        .route("/problems/{id}/focus", post(routes::focus))
        .route("/tracks", get(routes::tracks))
        .route("/tracks/{track}", get(routes::track))
        .route("/tracks/{track}/reset", post(routes::reset_track))
        .route("/problems/{id}", get(routes::problem))
        .route(
            "/problems/{id}/draft",
            put(routes::save_draft).delete(routes::reset),
        )
        .route("/problems/{id}/run", post(routes::run))
        .route("/problems/{id}/scratch", put(routes::save_scratch))
        .route("/problems/{id}/scratch/run", post(routes::run_scratch))
        .route("/problems/{id}/submit", post(routes::submit))
        .route("/problems/{id}/hints", post(routes::reveal_hint))
        .route("/problems/{id}/solution", post(routes::reveal_solution))
        .route("/dsa", get(dsa::overview))
        .route("/dsa/problems/{id}/page", get(dsa::page))
        .route("/dsa/problems/{id}/row", get(dsa::problem_row))
        .route("/dsa/problems/{id}/log", post(dsa::log).delete(dsa::reset))
        .route("/dsa/problems/{id}/undo", post(dsa::undo))
        .route("/dsa/problems/{id}/why", post(dsa::why))
        .route("/dsa/problems/{id}/forecast", get(dsa::forecast))
        .route("/dsa/problems/{id}/restore", post(dsa::restore))
        .route("/dsa/problems/{id}/statement", get(statement::get))
        .route("/dsa/problems/{id}/preview", get(review::preview))
        .route("/streaks", get(streaks::get))
        .route("/streaks/marks", post(streaks::post_mark))
        .route("/streaks/marks/{id}", axum::routing::delete(streaks::delete_mark))
        .route("/plan", get(calendar::get))
        .route("/plan/preview", post(calendar::preview))
        .route("/plan/state", put(calendar::put_state))
        .route("/plans", post(calendar::create))
        .route("/plans/company", post(calendar::company_plan))
        .route("/plans/active", put(calendar::set_active))
        .route("/plans/{id}", axum::routing::delete(calendar::delete))
        .route("/dsa/problems/{id}/solutions", get(solutions::list).post(solutions::create))
        .route("/dsa/solutions/{sid}", put(solutions::update).delete(solutions::delete))
        .route("/dsa/start", post(dsa::start))
        .route("/dsa/practice/{pattern}", get(dsa::practice))
        .route("/dsa/patterns/{pattern}", get(dsa::pattern))
        .route("/dsa/review", get(review::queue))
        .route("/dsa/mock", get(mock::get))
        .route("/dsa/meaning", get(meaning::get))
        .route("/sd", get(sd::overview))
        .route("/sd/concepts/{id}", get(sd::concept))
        .route("/sd/problems/{id}", get(sd::problem))
        .route("/sd/attempts", post(sd::add_attempt))
        .route("/sd/attempts/{id}/marks", axum::routing::put(sd::mark_attempt))
        .route("/sd/checks", post(sd::mark_check))
        .route("/sd/review", get(sd::review))
        .route("/sd/review/{card}", post(sd::grade))
        .route("/sd/attempts/{id}/ai-review", post(ai::review_attempt))
        .route("/ai", get(ai::status))
        .route("/ai/ask", post(ai::ask))
        .route("/dsa/mock/config", put(mock::put_config))
        .route("/dsa/mock/rounds", post(mock::post_round))
        .route("/dsa/handwritten/{pattern}", get(practice::for_pattern))
        .route("/courses/{course}", get(course::overview))
        .route("/courses/{course}/stages/{id}", get(course::stage))
        .route("/courses/{course}/concepts/{id}", get(course::concept))
        .route("/courses/{course}/concepts/{id}/read", put(course::set_concept_read))
        .route("/courses/{course}/stages/{id}/hints", post(course::reveal_hint))
        .route("/courses/{course}/stages/{id}/solution", post(course::reveal_solution))
        .route("/courses/{course}/runs", post(course::post_run))
        .route("/courses/{course}/runs/start", post(course::start_run))
        .route("/courses/{course}/reset", post(course::reset_progress))
        .route("/courses/{course}/solutions", put(course::put_solutions))
        .route("/courses/{course}/repo", get(course::get_repo).put(course::put_repo))
        .route("/settings", get(settings::get))
        .route("/system", get(system::get))
        .route("/format", post(format::format))
        .route("/settings/editor", put(settings::put_editor))
        .route("/settings/appearance", put(settings::put_appearance))
        .route("/settings/srs", put(settings::put_srs))
        .route("/lsp/{id}", get(routes::lsp))
        .route_layer(middleware::from_fn_with_state(state.clone(), auth::require));
    let api = Router::new()
        .route("/health", get(routes::health))
        .route("/auth/session", get(auth::session))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        // the calendar feed: public, but only with its long random token
        .route("/feed/{file}", get(feed::ics))
        .merge(protected)
        .with_state(state)
        .layer(middleware::from_fn(system::record));
    system::start();
    let mut router = Router::new().nest("/api", api);
    // The CLI for other computers: public, because it is fetched before there is anything to sign in with (docs/CLI_INSTALL.md).
    router = router.route("/install.sh", get(install_script));
    if let Some(dir) = std::env::var_os("ANNEAL_DOWNLOADS").map(std::path::PathBuf::from).filter(|d| d.is_dir()) {
        router = router.merge(Router::new().nest_service("/downloads", ServeDir::new(dir)).layer(middleware::from_fn(revalidate)));
    }
    // What an installed CLI asks to update itself and its course files (also public; see cli_updates).
    router = router.merge(cli_updates::router(
        Some(std::env::var_os("ANNEAL_COURSES").map(std::path::PathBuf::from).unwrap_or_else(|| "courses".into())).filter(|d| d.is_dir()),
        std::env::var_os("ANNEAL_DOWNLOADS").map(std::path::PathBuf::from).filter(|d| d.is_dir()),
    ));
    if let Some(dist) = web_dist.filter(|d| d.join("index.html").is_file()) {
        // Vite names everything under /assets by content hash, so those files never change: browsers and
        // Cloudflare may keep them for a year without asking again. A missing one is a plain 404, never the
        // index.html fallback, so a stale page can't get HTML cached under an asset's name.
        let assets = Router::new()
            .nest_service("/assets", ServeDir::new(dist.join("assets")))
            .layer(middleware::from_fn(cache_for_a_year));
        // Client-side routes all load index.html, which must be revalidated so a deploy shows up at once.
        let pages = Router::new()
            .fallback_service(ServeDir::new(dist).fallback(ServeFile::new(dist.join("index.html"))))
            .layer(middleware::from_fn(revalidate));
        router = router.merge(assets).merge(pages);
    }
    // Large answers (the problem list is over a megabyte of JSON, the app itself over one of script) are sent compressed.
    router.layer(CompressionLayer::new()).layer(TraceLayer::new_for_http())
}

const INSTALL_SH: &str = include_str!("install.sh");

/// `GET /install.sh`: the installer, with the address it was fetched from filled in.
async fn install_script(headers: HeaderMap) -> impl IntoResponse {
    let header_text = |name: &str| headers.get(name).and_then(|v| v.to_str().ok()).map(|v| v.split(',').next().unwrap_or("").trim().to_owned());
    let host = header_text("x-forwarded-host")
        .or_else(|| header_text("host"))
        .filter(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':')))
        .unwrap_or_else(|| "localhost:8787".into());
    let local = host.starts_with("localhost") || host.starts_with("127.");
    let proto = header_text("x-forwarded-proto").filter(|p| p == "http" || p == "https").unwrap_or_else(|| if local { "http" } else { "https" }.into());
    ([(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")], INSTALL_SH.replace("__BASE__", &format!("{proto}://{host}")))
}

async fn cache_for_a_year(req: Request, next: Next) -> Response {
    with_cache_control(next.run(req).await, "public, max-age=31536000, immutable")
}

async fn revalidate(req: Request, next: Next) -> Response {
    with_cache_control(next.run(req).await, "no-cache")
}

fn with_cache_control(mut res: Response, value: &'static str) -> Response {
    if res.status().is_success() {
        res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(value));
    }
    res
}
