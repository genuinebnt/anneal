//! RECIPE 1201 (web-backend): authorisation: RBAC + ABAC + ownership + tenant scoping as a pure, truth-table-tested policy.
//! LEVEL: advanced
//! Use in: any multi-user API (who may do WHAT to WHICH resource)   Run: cargo run --release --example backend_authz_rbac_abac_policies
//! COMMON USES: SaaS admin consoles, document/project tools, internal platforms, API keys with scopes, audit-sensitive systems.
#![allow(dead_code)]
//!
//! Extends: auth_basic_bearer_jwt (that recipe answers "who are you?"; this one answers "what may you do?"), axum_middleware.
//!
//! Model: `decide(&Request) -> Decision` is a PURE function: no I/O, no clock (the hour comes in via context), so it can be
//! exhaustively tested with a truth table and a differential oracle. Rules are `Allow` or `Deny`; DENY OVERRIDES; default is deny.
//! HTTP glue: middleware does authentication + coarse route checks ("must be admin"), the handler does the fine resource check
//! (ownership/tenant), because only the handler has loaded the resource. IDOR (insecure direct object reference) is shown as an
//! exploit test against a vulnerable route and the fixed route. Every decision is appended to an audit log with the rule names.
//!
//! Cheat sheet (I want to ... -> use ...):
//!   401 vs 403 vs 404            -> no/invalid credential = 401 (+WWW-Authenticate); known but not allowed = 403; hide existence = 404
//!   role hierarchy               -> derive `Ord` on the enum; perms(role) = union of perms of every role <= role
//!   resource ownership           -> rule `owner => read/write` that is still subject to deny rules
//!   tenant isolation             -> a DENY rule `resource.tenant != subject.tenant` (and also scope the SQL, see backend_multi_tenancy_patterns)
//!   attribute rules              -> deny classification > clearance, deny writes on archived, deny delete after hours
//!   API key scopes               -> effective permission = user permission AND key scope (intersection, never union)
//!   coarse check on a route group-> `route_layer(from_fn(require_role))`
//!   fine check on a resource     -> call `authorize()` inside the handler after loading the resource
//!   audit                        -> append (subject, action, resource, effect, rules) for allow AND deny
//!   production crates            -> `casbin`, `oso`, `cedar-policy` (AWS Cedar), `openfga` clients; OPA/Rego as a sidecar
//!
//! Read: OWASP Authorization Cheat Sheet (https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html),
//! OWASP API Security Top 10 "Broken Object Level Authorization" (title only), Google Zanzibar paper (title only), AWS Cedar docs (not fetched).
//!
//! TRY NEXT:
//!   - [S] Add a `Role::Auditor` that has `doc:read` on every doc in the tenant but may not write; extend the truth table and see what breaks.
//!   - [S] Predict then measure: flip rule order in `RULES`; does any decision change? Why is deny-overrides order independent?
//!   - [M] Add a `shared_with: Vec<UserId>` attribute (ReBAC-lite) and a rule granting read; add the IDOR exploit test for it.
//!   - [M] Add decision caching keyed by (subject, action, resource, policy_version) and a test that a policy bump invalidates it.
//!   - [A] PROJECT: combine with backend_multi_tenancy_patterns (typed scoped repo) and backend_api_keys_tokens_rotation (scoped keys) into one service.
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Path, Request, State},
    http::{StatusCode, header},
    middleware::{Next, from_fn, from_fn_with_state},
    response::{IntoResponse, Response},
    routing::{delete, get},
};
use http_body_util::BodyExt;
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use tower::ServiceExt;

// ------------------------------------------------------------------ domain types (newtypes) -------------------------
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct UserId(u32);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TenantId(u32);

/// Ord = hierarchy: each role inherits everything below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Role {
    Viewer,
    Editor,
    Admin,
    Owner,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Action {
    Read,
    Write,
    Delete,
    Stats,
}
impl Action {
    fn perm(self) -> &'static str {
        match self {
            Action::Read => "doc:read",
            Action::Write => "doc:write",
            Action::Delete => "doc:delete",
            Action::Stats => "admin:stats",
        }
    }
}
/// RBAC table: the permissions a role adds on top of the roles below it.
fn role_adds(r: Role) -> &'static [&'static str] {
    match r {
        Role::Viewer => &["doc:read"],
        Role::Editor => &["doc:write"],
        Role::Admin => &["doc:delete", "admin:stats"],
        Role::Owner => &["billing:manage"],
    }
}
fn role_has(r: Role, perm: &str) -> bool {
    [Role::Viewer, Role::Editor, Role::Admin, Role::Owner]
        .iter()
        .filter(|x| **x <= r)
        .any(|x| role_adds(*x).contains(&perm))
}

