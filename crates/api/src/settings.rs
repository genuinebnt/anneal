//! User settings: the editor, and the app's appearance.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use sqlx::types::Json as Jsonb;

use crate::AppState;
use crate::error::{ApiError, ApiResult};

/// Font families the web app knows how to load. `system` is the OS monospace font.
pub const FONT_FAMILIES: &[&str] = &["Fira Code", "JetBrains Mono", "IBM Plex Mono", "Source Code Pro", "Roboto Mono", "system"];
pub const FONT_SIZES: std::ops::RangeInclusive<u8> = 10..=24;
/// Accent colours the web app has palettes for. Copper is the design's own.
/// The accent colours a page can use. The first six are deep, the next three soft; `custom` uses the colour in `AppearanceSettings::custom`.
pub const ACCENTS: &[&str] = &["red", "teal", "orange", "blue", "purple", "yellow", "copper", "rose", "sky", "custom"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorSettings {
    pub font_size: u8,
    pub font_family: String,
    /// Vim keybindings; `jk` / `kj` leave insert mode.
    #[serde(default)]
    pub vim: bool,
    /// The workspace status-bar toggles, remembered across problems and reloads.
    #[serde(default = "on")]
    pub autocomplete: bool,
    #[serde(default = "on")]
    pub rust_analyzer: bool,
    /// Borrow lanes in the editor; off until turned on.
    #[serde(default)]
    pub borrow_lanes: bool,
    /// rust-analyzer checks with clippy instead of `cargo check`, so lints show while editing.
    #[serde(default = "on")]
    pub live_clippy: bool,
    /// Run rustfmt after a pause in typing, not only on ⌘S / ⇧⌥F.
    #[serde(default)]
    pub format_on_pause: bool,
    /// Programming ligatures in the editor (`->` drawn as one arrow). Off by default: they form and un-form as you
    /// type, and the cursor and selection are placed per character.
    #[serde(default)]
    pub ligatures: bool,
}

fn on() -> bool {
    true
}

impl Default for EditorSettings {
    fn default() -> Self {
        EditorSettings {
            font_size: 13,
            font_family: "Fira Code".into(),
            vim: false,
            autocomplete: true,
            rust_analyzer: true,
            borrow_lanes: false,
            live_clippy: true,
            format_on_pause: false,
            ligatures: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppearanceSettings {
    pub accent: String,
    /// The colour of the `custom` accent, as `#rrggbb`. Absent in settings saved before it existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom: Option<String>,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        AppearanceSettings { accent: "copper".into(), custom: None }
    }
}

#[derive(Debug, Serialize)]
pub struct Settings {
    pub editor: EditorSettings,
    pub appearance: AppearanceSettings,
    /// Review scheduling: retention, review days, the daily cap and the target date.
    pub srs: crate::reviews::Settings,
    pub accents: &'static [&'static str],
    /// What the editor settings may be set to.
    pub font_families: &'static [&'static str],
    pub font_sizes: [u8; 2],
}

/// The saved editor settings, or the defaults.
pub async fn editor(db: &sqlx::PgPool) -> sqlx::Result<EditorSettings> {
    let stored: Option<Jsonb<EditorSettings>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'editor'")
        .fetch_optional(db)
        .await?;
    Ok(stored.map(|j| j.0).unwrap_or_default())
}

pub async fn get(State(s): State<AppState>) -> ApiResult<Json<Settings>> {
    let editor: Option<Jsonb<EditorSettings>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'editor'")
        .fetch_optional(&s.db)
        .await?;
    let appearance: Option<Jsonb<AppearanceSettings>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'appearance'")
        .fetch_optional(&s.db)
        .await?;
    Ok(Json(Settings {
        editor: editor.map(|j| j.0).unwrap_or_default(),
        appearance: appearance.map(|j| j.0).unwrap_or_default(),
        srs: srs(&s.db).await?,
        accents: ACCENTS,
        font_families: FONT_FAMILIES,
        font_sizes: [*FONT_SIZES.start(), *FONT_SIZES.end()],
    }))
}

pub async fn put_editor(State(s): State<AppState>, Json(e): Json<EditorSettings>) -> ApiResult<Json<EditorSettings>> {
    if !FONT_SIZES.contains(&e.font_size) {
        return Err(ApiError::BadRequest(format!("font_size must be {}–{}", FONT_SIZES.start(), FONT_SIZES.end())));
    }
    if !FONT_FAMILIES.contains(&e.font_family.as_str()) {
        return Err(ApiError::BadRequest(format!("font_family must be one of {}", FONT_FAMILIES.join(", "))));
    }
    store(&s, "editor", &e).await?;
    Ok(Json(e))
}

pub async fn put_appearance(State(s): State<AppState>, Json(a): Json<AppearanceSettings>) -> ApiResult<Json<AppearanceSettings>> {
    if !ACCENTS.contains(&a.accent.as_str()) {
        return Err(ApiError::BadRequest(format!("accent must be one of {}", ACCENTS.join(", "))));
    }
    let hex = |c: &str| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    match &a.custom {
        Some(c) if !hex(c) => return Err(ApiError::BadRequest("custom must be a colour written #rrggbb".into())),
        None if a.accent == "custom" => return Err(ApiError::BadRequest("the custom accent needs a colour in custom".into())),
        _ => {}
    }
    store(&s, "appearance", &a).await?;
    Ok(Json(a))
}

pub(crate) async fn store<T: Serialize>(s: &AppState, key: &str, value: &T) -> ApiResult<()> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = now()",
    )
    .bind(key)
    .bind(Jsonb(value))
    .execute(&s.db)
    .await?;
    Ok(())
}

