//! BLOCK 2 / 15: a ready-made application with fakes, driven in process through `tower::ServiceExt`.
//! Used by the integration tests and the demo. Public so exercises in `docs/EXTEND.md` can reuse it.
//! It panics on unexpected failures, which is right for tests and wrong for production code.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{HeaderMap, Method, Request};
use bytes::Bytes;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::config::Config;
use crate::db::Db;
use crate::http::router;
use crate::jobs::worker::drain_due;
use crate::objstore::{MemoryStore, ObjectStore};
use crate::ports::{ManualClock, MemoryMailer, UuidIds};
use crate::state::{App, AppState, Deps};

pub const START_MS: i64 = 1_700_000_000_000;
pub const PASSWORD: &str = "correct-horse-battery";

/// Cheap hashing and generous limits. Individual tests tighten what they are testing.
#[must_use]
pub fn test_config() -> Config {
    let mut cfg = Config::default();
    cfg.auth.argon2_m_kib = 64;
    cfg.auth.argon2_t = 1;
    cfg.rate_limit.capacity = 10_000;
    cfg.rate_limit.refill_per_sec = 1_000.0;
    cfg.jobs.poll_ms = 5;
    cfg
}

pub struct Req {
    method: Method,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Req {
    #[must_use]
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self { method, path: path.into(), headers: vec![], body: vec![] }
    }
    #[must_use]
    pub fn get(path: impl Into<String>) -> Self {
        Self::new(Method::GET, path)
    }
    #[must_use]
    pub fn post(path: impl Into<String>) -> Self {
        Self::new(Method::POST, path)
    }
    #[must_use]
    pub fn patch(path: impl Into<String>) -> Self {
        Self::new(Method::PATCH, path)
    }
    #[must_use]
    pub fn delete(path: impl Into<String>) -> Self {
        Self::new(Method::DELETE, path)
    }
    #[must_use]
    pub fn header(mut self, name: &str, value: impl Into<String>) -> Self {
        self.headers.push((name.to_string(), value.into()));
        self
    }
    #[must_use]
    pub fn auth(self, token: &str) -> Self {
        self.header("authorization", format!("Bearer {token}"))
    }
    #[must_use]
    pub fn if_match(self, version: i64) -> Self {
        self.header("if-match", format!("\"v{version}\""))
    }
    #[must_use]
    pub fn json(mut self, value: &Value) -> Self {
        self.body = value.to_string().into_bytes();
        self.header("content-type", "application/json")
    }
    #[must_use]
    pub fn raw(mut self, content_type: &str, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self.header("content-type", content_type)
    }
    /// A multipart body with a single `file` field. The declared part type is deliberately wrong
    /// (`text/html`): the server must not believe it.
    #[must_use]
    pub fn file(self, filename: &str, content: &[u8]) -> Self {
        const BOUNDARY: &str = "testkit-boundary-7d9f";
        let mut body = format!(
            "--{BOUNDARY}\r\ncontent-disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
             content-type: text/html\r\n\r\n"
        )
        .into_bytes();
        body.extend_from_slice(content);
        body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
        self.raw(&format!("multipart/form-data; boundary={BOUNDARY}"), body)
    }
}

#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Reply {
    #[must_use]
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|e| panic!("not JSON ({e}): {self:?}"))
    }
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }
    /// Panic with the whole response when the status is not the expected one.
    #[track_caller]
    pub fn expect(self, status: u16) -> Self {
        assert_eq!(self.status, status, "unexpected status: {self:?}");
        self
    }
    /// A string field of the JSON body.
    #[must_use]
    #[track_caller]
    pub fn str(&self, field: &str) -> String {
        let body = self.json();
        body[field].as_str().unwrap_or_else(|| panic!("no string {field:?} in {body}")).to_string()
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub user_id: String,
    pub org_id: String,
    pub email: String,
    pub access: String,
    pub refresh: String,
}

