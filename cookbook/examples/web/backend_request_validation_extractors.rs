//! RECIPE 1200 (web-backend): request parsing + validation: custom extractors, composable rules, problem+json `errors` map.
//! LEVEL: intermediate
//! Use in: every JSON/HTTP backend (the first line of defence)   Run: cargo run --release --example backend_request_validation_extractors
//! COMMON USES: signup/checkout forms, public REST APIs, internal admin APIs; same shape as Stripe/GitHub style field errors.
#![allow(dead_code)]
//!
//! Extends (does not repeat): axum_errors (one error enum -> one JSON shape), rest_api_patterns, multipart_forms_uploads.
//!
//! Pipeline used here: bytes -> `Raw` DTO (serde, `deny_unknown_fields`) -> normalise (trim/case-fold) -> validate
//! (composable rules, ALL field errors collected) -> `TryFrom`-style conversion into a domain type that cannot be invalid
//! ("parse, don't validate"). Failures become RFC 9457 `application/problem+json` with an `errors` map keyed by field;
//! each error carries a stable machine `code` + `params` (i18n-ready); the human `message` is rendered from a catalog by
//! `Accept-Language`, so clients localise by code and servers never hard-code prose into logic.
//!
//! Cheat sheet (I want to ... -> use ...):
//!   limit body size              -> `DefaultBodyLimit::max(n)` (413 from the Bytes extractor) / tower-http RequestBodyLimitLayer
//!   require JSON content type    -> check Content-Type in the extractor, 415 otherwise
//!   custom extractor on body     -> `impl FromRequest<S>` (consumes body, must be LAST arg)
//!   custom extractor on headers  -> `impl FromRequestParts<S>` (any order, many allowed)
//!   repeated query keys `?a=1&a=2` -> hand-parse `uri.query()` (serde_urlencoded cannot do Vec)
//!   comma lists `?status=a,b`    -> split(',') after decoding, then parse each into an enum
//!   report every bad field       -> `Collector` gathers `BTreeMap<field, Vec<FieldError>>`
//!   stop at first rule per field -> `Collector::field` uses `find_map`
//!   default a missing field      -> `#[serde(default = "fn")]`
//!   reject unknown fields        -> `#[serde(deny_unknown_fields)]` (strict) or omit (lenient, forward compatible)
//!   case-insensitive identity    -> trim + `to_lowercase()` BEFORE uniqueness checks (production: NFKC via `unicode-normalization`)
//!   pick response format         -> parse `Accept` with q-values, `negotiate()` -> 406 if none
//!   localise errors              -> stable `code` + `params`, catalog keyed by (lang, code)
//!   production crates            -> `validator`/`garde` derive rules, `serde_path_to_error` for JSON paths, `axum-extra`
//!
//! Read: RFC 9457 Problem Details (https://www.rfc-editor.org/rfc/rfc9457), axum extractors docs (https://docs.rs/axum/latest/axum/extract/),
//! "Parse, don't validate" by Alexis King (title only; not re-fetched).
//!
//! TRY NEXT:
//!   - [S] Add a `url()` rule<str> that rejects non-https schemes; wire it to a new `website` field and extend the fuzz loop.
//!   - [S] Predict then measure: what status does `{"age": "12"}` (string) give? Where does the field name go and why (see `classify`)?
//!   - [M] Replace the first-failing-rule policy in `Collector::field` with "all rules"; update tests, discuss UX noise.
//!   - [M] Port `Valid<P>` to read `application/x-www-form-urlencoded` too (reuse `parse_query`) for plain HTML forms.
//!   - [A] PROJECT: combine with axum_middleware + backend_request_context_observability: log `errors` keys (never values) per request id.
use axum::{
    Router,
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, FromRequest, FromRequestParts, Request},
    http::{HeaderMap, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{patch, post},
};
use http_body_util::BodyExt;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use tower::ServiceExt;

// ---------------------------------------------------------------- errors + rules (everyday core) ----------------------
#[derive(Debug, Clone, PartialEq, Serialize)]
struct FieldError {
    code: &'static str,
    params: Value,
}
fn fe(code: &'static str, params: Value) -> FieldError {
    FieldError { code, params }
}
type Errors = BTreeMap<String, Vec<FieldError>>;
type Rule<T> = Box<dyn Fn(&T) -> Option<FieldError>>;