#[derive(Debug, Clone)]
struct Subject {
    id: UserId,
    tenant: TenantId,
    role: Role,
    clearance: u8,
    /// `Some` when the caller used an API key: the key can only NARROW what the user may do.
    scopes: Option<BTreeSet<String>>,
}
#[derive(Debug, Clone)]
struct Doc {
    id: u32,
    tenant: TenantId,
    owner: UserId,
    classification: u8,
    archived: bool,
    title: String,
}
#[derive(Debug, Clone, Copy)]
struct Ctx {
    hour: u32, // injected clock: policy stays pure
}
struct Req<'a> {
    subject: &'a Subject,
    action: Action,
    resource: Option<&'a Doc>,
    ctx: Ctx,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Effect {
    Allow,
    Deny,
}
#[derive(Debug, Clone)]
struct Decision {
    allowed: bool,
    allow_rules: Vec<&'static str>,
    deny_rules: Vec<&'static str>,
}

type RuleFn = fn(&Req) -> bool;
/// (name, effect, predicate "does this rule apply?")
const RULES: &[(&str, Effect, RuleFn)] = &[
    ("rbac", Effect::Allow, |r| {
        role_has(r.subject.role, r.action.perm())
    }),
    ("owner", Effect::Allow, |r| {
        matches!(r.action, Action::Read | Action::Write)
            && r.resource.is_some_and(|d| d.owner == r.subject.id)
    }),
    ("cross_tenant", Effect::Deny, |r| {
        r.resource.is_some_and(|d| d.tenant != r.subject.tenant)
    }),
    ("classification", Effect::Deny, |r| {
        r.resource
            .is_some_and(|d| d.classification > r.subject.clearance)
    }),
    ("archived_readonly", Effect::Deny, |r| {
        matches!(r.action, Action::Write | Action::Delete) && r.resource.is_some_and(|d| d.archived)
    }),
    ("after_hours_delete", Effect::Deny, |r| {
        r.action == Action::Delete && !(8..18).contains(&r.ctx.hour) && r.subject.role < Role::Owner
    }),
    ("scope", Effect::Deny, |r| {
        r.subject
            .scopes
            .as_ref()
            .is_some_and(|s| !s.contains(r.action.perm()))
    }),
];

fn decide(r: &Req) -> Decision {
    let mut d = Decision {
        allowed: false,
        allow_rules: vec![],
        deny_rules: vec![],
    };
    for (name, effect, applies) in RULES {
        if applies(r) {
            match effect {
                Effect::Allow => d.allow_rules.push(name),
                Effect::Deny => d.deny_rules.push(name),
            }
        }
    }
    d.allowed = d.deny_rules.is_empty() && !d.allow_rules.is_empty(); // deny overrides; default deny
    d
}

// ------------------------------------------------------------------ app state + audit --------------------------------
#[derive(Debug, Clone)]
struct AuditEntry {
    subject: UserId,
    action: Action,
    resource: Option<u32>,
    allowed: bool,
    rules: Vec<&'static str>,
}
struct App {
    docs: Mutex<HashMap<u32, Doc>>,
    tokens: HashMap<&'static str, Subject>,
    audit: Mutex<Vec<AuditEntry>>,
    hour: AtomicU32,
}
impl App {
    fn authorize(&self, s: &Subject, action: Action, doc: Option<&Doc>) -> Decision {
        let ctx = Ctx {
            hour: self.hour.load(Ordering::Relaxed),
        };
        let d = decide(&Req {
            subject: s,
            action,
            resource: doc,
            ctx,
        });
        let rules = if d.allowed {
            d.allow_rules.clone()
        } else {
            d.deny_rules.clone()
        };
        self.audit.lock().push(AuditEntry {
            subject: s.id,
            action,
            resource: doc.map(|d| d.id),
            allowed: d.allowed,
            rules,
        });
        d
    }
}

