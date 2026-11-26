//! Task aggregate repository (+ its children: comments, attachments) and FTS5 search.
//!
//! The FTS index is a separate table, so it is maintained here, in the SAME connection/transaction as the
//! row write. Pitfall: an FTS row written outside the transaction can outlive a rolled-back task.

use serde::Serialize;
use sqlx::{QueryBuilder, Sqlite, SqliteConnection};

use super::Write;
use crate::error::{AppError, AppResult};
use crate::query::{Cursor, Page, SortField, SortValue, TaskFilter, fts_query};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub org_id: String,
    pub project_id: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub priority: i64,
    pub assignee_id: Option<String>,
    pub created_by: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip)]
    pub deleted_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub body: Option<String>,
    pub status: Option<String>,
    pub priority: Option<i64>,
    /// `None` = leave alone, `Some(None)` = unassign, `Some(Some(id))` = assign.
    pub assignee: Option<Option<String>>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Comment {
    pub id: String,
    pub org_id: String,
    pub task_id: String,
    pub author_id: String,
    pub body: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize)]
pub struct Attachment {
    pub id: String,
    pub org_id: String,
    pub task_id: String,
    #[serde(skip)]
    pub object_key: String,
    pub filename: String,
    pub content_type: String,
    pub size: i64,
    pub sha256: String,
    pub created_at: i64,
}

async fn fts_replace(c: &mut SqliteConnection, t: &Task) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM tasks_fts WHERE task_id = ?").bind(&t.id).execute(&mut *c).await?;
    sqlx::query("INSERT INTO tasks_fts (task_id, title, body) VALUES (?,?,?)")
        .bind(&t.id)
        .bind(&t.title)
        .bind(&t.body)
        .execute(c)
        .await
        .map(|_| ())
}

pub async fn insert(c: &mut SqliteConnection, t: &Task) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tasks (id, org_id, project_id, title, body, status, priority, assignee_id, created_by, version, created_at, updated_at) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&t.id)
    .bind(&t.org_id)
    .bind(&t.project_id)
    .bind(&t.title)
    .bind(&t.body)
    .bind(&t.status)
    .bind(t.priority)
    .bind(&t.assignee_id)
    .bind(&t.created_by)
    .bind(t.version)
    .bind(t.created_at)
    .bind(t.updated_at)
    .execute(&mut *c)
    .await?;
    fts_replace(c, t).await
}

pub async fn get(c: &mut SqliteConnection, org_id: &str, id: &str) -> Result<Option<Task>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM tasks WHERE id = ? AND org_id = ? AND deleted_at IS NULL")
        .bind(id)
        .bind(org_id)
        .fetch_optional(c)
        .await
}

pub async fn update(
    c: &mut SqliteConnection,
    org_id: &str,
    id: &str,
    expected_version: i64,
    patch: &TaskPatch,
    now: i64,
) -> Result<Write<Task>, sqlx::Error> {
    let (set_assignee, assignee) = match &patch.assignee {
        None => (false, None),
        Some(a) => (true, a.clone()),
    };
    let updated: Option<Task> = sqlx::query_as(
        "UPDATE tasks SET title = COALESCE(?, title), body = COALESCE(?, body), status = COALESCE(?, status), \
         priority = COALESCE(?, priority), assignee_id = CASE WHEN ? THEN ? ELSE assignee_id END, \
         version = version + 1, updated_at = ? \
         WHERE id = ? AND org_id = ? AND deleted_at IS NULL AND version = ? RETURNING *",
    )
    .bind(&patch.title)
    .bind(&patch.body)
    .bind(&patch.status)
    .bind(patch.priority)
    .bind(set_assignee)
    .bind(assignee)
    .bind(now)
    .bind(id)
    .bind(org_id)
    .bind(expected_version)
    .fetch_optional(&mut *c)
    .await?;
    match updated {
        Some(t) => {
            fts_replace(c, &t).await?;
            Ok(Write::Done(t))
        }
        None if get(c, org_id, id).await?.is_some() => Ok(Write::Stale),
        None => Ok(Write::NotFound),
    }
}