fn required() -> Rule<str> {
    Box::new(|s| s.is_empty().then(|| fe("required", json!({}))))
}
fn min_chars(n: usize) -> Rule<str> {
    // chars, not bytes: "é" is 2 bytes but 1 character to the user
    Box::new(move |s| (s.chars().count() < n).then(|| fe("too_short", json!({"min": n}))))
}
fn max_chars(n: usize) -> Rule<str> {
    Box::new(move |s| (s.chars().count() > n).then(|| fe("too_long", json!({"max": n}))))
}
fn one_of(allowed: &'static [&'static str]) -> Rule<str> {
    Box::new(move |s| {
        (!allowed.contains(&s)).then(|| fe("invalid_value", json!({"allowed": allowed})))
    })
}
fn range(lo: i64, hi: i64) -> Rule<i64> {
    Box::new(move |v| {
        (!(lo..=hi).contains(v)).then(|| fe("out_of_range", json!({"min": lo, "max": hi})))
    })
}
fn email() -> Rule<str> {
    Box::new(|s| {
        let ok = s.len() <= 254
            && s.matches('@').count() == 1
            && !s.chars().any(|c| c.is_whitespace() || c.is_control())
            && s.split_once('@').is_some_and(|(l, d)| {
                !l.is_empty()
                    && l.len() <= 64
                    && d.contains('.')
                    && !d.starts_with('.')
                    && !d.ends_with('.')
                    && !d.contains("..")
                    && d.chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '.')
            });
        (!ok).then(|| fe("invalid_email", json!({})))
    })
}

#[derive(Default)]
struct Collector {
    errors: Errors,
}
impl Collector {
    /// First failing rule per field (clean UX: one message per field).
    fn field<T: ?Sized>(&mut self, name: &str, v: &T, rules: &[Rule<T>]) {
        if let Some(e) = rules.iter().find_map(|r| r(v)) {
            self.push(name, e);
        }
    }
    fn push(&mut self, name: &str, e: FieldError) {
        self.errors.entry(name.to_string()).or_default().push(e);
    }
    fn finish<T>(self, ok: T) -> Result<T, Errors> {
        if self.errors.is_empty() {
            Ok(ok)
        } else {
            Err(self.errors)
        }
    }
}

// ---------------------------------------------------------------- problem+json + i18n --------------------------------
fn message(lang: &str, code: &str, p: &Value) -> String {
    let n = |k: &str| p.get(k).map(|v| v.to_string()).unwrap_or_default();
    match (lang, code) {
        ("es", "required") => "es obligatorio".into(),
        ("es", "too_short") => format!("debe tener al menos {} caracteres", n("min")),
        ("es", "too_long") => format!("debe tener como máximo {} caracteres", n("max")),
        ("es", "invalid_email") => "correo electrónico no válido".into(),
        ("es", "out_of_range") => format!("debe estar entre {} y {}", n("min"), n("max")),
        ("es", "invalid_value") => "valor no permitido".into(),
        ("es", "unknown_field") => "campo desconocido".into(),
        (_, "required") => "is required".into(),
        (_, "too_short") => format!("must have at least {} characters", n("min")),
        (_, "too_long") => format!("must have at most {} characters", n("max")),
        (_, "invalid_email") => "is not a valid email address".into(),
        (_, "out_of_range") => format!("must be between {} and {}", n("min"), n("max")),
        (_, "invalid_value") => "is not an allowed value".into(),
        (_, "unknown_field") => "is not a known field".into(),
        (_, "too_many") => format!("at most {} items allowed", n("max")),
        _ => code.to_string(), // unknown code: fall back to the code itself, never panic
    }
}

#[derive(Debug)]
struct Problem {
    status: StatusCode,
    kind: &'static str,
    title: &'static str,
    errors: Errors,
    lang: String,
}
impl Problem {
    fn simple(status: StatusCode, kind: &'static str, title: &'static str) -> Self {
        Problem {
            status,
            kind,
            title,
            errors: Errors::new(),
            lang: "en".into(),
        }
    }
}
impl IntoResponse for Problem {
    fn into_response(self) -> Response {
        let errors: BTreeMap<_, _> = self
            .errors
            .iter()
            .map(|(f, v)| {
                let items: Vec<Value> = v
                    .iter()
                    .map(|e| json!({"code": e.code, "params": e.params, "message": message(&self.lang, e.code, &e.params)}))
                    .collect();
                (f.clone(), items)
            })
            .collect();
        let body = json!({"type": format!("https://example.test/problems/{}", self.kind), "title": self.title,
                          "status": self.status.as_u16(), "errors": errors});
        (
            self.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            body.to_string(),
        )
            .into_response()
    }
}

