//! Project aggregate repository. Every query is scoped by `org_id` (tenant isolation) and hides
//! soft-deleted rows. Writes use optimistic locking: `WHERE version = ?` + `version = version + 1`.

use serde::Serialize;
use sqlx::SqliteConnection;

use super::Write;
use crate::error::{AppError, AppResult};
use crate::query::{Cursor, Page, SortValue};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub org_id: String,
    pub name: String,
    pub description: String,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip)]
    pub deleted_at: Option<i64>,
}

pub async fn insert(c: &mut SqliteConnection, p: &Project) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO projects (id, org_id, name, description, created_by, version, created_at, updated_at) VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&p.id)
    .bind(&p.org_id)
    .bind(&p.name)
    .bind(&p.description)
    .bind(&p.created_by)
    .bind(p.version)
    .bind(p.created_at)
    .bind(p.updated_at)
    .execute(c)
    .await
    .map(|_| ())
}

pub async fn get(c: &mut SqliteConnection, org_id: &str, id: &str) -> Result<Option<Project>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM projects WHERE id = ? AND org_id = ? AND deleted_at IS NULL")
        .bind(id)
        .bind(org_id)
        .fetch_optional(c)
        .await
}

/// Newest first, keyset on (created_at, id).
pub async fn list(
    c: &mut SqliteConnection,
    org_id: &str,
    cursor: Option<&Cursor>,
    limit: u32,
) -> AppResult<Page<Project>> {
    let (after_ts, after_id) = match cursor.map(|c| (&c.value, &c.id)) {
        None => (i64::MAX, String::from("\u{10FFFF}")),
        Some((SortValue::Int(v), id)) => (*v, id.clone()),
        // Silently restarting at page one would make a paging client loop forever.
        Some((SortValue::Text(_), _)) => {
            return Err(AppError::BadRequest("cursor does not match the sort field".into()));
        }
    };
    let mut rows: Vec<Project> = sqlx::query_as(
        "SELECT * FROM projects WHERE org_id = ? AND deleted_at IS NULL AND (created_at, id) < (?, ?) \
         ORDER BY created_at DESC, id DESC LIMIT ?",
    )
    .bind(org_id)
    .bind(after_ts)
    .bind(after_id)
    .bind(i64::from(limit) + 1)
    .fetch_all(c)
    .await?;
    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);
    let next_cursor = has_more.then(|| rows.last()).flatten().map(|p| {
        Cursor { sort: "-created_at".into(), value: SortValue::Int(p.created_at), id: p.id.clone() }.encode()
    });
    Ok(Page { items: rows, next_cursor })
}

pub async fn update(
    c: &mut SqliteConnection,
    org_id: &str,
    id: &str,
    expected_version: i64,
    name: Option<&str>,
    description: Option<&str>,
    now: i64,
) -> Result<Write<Project>, sqlx::Error> {
    let updated: Option<Project> = sqlx::query_as(
        "UPDATE projects SET name = COALESCE(?, name), description = COALESCE(?, description), \
         version = version + 1, updated_at = ? \
         WHERE id = ? AND org_id = ? AND deleted_at IS NULL AND version = ? RETURNING *",
    )
    .bind(name)
    .bind(description)
    .bind(now)
    .bind(id)
    .bind(org_id)
    .bind(expected_version)
    .fetch_optional(&mut *c)
    .await?;
    match updated {
        Some(p) => Ok(Write::Done(p)),
        None if get(c, org_id, id).await?.is_some() => Ok(Write::Stale),
        None => Ok(Write::NotFound),
    }
}