pub async fn soft_delete(
    c: &mut SqliteConnection,
    org_id: &str,
    id: &str,
    now: i64,
) -> Result<Write<()>, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE tasks SET deleted_at = ?, version = version + 1, updated_at = ? WHERE id = ? AND org_id = ? AND deleted_at IS NULL",
    )
    .bind(now)
    .bind(now)
    .bind(id)
    .bind(org_id)
    .execute(&mut *c)
    .await?;
    if r.rows_affected() == 0 {
        return Ok(Write::NotFound);
    }
    sqlx::query("DELETE FROM tasks_fts WHERE task_id = ?").bind(id).execute(c).await?;
    Ok(Write::Done(()))
}

/// Cascade of a project soft delete. Must run in the caller's transaction, otherwise a crash between
/// the two statements leaves live tasks under a deleted project.
pub async fn soft_delete_by_project(
    c: &mut SqliteConnection,
    org_id: &str,
    project_id: &str,
    now: i64,
) -> Result<u64, sqlx::Error> {
    sqlx::query(
        "DELETE FROM tasks_fts WHERE task_id IN (SELECT id FROM tasks WHERE org_id = ? AND project_id = ? AND deleted_at IS NULL)",
    )
    .bind(org_id)
    .bind(project_id)
    .execute(&mut *c)
    .await?;
    sqlx::query(
        "UPDATE tasks SET deleted_at = ?, version = version + 1, updated_at = ? WHERE org_id = ? AND project_id = ? AND deleted_at IS NULL",
    )
    .bind(now)
    .bind(now)
    .bind(org_id)
    .bind(project_id)
    .execute(c)
    .await
    .map(|r| r.rows_affected())
}

fn sort_value(t: &Task, f: SortField) -> SortValue {
    match f {
        SortField::CreatedAt => SortValue::Int(t.created_at),
        SortField::UpdatedAt => SortValue::Int(t.updated_at),
        SortField::Priority => SortValue::Int(t.priority),
        SortField::Title => SortValue::Text(t.title.clone()),
    }
}

fn is_text_field(f: SortField) -> bool {
    matches!(f, SortField::Title)
}

/// Filtered, sorted, keyset-paginated listing. `project` pins the project (nested route) on top of the filter.
pub async fn list(
    c: &mut SqliteConnection,
    org_id: &str,
    project: Option<&str>,
    f: &TaskFilter,
) -> AppResult<Page<Task>> {
    let cursor = f.cursor.as_deref().map(|s| Cursor::decode(s, &f.sort.spec())).transpose()?;
    let mut qb: QueryBuilder<Sqlite> =
        QueryBuilder::new("SELECT t.* FROM tasks t WHERE t.deleted_at IS NULL AND t.org_id = ");
    qb.push_bind(org_id);
    if let Some(p) = project.or(f.project_id.as_deref()) {
        qb.push(" AND t.project_id = ").push_bind(p.to_string());
    }
    if let Some(a) = &f.assignee {
        qb.push(" AND t.assignee_id = ").push_bind(a.clone());
    }
    if !f.statuses.is_empty() {
        qb.push(" AND t.status IN (");
        let mut sep = qb.separated(", ");
        for s in &f.statuses {
            sep.push_bind(s.clone());
        }
        sep.push_unseparated(")");
    }
    if let Some(n) = f.priority_min {
        qb.push(" AND t.priority >= ").push_bind(n);
    }
    if let Some(m) = f.q.as_deref().and_then(fts_query) {
        qb.push(" AND t.id IN (SELECT task_id FROM tasks_fts WHERE tasks_fts MATCH ").push_bind(m).push(")");
    }
    if let Some(cur) = &cursor {
        // A cursor whose value type does not fit the column would silently compare text with ints.
        let ok = matches!(
            (&cur.value, is_text_field(f.sort.field)),
            (SortValue::Text(_), true) | (SortValue::Int(_), false)
        );
        if !ok {
            return Err(AppError::BadRequest("cursor does not match the sort field".into()));
        }
        // Table-qualified column name comes from a closed enum, never from user text.
        qb.push(format!(" AND (t.{}, t.id) {} (", f.sort.column(), f.sort.after_op()));
        match &cur.value {
            SortValue::Int(v) => qb.push_bind(*v),
            SortValue::Text(v) => qb.push_bind(v.clone()),
        };
        qb.push(", ").push_bind(cur.id.clone()).push(")");
    }
    let dir = f.sort.direction();
    qb.push(format!(" ORDER BY t.{} {dir}, t.id {dir} LIMIT ", f.sort.column()));
    qb.push_bind(i64::from(f.limit) + 1);

    let mut rows: Vec<Task> = qb.build_query_as().fetch_all(c).await?;
    let has_more = rows.len() > f.limit as usize;
    rows.truncate(f.limit as usize);
    let next_cursor = has_more
        .then(|| rows.last())
        .flatten()
        .map(|t| Cursor { sort: f.sort.spec(), value: sort_value(t, f.sort.field), id: t.id.clone() }.encode());
    Ok(Page { items: rows, next_cursor })
}