// ---------------------------------------------------------------- Accept parsing / negotiation -----------------------
fn parse_accept(h: &str) -> Vec<(String, f32)> {
    let mut v: Vec<(String, f32)> = h
        .split(',')
        .filter_map(|part| {
            let mut it = part.split(';');
            let range = it.next()?.trim().to_ascii_lowercase();
            if range.is_empty() {
                return None;
            }
            let q = it
                .filter_map(|p| {
                    p.trim()
                        .strip_prefix("q=")
                        .and_then(|q| q.parse::<f32>().ok())
                })
                .next()
                .unwrap_or(1.0);
            Some((range, q.clamp(0.0, 1.0)))
        })
        .collect();
    v.sort_by(|a, b| b.1.total_cmp(&a.1)); // stable: ties keep client order
    v
}
/// Most specific matching range wins (exact > type/* > */*); q=0 means "not acceptable".
fn negotiate<'a>(accept: &str, supported: &[&'a str]) -> Option<&'a str> {
    let ranges = parse_accept(accept);
    let mut best: Option<(&str, f32)> = None;
    for &s in supported {
        let (ty, _) = s.split_once('/').unwrap_or((s, ""));
        let spec = |r: &str| {
            if r == s {
                3
            } else if r == format!("{ty}/*") {
                2
            } else if r == "*/*" {
                1
            } else {
                0
            }
        };
        let hit = ranges
            .iter()
            .map(|(r, q)| (spec(r), *q))
            .filter(|(sp, _)| *sp > 0)
            .max_by_key(|(sp, _)| *sp);
        if let Some((_, q)) = hit {
            if q > 0.0 && best.is_none_or(|(_, bq)| q > bq) {
                best = Some((s, q));
            }
        }
    }
    best.map(|(s, _)| s)
}
fn lang_from(h: &HeaderMap) -> String {
    let raw = h
        .get(header::ACCEPT_LANGUAGE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    parse_accept(raw)
        .into_iter()
        .filter(|(_, q)| *q > 0.0)
        .map(|(l, _)| l.split('-').next().unwrap_or("").to_string())
        .find(|l| l == "en" || l == "es")
        .unwrap_or_else(|| "en".into())
}

// ---------------------------------------------------------------- body extractor (FromRequest) ------------------------
trait Parse {
    type Raw: DeserializeOwned;
    type Out;
    fn parse(raw: Self::Raw) -> Result<Self::Out, Errors>;
}
struct Valid<P: Parse>(P::Out);

fn classify(e: &serde_json::Error, lang: String) -> Problem {
    use serde_json::error::Category::*;
    let msg = e.to_string();
    // Only at the START of the message: further on it may quote a submitted VALUE, which could contain these very words and
    // so pick the code and be echoed back as a field name.
    let backticked = |marker: &str| {
        msg.strip_prefix(marker)
            .and_then(|r| r.split('`').next())
            .map(str::to_string)
    };
    let mut p = Problem {
        lang,
        ..Problem::simple(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation",
            "Validation failed",
        )
    };
    match e.classify() {
        Syntax | Eof | Io => {
            p.status = StatusCode::BAD_REQUEST;
            p.kind = "malformed_json";
            p.title = "Malformed JSON";
        }
        Data => {
            if let Some(f) = backticked("unknown field `") {
                p.errors.insert(f, vec![fe("unknown_field", json!({}))]);
            } else if let Some(f) = backticked("missing field `") {
                p.errors.insert(f, vec![fe("required", json!({}))]);
            } else {
                // serde_json does not report the path of a type error; `serde_path_to_error` does (production tip).
                p.errors
                    .insert("$body".into(), vec![fe("invalid_type", json!({}))]);
            }
        }
    }
    p
}

impl<S, P> FromRequest<S> for Valid<P>
where
    S: Send + Sync,
    P: Parse,
{
    type Rejection = Problem;
    async fn from_request(req: Request, state: &S) -> Result<Self, Problem> {
        let lang = lang_from(req.headers());
        let is_json = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                // the media type itself, without its parameters: a substring test lets `text/plain; x=+json` through
                let essence = v
                    .split(';')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_ascii_lowercase();
                essence == "application/json"
                    || (essence.starts_with("application/") && essence.ends_with("+json"))
            });
        if !is_json {
            return Err(Problem::simple(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported_media_type",
                "Send application/json",
            ));
        }
        let bytes = Bytes::from_request(req, state).await.map_err(|e| {
            Problem::simple(
                e.status(),
                "body_unreadable",
                "Body too large or unreadable",
            )
        })?;
        let raw: P::Raw = serde_json::from_slice(&bytes).map_err(|e| classify(&e, lang.clone()))?;
        P::parse(raw).map(Valid).map_err(|errors| Problem {
            errors,
            lang,
            ..Problem::simple(
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation",
                "Validation failed",
            )
        })
    }
}