/// Soft delete: the row stays (audit, undelete, FK integrity); queries filter it out.
pub async fn soft_delete(
    c: &mut SqliteConnection,
    org_id: &str,
    id: &str,
    expected_version: Option<i64>,
    now: i64,
) -> Result<Write<()>, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE projects SET deleted_at = ?, version = version + 1, updated_at = ? \
         WHERE id = ? AND org_id = ? AND deleted_at IS NULL AND (? IS NULL OR version = ?)",
    )
    .bind(now)
    .bind(now)
    .bind(id)
    .bind(org_id)
    .bind(expected_version)
    .bind(expected_version)
    .execute(&mut *c)
    .await?;
    if r.rows_affected() == 1 {
        return Ok(Write::Done(()));
    }
    Ok(if get(c, org_id, id).await?.is_some() { Write::Stale } else { Write::NotFound })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, accounts};

    fn project(id: &str, org: &str, at: i64) -> Project {
        Project {
            id: id.into(),
            org_id: org.into(),
            name: format!("P {id}"),
            description: String::new(),
            created_by: "u1".into(),
            version: 1,
            created_at: at,
            updated_at: at,
            deleted_at: None,
        }
    }

    async fn db() -> Db {
        let db = Db::memory().await.unwrap();
        let mut c = db.pool().acquire().await.unwrap();
        accounts::insert_org(&mut c, "o1", "A", 0).await.unwrap();
        accounts::insert_org(&mut c, "o2", "B", 0).await.unwrap();
        db
    }

    #[tokio::test]
    async fn tenant_isolation() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &project("p1", "o1", 1)).await.unwrap();
        assert!(get(&mut c, "o1", "p1").await.unwrap().is_some());
        assert!(get(&mut c, "o2", "p1").await.unwrap().is_none(), "other tenant must not see it");
        assert_eq!(update(&mut c, "o2", "p1", 1, Some("x"), None, 5).await.unwrap(), Write::NotFound);
    }

    #[tokio::test]
    async fn optimistic_lock_detects_stale_writes() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &project("p1", "o1", 1)).await.unwrap();
        let Write::Done(p) = update(&mut c, "o1", "p1", 1, Some("v2"), None, 2).await.unwrap() else { panic!() };
        assert_eq!((p.version, p.name.as_str()), (2, "v2"));
        assert_eq!(update(&mut c, "o1", "p1", 1, Some("lost"), None, 3).await.unwrap(), Write::Stale);
        assert_eq!(get(&mut c, "o1", "p1").await.unwrap().unwrap().name, "v2");
    }

    #[tokio::test]
    async fn soft_delete_hides_but_keeps_row() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &project("p1", "o1", 1)).await.unwrap();
        assert_eq!(soft_delete(&mut c, "o1", "p1", Some(9), 2).await.unwrap(), Write::Stale);
        assert_eq!(soft_delete(&mut c, "o1", "p1", Some(1), 2).await.unwrap(), Write::Done(()));
        assert!(get(&mut c, "o1", "p1").await.unwrap().is_none());
        assert_eq!(soft_delete(&mut c, "o1", "p1", None, 3).await.unwrap(), Write::NotFound);
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM projects").fetch_one(&mut *c).await.unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn wrong_cursor_type_is_an_error_not_page_one() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        let bad = Cursor { sort: "-created_at".into(), value: SortValue::Text("x".into()), id: "a".into() };
        assert!(matches!(list(&mut c, "o1", Some(&bad), 5).await, Err(AppError::BadRequest(_))));
    }

    #[tokio::test]
    async fn keyset_pages_cover_everything_exactly_once() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        // Many rows share a timestamp: the id tie-breaker must keep the order total.
        for i in 0..25 {
            insert(&mut c, &project(&format!("p{i:02}"), "o1", i64::from(i / 5))).await.unwrap();
        }
        let mut seen = vec![];
        let mut cursor: Option<Cursor> = None;
        loop {
            let page = list(&mut c, "o1", cursor.as_ref(), 7).await.unwrap();
            seen.extend(page.items.iter().map(|p| p.id.clone()));
            match page.next_cursor {
                Some(s) => cursor = Some(Cursor::decode(&s, "-created_at").unwrap()),
                None => break,
            }
        }
        assert_eq!(seen.len(), 25);
        let mut dedup = seen.clone();
        dedup.sort();
        dedup.dedup();
        assert_eq!(dedup.len(), 25, "no duplicates");
        assert_eq!(seen.first().map(String::as_str), Some("p24"), "newest first");
    }
}