/// `(status, count)` for a project: the "expensive" aggregate that the cache block memoises.
pub async fn status_counts(
    c: &mut SqliteConnection,
    org_id: &str,
    project: &str,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT status, count(*) FROM tasks WHERE org_id = ? AND project_id = ? AND deleted_at IS NULL GROUP BY status ORDER BY status",
    )
    .bind(org_id)
    .bind(project)
    .fetch_all(c)
    .await
}

pub async fn insert_comment(c: &mut SqliteConnection, m: &Comment) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO comments (id, org_id, task_id, author_id, body, created_at) VALUES (?,?,?,?,?,?)")
        .bind(&m.id)
        .bind(&m.org_id)
        .bind(&m.task_id)
        .bind(&m.author_id)
        .bind(&m.body)
        .bind(m.created_at)
        .execute(c)
        .await
        .map(|_| ())
}

pub async fn comments(c: &mut SqliteConnection, org_id: &str, task_id: &str) -> Result<Vec<Comment>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM comments WHERE org_id = ? AND task_id = ? ORDER BY created_at, id LIMIT 500")
        .bind(org_id)
        .bind(task_id)
        .fetch_all(c)
        .await
}

pub async fn insert_attachment(c: &mut SqliteConnection, a: &Attachment) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO attachments (id, org_id, task_id, object_key, filename, content_type, size, sha256, created_at) VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(&a.id)
    .bind(&a.org_id)
    .bind(&a.task_id)
    .bind(&a.object_key)
    .bind(&a.filename)
    .bind(&a.content_type)
    .bind(a.size)
    .bind(&a.sha256)
    .bind(a.created_at)
    .execute(c)
    .await
    .map(|_| ())
}

pub async fn attachments(
    c: &mut SqliteConnection,
    org_id: &str,
    task_id: &str,
) -> Result<Vec<Attachment>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM attachments WHERE org_id = ? AND task_id = ? ORDER BY created_at, id")
        .bind(org_id)
        .bind(task_id)
        .fetch_all(c)
        .await
}