fn subject(id: u32, tenant: u32, role: Role, clearance: u8, scopes: Option<&[&str]>) -> Subject {
    Subject {
        id: UserId(id),
        tenant: TenantId(tenant),
        role,
        clearance,
        scopes: scopes.map(|s| s.iter().map(|x| x.to_string()).collect()),
    }
}
fn seed() -> Arc<App> {
    let doc = |id, tenant, owner, class, title: &str| {
        (
            id,
            Doc {
                id,
                tenant: TenantId(tenant),
                owner: UserId(owner),
                classification: class,
                archived: false,
                title: title.into(),
            },
        )
    };
    let tokens = HashMap::from([
        ("tok-alice", subject(1, 1, Role::Editor, 1, None)),
        ("tok-bob", subject(2, 1, Role::Admin, 3, None)),
        ("tok-carol", subject(3, 2, Role::Admin, 3, None)),
        ("tok-dave", subject(4, 1, Role::Viewer, 1, None)),
        (
            "key-alice-ro",
            subject(1, 1, Role::Editor, 1, Some(&["doc:read"])),
        ),
    ]);
    Arc::new(App {
        docs: Mutex::new(HashMap::from([
            doc(1, 1, 1, 0, "alice's plan"),
            doc(2, 1, 2, 3, "bob's secret"),
            doc(3, 2, 3, 0, "carol's (other tenant)"),
        ])),
        tokens,
        audit: Mutex::new(vec![]),
        hour: AtomicU32::new(10),
    })
}

// ------------------------------------------------------------------ HTTP glue ----------------------------------------
/// Authentication middleware: identity comes ONLY from the credential. Headers like X-Tenant / X-Role are never read.
async fn authn(State(app): State<Arc<App>>, mut req: Request, next: Next) -> Response {
    let tok = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match tok.and_then(|t| app.tokens.get(t)) {
        Some(s) => {
            req.extensions_mut().insert(s.clone());
            next.run(req).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Bearer")],
            "unauthenticated",
        )
            .into_response(),
    }
}
/// Coarse route-level check: cheap, needs no resource. Fine-grained checks stay in handlers.
async fn require_admin(Extension(s): Extension<Subject>, req: Request, next: Next) -> Response {
    if s.role >= Role::Admin {
        next.run(req).await
    } else {
        (StatusCode::FORBIDDEN, "admin only").into_response()
    }
}

