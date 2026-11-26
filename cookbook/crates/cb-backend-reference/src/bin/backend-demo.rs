//! `backend-demo`               scripted end-to-end session against an in-process server on 127.0.0.1:0
//! `backend-demo serve`         run the server (config: file named by `APP_CONFIG`, then `APP__*` env)
//! `backend-demo healthcheck`   exit 0 if the configured server is ready (used by the container)

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, bail, ensure};
use axum::body::Bytes;
use axum::http::HeaderMap;
use axum::routing::post;
use backend_ref::config::Config;
use backend_ref::db::Db;
use backend_ref::jobs::webhook::{SIGNATURE_HEADER, verify};
use backend_ref::objstore::MemoryStore;
use backend_ref::ports::{MemoryMailer, SystemClock, UuidIds};
use backend_ref::server::{init_tracing, serve, shutdown_signal};
use backend_ref::state::{App, Deps};
use reqwest::{Client, Method, RequestBuilder, StatusCode};
use serde_json::{Value, json};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    match std::env::args().nth(1).as_deref() {
        None => demo().await,
        Some("serve") => run_server().await,
        Some("healthcheck") => healthcheck().await,
        Some(other) => bail!("unknown command {other:?}; use nothing, `serve` or `healthcheck`"),
    }
}

fn load_config() -> anyhow::Result<Config> {
    let file = std::env::var_os("APP_CONFIG").map(PathBuf::from);
    Ok(Config::load(file.as_deref(), std::env::vars())?)
}

async fn run_server() -> anyhow::Result<()> {
    init_tracing();
    let cfg = load_config()?;
    tracing::info!(config = %cfg.redacted(), "starting");
    let db = Db::open(&cfg.db).await.context("opening the database")?;
    let deps = Deps::production(&cfg);
    let listener = TcpListener::bind(&cfg.server.bind).await?;
    let app = App::build(cfg, db, deps)?;
    let stop = app.shutdown.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown requested");
        stop.cancel();
    });
    serve(app, listener).await
}

async fn healthcheck() -> anyhow::Result<()> {
    let cfg = load_config()?;
    let port = cfg.server.bind.parse::<std::net::SocketAddr>()?.port();
    let client = Client::builder().timeout(Duration::from_secs(3)).no_proxy().build()?;
    let res = client.get(format!("http://127.0.0.1:{port}/readyz")).send().await?;
    ensure!(res.status().is_success(), "not ready: {}", res.status());
    Ok(())
}

/// Prints each exchange. Tokens and secrets are never printed.
struct Transcript {
    http: Client,
    base: String,
}

struct Answer {
    status: StatusCode,
    headers: reqwest::header::HeaderMap,
    body: Bytes,
}

impl Answer {
    fn json(&self) -> anyhow::Result<Value> {
        Ok(serde_json::from_slice(&self.body)?)
    }
    fn text(&self, field: &str) -> anyhow::Result<String> {
        let v = self.json()?;
        v[field].as_str().map(str::to_string).with_context(|| format!("no {field} in {v}"))
    }
}

fn redact(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                if k.contains("token") && v.is_string() && k != "token_type" || k == "secret" || k == "key" {
                    *v = json!("<redacted>");
                } else if k == "url" && v.as_str().is_some_and(|s| s.contains("sig=")) {
                    *v = json!("<presigned url>");
                } else {
                    redact(v);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(redact),
        _ => {}
    }
}

impl Transcript {
    fn req(&self, method: Method, path: &str) -> RequestBuilder {
        self.http.request(method, format!("{}{path}", self.base))
    }

    /// Send, print one line (plus a body excerpt) and insist on the expected status.
    async fn step(&self, title: &str, expected: u16, req: RequestBuilder) -> anyhow::Result<Answer> {
        let request = req.build()?;
        let (method, path) = (request.method().clone(), request.url().path().to_string());
        let res = self.http.execute(request).await?;
        let (status, headers) = (res.status(), res.headers().clone());
        let body = res.bytes().await?;
        println!("\n# {title}\n  {method} {path} -> {}", status.as_u16());
        for name in ["etag", "idempotency-replayed", "retry-after", "ratelimit-remaining"] {
            if let Some(v) = headers.get(name) {
                println!("  {name}: {}", v.to_str().unwrap_or("?"));
            }
        }
        if let Ok(mut json) = serde_json::from_slice::<Value>(&body) {
            redact(&mut json);
            let mut shown: String = json.to_string().chars().take(220).collect();
            if shown.len() < json.to_string().len() {
                shown.push_str("...");
            }
            println!("  {shown}");
        }
        ensure!(status.as_u16() == expected, "{title}: expected {expected}, got {status}: {body:?}");
        Ok(Answer { status, headers, body })
    }
}