pub struct TestApp {
    pub app: AppState,
    pub router: Router,
    pub clock: Arc<ManualClock>,
    pub mailer: Arc<MemoryMailer>,
    pub store: MemoryStore,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with(|_| {}).await
    }

    pub async fn with(tweak: impl FnOnce(&mut Config)) -> Self {
        let store = MemoryStore::default();
        Self::build(tweak, Arc::new(store.clone()), store).await
    }

    /// Substitute the object store (to inject delays or faults).
    pub async fn with_store(tweak: impl FnOnce(&mut Config), custom: Arc<dyn ObjectStore>) -> Self {
        Self::build(tweak, custom, MemoryStore::default()).await
    }

    async fn build(tweak: impl FnOnce(&mut Config), custom: Arc<dyn ObjectStore>, store: MemoryStore) -> Self {
        let mut cfg = test_config();
        tweak(&mut cfg);
        cfg.validate().expect("test config");
        let db = Db::open(&cfg.db).await.expect("database");
        let clock = Arc::new(ManualClock::new(START_MS));
        let mailer = Arc::new(MemoryMailer::default());
        let deps = Deps { clock: clock.clone(), ids: Arc::new(UuidIds), mailer: mailer.clone(), store: custom };
        let app = App::build(cfg, db, deps).expect("app");
        Self { router: router(app.clone()), app, clock, mailer, store }
    }

    pub async fn send(&self, req: Req) -> Reply {
        let mut builder = Request::builder().method(req.method).uri(req.path);
        for (k, v) in req.headers {
            builder = builder.header(k, v);
        }
        let request = builder.body(Body::from(req.body)).expect("request");
        let res = self.router.clone().oneshot(request).await.expect("infallible");
        let (parts, body) = res.into_parts();
        let body = to_bytes(body, 32 * 1024 * 1024).await.expect("body");
        Reply { status: parts.status.as_u16(), headers: parts.headers, body }
    }

    /// Do what the background tasks would do now: relay the outbox, then run every due job.
    pub async fn run_background(&self) -> usize {
        self.app.relay_events().await.expect("relay");
        drain_due(&self.app.job_context()).await.expect("jobs")
    }

    pub async fn login(&self, email: &str) -> Reply {
        self.send(Req::post("/v1/auth/login").json(&json!({ "email": email, "password": PASSWORD }))).await
    }

    async fn session(&self, email: &str) -> Session {
        let tokens = self.login(email).await.expect(200);
        let (access, refresh) = (tokens.str("access_token"), tokens.str("refresh_token"));
        let me = self.send(Req::get("/v1/me").auth(&access)).await.expect(200);
        Session { user_id: me.str("user_id"), org_id: me.str("org_id"), email: email.into(), access, refresh }
    }

    /// Register an organisation, click the emailed link, log in.
    pub async fn signup(&self, org: &str, email: &str) -> Session {
        let body = json!({ "org_name": org, "email": email, "password": PASSWORD });
        self.send(Req::post("/v1/auth/register").json(&body)).await.expect(201);
        self.run_background().await;
        let token = self.mailer.last_token_for(email).expect("verification mail");
        self.send(Req::post("/v1/auth/verify").json(&json!({ "token": token }))).await.expect(204);
        self.session(email).await
    }

    pub async fn member(&self, admin: &Session, email: &str, role: &str) -> Session {
        let body = json!({ "email": email, "password": PASSWORD, "role": role });
        self.send(Req::post("/v1/members").auth(&admin.access).json(&body)).await.expect(201);
        self.session(email).await
    }

    /// Returns the new project's id.
    pub async fn project(&self, s: &Session, name: &str) -> String {
        self.send(Req::post("/v1/projects").auth(&s.access).json(&json!({ "name": name })))
            .await
            .expect(201)
            .str("id")
    }

    pub async fn task(&self, s: &Session, project: &str, title: &str) -> String {
        let req = Req::post(format!("/v1/projects/{project}/tasks")).auth(&s.access).json(&json!({ "title": title }));
        self.send(req).await.expect(201).str("id")
    }

    /// `SELECT count(*) FROM <table>`, to check what really got committed.
    pub async fn count(&self, table: &'static str) -> i64 {
        // `table` is a compile-time literal, never input.
        let sql = format!("SELECT count(*) FROM {table}");
        sqlx::query_scalar(&sql).fetch_one(self.app.db.pool()).await.expect("count")
    }
}
