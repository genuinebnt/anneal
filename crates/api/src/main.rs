use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anneal_api::lsp::LspConfig;
use anneal_api::auth::AuthConfig;
use anneal_api::{AppState, MIGRATOR, app, preflight};
use anneal_content::Catalog;
use anneal_runner::{Runner, RunnerConfig, Sandbox};
use anyhow::{Context, bail};
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::EnvFilter;

/// Settings, all from the environment:
///
/// | variable | default |
/// |---|---|
/// | `ANNEAL_DATABASE_URL` | set in `.cargo/config.toml` for local development |
/// | `ANNEAL_CONTENT` | `content` |
/// | `ANNEAL_WEB_DIST` | `web/dist` |
/// | `ANNEAL_COURSES` | `courses` |
/// | `ANNEAL_SANDBOX` | `docker` (or `host`, for development without Docker) |
/// | `ANNEAL_RUNNER_IMAGE` | `anneal-runner:1.98` |
/// | `ANNEAL_DOCKER_CONTEXT` | `orbstack` (empty = the current Docker context) |
/// | `ANNEAL_RUST_ANALYZER` | `rust-analyzer` |
/// | `ANNEAL_ADDR` | `127.0.0.1:8787` |
/// | `ANNEAL_PASSPHRASE_HASH` | unset: no login, allowed only on a loopback address. Make one with `anneal passphrase` |
/// | `ANNEAL_COOKIE_SECURE` | `false`; set `true` when served over HTTPS |
/// | `ANNEAL_AI_PROVIDER`, `ANNEAL_AI_KEY`, `ANNEAL_AI_MODEL`, `ANNEAL_AI_URL` | unset: no AI help. See `anneal_api::ai` |
/// | `ANNEAL_MODEL` | unset: no search by meaning text. The bge-small-en-v1.5 directory (`tools/fetch-model.sh`) |
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("anneal_api=info,tower_http=info")),
        )
        .init();

    let env = |k: &str, d: &str| std::env::var(k).unwrap_or_else(|_| d.to_owned());
    let content = PathBuf::from(env("ANNEAL_CONTENT", "content"));
    let loaded =
        Catalog::load(&content).with_context(|| format!("loading {}", content.display()))?;
    for issue in &loaded.issues {
        tracing::warn!(%issue, "content issue");
    }
    let known = anneal_runner::known_crates();
    for p in loaded.catalog.tracks.iter().flat_map(|t| &t.problems) {
        if let Some(c) = p.meta.crates.iter().find(|c| !known.contains(&c.as_str())) {
            bail!("{}: crate {c:?} isn't in the sandbox's crate set", p.id);
        }
    }
    if !loaded.issues.is_empty() {
        bail!(
            "{} content issues; run `anneal validate`",
            loaded.issues.len()
        );
    }

    let sandbox = match env("ANNEAL_SANDBOX", "docker").as_str() {
        "docker" => {
            Sandbox::docker(env("ANNEAL_RUNNER_IMAGE", "anneal-runner:1.98")).in_context(&env(
                "ANNEAL_DOCKER_CONTEXT",
                anneal_runner::DEFAULT_DOCKER_CONTEXT,
            ))
        }
        "host" => Sandbox::Host,
        other => bail!("ANNEAL_SANDBOX must be docker or host, not {other:?}"),
    };
    let work_root = anneal_runner::default_work_root();
    let runner = Runner::new(RunnerConfig::new(sandbox, &work_root));
    let lsp = LspConfig::new(env("ANNEAL_RUST_ANALYZER", "rust-analyzer"), &work_root);

    let url = std::env::var("ANNEAL_DATABASE_URL").context("ANNEAL_DATABASE_URL is not set")?;
    let db = PgPoolOptions::new()
        .max_connections(8)
        .connect(&url)
        .await
        .context("connecting to Postgres")?;
    // `anneal-api preflight`: deploy/deploy.sh runs this with the new image before switching over. It changes nothing.
    if std::env::args().nth(1).as_deref() == Some("preflight") {
        let report = preflight::check(&db, &loaded.catalog, &MIGRATOR).await.context("preflight")?;
        for (id, rows) in &report.stranded {
            eprintln!("preflight: {rows} progress rows for {id}, which this version doesn't know; add it to the problem's renamed_from");
        }
        for m in &report.migrations {
            eprintln!("preflight: migration {m}; add a new migration instead");
        }
        if !report.is_ok() {
            bail!("preflight failed: deploying this version would lose progress or fail to start");
        }
        println!("preflight: ok ({} tracks, {} renames)", loaded.catalog.tracks.len(), loaded.catalog.renames().len());
        return Ok(());
    }
    MIGRATOR.run(&db).await.context("running migrations")?;
    let purged = preflight::purge_retired(&db, &loaded.catalog).await.context("deleting progress of retired problems")?;
    if purged > 0 {
        tracing::info!(purged, "deleted progress rows of retired problems");
    }
    let moved = preflight::apply_renames(&db, &loaded.catalog).await.context("moving progress to renamed problems")?;
    if moved > 0 {
        tracing::info!(moved, "moved progress rows from old problem ids");
    }

    let addr: SocketAddr = env("ANNEAL_ADDR", "127.0.0.1:8787")
        .parse()
        .context("ANNEAL_ADDR")?;
    let auth = match std::env::var("ANNEAL_PASSPHRASE_HASH").ok().filter(|h| !h.is_empty()) {
        Some(hash) => AuthConfig::new(&hash, env("ANNEAL_COOKIE_SECURE", "false") == "true")
            .map_err(|e| anyhow::anyhow!("ANNEAL_PASSPHRASE_HASH is not an argon2 hash: {e}"))?,
        None if addr.ip().is_loopback() => {
            tracing::warn!("ANNEAL_PASSPHRASE_HASH is not set: no login required (loopback only)");
            AuthConfig::disabled()
        }
        None => bail!("set ANNEAL_PASSPHRASE_HASH (from `anneal passphrase`) before listening on {addr}"),
    };

    // Courses (courses/<id>/course.toml) are optional: a deploy without them just has no Courses section.
    let courses_dir = PathBuf::from(env("ANNEAL_COURSES", "courses"));
    let mut courses = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&courses_dir) {
        let mut dirs: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.join("course.toml").is_file()).collect();
        dirs.sort();
        for d in dirs {
            let course = anneal_content::course::Course::load(&d).with_context(|| format!("loading course {}", d.display()))?;
            // the solutions the course ships with (courses/<id>/solutions.json), so they need no manual upload
            match anneal_api::course::seed_solutions(&db, &course, &d).await {
                Ok(0) => {}
                Ok(n) => tracing::info!(course = %course.id, written = n, "loaded the shipped solutions"),
                Err(e) => tracing::warn!(course = %course.id, "could not load the shipped solutions: {e:#}"),
            }
            courses.push(course);
        }
    }
    tracing::info!(courses = courses.len(), stages = courses.iter().map(|c| c.stages().count()).sum::<usize>(), "courses loaded");

    let web_dist = PathBuf::from(env("ANNEAL_WEB_DIST", "web/dist"));
    let meaning = anneal_api::meaning::Meaning::start(&loaded.catalog, &content, std::env::var_os("ANNEAL_MODEL").map(PathBuf::from).filter(|d| d.is_dir()));
    let state = AppState {
        catalog: Arc::new(loaded.catalog),
        runner: Arc::new(runner),
        db,
        lsp,
        auth,
        courses: Arc::new(courses),
        meaning,
        ai: anneal_api::ai::AiConfig::from_env(),
    };
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, sandbox = ?state.runner.config().sandbox, login = state.auth.required(), "anneal is listening");
    axum::serve(listener, app(state, Some(&web_dist))).await?;
    Ok(())
}
