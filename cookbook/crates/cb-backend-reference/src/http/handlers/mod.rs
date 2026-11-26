//! Handlers: parse, authorize, run ONE unit of work, respond. SQL stays in `db`, rules in `auth`.

pub mod accounts;
pub mod events;
pub mod files;
pub mod ops;
pub mod projects;
pub mod tasks;

use serde_json::{Value, json};
use sqlx::SqliteConnection;

use crate::auth::policy::Principal;
use crate::db::{Actor, audit, outbox_push};

/// The two records that must commit or roll back together with every domain write: who did it (audit)
/// and what happened (outbox event). Call with the SAME transaction as the write.
pub async fn record(
    c: &mut SqliteConnection,
    p: &Principal,
    topic: &str,
    entity: &str,
    id: &str,
    extra: Value,
    now: i64,
) -> Result<(), sqlx::Error> {
    audit(c, Actor { org_id: &p.org_id, user_id: &p.user_id }, topic, entity, id, now, &extra).await?;
    let mut payload = json!({ "id": id });
    if let (Some(target), Value::Object(more)) = (payload.as_object_mut(), extra) {
        target.extend(more);
    }
    outbox_push(c, &p.org_id, topic, &payload, now).await
}
