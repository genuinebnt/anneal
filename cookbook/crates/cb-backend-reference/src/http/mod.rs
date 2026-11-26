//! BLOCK 3: the HTTP layer. Routes are DATA (`routes()`): the router and the OpenAPI document are both
//! built from that one table, so a route cannot exist without being documented.

pub mod etag;
pub mod extract;
pub mod handlers;
pub mod middleware;
pub mod openapi;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::http::{Method, StatusCode};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, delete, get, patch, post};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::compression::CompressionLayer;

use crate::error::AppError;
use crate::state::AppState;
use handlers::{accounts, events, files, ops, projects, tasks};

/// Room for multipart boundaries and part headers on top of the file itself.
const MULTIPART_OVERHEAD: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Public,
    /// Needs a bearer credential; rate limited.
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    None,
    /// Name of a schema in the OpenAPI components.
    Json(&'static str),
    Multipart,
}

pub struct RouteDef {
    pub method: Method,
    pub path: &'static str,
    pub summary: &'static str,
    pub access: Access,
    pub status: StatusCode,
    pub body: Body,
    pub idempotent: bool,
    /// Success responses are JSON, so the `Accept` header is enforced.
    pub json_response: bool,
    pub query: &'static [&'static str],
    handler: MethodRouter<AppState>,
}

fn route(method: Method, path: &'static str, summary: &'static str, handler: MethodRouter<AppState>) -> RouteDef {
    RouteDef {
        method,
        path,
        summary,
        access: Access::User,
        status: StatusCode::OK,
        body: Body::None,
        idempotent: false,
        json_response: true,
        query: &[],
        handler,
    }
}

impl RouteDef {
    fn public(mut self) -> Self {
        self.access = Access::Public;
        self
    }
    fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        if status == StatusCode::NO_CONTENT {
            self.json_response = false;
        }
        self
    }
    fn takes(mut self, schema: &'static str) -> Self {
        self.body = Body::Json(schema);
        self
    }
    fn multipart(mut self) -> Self {
        self.body = Body::Multipart;
        self
    }
    fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }
    fn not_json(mut self) -> Self {
        self.json_response = false;
        self
    }
    fn query(mut self, names: &'static [&'static str]) -> Self {
        self.query = names;
        self
    }
}

const PAGE_QUERY: &[&str] = &["cursor", "limit"];
const TASK_QUERY: &[&str] = &["status", "assignee", "project_id", "priority_min", "q", "sort", "limit", "cursor"];

fn account_routes() -> Vec<RouteDef> {
    use Method as M;
    const CREATED: StatusCode = StatusCode::CREATED;
    const EMPTY: StatusCode = StatusCode::NO_CONTENT;
    vec![
        route(M::POST, "/v1/auth/register", "Create an organisation and its owner", post(accounts::register))
            .public()
            .status(CREATED)
            .takes("Register"),
        route(M::POST, "/v1/auth/verify", "Verify an email address", post(accounts::verify))
            .public()
            .status(EMPTY)
            .takes("VerifyEmail"),
        route(M::POST, "/v1/auth/login", "Log in with email and password", post(accounts::login))
            .public()
            .takes("Login"),
        route(M::POST, "/v1/auth/refresh", "Rotate a refresh token", post(accounts::refresh))
            .public()
            .takes("Refresh"),
        route(M::POST, "/v1/auth/logout", "Revoke the current session", post(accounts::logout)).status(EMPTY),
        route(M::GET, "/v1/me", "Who am I", get(accounts::me)),
        route(M::POST, "/v1/members", "Add a member", post(accounts::add_member))
            .status(CREATED)
            .takes("NewMember")
            .idempotent(),
        route(M::POST, "/v1/api-keys", "Create an API key (shown once)", post(accounts::create_api_key))
            .status(CREATED)
            .takes("NewApiKey"),
        route(M::DELETE, "/v1/api-keys/{id}", "Revoke an API key", delete(accounts::revoke_api_key)).status(EMPTY),
        route(M::POST, "/v1/webhooks", "Register a webhook (secret shown once)", post(accounts::create_webhook))
            .status(CREATED)
            .takes("NewWebhook"),
        route(M::GET, "/v1/webhooks", "List webhooks", get(accounts::list_webhooks)),
        route(M::GET, "/v1/audit", "Read the audit log", get(accounts::audit_log)),
    ]
}

