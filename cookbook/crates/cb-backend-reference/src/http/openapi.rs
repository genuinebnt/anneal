//! BLOCK 14: the OpenAPI 3.1 document, generated from the route table.
//!
//! WHY generated from `routes()` and not hand-listed: a second list drifts. What IS hand-written here are
//! the request schemas; response schemas are not described.
//! Production alternatives: `utoipa` or `aide` (derive schemas from the Rust types).

use axum::Json;
use axum::extract::State;
use serde_json::{Map, Value, json};

use super::{Access, Body, RouteDef, routes};
use crate::state::AppState;

fn object(required: &[&str], optional: &[&str]) -> Value {
    let properties: Map<String, Value> = required
        .iter()
        .chain(optional)
        .map(|name| {
            let kind = if *name == "priority" { "integer" } else { "string" };
            (name.to_string(), json!({ "type": kind }))
        })
        .collect();
    json!({ "type": "object", "additionalProperties": false, "required": required, "properties": properties })
}

fn schemas() -> Value {
    json!({
        "Register": object(&["org_name", "email", "password"], &[]),
        "VerifyEmail": object(&["token"], &[]),
        "Login": object(&["email", "password"], &[]),
        "Refresh": object(&["refresh_token"], &[]),
        "NewMember": object(&["email", "password", "role"], &[]),
        "NewApiKey": object(&["name"], &[]),
        "NewWebhook": object(&["url"], &[]),
        "NewProject": object(&["name"], &["description"]),
        "PatchProject": object(&[], &["name", "description"]),
        "NewTask": object(&["title"], &["body", "priority", "assignee_id"]),
        "PatchTask": object(&[], &["title", "body", "status", "priority", "assignee_id"]),
        "NewComment": object(&["body"], &[]),
        "Problem": object(&["type", "title", "status"], &["detail", "request_id"]),
    })
}

fn parameter(name: &str, location: &str, required: bool) -> Value {
    json!({ "name": name, "in": location, "required": required, "schema": { "type": "string" } })
}

/// `/v1/files/{*key}` is axum syntax; OpenAPI writes `{key}`.
#[must_use]
pub fn openapi_path(path: &str) -> String {
    path.replace("{*", "{")
}

fn parameters(r: &RouteDef) -> Vec<Value> {
    let in_path = openapi_path(r.path);
    let mut out: Vec<Value> = in_path
        .split('{')
        .skip(1)
        .filter_map(|s| s.split_once('}'))
        .map(|(name, _)| parameter(name, "path", true))
        .collect();
    out.extend(r.query.iter().map(|q| parameter(q, "query", false)));
    let conditional_write = r.summary.contains("(If-Match)");
    if conditional_write {
        out.push(parameter("If-Match", "header", true));
    }
    if r.idempotent {
        out.push(parameter("Idempotency-Key", "header", false));
    }
    out
}

fn operation(r: &RouteDef) -> Value {
    let mut op = json!({
        "summary": r.summary,
        "parameters": parameters(r),
        "responses": {
            r.status.as_str(): { "description": "success" },
            "default": {
                "description": "error",
                "content": { "application/problem+json": { "schema": { "$ref": "#/components/schemas/Problem" } } }
            }
        }
    });
    if r.access == Access::User {
        op["security"] = json!([{ "bearer": [] }]);
    }
    match r.body {
        Body::None => {}
        Body::Json(name) => {
            let schema = json!({ "$ref": format!("#/components/schemas/{name}") });
            op["requestBody"] = json!({ "required": true, "content": { "application/json": { "schema": schema } } });
        }
        Body::Multipart => {
            let schema =
                json!({ "type": "object", "properties": { "file": { "type": "string", "format": "binary" } } });
            op["requestBody"] =
                json!({ "required": true, "content": { "multipart/form-data": { "schema": schema } } });
        }
    }
    op
}

#[must_use]
pub fn document(debug: bool) -> Value {
    let mut paths = Map::new();
    for r in routes(debug) {
        let item = paths.entry(openapi_path(r.path)).or_insert_with(|| json!({}));
        item[r.method.as_str().to_lowercase()] = operation(&r);
    }
    json!({
        "openapi": "3.1.0",
        "info": { "title": "Reference backend", "version": env!("CARGO_PKG_VERSION") },
        "paths": paths,
        "components": {
            "schemas": schemas(),
            "securitySchemes": { "bearer": { "type": "http", "scheme": "bearer" } }
        }
    })
}

pub async fn serve(State(app): State<AppState>) -> Json<Value> {
    Json(document(app.cfg.debug_routes))
}
