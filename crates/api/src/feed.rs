//! A calendar feed of what is coming up: the reviews that fall due and the weekly solve days, as an `.ics` file a phone calendar can
//! subscribe to. The address carries a long random token, because a calendar app cannot sign in; it only ever shows problem titles and
//! dates, and the token can be replaced from Settings (the old address then stops working).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::{Datelike, Days, NaiveDate};
use serde_json::{Value, json};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::error::{ApiError, ApiResult};
use crate::store;

/// How many days ahead the feed looks.
const DAYS: u64 = 90;

async fn token(db: &sqlx::PgPool) -> sqlx::Result<Option<String>> {
    let v: Option<Jsonb<String>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'feed_token'").fetch_optional(db).await?;
    Ok(v.map(|j| j.0))
}

/// `GET /api/feed`: the feed's address, if one was made.
pub async fn get(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    Ok(Json(json!({ "path": token(&s.db).await?.map(|t| format!("/api/feed/{t}.ics")) })))
}

/// `POST /api/feed`: makes the feed's address, or replaces it with a new one (the old address stops working).
pub async fn make(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    use argon2::password_hash::rand_core::{OsRng, RngCore};
    use base64::Engine;
    let mut bytes = [0u8; 24];
    OsRng.fill_bytes(&mut bytes);
    let t = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    crate::settings::store(&s, "feed_token", &t).await?;
    Ok(Json(json!({ "path": format!("/api/feed/{t}.ics") })))
}

fn esc(s: &str) -> String {
    s.replace('\\', "\\\\").replace(';', "\\;").replace(',', "\\,").replace('\n', "\\n")
}

fn day(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

/// `GET /api/feed/{token}.ics`: public, but only with the right token.
pub async fn ics(State(s): State<AppState>, Path(file): Path<String>) -> ApiResult<Response> {
    let given = file.strip_suffix(".ics").unwrap_or(&file);
    match token(&s.db).await? {
        Some(t) if !t.is_empty() && t == given => {}
        _ => return Err(ApiError::NotFound("feed".into())),
    }
    let today = crate::activity::today();
    let settings = crate::settings::srs(&s.db).await?;
    let reviews = crate::dsa::dsa_reviews(&s.catalog, store::reviews(&s.db).await?);
    let mut by_day: std::collections::BTreeMap<NaiveDate, Vec<String>> = std::collections::BTreeMap::new();
    for r in &reviews {
        let due = r.due_at.with_timezone(&chrono::Local).date_naive().max(today);
        if due <= today + Days::new(DAYS)
            && let Some((_, p)) = s.catalog.problem(&r.problem_id)
        {
            by_day.entry(due).or_default().push(p.meta.title.clone());
        }
    }
    let mut out = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//anneal//review feed//EN\r\nCALSCALE:GREGORIAN\r\nX-WR-CALNAME:anneal\r\n");
    for (d, titles) in &by_day {
        out.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:anneal-reviews-{}@anneal\r\nDTSTAMP:{}T000000Z\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nSUMMARY:{}\r\nDESCRIPTION:{}\r\nEND:VEVENT\r\n",
            day(*d),
            day(today),
            day(*d),
            day(*d + Days::new(1)),
            esc(&format!("anneal: {} review{} due", titles.len(), if titles.len() == 1 { "" } else { "s" })),
            esc(&titles.join("\n")),
        ));
    }
    // The weekly solve days, as one repeating event.
    let routine = crate::calendar::routine_of(&settings);
    if !routine.solve_days.is_empty() {
        let first = (0..7).map(|i| today + Days::new(i)).find(|d| routine.solve_days.contains(&d.weekday())).unwrap_or(today);
        let byday: Vec<&str> = routine.solve_days.iter().map(|w| match w { chrono::Weekday::Mon => "MO", chrono::Weekday::Tue => "TU", chrono::Weekday::Wed => "WE", chrono::Weekday::Thu => "TH", chrono::Weekday::Fri => "FR", chrono::Weekday::Sat => "SA", chrono::Weekday::Sun => "SU" }).collect();
        out.push_str(&format!(
            "BEGIN:VEVENT\r\nUID:anneal-solve-days@anneal\r\nDTSTAMP:{}T000000Z\r\nDTSTART;VALUE=DATE:{}\r\nDTEND;VALUE=DATE:{}\r\nRRULE:FREQ=WEEKLY;BYDAY={}\r\nSUMMARY:anneal: solve a new problem\r\nEND:VEVENT\r\n",
            day(today),
            day(first),
            day(first + Days::new(1)),
            byday.join(","),
        ));
    }
    out.push_str("END:VCALENDAR\r\n");
    let mut res = out.into_response();
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("text/calendar; charset=utf-8"));
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(res)
}