/// The review settings (`settings.srs`), or the defaults.
pub async fn srs(db: &sqlx::PgPool) -> sqlx::Result<crate::reviews::Settings> {
    let stored: Option<Jsonb<crate::reviews::Settings>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'srs'").fetch_optional(db).await?;
    Ok(stored.map(|j| j.0).unwrap_or_default())
}

/// Whether premium LeetCode problems are in the completion totals (progress bars, readiness, goals). Off by default. It is
/// the inverse of the goal's `free_only` flag, so there is one stored value and the plan page's switch edits the same one.
/// What the owner *did* (streak, heatmap, reviews) always includes premium, whatever this says.
pub async fn count_premium(db: &sqlx::PgPool) -> sqlx::Result<bool> {
    Ok(!srs(db).await?.goal.free_only)
}

/// `GET /api/prefs`: the small browser choices (panel sizes, timer lengths, what is hidden), kept on the server so they
/// follow the owner to another laptop.
pub async fn get_prefs(State(s): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let stored: Option<Jsonb<serde_json::Value>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'ui_prefs'").fetch_optional(&s.db).await?;
    Ok(Json(stored.map_or_else(|| serde_json::json!({}), |j| j.0)))
}

#[derive(serde::Deserialize)]
pub struct PrefBody {
    key: String,
    value: serde_json::Value,
}

/// `POST /api/prefs`: sets one preference, leaving the others as they are.
pub async fn put_pref(State(s): State<AppState>, Json(b): Json<PrefBody>) -> ApiResult<Json<serde_json::Value>> {
    if b.key.is_empty() || b.key.len() > 80 || b.value.to_string().len() > 16 * 1024 {
        return Err(ApiError::BadRequest("a preference key is 1 to 80 characters and its value at most 16 KB".into()));
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('ui_prefs', jsonb_build_object($1::text, $2::jsonb))
         ON CONFLICT (key) DO UPDATE SET value = settings.value || jsonb_build_object($1::text, $2::jsonb), updated_at = now()",
    )
    .bind(&b.key)
    .bind(Jsonb(&b.value))
    .execute(&s.db)
    .await?;
    Ok(Json(serde_json::json!({ "saved": b.key })))
}

pub async fn put_srs(State(s): State<AppState>, Json(v): Json<crate::reviews::Settings>) -> ApiResult<Json<crate::reviews::Settings>> {
    v.validate().map_err(ApiError::BadRequest)?;
    store(&s, "srs", &v).await?;
    Ok(Json(v))
}