// ---------------------------------------------------------------- domain: strict create + lenient patch ---------------
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Role {
    Admin,
    Member,
    Viewer,
}
#[derive(Debug, PartialEq, Serialize)]
struct User {
    name: String,
    email: String,
    age: Option<u8>,
    role: Role,
    locale: String,
    tags: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)] // strict: typos like "emial" fail loudly instead of being silently dropped
struct RawCreateUser {
    name: String,
    email: String,
    #[serde(default)]
    age: Option<i64>,
    #[serde(default = "default_role")]
    role: String,
    #[serde(default = "default_locale")]
    locale: String,
    #[serde(default)]
    tags: Vec<String>,
}
fn default_role() -> String {
    "member".into()
}
fn default_locale() -> String {
    "en".into()
}
fn norm_email(s: &str) -> String {
    s.trim().to_lowercase()
}
fn norm_name(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ") // trims AND collapses inner runs
}
struct CreateUser;
impl Parse for CreateUser {
    type Raw = RawCreateUser;
    type Out = User;
    fn parse(mut r: RawCreateUser) -> Result<User, Errors> {
        // 1. normalise
        r.name = norm_name(&r.name);
        r.email = norm_email(&r.email);
        r.role = r.role.trim().to_lowercase();
        let mut tags: Vec<String> = r.tags.iter().map(|t| t.trim().to_lowercase()).collect();
        // 2. validate (collect everything)
        let mut c = Collector::default();
        c.field(
            "name",
            r.name.as_str(),
            &[required(), min_chars(2), max_chars(50)],
        );
        c.field("email", r.email.as_str(), &[required(), email()]);
        if let Some(a) = &r.age {
            c.field("age", a, &[range(13, 120)]);
        }
        c.field(
            "role",
            r.role.as_str(),
            &[one_of(&["admin", "member", "viewer"])],
        );
        // checked while the list is still as the client sent it, so `tags[i]` names the client's element
        for (i, t) in tags.iter().enumerate() {
            c.field(&format!("tags[{i}]"), t.as_str(), &[max_chars(20)]);
        }
        tags.retain(|t| !t.is_empty());
        tags.sort();
        tags.dedup();
        if tags.len() > 10 {
            c.push("tags", fe("too_many", json!({"max": 10})));
        }
        c.field("locale", r.locale.as_str(), &[one_of(&["en", "es"])]);
        // 3. convert: the domain type is only constructible after validation
        let role = match r.role.as_str() {
            "admin" => Role::Admin,
            "viewer" => Role::Viewer,
            _ => Role::Member,
        };
        let age = r.age.map(|a| a.clamp(0, 255) as u8);
        c.finish(User {
            name: r.name,
            email: r.email,
            age,
            role,
            locale: r.locale,
            tags,
        })
    }
}

#[derive(Deserialize)] // lenient: no deny_unknown_fields, newer clients may send fields we do not know yet
struct RawPrefs {
    #[serde(default)]
    theme: Option<String>,
}
struct Prefs;
impl Parse for Prefs {
    type Raw = RawPrefs;
    type Out = Option<String>;
    fn parse(r: RawPrefs) -> Result<Option<String>, Errors> {
        let mut c = Collector::default();
        let theme = r.theme.map(|t| t.trim().to_lowercase());
        if let Some(t) = &theme {
            c.field("theme", t.as_str(), &[one_of(&["light", "dark"])]);
        }
        c.finish(theme)
    }
}