type Deliveries = Arc<Mutex<Vec<(HeaderMap, Bytes)>>>;

/// A customer's webhook endpoint.
async fn receiver() -> anyhow::Result<(Deliveries, String)> {
    let seen: Deliveries = Arc::default();
    let sink = seen.clone();
    let app = axum::Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: Bytes| async move {
            sink.lock().unwrap().push((headers, body));
        }),
    );
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let url = format!("http://{}/hook", listener.local_addr()?);
    tokio::spawn(async move { axum::serve(listener, app).await });
    Ok((seen, url))
}

async fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> anyhow::Result<T> {
    let wait = async {
        loop {
            if let Some(v) = probe() {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    };
    tokio::time::timeout(Duration::from_secs(20), wait).await.with_context(|| format!("timed out waiting for {what}"))
}

async fn sign_up(t: &Transcript, mailer: &MemoryMailer) -> anyhow::Result<String> {
    let account = json!({ "org_name": "Acme", "email": "ann@acme.example", "password": "demo-passphrase-123" });
    t.step("register", 201, t.req(Method::POST, "/v1/auth/register").json(&account)).await?;
    let login = json!({ "email": "ann@acme.example", "password": "demo-passphrase-123" });
    t.step("login before verification is refused", 403, t.req(Method::POST, "/v1/auth/login").json(&login)).await?;
    let token = eventually("the verification mail", || mailer.last_token_for("ann@acme.example")).await?;
    println!("\n(a background worker sent the verification mail)");
    let verify = t.req(Method::POST, "/v1/auth/verify").json(&json!({ "token": token }));
    t.step("click the emailed link", 204, verify).await?;
    let tokens = t.step("login", 200, t.req(Method::POST, "/v1/auth/login").json(&login)).await?;
    let first_refresh = tokens.text("refresh_token")?;
    let rotate = || t.req(Method::POST, "/v1/auth/refresh").json(&json!({ "refresh_token": first_refresh }));
    t.step("rotate the refresh token", 200, rotate()).await?;
    t.step("reusing it revokes the whole session", 401, rotate()).await?;
    let again = t.step("log in again", 200, t.req(Method::POST, "/v1/auth/login").json(&login)).await?;
    again.text("access_token")
}

/// Returns the project id and the task id.
async fn work(t: &Transcript, token: &str) -> anyhow::Result<(String, String)> {
    let create = || {
        t.req(Method::POST, "/v1/projects")
            .bearer_auth(token)
            .header("idempotency-key", "demo-1")
            .json(&json!({ "name": "Launch" }))
    };
    let project = t.step("create a project", 201, create()).await?.text("id")?;
    let replay = t.step("retry with the same Idempotency-Key", 201, create()).await?;
    ensure!(replay.text("id")? == project && replay.headers.contains_key("idempotency-replayed"));

    let path = format!("/v1/projects/{project}");
    let read = t.step("read it", 200, t.req(Method::GET, &path).bearer_auth(token)).await?;
    let etag = read.headers.get("etag").context("no etag")?.clone();
    let revalidate = t.req(Method::GET, &path).bearer_auth(token).header("if-none-match", etag.clone());
    let unchanged = t.step("conditional GET", 304, revalidate).await?;
    ensure!(unchanged.body.is_empty() && unchanged.status == StatusCode::NOT_MODIFIED);
    let patch = |name: &str| {
        t.req(Method::PATCH, &path).bearer_auth(token).header("if-match", etag.clone()).json(&json!({ "name": name }))
    };
    t.step("update with If-Match", 200, patch("Launch v2")).await?;
    t.step("a second writer with the old version loses", 412, patch("clobber")).await?;

    let tasks = format!("{path}/tasks");
    let mut first = None;
    for title in ["Write launch checklist", "Fix login redirect", "Rehearse rollback"] {
        let req = t.req(Method::POST, &tasks).bearer_auth(token).json(&json!({ "title": title, "priority": 2 }));
        let id = t.step("add a task", 201, req).await?.text("id")?;
        first.get_or_insert(id);
    }
    let bad = t.req(Method::POST, &tasks).bearer_auth(token).json(&json!({ "title": "", "priority": 9 }));
    t.step("validation errors are problem+json", 422, bad).await?;
    let search = t.req(Method::GET, "/v1/tasks?q=logi").bearer_auth(token);
    t.step("full-text prefix search", 200, search).await?;
    let page = t.req(Method::GET, "/v1/tasks?sort=title&limit=2").bearer_auth(token);
    let cursor = t.step("keyset page 1", 200, page).await?.text("next_cursor")?;
    let next = t.req(Method::GET, &format!("/v1/tasks?sort=title&limit=2&cursor={cursor}")).bearer_auth(token);
    t.step("keyset page 2", 200, next).await?;
    let stats = t.req(Method::GET, &format!("{path}/stats")).bearer_auth(token);
    t.step("cached stats", 200, stats).await?;
    Ok((project, first.context("no task")?))
}

async fn files(t: &Transcript, token: &str, task: &str) -> anyhow::Result<()> {
    let boundary = "demo-boundary";
    let body = format!(
        "--{boundary}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"notes.txt\"\r\n\r\n\
         ship it\r\n--{boundary}--\r\n"
    );
    let upload = t
        .req(Method::POST, &format!("/v1/tasks/{task}/attachments"))
        .bearer_auth(token)
        .header("content-type", format!("multipart/form-data; boundary={boundary}"))
        .body(body);
    let id = t.step("upload an attachment", 201, upload).await?.text("id")?;
    let sign = t.req(Method::GET, &format!("/v1/attachments/{id}/url")).bearer_auth(token);
    let url = t.step("get a presigned URL", 200, sign).await?.text("url")?;
    let file = t.step("download without credentials", 200, t.req(Method::GET, &url)).await?;
    ensure!(&file.body[..] == b"ship it");
    Ok(())
}

async fn integrations(t: &Transcript, token: &str, project: &str) -> anyhow::Result<()> {
    let (deliveries, hook_url) = receiver().await?;
    let register = t.req(Method::POST, "/v1/webhooks").bearer_auth(token).json(&json!({ "url": hook_url }));
    let secret = t.step("register a webhook", 201, register).await?.text("secret")?;
    let key_req = t.req(Method::POST, "/v1/api-keys").bearer_auth(token).json(&json!({ "name": "ci" }));
    let key = t.step("create an API key", 201, key_req).await?.text("key")?;

    let mut stream = t.req(Method::GET, "/v1/events").bearer_auth(token).send().await?;
    let task = t
        .req(Method::POST, &format!("/v1/projects/{project}/tasks"))
        .bearer_auth(&key)
        .json(&json!({ "title": "Announce" }));
    let id = t.step("create a task using the API key", 201, task).await?.text("id")?;

    let frame = tokio::time::timeout(Duration::from_secs(20), async {
        let mut seen = String::new();
        while let Some(chunk) = stream.chunk().await? {
            seen.push_str(&String::from_utf8_lossy(&chunk));
            if let Some(f) = seen.split("\n\n").find(|f| f.contains(&id) && seen.ends_with("\n\n")) {
                return Ok(f.to_string());
            }
        }
        bail!("event stream ended early")
    })
    .await??;
    println!("\n# server-sent event\n  {}", frame.replace('\n', "\n  "));

    let (headers, body) = eventually("a webhook delivery for the new task", || {
        deliveries.lock().unwrap().iter().find(|(_, b)| String::from_utf8_lossy(b).contains(&id)).cloned()
    })
    .await?;
    let signature = headers.get(SIGNATURE_HEADER).context("unsigned")?.to_str()?;
    let now = chrono::Utc::now().timestamp();
    ensure!(verify(&secret, signature, &body, now, 300), "bad webhook signature");
    println!("\n# webhook received, signature verified\n  {}", String::from_utf8_lossy(&body));
    Ok(())
}

async fn demo() -> anyhow::Result<()> {
    if std::env::var_os("RUST_LOG").is_some() {
        init_tracing();
    }
    // Real server, real background workers; only the mailer and object store are in memory.
    let mut cfg = Config::default();
    cfg.jobs.poll_ms = 20;
    cfg.rate_limit.capacity = 200;
    let mailer = Arc::new(MemoryMailer::default());
    let deps = Deps {
        clock: Arc::new(SystemClock),
        ids: Arc::new(UuidIds),
        mailer: mailer.clone(),
        store: Arc::new(MemoryStore::default()),
    };
    let db = Db::open(&cfg.db).await?;
    let app = App::build(cfg, db, deps)?;
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let base = format!("http://{}", listener.local_addr()?);
    println!("server listening on {base}");
    let server = tokio::spawn(serve(app.clone(), listener));
    let t = Transcript { http: Client::builder().no_proxy().timeout(Duration::from_secs(60)).build()?, base };

    let token = sign_up(&t, &mailer).await?;
    let (project, task) = work(&t, &token).await?;
    files(&t, &token, &task).await?;
    integrations(&t, &token, &project).await?;
    t.step("readiness", 200, t.req(Method::GET, "/readyz")).await?;
    let metrics = t.step("metrics", 200, t.req(Method::GET, "/metrics")).await?;
    String::from_utf8_lossy(&metrics.body)
        .lines()
        .filter(|l| l.starts_with("jobs{") || l.contains("route=\"/v1/auth/login\",status"))
        .for_each(|l| println!("  {l}"));

    app.shutdown.cancel();
    server.await??;
    println!("\nshut down cleanly; all steps behaved as expected");
    Ok(())
}
