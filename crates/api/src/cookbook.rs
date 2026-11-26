//! The cookbook (`content/cookbook`): reference pages by subject. Read-only; nothing about the reader is stored.

use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};

use anneal_content::cookbook::{Cookbook, Page, SAMPLES, SUBJECTS};

use crate::AppState;
use crate::error::{ApiError, ApiResult};

/// The pages of one subject in reading order: module, chapter, then page.
fn in_order<'a>(c: &'a Cookbook, subject: &str) -> Vec<&'a Page> {
    let mut pages: Vec<&Page> = c.pages.values().filter(|p| p.subject == subject).collect();
    pages.sort_by(|a, b| (a.module_order, &a.module, a.chapter_order, a.order, &a.id).cmp(&(b.module_order, &b.module, b.chapter_order, b.order, &b.id)));
    pages
}

/// A list shows the first couple of hundred characters of a page's description, ending at a word.
fn short(text: &str) -> String {
    const MOST: usize = 220;
    match text.char_indices().nth(MOST) {
        None => text.to_owned(),
        Some((end, _)) => {
            let cut = &text[..end];
            format!("{}…", cut.rsplit_once(' ').map_or(cut, |(head, _)| head))
        }
    }
}

fn row(p: &Page) -> Value {
    let lines: usize = p.labs.iter().flat_map(|l| &l.contents).map(|f| f.text.lines().count()).sum();
    json!({ "id": p.id, "title": p.title, "kind": p.kind, "module": p.module, "chapter": p.chapter, "lede": short(&p.lede), "status": p.status, "level": p.level, "lines": lines })
}

/// `GET /api/cookbook`: every subject with its pages in reading order, and the draft sample pages.
pub async fn index(State(s): State<AppState>) -> Json<Value> {
    let c = &s.catalog.cookbook;
    let subjects: Vec<Value> = SUBJECTS
        .iter()
        .map(|(id, title, about)| json!({ "id": id, "title": title, "about": about, "pages": in_order(c, id).into_iter().map(row).collect::<Vec<_>>() }))
        .collect();
    let samples: Vec<Value> = in_order(c, SAMPLES).into_iter().map(row).collect();
    Json(json!({ "subjects": subjects, "samples": samples }))
}

/// `GET /api/cookbook/pages/{id}`: one page, with the pages before and after it in its subject and those that link to it.
pub async fn page(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let c = &s.catalog.cookbook;
    let p = c.pages.get(&id).ok_or_else(|| ApiError::NotFound(format!("cookbook page {id}")))?;
    let list = in_order(c, &p.subject);
    let at = list.iter().position(|x| x.id == p.id).unwrap_or(0);
    let near = |i: Option<usize>| i.and_then(|i| list.get(i)).map(|x| json!({ "id": x.id, "title": x.title, "chapter": x.chapter }));
    // the pages whose text links to this one
    let link = format!("\"/cookbook/p/{}\"", p.id);
    let linked_from: Vec<Value> = c.pages.values().filter(|x| x.id != p.id && x.sections.iter().any(|s| s.body.contains(&link))).map(row).collect();
    Ok(Json(json!({ "page": p, "previous": near(at.checked_sub(1)), "next": near(Some(at + 1)), "linked_from": linked_from })))
}
