//! BLOCK 10 (part): streaming multipart upload and presigned download.
//!
//! The upload never holds the whole file in memory (with the disk store): chunks go straight to the
//! writer while we count, hash and sniff. The object is written BEFORE the database row, so a row never
//! points at a missing object; if the transaction fails we delete the object. A crash in between leaves
//! an orphan object, which production systems sweep with a periodic job or bucket lifecycle rule.
//! Recipe: `examples/web/multipart_forms_uploads.rs`.

use axum::Json;
use axum::extract::multipart::{Field, Multipart, MultipartError, MultipartRejection};
use axum::extract::{Extension, Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::{record, tasks};
use crate::auth::policy::{Action, Principal};
use crate::db::tasks::Attachment;
use crate::error::{AppError, AppResult};
use crate::http::extract::QueryPairs;
use crate::objstore::{ObjectWriter, StoreError, sanitize_filename, sniff};
use crate::state::{App, AppState};

/// How many leading bytes the content sniffer looks at.
const SNIFF_LEN: usize = 512;

fn store_error(e: StoreError) -> AppError {
    match e {
        StoreError::NotFound => AppError::NotFound("file"),
        other => AppError::internal(other),
    }
}

fn multipart_error(e: &MultipartError) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::PayloadTooLarge
    } else {
        AppError::BadRequest("malformed multipart body".into())
    }
}

struct Received {
    content_type: &'static str,
    size: u64,
    sha256: String,
}

fn sniffed(head: &[u8]) -> AppResult<&'static str> {
    // The DECLARED content type is ignored: it is whatever the client says.
    sniff(head).ok_or_else(|| AppError::UnsupportedMediaType("file type is not allowed".into()))
}

async fn copy_field(field: &mut Field<'_>, writer: &mut dyn ObjectWriter, limit: u64) -> AppResult<Received> {
    let mut head: Vec<u8> = Vec::with_capacity(SNIFF_LEN);
    let mut content_type = None;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    while let Some(chunk) = field.chunk().await.map_err(|e| multipart_error(&e))? {
        size += chunk.len() as u64;
        if size > limit {
            return Err(AppError::PayloadTooLarge);
        }
        hasher.update(&chunk);
        let wanted = SNIFF_LEN - head.len();
        head.extend_from_slice(&chunk[..chunk.len().min(wanted)]);
        if content_type.is_none() && head.len() == SNIFF_LEN {
            content_type = Some(sniffed(&head)?); // reject early, before storing megabytes
        }
        writer.write(chunk).await.map_err(store_error)?;
    }
    let content_type = match content_type {
        Some(ct) => ct,
        None => sniffed(&head)?,
    };
    Ok(Received { content_type, size, sha256: hex::encode(hasher.finalize()) })
}

async fn store_field(app: &App, key: &str, field: &mut Field<'_>) -> AppResult<Received> {
    let mut writer = app.deps.store.begin_put(key).await.map_err(store_error)?;
    match copy_field(field, writer.as_mut(), app.cfg.server.upload_limit_bytes as u64).await {
        Ok(received) => {
            writer.finish().await.map_err(store_error)?;
            Ok(received)
        }
        Err(e) => {
            writer.abort().await;
            Err(e)
        }
    }
}

pub async fn upload(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(task_id): Path<String>,
    multipart: Result<Multipart, MultipartRejection>,
) -> AppResult<impl IntoResponse> {
    // Authorize before reading a single byte of the body.
    tasks::load(&app, &p, &task_id).await?;
    p.require(Action::Create)?;
    let mut multipart =
        multipart.map_err(|_| AppError::UnsupportedMediaType("expected multipart/form-data".into()))?;
    let mut field = loop {
        match multipart.next_field().await.map_err(|e| multipart_error(&e))? {
            Some(f) if f.name() == Some("file") => break f,
            Some(_) => {}
            None => return Err(AppError::BadRequest("missing form field 'file'".into())),
        }
    };
    let filename = sanitize_filename(field.file_name().unwrap_or(""));
    let id = app.new_id();
    // The key is built from our own ids only; the client's filename never touches the storage path.
    let key = format!("{}/{id}", p.org_id);
    let received = store_field(&app, &key, &mut field).await?;
    let attachment = Attachment {
        id,
        org_id: p.org_id.clone(),
        task_id,
        object_key: key.clone(),
        filename,
        content_type: received.content_type.to_string(),
        size: i64::try_from(received.size).unwrap_or(i64::MAX),
        sha256: received.sha256,
        created_at: app.now_ms(),
    };
    if let Err(e) = save(&app, &p, &attachment).await {
        // Compensate: do not leave an object that no row refers to.
        let _ = app.deps.store.delete(&key).await;
        return Err(e);
    }
    Ok((StatusCode::CREATED, Json(attachment)))
}

async fn save(app: &App, p: &Principal, a: &Attachment) -> AppResult<()> {
    let mut tx = app.db.begin().await?;
    crate::db::tasks::insert_attachment(&mut tx, a).await?;
    let extra = json!({ "task_id": a.task_id });
    record(&mut tx, p, "attachment.created", "attachment", &a.id, extra, a.created_at).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn list(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(task_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    p.require(Action::Read)?;
    tasks::load(&app, &p, &task_id).await?;
    let mut c = app.db.pool().acquire().await?;
    Ok(Json(json!({ "items": crate::db::tasks::attachments(&mut c, &p.org_id, &task_id).await? })))
}

/// Mint a short-lived URL that works without credentials (for `<img src>`, CDNs, CLI downloads).
pub async fn presign(
    State(app): State<AppState>,
    Extension(p): Extension<Principal>,
    Path(id): Path<String>,
) -> AppResult<impl IntoResponse> {
    p.require(Action::Read)?;
    let mut c = app.db.pool().acquire().await?;
    let a = crate::db::tasks::attachment(&mut c, &p.org_id, &id).await?.ok_or(AppError::NotFound("attachment"))?;
    drop(c);
    // An attachment dies with its task.
    tasks::load(&app, &p, &a.task_id).await?;
    let expires = app.now_ms() + app.cfg.storage.download_ttl_secs * 1000;
    let signature = app.presigner.sign(&a.object_key, expires);
    let url = format!("/v1/files/{}?exp={expires}&sig={signature}", a.object_key);
    Ok(Json(json!({ "url": url, "expires_at": expires })))
}

/// Public route: the signature IS the authorization. Anyone holding the URL can use it until it expires.
pub async fn download(
    State(app): State<AppState>,
    Path(key): Path<String>,
    QueryPairs(query): QueryPairs,
) -> AppResult<Response> {
    let param = |name: &str| query.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let exp = param("exp").and_then(|v| v.parse::<i64>().ok());
    let valid = match (exp, param("sig")) {
        (Some(exp), Some(sig)) => app.presigner.verify(&key, exp, sig, app.now_ms()),
        _ => false,
    };
    if !valid {
        return Err(AppError::Forbidden);
    }
    let meta: Option<(String, String)> =
        sqlx::query_as("SELECT content_type, filename FROM attachments WHERE object_key = ?")
            .bind(&key)
            .fetch_optional(app.db.pool())
            .await?;
    let (content_type, filename) = meta.ok_or(AppError::NotFound("file"))?;
    let bytes = app.deps.store.get(&key).await.map_err(store_error)?;
    // `attachment` + nosniff: uploaded content is never rendered inline on our origin.
    let headers = [
        (header::CONTENT_TYPE, content_type),
        (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\"")),
    ];
    Ok((headers, bytes).into_response())
}