fn project_routes() -> Vec<RouteDef> {
    use Method as M;
    const CREATED: StatusCode = StatusCode::CREATED;
    const EMPTY: StatusCode = StatusCode::NO_CONTENT;
    vec![
        route(M::POST, "/v1/projects", "Create a project", post(projects::create))
            .status(CREATED)
            .takes("NewProject")
            .idempotent(),
        route(M::GET, "/v1/projects", "List projects", get(projects::list)).query(PAGE_QUERY),
        route(M::GET, "/v1/projects/{id}", "Get a project", get(projects::get)),
        route(M::PATCH, "/v1/projects/{id}", "Update a project (If-Match)", patch(projects::update))
            .takes("PatchProject"),
        route(
            M::DELETE,
            "/v1/projects/{id}",
            "Soft delete a project and its tasks (If-Match)",
            delete(projects::delete),
        )
        .status(EMPTY),
        route(M::GET, "/v1/projects/{id}/stats", "Task counts by status (cached)", get(projects::stats)),
        route(M::POST, "/v1/projects/{id}/tasks", "Create a task", post(tasks::create))
            .status(CREATED)
            .takes("NewTask")
            .idempotent(),
        route(M::GET, "/v1/projects/{id}/tasks", "List tasks of a project", get(tasks::list_in_project))
            .query(TASK_QUERY),
        route(M::GET, "/v1/tasks", "Search tasks", get(tasks::search)).query(TASK_QUERY),
        route(M::GET, "/v1/tasks/{id}", "Get a task", get(tasks::get)),
        route(M::PATCH, "/v1/tasks/{id}", "Update a task (If-Match)", patch(tasks::update)).takes("PatchTask"),
        route(M::DELETE, "/v1/tasks/{id}", "Soft delete a task", delete(tasks::delete)).status(EMPTY),
        route(M::POST, "/v1/tasks/{id}/comments", "Comment on a task", post(tasks::add_comment))
            .status(CREATED)
            .takes("NewComment")
            .idempotent(),
        route(M::GET, "/v1/tasks/{id}/comments", "List comments", get(tasks::comments)),
        route(M::POST, "/v1/tasks/{id}/attachments", "Upload a file", post(files::upload))
            .status(CREATED)
            .multipart(),
        route(M::GET, "/v1/tasks/{id}/attachments", "List attachments", get(files::list)),
        route(M::GET, "/v1/attachments/{id}/url", "Get a presigned download URL", get(files::presign)),
        route(M::GET, "/v1/files/{*key}", "Download with a presigned URL", get(files::download))
            .public()
            .not_json()
            .query(&["exp", "sig"]),
        route(M::GET, "/v1/events", "Server-sent events (Last-Event-ID)", get(events::stream)).not_json(),
    ]
}

fn ops_routes(debug: bool) -> Vec<RouteDef> {
    use Method as M;
    let mut list = vec![
        route(M::GET, "/healthz", "Liveness", get(ops::healthz)).public(),
        route(M::GET, "/readyz", "Readiness", get(ops::readyz)).public(),
        route(M::GET, "/metrics", "Prometheus metrics", get(ops::metrics)).public().not_json(),
        route(M::GET, "/openapi.json", "This document", get(openapi::serve)).public(),
    ];
    if debug {
        list.push(route(M::GET, "/debug/config", "Redacted configuration", get(ops::debug_config)).public());
    }
    list
}

/// Every route of the application.
#[must_use]
pub fn routes(debug: bool) -> Vec<RouteDef> {
    let mut all = account_routes();
    all.extend(project_routes());
    all.extend(ops_routes(debug));
    all
}

/// Wrap one route in the layers its definition asks for. The LAST layer added runs FIRST.
fn assemble(def: RouteDef, app: &AppState) -> MethodRouter<AppState> {
    let mut handler = def.handler;
    if def.body == Body::Multipart {
        handler = handler.route_layer(DefaultBodyLimit::max(app.cfg.server.upload_limit_bytes + MULTIPART_OVERHEAD));
    }
    if def.idempotent {
        // Inside authentication: it needs the principal.
        handler = handler.route_layer(from_fn_with_state(app.clone(), crate::idempotency::middleware));
    }
    if def.access == Access::User {
        handler = handler.route_layer(from_fn_with_state(app.clone(), middleware::authenticate));
    }
    if def.json_response {
        handler = handler.route_layer(from_fn(middleware::require_json_accept));
    }
    handler
}

async fn not_found() -> Response {
    AppError::NotFound("route").into_response()
}

async fn method_not_allowed() -> Response {
    let body = r#"{"type":"about:blank","title":"Method Not Allowed","status":405}"#;
    let headers = [(axum::http::header::CONTENT_TYPE, "application/problem+json")];
    (StatusCode::METHOD_NOT_ALLOWED, headers, body).into_response()
}

/// The complete application as a `tower::Service`.
pub fn router(app: AppState) -> Router {
    let mut router = Router::new();
    for def in routes(app.cfg.debug_routes) {
        let path = def.path;
        router = router.route(path, assemble(def, &app));
    }
    // Each `.layer` wraps the ones before it, so read this list from the BOTTOM to follow a request.
    router
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .layer(DefaultBodyLimit::max(app.cfg.server.body_limit_bytes))
        .layer(from_fn_with_state(app.clone(), middleware::timeout))
        .layer(CatchPanicLayer::custom(middleware::panic_response))
        .layer(CompressionLayer::new())
        .layer(middleware::cors(&app.cfg))
        .layer(from_fn_with_state(app.clone(), middleware::security_headers))
        .layer(from_fn_with_state(app.clone(), middleware::observe))
        .with_state(app)
}