// ---------------------------------------------------------------- query extractor (FromRequestParts) -----------------
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len()
                && b[i + 1].is_ascii_hexdigit()
                && b[i + 2].is_ascii_hexdigit() =>
            {
                // byte-wise hex parse: slicing the &str here could split a multi-byte char and panic
                let h = |c: u8| (c as char).to_digit(16).unwrap() as u8;
                out.push(h(b[i + 1]) * 16 + h(b[i + 2]));
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
fn parse_query(q: &str) -> Vec<(String, String)> {
    q.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (pct_decode(k), pct_decode(v))
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
enum Status {
    Active,
    Suspended,
}
impl std::str::FromStr for Status {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        match s.to_ascii_lowercase().as_str() {
            "active" => Ok(Status::Active),
            "suspended" => Ok(Status::Suspended),
            _ => Err(()),
        }
    }
}
#[derive(Debug, PartialEq, Serialize)]
struct ListParams {
    tags: Vec<String>,
    statuses: Vec<Status>,
    limit: u32,
    sort: String,
    desc: bool,
}
impl ListParams {
    fn from_query(q: &str) -> Result<Self, Errors> {
        let pairs = parse_query(q);
        let mut c = Collector::default();
        let tags: Vec<String> = pairs
            .iter()
            .filter(|(k, _)| k == "tag" || k == "tag[]")
            .map(|(_, v)| v.to_lowercase())
            .collect();
        let mut statuses = vec![];
        for (_, v) in pairs.iter().filter(|(k, _)| k == "status") {
            for part in v.split(',').filter(|s| !s.is_empty()) {
                match part.parse::<Status>() {
                    Ok(s) => statuses.push(s),
                    Err(()) => c.push(
                        "status",
                        fe("invalid_value", json!({"allowed": ["active", "suspended"]})),
                    ),
                }
            }
        }
        let mut limit = 20u32;
        if let Some((_, v)) = pairs.iter().find(|(k, _)| k == "limit") {
            match v.parse::<i64>() {
                Ok(n) => limit = n.clamp(1, 100) as u32, // policy: CLAMP instead of reject (documented in tips)
                Err(_) => c.push("limit", fe("invalid_value", json!({}))),
            }
        }
        let sort_raw = pairs
            .iter()
            .find(|(k, _)| k == "sort")
            .map(|(_, v)| v.as_str())
            .unwrap_or("name");
        let (desc, sort) = match sort_raw.strip_prefix('-') {
            Some(s) => (true, s),
            None => (false, sort_raw),
        };
        c.field("sort", sort, &[one_of(&["name", "created"])]);
        c.finish(ListParams {
            tags,
            statuses,
            limit,
            sort: sort.to_string(),
            desc,
        })
    }
}
impl<S: Send + Sync> FromRequestParts<S> for ListParams {
    type Rejection = Problem;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Problem> {
        let lang = lang_from(&parts.headers);
        ListParams::from_query(parts.uri.query().unwrap_or("")).map_err(|errors| Problem {
            errors,
            lang,
            ..Problem::simple(StatusCode::BAD_REQUEST, "bad_query", "Invalid query string")
        })
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Format {
    Json,
    Csv,
}
impl<S: Send + Sync> FromRequestParts<S> for Format {
    type Rejection = Problem;
    async fn from_request_parts(parts: &mut Parts, _: &S) -> Result<Self, Problem> {
        let accept = parts
            .headers
            .get(header::ACCEPT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("*/*");
        match negotiate(accept, &["application/json", "text/csv"]) {
            Some("text/csv") => Ok(Format::Csv),
            Some(_) => Ok(Format::Json),
            None => Err(Problem::simple(
                StatusCode::NOT_ACCEPTABLE,
                "not_acceptable",
                "Supported: application/json, text/csv",
            )),
        }
    }
}

// ---------------------------------------------------------------- handlers + app -------------------------------------
async fn create_user(Valid(u): Valid<CreateUser>) -> (StatusCode, axum::Json<User>) {
    (StatusCode::CREATED, axum::Json(u))
}
async fn patch_prefs(Valid(theme): Valid<Prefs>) -> axum::Json<Value> {
    axum::Json(json!({"theme": theme}))
}
async fn list_users(p: ListParams, fmt: Format) -> Response {
    let vary = [(header::VARY, "Accept")]; // caches must key on Accept
    match fmt {
        Format::Json => (vary, axum::Json(&p)).into_response(),
        Format::Csv => (
            vary,
            [(header::CONTENT_TYPE, "text/csv")],
            format!("limit,sort\n{},{}\n", p.limit, p.sort),
        )
            .into_response(),
    }
}
fn app() -> Router {
    Router::new()
        .route("/users", post(create_user).get(list_users))
        .route("/prefs", patch(patch_prefs))
        .layer(DefaultBodyLimit::max(512)) // 512 B for the demo; real APIs: 64 KiB-1 MiB for JSON, bigger only on upload routes
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    hdrs: &[(&str, &str)],
    body: Option<&str>,
) -> (StatusCode, HeaderMap, Value, String) {
    let mut b = Request::builder().method(method).uri(uri);
    for (k, v) in hdrs {
        b = b.header(*k, *v);
    }
    let req = b.body(Body::from(body.unwrap_or("").to_string())).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let (parts, body) = resp.into_parts();
    let bytes = body.collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).into_owned();
    (
        parts.status,
        parts.headers,
        serde_json::from_str(&text).unwrap_or(Value::Null),
        text,
    )
}
const JSON: (&str, &str) = ("content-type", "application/json");

// ---------------------------------------------------------------- sections ------------------------------------------
async fn everyday_core() {
    println!("== Everyday core: valid body, field-keyed errors, defaults, normalisation");
    let app = app();
    let (st, _, body, _) = call(&app, "POST", "/users", &[JSON], Some(r#"{"name":"  Ada   Lovelace ","email":" Ada@Example.COM ","tags":["Rust"," rust ","","Web"]}"#)).await;
    assert_eq!(st, StatusCode::CREATED);
    assert_eq!(body["name"], "Ada Lovelace");
    assert_eq!(body["email"], "ada@example.com");
    assert_eq!(
        (&body["role"], &body["locale"]),
        (&json!("member"), &json!("en"))
    ); // defaults applied
    assert_eq!(body["tags"], json!(["rust", "web"])); // trimmed, folded, de-duplicated, sorted

    let (st, h, body, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"name":" ","email":"nope","age":200,"role":"root"}"#),
    )
    .await;
    assert_eq!(st, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(h[header::CONTENT_TYPE], "application/problem+json");
    let keys: Vec<_> = body["errors"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    assert_eq!(keys, ["age", "email", "name", "role"]); // ALL bad fields reported at once
    assert_eq!(body["errors"]["name"][0]["code"], "required"); // first failing rule only
    assert_eq!(
        body["errors"]["age"][0]["params"],
        json!({"min": 13, "max": 120})
    );
    println!("   errors map keys = {keys:?}");

    let (st, _, body, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"email":"a@b.co"}"#),
    )
    .await;
    assert_eq!(
        (st, &body["errors"]["name"][0]["code"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("required"))
    ); // missing field
}

async fn intermediate() {
    println!(
        "== Intermediate: unknown-field policy, 415/400/413, query arrays/enums, negotiation, i18n"
    );
    let app = app();
    let (st, _, body, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"name":"Bob","emial":"x@y.co","email":"x@y.co"}"#),
    )
    .await;
    assert_eq!(
        (st, &body["errors"]["emial"][0]["code"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("unknown_field"))
    );
    let (st, _, body, _) = call(
        &app,
        "PATCH",
        "/prefs",
        &[JSON],
        Some(r#"{"theme":"Dark","future_flag":true}"#),
    )
    .await;
    assert_eq!((st, &body["theme"]), (StatusCode::OK, &json!("dark"))); // lenient endpoint ignores what it does not know

    let (st, ..) = call(
        &app,
        "POST",
        "/users",
        &[("content-type", "text/plain")],
        Some("hi"),
    )
    .await;
    assert_eq!(st, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let (st, _, body, _) = call(&app, "POST", "/users", &[JSON], Some("{not json")).await;
    assert_eq!(
        (st, body["status"].as_u64()),
        (StatusCode::BAD_REQUEST, Some(400))
    );
    let big = format!(r#"{{"name":"{}","email":"a@b.co"}}"#, "x".repeat(1000));
    let (st, ..) = call(&app, "POST", "/users", &[JSON], Some(&big)).await;
    assert_eq!(st, StatusCode::PAYLOAD_TOO_LARGE);
    let (st, _, body, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"name":"Bob","email":"a@b.co","age":"12"}"#),
    )
    .await;
    assert_eq!(
        (st, &body["errors"]["$body"][0]["code"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("invalid_type"))
    );

    let (st, _, body, _) = call(
        &app,
        "GET",
        "/users?tag=A&tag=b&status=active,SUSPENDED&limit=1000&sort=-created",
        &[],
        None,
    )
    .await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(
        body,
        json!({"tags":["a","b"],"statuses":["Active","Suspended"],"limit":100,"sort":"created","desc":true})
    );
    let (st, _, body, _) = call(&app, "GET", "/users?status=zombie&sort=password", &[], None).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    assert!(body["errors"].get("status").is_some() && body["errors"].get("sort").is_some());

    let (_, h, _, text) = call(
        &app,
        "GET",
        "/users",
        &[("accept", "text/csv;q=0.9, application/json;q=0.1")],
        None,
    )
    .await;
    assert_eq!(
        (
            h[header::CONTENT_TYPE].to_str().unwrap(),
            h[header::VARY].to_str().unwrap()
        ),
        ("text/csv", "Accept")
    );
    assert!(text.starts_with("limit,sort"));
    let (st, ..) = call(&app, "GET", "/users", &[("accept", "image/png")], None).await;
    assert_eq!(st, StatusCode::NOT_ACCEPTABLE);

    let (_, _, body, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON, ("accept-language", "es-MX,es;q=0.9,en;q=0.5")],
        Some(r#"{"name":"A","email":"x"}"#),
    )
    .await;
    assert_eq!(body["errors"]["name"][0]["code"], "too_short"); // code is stable ...
    assert_eq!(
        body["errors"]["name"][0]["message"],
        "debe tener al menos 2 caracteres"
    ); // ... message is localised
    println!("   i18n: {}", body["errors"]["email"][0]["message"]);
}

async fn less_known() {
    println!("== Less known: pure negotiation table, percent-decoding edge cases, seeded fuzz");
    let sup = ["application/json", "text/csv"];
    assert_eq!(negotiate("*/*", &sup), Some("application/json")); // tie -> server preference order
    assert_eq!(negotiate("text/*", &sup), Some("text/csv"));
    assert_eq!(
        negotiate("application/json;q=0, */*;q=0.1", &sup),
        Some("text/csv")
    ); // explicit q=0 excludes
    assert_eq!(negotiate("text/html", &sup), None);
    assert_eq!(pct_decode("a%20b+c%zz%4"), "a b c%zz%4"); // malformed escapes survive, no panic
    assert_eq!(pct_decode("%E2%9C%93"), "✓");

    let mut x = 0x9E3779B97F4A7C15u64;
    let mut next = move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        x
    };
    let alphabet: Vec<char> = "aAbB09 @.-_é\u{3000}\t+%ß".chars().collect();
    let mut accepted = 0;
    for _ in 0..3000 {
        let mut s = |n: usize| {
            (0..n)
                .map(|_| alphabet[next() as usize % alphabet.len()])
                .collect::<String>()
        };
        let (n, e) = (s(12), s(14));
        assert_eq!(norm_email(&norm_email(&e)), norm_email(&e)); // normalisation is idempotent
        assert_eq!(norm_name(&norm_name(&n)), norm_name(&n));
        let raw = RawCreateUser {
            name: n,
            email: e,
            age: None,
            role: "member".into(),
            locale: "en".into(),
            tags: vec![],
        };
        if let Ok(u) = CreateUser::parse(raw) {
            accepted += 1;
            assert_eq!(u.email.matches('@').count(), 1); // invariant of the validated domain type
            assert!((2..=50).contains(&u.name.chars().count()));
            assert_eq!(u.email, norm_email(&u.email));
        }
    }
    // the loop's invariants are only checked on accepted inputs, and random strings are rarely a valid e-mail address
    let known = RawCreateUser {
        name: " Zoë  ß ".into(),
        email: " A@B.Co ".into(),
        age: None,
        role: "member".into(),
        locale: "en".into(),
        tags: vec![],
    };
    assert!(CreateUser::parse(known).is_ok());
    println!("   fuzz: 3000 random inputs, {accepted} accepted, 0 panics");
}

async fn production_tips() {
    println!("== Production tips and tricks");
    let app = app();
    // 1. Parse, don't validate: User only exists via CreateUser::parse (see everyday_core). WHY: handlers cannot forget a check.
    // 2. Report ALL field errors in one round trip. WHY: forms with 5 bad fields should not need 5 submits.
    let (_, _, b, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"name":"","email":"","age":1}"#),
    )
    .await;
    assert_eq!(b["errors"].as_object().unwrap().len(), 3);
    // 3. Never echo submitted values (passwords, tokens) in errors or logs. WHY: error bodies end up in logs/Sentry.
    let (_, _, _, text) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"name":"x","email":"hunter2-secret","age":1}"#),
    )
    .await;
    assert!(!text.contains("hunter2-secret"));
    // 4. Stable `code` + `params`, localised `message` only at the edge. WHY: clients branch on codes; copy changes without breaking them.
    // 5. Count chars not bytes, but also cap BYTES at the body limit. WHY: "é"*50 is 100 bytes.
    assert!("é".repeat(50).len() == 100 && "é".repeat(50).chars().count() == 50);
    // 6. Limit size BEFORE parsing (DefaultBodyLimit). WHY: unbounded JSON is a memory DoS; also cap collection sizes (tags <= 10).
    let many = format!(
        r#"{{"name":"Bob","email":"a@b.co","tags":[{}]}}"#,
        (0..12)
            .map(|i| format!("\"t{i}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let (st, _, b, _) = call(&app, "POST", "/users", &[JSON], Some(&many)).await;
    assert_eq!(
        (st, &b["errors"]["tags"][0]["code"]),
        (StatusCode::UNPROCESSABLE_ENTITY, &json!("too_many"))
    );
    // 7. Clamp vs reject is a documented policy: `limit=0` -> 1, `limit=abc` -> 400. WHY: clients get predictable paging.
    assert_eq!(ListParams::from_query("limit=0").unwrap().limit, 1);
    assert!(ListParams::from_query("limit=abc").is_err());
    // 8. Allow-list sort/filter columns; never interpolate user text. WHY: sort=password must be a 400, not an ORDER BY.
    assert!(ListParams::from_query("sort=password").is_err());
    // 9. `Vary: Accept` on negotiated responses. WHY: a CDN would otherwise serve CSV to a JSON client (see http_caching_conditional).
    // 10. deny_unknown_fields only on endpoints you control end-to-end; lenient for webhooks/partners. WHY: forward compatibility.
    // 11. Extractor order: body extractor LAST, FromRequestParts first. WHY: the body can be consumed once.
    // 12. Anti-pattern: `#[derive(Deserialize)] struct User` used directly as the domain type lets `{"role":"admin"}` through
    //     (mass assignment). Use a Raw DTO that has only client-settable fields. clippy cannot catch this; review does.
    let (_, _, b, _) = call(
        &app,
        "POST",
        "/users",
        &[JSON],
        Some(r#"{"name":"Eve","email":"e@x.co","is_admin":true}"#),
    )
    .await;
    assert_eq!(b["errors"]["is_admin"][0]["code"], "unknown_field");
    // a value that imitates serde's own wording must not choose the error or come back in it
    for age in [
        "unknown field `hunter2-secret",
        "missing field `hunter2-secret",
    ] {
        let body = format!(r#"{{"name":"Bob","email":"a@b.co","age":"{age}"}}"#);
        let (_, _, b, text) = call(&app, "POST", "/users", &[JSON], Some(&body)).await;
        assert!(!text.contains("hunter2-secret"), "echoed: {text}");
        assert_eq!(b["errors"]["$body"][0]["code"], "invalid_type");
    }
    // the media type decides, not a substring of the header; its case does not matter
    for (content_type, ok) in [
        ("text/plain; x=+json", false),
        ("application/jsonp", false),
        ("Application/JSON; charset=utf-8", true),
        ("application/merge-patch+json", true),
    ] {
        let headers = [("content-type", content_type)];
        let (st, ..) = call(&app, "PATCH", "/prefs", &headers, Some("{}")).await;
        let want = if ok {
            StatusCode::OK
        } else {
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        };
        assert_eq!(st, want, "{content_type}");
    }
    // `tags[i]` is the position in the request, although the stored list is sorted
    let body = format!(
        r#"{{"name":"Bob","email":"a@b.co","tags":["b","{}","a"]}}"#,
        "z".repeat(21)
    );
    let (_, _, b, _) = call(&app, "POST", "/users", &[JSON], Some(&body)).await;
    assert!(b["errors"].get("tags[1]").is_some(), "{b}");
    let body = r#"{"name":"Bob","email":"a@b.co","locale":"<script>"}"#;
    let (_, _, b, _) = call(&app, "POST", "/users", &[JSON], Some(body)).await;
    assert_eq!(b["errors"]["locale"][0]["code"], "invalid_value");
    println!("   12 tips asserted");
}

#[tokio::main]
async fn main() {
    everyday_core().await;
    intermediate().await;
    less_known().await;
    production_tips().await;
    println!("OK");
}