pub async fn attachment(c: &mut SqliteConnection, org_id: &str, id: &str) -> Result<Option<Attachment>, sqlx::Error> {
    sqlx::query_as("SELECT * FROM attachments WHERE id = ? AND org_id = ?")
        .bind(id)
        .bind(org_id)
        .fetch_optional(c)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{Db, accounts, projects};
    use crate::query::{Sort, parse_task_filter};

    fn task(id: &str, title: &str, prio: i64, at: i64) -> Task {
        Task {
            id: id.into(),
            org_id: "o1".into(),
            project_id: "p1".into(),
            title: title.into(),
            body: String::new(),
            status: "open".into(),
            priority: prio,
            assignee_id: None,
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
        let p = projects::Project {
            id: "p1".into(),
            org_id: "o1".into(),
            name: "P".into(),
            description: String::new(),
            created_by: "u1".into(),
            version: 1,
            created_at: 0,
            updated_at: 0,
            deleted_at: None,
        };
        projects::insert(&mut c, &p).await.unwrap();
        drop(c);
        db
    }

    fn filter(pairs: &[(&str, &str)]) -> TaskFilter {
        let v: Vec<(String, String)> = pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        parse_task_filter(&v).unwrap()
    }

    #[tokio::test]
    async fn fts_finds_prefix_and_follows_updates_and_deletes() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &task("t1", "Fix login redirect", 1, 1)).await.unwrap();
        insert(&mut c, &task("t2", "Write docs", 1, 2)).await.unwrap();
        let hits = |q: &'static str| {
            let f = filter(&[("q", q)]);
            async move { f }
        };
        let f = hits("logi").await;
        assert_eq!(list(&mut c, "o1", None, &f).await.unwrap().items.len(), 1, "prefix match");
        let patch = TaskPatch { title: Some("Fix signup".into()), ..Default::default() };
        let Write::Done(_) = update(&mut c, "o1", "t1", 1, &patch, 3).await.unwrap() else { panic!() };
        assert!(
            list(&mut c, "o1", None, &filter(&[("q", "login")])).await.unwrap().items.is_empty(),
            "index follows update"
        );
        assert_eq!(list(&mut c, "o1", None, &filter(&[("q", "signup")])).await.unwrap().items.len(), 1);
        let _ = soft_delete(&mut c, "o1", "t1", 4).await.unwrap();
        assert!(list(&mut c, "o1", None, &filter(&[("q", "signup")])).await.unwrap().items.is_empty());
        // Hostile input must not be a syntax error.
        for q in ["\"", "AND OR", "title:x", "a*b(", "-"] {
            list(&mut c, "o1", None, &filter(&[("q", q)])).await.unwrap();
        }
    }

    #[tokio::test]
    async fn patch_semantics_for_assignee() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &task("t1", "x", 0, 1)).await.unwrap();
        let assign = TaskPatch { assignee: Some(Some("u9".into())), ..Default::default() };
        let Write::Done(t) = update(&mut c, "o1", "t1", 1, &assign, 2).await.unwrap() else { panic!() };
        assert_eq!(t.assignee_id.as_deref(), Some("u9"));
        let untouched = TaskPatch { priority: Some(3), ..Default::default() };
        let Write::Done(t) = update(&mut c, "o1", "t1", 2, &untouched, 3).await.unwrap() else { panic!() };
        assert_eq!((t.assignee_id.as_deref(), t.priority, t.version), (Some("u9"), 3, 3));
        let clear = TaskPatch { assignee: Some(None), ..Default::default() };
        let Write::Done(t) = update(&mut c, "o1", "t1", 3, &clear, 4).await.unwrap() else { panic!() };
        assert_eq!(t.assignee_id, None);
    }

    #[tokio::test]
    async fn stale_version_is_refused_and_changes_nothing() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &task("t1", "original", 0, 1)).await.unwrap();
        let rename = |title: &str| TaskPatch { title: Some(title.into()), ..Default::default() };
        let Write::Done(_) = update(&mut c, "o1", "t1", 1, &rename("second"), 2).await.unwrap() else { panic!() };
        assert_eq!(update(&mut c, "o1", "t1", 1, &rename("lost"), 3).await.unwrap(), Write::Stale);
        assert_eq!(update(&mut c, "o2", "t1", 2, &rename("foreign"), 3).await.unwrap(), Write::NotFound);
        let stored = get(&mut c, "o1", "t1").await.unwrap().unwrap();
        assert_eq!((stored.title.as_str(), stored.version), ("second", 2));
        let found = list(&mut c, "o1", None, &filter(&[("q", "lost")])).await.unwrap();
        assert!(found.items.is_empty(), "a refused write must not reach the search index");
    }

    /// Differential test: keyset paging over every sort order equals sorting everything in memory.
    #[tokio::test]
    async fn keyset_paging_matches_in_memory_oracle_for_every_sort() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        let mut all = vec![];
        for i in 0..53_i64 {
            // Deliberately many ties on priority and created_at.
            let t = task(&format!("t{i:03}"), &format!("title {}", (i * 7) % 11), i % 4, i / 6);
            insert(&mut c, &t).await.unwrap();
            all.push(t);
        }
        for spec in
            ["created_at", "-created_at", "priority", "-priority", "title", "-title", "updated_at", "-updated_at"]
        {
            let sort = Sort::parse(spec).unwrap();
            let mut expected = all.clone();
            expected.sort_by(|a, b| {
                let ord = match sort.field {
                    SortField::CreatedAt => a.created_at.cmp(&b.created_at),
                    SortField::UpdatedAt => a.updated_at.cmp(&b.updated_at),
                    SortField::Priority => a.priority.cmp(&b.priority),
                    SortField::Title => a.title.cmp(&b.title),
                }
                .then_with(|| a.id.cmp(&b.id));
                if sort.desc { ord.reverse() } else { ord }
            });
            let mut got = vec![];
            let mut cursor: Option<String> = None;
            loop {
                let mut f = filter(&[("sort", spec), ("limit", "8")]);
                f.cursor = cursor.take();
                let page = list(&mut c, "o1", None, &f).await.unwrap();
                got.extend(page.items.into_iter().map(|t| t.id));
                match page.next_cursor {
                    Some(n) => cursor = Some(n),
                    None => break,
                }
            }
            let want: Vec<String> = expected.into_iter().map(|t| t.id).collect();
            assert_eq!(got, want, "sort {spec}");
        }
    }

    #[tokio::test]
    async fn filters_combine_and_cursor_type_is_checked() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        for i in 0..6 {
            let mut t = task(&format!("t{i}"), "x", i, i);
            t.status = if i % 2 == 0 { "done".into() } else { "open".into() };
            insert(&mut c, &t).await.unwrap();
        }
        let f = filter(&[("status", "done"), ("priority_min", "2")]);
        let ids: Vec<_> = list(&mut c, "o1", None, &f).await.unwrap().items.into_iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), 2);
        // Text cursor against an integer sort column is refused.
        let forged =
            Cursor { sort: "-created_at".into(), value: SortValue::Text("zzz".into()), id: "a".into() }.encode();
        let mut f = filter(&[]);
        f.cursor = Some(forged);
        assert!(matches!(list(&mut c, "o1", None, &f).await, Err(AppError::BadRequest(_))));
    }

    #[tokio::test]
    async fn comments_and_attachments_are_tenant_scoped() {
        let db = db().await;
        let mut c = db.pool().acquire().await.unwrap();
        insert(&mut c, &task("t1", "x", 0, 1)).await.unwrap();
        let m = Comment {
            id: "c1".into(),
            org_id: "o1".into(),
            task_id: "t1".into(),
            author_id: "u1".into(),
            body: "hi".into(),
            created_at: 2,
        };
        insert_comment(&mut c, &m).await.unwrap();
        assert_eq!(comments(&mut c, "o1", "t1").await.unwrap().len(), 1);
        assert!(comments(&mut c, "o2", "t1").await.unwrap().is_empty());
        let a = Attachment {
            id: "a1".into(),
            org_id: "o1".into(),
            task_id: "t1".into(),
            object_key: "o1/a1".into(),
            filename: "f.txt".into(),
            content_type: "text/plain".into(),
            size: 1,
            sha256: "x".into(),
            created_at: 3,
        };
        insert_attachment(&mut c, &a).await.unwrap();
        // The HTTP handler also re-loads the parent task, which would HIDE a missing filter here. Each
        // layer therefore needs its own test.
        assert!(attachment(&mut c, "o1", "a1").await.unwrap().is_some());
        assert!(attachment(&mut c, "o2", "a1").await.unwrap().is_none());
        assert_eq!(attachments(&mut c, "o1", "t1").await.unwrap().len(), 1);
        assert!(attachments(&mut c, "o2", "t1").await.unwrap().is_empty());
    }
}