/// VULNERABLE (on purpose): authenticated is not the same as authorised. Any user can read any doc id.
async fn get_doc_v0(State(app): State<Arc<App>>, Path(id): Path<u32>) -> Response {
    match app.docs.lock().get(&id) {
        Some(d) => Json(json!({"id": d.id, "title": d.title})).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
/// FIXED: load, authorise per resource, and conceal existence (404) when the caller may not even read it.
async fn get_doc_v1(
    State(app): State<Arc<App>>,
    Extension(s): Extension<Subject>,
    Path(id): Path<u32>,
) -> Response {
    let doc = app.docs.lock().get(&id).cloned();
    let Some(d) = doc else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if app.authorize(&s, Action::Read, Some(&d)).allowed {
        Json(json!({"id": d.id, "title": d.title})).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response() // 404, not 403: do not confirm that id 3 exists in another tenant
    }
}
async fn delete_doc(
    State(app): State<Arc<App>>,
    Extension(s): Extension<Subject>,
    Path(id): Path<u32>,
) -> Response {
    let doc = app.docs.lock().get(&id).cloned();
    let Some(d) = doc else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if !app.authorize(&s, Action::Read, Some(&d)).allowed {
        return StatusCode::NOT_FOUND.into_response(); // cannot see it => cannot know it exists
    }
    if !app.authorize(&s, Action::Delete, Some(&d)).allowed {
        return StatusCode::FORBIDDEN.into_response(); // can see it, may not delete it
    }
    app.docs.lock().remove(&id);
    StatusCode::NO_CONTENT.into_response()
}
async fn stats(State(app): State<Arc<App>>, Extension(s): Extension<Subject>) -> Response {
    if !app.authorize(&s, Action::Stats, None).allowed {
        return StatusCode::FORBIDDEN.into_response(); // defence in depth: handler re-checks even though the route has a guard
    }
    Json(json!({"docs": app.docs.lock().len()})).into_response()
}
fn router(app: Arc<App>) -> Router {
    let admin = Router::new()
        .route("/admin/stats", get(stats))
        .route_layer(from_fn(require_admin));
    Router::new()
        .route("/v0/docs/{id}", get(get_doc_v0))
        .route("/v1/docs/{id}", get(get_doc_v1).merge(delete(delete_doc)))
        .merge(admin)
        .layer(from_fn_with_state(app.clone(), authn))
        .with_state(app)
}
async fn call(
    r: &Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    extra: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    for (k, v) in extra {
        b = b.header(*k, *v);
    }
    let resp = r
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let st = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (st, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

// ------------------------------------------------------------------ everyday core ------------------------------------
async fn everyday_core() {
    println!("== Everyday core: 401/403/404, RBAC hierarchy, ownership, audit");
    let app = seed();
    let r = router(app.clone());
    assert_eq!(
        call(&r, "GET", "/v1/docs/1", None, &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&r, "GET", "/v1/docs/1", Some("nope"), &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        call(&r, "GET", "/v1/docs/1", Some("tok-dave"), &[]).await.0,
        StatusCode::OK
    ); // viewer reads
    assert_eq!(
        call(&r, "GET", "/admin/stats", Some("tok-alice"), &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    ); // route guard
    assert_eq!(
        call(&r, "GET", "/admin/stats", Some("tok-bob"), &[])
            .await
            .0,
        StatusCode::OK
    );
    // hierarchy: admin inherits viewer+editor perms
    assert!(
        role_has(Role::Admin, "doc:read")
            && role_has(Role::Admin, "doc:write")
            && !role_has(Role::Editor, "doc:delete")
    );
    // delete: viewer can read doc 1 but not delete -> 403; stranger gets 404 for the secret doc
    assert_eq!(
        call(&r, "DELETE", "/v1/docs/1", Some("tok-dave"), &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        call(&r, "DELETE", "/v1/docs/2", Some("tok-dave"), &[])
            .await
            .0,
        StatusCode::NOT_FOUND
    ); // clearance too low: concealed
    // audit contains both allow and deny with rule names
    let audit = app.audit.lock().clone();
    assert!(audit.iter().any(|a| a.allowed && a.rules == ["rbac"]));
    assert!(
        audit
            .iter()
            .any(|a| !a.allowed && a.rules.contains(&"classification"))
    );
    println!("   audit entries so far: {}", audit.len());
}

// ------------------------------------------------------------------ intermediate -------------------------------------
async fn intermediate() {
    println!(
        "== Intermediate: IDOR exploit, spoofed headers, tenant scoping, API-key scopes, after-hours rule"
    );
    let app = seed();
    let r = router(app.clone());
    // EXPLOIT: alice (tenant 1) enumerates ids on the vulnerable route and reads carol's tenant-2 doc.
    let leaked: Vec<u64> = {
        let mut v = vec![];
        for id in 1..=3 {
            let (st, body) =
                call(&r, "GET", &format!("/v0/docs/{id}"), Some("tok-dave"), &[]).await;
            if st == StatusCode::OK && id != 1 {
                v.push(body["id"].as_u64().unwrap());
            }
        }
        v
    };
    assert_eq!(
        leaked,
        [2, 3],
        "v0 leaks other users' and other tenants' docs (the bug)"
    );
    // FIX: same enumeration on v1 only ever returns what dave may read.
    let mut visible = vec![];
    for id in 1..=3 {
        if call(&r, "GET", &format!("/v1/docs/{id}"), Some("tok-dave"), &[])
            .await
            .0
            == StatusCode::OK
        {
            visible.push(id);
        }
    }
    assert_eq!(visible, [1]);
    // 404 for "exists in another tenant" and "does not exist" must be indistinguishable
    let a = call(&r, "GET", "/v1/docs/3", Some("tok-alice"), &[]).await;
    let b = call(&r, "GET", "/v1/docs/999", Some("tok-alice"), &[]).await;
    assert_eq!(a, b);
    // client-supplied identity headers are ignored: carol cannot become tenant 1
    let (st, _) = call(
        &r,
        "GET",
        "/v1/docs/1",
        Some("tok-carol"),
        &[("x-tenant-id", "1"), ("x-role", "owner")],
    )
    .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    // API key narrowed to doc:read: alice may read but a write-ish delete is denied by the `scope` rule
    assert_eq!(
        call(&r, "GET", "/v1/docs/1", Some("key-alice-ro"), &[])
            .await
            .0,
        StatusCode::OK
    );
    let d = decide(&Req {
        subject: &app.tokens["key-alice-ro"],
        action: Action::Write,
        resource: app.docs.lock().get(&1),
        ctx: Ctx { hour: 10 },
    });
    assert!(!d.allowed && d.deny_rules == ["scope"] && d.allow_rules == ["rbac", "owner"]);
    // after-hours delete rule (clock injected)
    app.hour.store(23, Ordering::Relaxed);
    assert_eq!(
        call(&r, "DELETE", "/v1/docs/1", Some("tok-bob"), &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    app.hour.store(9, Ordering::Relaxed);
    assert_eq!(
        call(&r, "DELETE", "/v1/docs/1", Some("tok-bob"), &[])
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    println!("   v0 leaked {:?}; v1 visible {:?}", leaked, visible);
}

// ------------------------------------------------------------------ less known: truth table + oracle -----------------
fn oracle(
    role: Role,
    action: Action,
    owner: bool,
    same_tenant: bool,
    clear_ok: bool,
    archived: bool,
    hour: u32,
    scope: Option<bool>,
) -> bool {
    let rbac = match action {
        Action::Read => true,
        Action::Write => role >= Role::Editor,
        Action::Delete | Action::Stats => role >= Role::Admin,
    };
    let owner_ok = owner && matches!(action, Action::Read | Action::Write);
    let blocked = !same_tenant
        || !clear_ok
        || (archived && matches!(action, Action::Write | Action::Delete))
        || (action == Action::Delete && !(8..18).contains(&hour) && role < Role::Owner)
        || scope == Some(false);
    (rbac || owner_ok) && !blocked
}

fn less_known() {
    println!("== Less known: exhaustive truth table vs independent oracle + properties");
    let roles = [Role::Viewer, Role::Editor, Role::Admin, Role::Owner];
    let actions = [Action::Read, Action::Write, Action::Delete];
    let mut cases = 0;
    let mut allowed_by_role = HashMap::new();
    for role in roles {
        for action in actions {
            for (owner, same, clear, archived, hour, scope) in itertools_product() {
                let s = Subject {
                    id: UserId(1),
                    tenant: TenantId(1),
                    role,
                    clearance: if clear { 2 } else { 0 },
                    scopes: scope.map(|has| {
                        if has {
                            BTreeSet::from([action.perm().to_string()])
                        } else {
                            BTreeSet::new()
                        }
                    }),
                };
                let d = Doc {
                    id: 9,
                    tenant: TenantId(if same { 1 } else { 2 }),
                    owner: UserId(if owner { 1 } else { 7 }),
                    classification: 1,
                    archived,
                    title: String::new(),
                };
                let got = decide(&Req {
                    subject: &s,
                    action,
                    resource: Some(&d),
                    ctx: Ctx { hour },
                })
                .allowed;
                assert_eq!(
                    got,
                    oracle(role, action, owner, same, clear, archived, hour, scope),
                    "{role:?} {action:?} owner={owner} same={same} clear={clear} arch={archived} h={hour} scope={scope:?}"
                );
                // property: cross-tenant is NEVER allowed, whatever else holds
                if !same {
                    assert!(!got);
                }
                *allowed_by_role.entry((role, action)).or_insert(0u32) += got as u32;
                cases += 1;
            }
        }
    }
    // property: role monotonicity for permissions that do not depend on the after-hours exception
    for action in [Action::Read, Action::Write] {
        for w in roles.windows(2) {
            assert!(allowed_by_role[&(w[1], action)] >= allowed_by_role[&(w[0], action)]);
        }
    }
    println!(
        "   {cases} combinations match the oracle; allowed counts (Viewer..Owner, Delete): {:?}",
        roles.map(|r| allowed_by_role[&(r, Action::Delete)])
    );
    println!("   role x action grid (plain case: same tenant, cleared, not owner, 10:00):");
    for role in roles {
        let row: String = actions
            .iter()
            .map(|a| {
                if oracle(role, *a, false, true, true, false, 10, None) {
                    'Y'
                } else {
                    '-'
                }
            })
            .collect();
        println!("     {role:<7?} read/write/delete = {row}");
    }
}
#[allow(clippy::type_complexity)]
fn itertools_product() -> Vec<(bool, bool, bool, bool, u32, Option<bool>)> {
    let mut v = vec![];
    for owner in [false, true] {
        for same in [false, true] {
            for clear in [false, true] {
                for archived in [false, true] {
                    for hour in [3, 10, 20] {
                        for scope in [None, Some(true), Some(false)] {
                            v.push((owner, same, clear, archived, hour, scope));
                        }
                    }
                }
            }
        }
    }
    v
}

// ------------------------------------------------------------------ production tips ----------------------------------
async fn production_tips() {
    println!("== Production tips and tricks");
    let app = seed();
    let r = router(app.clone());
    let s = subject(9, 1, Role::Viewer, 0, None);
    // 1. Default deny: an action nobody wrote a rule for is refused. WHY: new endpoints must be opt-in, not opt-out.
    assert!(
        !decide(&Req {
            subject: &s,
            action: Action::Stats,
            resource: None,
            ctx: Ctx { hour: 10 }
        })
        .allowed
    );
    // 2. Deny overrides allow, independent of rule order. WHY: a safety rule must not be defeated by adding a new allow.
    let d = Doc {
        id: 1,
        tenant: TenantId(2),
        owner: UserId(9),
        classification: 0,
        archived: false,
        title: String::new(),
    };
    let dec = decide(&Req {
        subject: &s,
        action: Action::Read,
        resource: Some(&d),
        ctx: Ctx { hour: 10 },
    });
    assert!(
        !dec.allowed && dec.allow_rules.contains(&"owner") && dec.deny_rules == ["cross_tenant"]
    );
    // 3. Authorise on the server-loaded resource, never on client-sent attributes (owner_id in the JSON body). WHY: mass assignment.
    // 4. Return 404 for resources the caller cannot see (assert: identical to missing). WHY: no existence oracle for enumeration.
    assert_eq!(
        call(&r, "GET", "/v1/docs/3", Some("tok-alice"), &[]).await,
        call(&r, "GET", "/v1/docs/12345", Some("tok-alice"), &[]).await
    );
    // 5. Unguessable IDs (UUIDv4/v7) are defence in depth, NOT authorisation: sequential ids made the exploit trivial, but policy is the fix.
    // 6. Intersect API-key scopes with user permissions; a key can never exceed its owner (asserted in intermediate()).
    // 7. Authorise every verb on every route; test with a table of (token, method, path, expected) so a new route cannot ship open.
    let table = [
        ("tok-dave", "GET", "/v1/docs/1", 200),
        ("tok-dave", "DELETE", "/v1/docs/1", 403),
        ("tok-carol", "DELETE", "/v1/docs/1", 404),
        ("", "GET", "/admin/stats", 401),
    ];
    for (tok, m, path, want) in table {
        let t = (!tok.is_empty()).then_some(tok);
        assert_eq!(
            call(&r, m, path, t, &[]).await.0.as_u16(),
            want,
            "{tok} {m} {path}"
        );
    }
    // 8. Audit denies too (they are the interesting events), with rule names and WITHOUT resource contents. WHY: forensics + alerting.
    assert!(
        app.audit
            .lock()
            .iter()
            .all(|a| a.resource.is_some() || a.action == Action::Stats)
    );
    // 9. Keep the policy pure + versioned (inject clock): exhaustive tests are only possible because decide() has no I/O.
    // 10. Authorisation also belongs in the QUERY (WHERE tenant_id = ?) so a forgotten check cannot leak rows: backend_multi_tenancy_patterns.
    // 11. Anti-pattern: `if user.role == "admin"` scattered in handlers; centralise in `decide()` so rules are reviewable in one file.
    // 12. Avoid role explosion: keep roles coarse, push the rest into attributes (clearance/archived) - ABAC for exceptions, RBAC for the base.
    println!("   12 tips asserted/noted");
}

#[tokio::main]
async fn main() {
    everyday_core().await;
    intermediate().await;
    less_known();
    production_tips().await;
    println!("OK");
}
