//! RECIPE 911 (microservices): migrating off a legacy service: strangler-fig routing, shadow-traffic diffing, canary analysis with rollback, dual write + backfill
//! LEVEL: advanced
//! Use in: replacing a service, endpoint or datastore while it keeps serving traffic; any risky rewrite that cannot be switched in one step.   Run: cargo run --release --example strangler_fig_shadow_traffic
//! COMMON USES: GitHub's Scientist library (compare old and new code paths), Diffy-style response diffing, canary analysis in Spinnaker/Kayenta and Argo Rollouts, online datastore migrations (dual write, backfill, verify, cut over).
#![allow(dead_code)]
//!
//! THE STRANGLER FIG: put a routing facade in front of the legacy system and move one capability at a time behind it. Every step is
//! small, measurable and REVERSIBLE; the legacy system is deleted only when nothing routes to it.
//!   1. ROUTE     : per-endpoint rules: kill switch > allow/deny list > cohort > sticky percentage. Sticky = hash(endpoint, user), so a user
//!                  does not flap between implementations, and raising the percentage only moves users ONE way.
//!   2. SHADOW    : (dark launch) legacy answers the user; a COPY goes to the new service and its answer is only compared. The shadow
//!                  must not be able to hurt: its errors are swallowed and counted, and its WRITES go to a sandbox, never to real
//!                  systems (double charges and double emails are the classic shadowing accident).
//!   3. DIFF      : compare structurally, not as bytes. NOISE (request ids, timestamps) is ignored by PATH; floats get a tolerance;
//!                  collections with no defined order are compared as multisets. Every ignore rule is a blind spot: keep the list short.
//!   4. CANARY    : serve a small share for real and compare error rates with a two-proportion z-test against a CONCURRENT control.
//!                  LOOKING REPEATEDLY inflates false alarms: each extra look is another chance to cross the line. This file uses a
//!                  Bonferroni split of alpha over the planned looks (simple and conservative; sequential tests such as SPRT or
//!                  alpha-spending are tighter), plus a minimum practical difference and a minimum event count for the normal approximation.
//!   5. DATA      : dual write + backfill. The race: backfill READS an old row, a live write lands in both stores, then backfill WRITES
//!                  its stale copy over it. Fix: every write carries the source VERSION and the target applies only newer ones,
//!                  with TOMBSTONES for deletes (otherwise backfill resurrects deleted rows). Then VERIFY with bucketed checksums.
//! LIMITS: the simulation's errors are independent Bernoulli trials. Real traffic is correlated (one user retrying), so real canaries
//! need more samples than this suggests. Error rate is one metric; latency percentiles need a different test.
//! Read: https://martinfowler.com/bliki/StranglerFigApplication.html , https://github.com/github/scientist
//! Read: https://sre.google/workbook/canarying-releases/
//!
//! TRY NEXT:
//!   - [S] Change `Verdict` evaluation to use `alpha` at every look (no split). Predict the false-rollback rate over 1000 good releases, then read the printed "naive" row.
//!   - [S] Add `/total` to the ignore list in `diff_catches_injected` and predict which fraction of injected faults goes unnoticed.
//!   - [M] Add a latency check to the canary (compare the share of requests slower than a threshold with the same z-test) and plant a release that is slow but never errors.
//!   - [M] Make the dual write itself fail sometimes AFTER the legacy commit and replace the repair queue with the outbox from `transactional_outbox_inbox_sqlite`.
//!   - [A] PROJECT: shadow one `axum` service with another on loopback, sample 5% of requests, and bound the extra latency added to the user's path to zero.

use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet, VecDeque};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
}
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
fn stable_hash(s: &str) -> u64 {
    mix(s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01b3)
    }))
}

// ---------------------------------------------------------------------------
// 1. routing facade
// ---------------------------------------------------------------------------
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Legacy,
    New,
}
#[derive(Default, Clone)]
struct Rule {
    killed: bool,
    deny: HashSet<u64>,
    allow: HashSet<u64>,
    cohort_to_new: Option<&'static str>, // e.g. "staff"
    basis_points: u64,
}
struct Facade {
    rules: BTreeMap<&'static str, Rule>,
}
impl Facade {
    fn route(&self, endpoint: &str, user: u64, cohort: &str) -> Target {
        let Some(r) = self.rules.get(endpoint) else {
            return Target::Legacy;
        }; // unknown things stay where they work today
        if r.killed || r.deny.contains(&user) {
            return Target::Legacy;
        }
        let new = r.allow.contains(&user)
            || r.cohort_to_new == Some(cohort)
            || stable_hash(&format!("{endpoint}/{user}")) % 10_000 < r.basis_points;
        if new { Target::New } else { Target::Legacy }
    }
}
fn routing_test() {
    let mut rule = Rule {
        allow: HashSet::from([1]),
        deny: HashSet::from([2]),
        cohort_to_new: Some("staff"),
        ..Default::default()
    };
    let mut f = Facade {
        rules: BTreeMap::new(),
    };
    let mut previous = BTreeSet::new();
    for bp in [0, 100, 500, 2500, 5000, 10_000] {
        rule.basis_points = bp;
        f.rules.insert("/orders", rule.clone());
        let on: BTreeSet<u64> = (100..40_100)
            .filter(|&u| f.route("/orders", u, "public") == Target::New)
            .collect();
        assert!(
            previous.is_subset(&on),
            "raising the percentage never moves a user back"
        );
        assert!((on.len() as f64 / 40_000.0 - bp as f64 / 10_000.0).abs() < 0.01);
        assert!(
            (100..1000).all(|u| f.route("/orders", u, "public") == f.route("/orders", u, "public")),
            "sticky"
        );
        assert_eq!(f.route("/orders", 1, "public"), Target::New);
        assert_eq!(
            f.route("/orders", 2, "staff"),
            Target::Legacy,
            "deny beats cohort and percentage"
        );
        assert_eq!(f.route("/orders", 3, "staff"), Target::New);
        assert_eq!(f.route("/billing", 1, "staff"), Target::Legacy);
        previous = on;
    }
    rule.killed = true;
    f.rules.insert("/orders", rule);
    assert!(
        (0..1000).all(|u| f.route("/orders", u, "staff") == Target::Legacy),
        "kill switch beats everything"
    );
    println!(
        "  ramp 0 -> 100% monotonic and sticky; precedence kill > deny > allow/cohort > percentage"
    );
}

// ---------------------------------------------------------------------------
// 2. structural diff with explicit noise rules
// ---------------------------------------------------------------------------
#[derive(Default)]
struct DiffRules {
    ignore: &'static [&'static str],    // paths; `*` matches one segment
    unordered: &'static [&'static str], // arrays compared as multisets
    float_tolerance: f64, // relative; applied only when at least one side is a non-integer number
}
fn path_matches(pattern: &str, path: &str) -> bool {
    let (p, q): (Vec<_>, Vec<_>) = (pattern.split('/').collect(), path.split('/').collect());
    p.len() == q.len() && p.iter().zip(&q).all(|(a, b)| *a == "*" || a == b)
}
/// Paths at which `a` (legacy) and `b` (new) differ. An empty list means "equivalent under these rules".
fn diff(rules: &DiffRules, path: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    if rules.ignore.iter().any(|p| path_matches(p, path)) {
        return;
    }
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            for k in keys {
                let sub = format!("{path}/{k}");
                match (x.get(k), y.get(k)) {
                    (Some(u), Some(v)) => diff(rules, &sub, u, v, out),
                    // missing and null are DIFFERENT: clients often treat them differently
                    _ if rules.ignore.iter().any(|p| path_matches(p, &sub)) => {}
                    _ => out.push(sub),
                }
            }
        }
        (Value::Array(x), Value::Array(y))
            if rules.unordered.iter().any(|p| path_matches(p, path)) =>
        {
            // greedy multiset match using the same rules, so noise inside elements is still ignored
            let mut unmatched: Vec<&Value> = y.iter().collect();
            let mut missing = 0;
            for u in x {
                let found = unmatched.iter().position(|v| {
                    let mut d = vec![];
                    diff(rules, &format!("{path}/0"), u, v, &mut d);
                    d.is_empty()
                });
                match found {
                    Some(i) => drop(unmatched.swap_remove(i)),
                    None => missing += 1,
                }
            }
            if missing > 0 || !unmatched.is_empty() {
                out.push(path.to_string());
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.push(path.to_string());
            } else {
                x.iter()
                    .zip(y)
                    .enumerate()
                    .for_each(|(i, (u, v))| diff(rules, &format!("{path}/{i}"), u, v, out));
            }
        }
        (Value::Number(x), Value::Number(y)) if x.is_f64() || y.is_f64() => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() > rules.float_tolerance * x.abs().max(y.abs()).max(1.0) {
                out.push(path.to_string());
            }
        }
        _ if a == b => {}
        _ => out.push(path.to_string()),
    }
}
const RULES: DiffRules = DiffRules {
    ignore: &["/meta/request_id", "/meta/served_at", "/items/*/etag"],
    unordered: &["/tags"],
    float_tolerance: 1e-9,
};

fn order_response(id: u64, noise: u64) -> Value {
    let mut r = Rng(id);
    let items: Vec<Value> = (0..1 + r.below(4))
        .map(|i| json!({"sku": format!("sku-{}", r.below(1000)), "qty": 1 + r.below(5), "price": r.below(10_000) as f64 / 100.0, "etag": noise + i}))
        .collect();
    let total: f64 = items
        .iter()
        .map(|i| i["price"].as_f64().unwrap() * i["qty"].as_u64().unwrap() as f64)
        .sum();
    json!({"id": id, "items": items, "total": total, "coupon": null, "tags": ["a", "b", format!("t{}", r.below(3))],
           "meta": {"request_id": format!("req-{noise}"), "served_at": 1_700_000_000 + noise}})
}
/// All leaf paths of a document.
fn leaves(path: &str, v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => m
            .iter()
            .for_each(|(k, v)| leaves(&format!("{path}/{k}"), v, out)),
        Value::Array(a) => a
            .iter()
            .enumerate()
            .for_each(|(i, v)| leaves(&format!("{path}/{i}"), v, out)),
        _ => out.push(path.to_string()),
    }
}
fn diff_catches_injected() {
    let mut r = Rng(2);
    let (mut caught, mut noise_quiet) = (0, 0);
    for case in 0..3000u64 {
        let legacy = order_response(case, 1);
        let mut new = order_response(case, 999); // different noise only
        // harmless differences: shuffled unordered array and a float that differs by rounding
        new["tags"].as_array_mut().unwrap().rotate_left(1);
        let t = new["total"].as_f64().unwrap();
        new["total"] = json!(t * (1.0 + 1e-12));
        let mut d = vec![];
        diff(&RULES, "", &legacy, &new, &mut d);
        assert!(d.is_empty(), "noise reported: {d:?}");
        noise_quiet += 1;

        // inject exactly one REAL fault at a random non-ignored place
        let mut paths = vec![];
        leaves("", &new, &mut paths);
        paths.retain(|p| !RULES.ignore.iter().any(|ig| path_matches(ig, p)));
        let target = paths[r.below(paths.len() as u64) as usize].clone();
        let slot = new.pointer_mut(&target).unwrap();
        let faulty = match &*slot {
            Value::Number(n) if n.is_f64() => json!(n.as_f64().unwrap() + 0.01),
            Value::Number(n) => json!(n.as_u64().unwrap() + 1),
            Value::String(s) => json!(format!("{s}!")),
            Value::Null => json!(0),
            other => json!([other.clone()]),
        };
        *slot = faulty;
        let mut d = vec![];
        diff(&RULES, "", &legacy, &new, &mut d);
        let expect = if target.starts_with("/tags/") {
            "/tags".to_string()
        } else {
            target.clone()
        };
        assert_eq!(d, [expect], "case {case}: fault at {target}");
        caught += 1;
    }
    // structural faults
    let base = order_response(1, 1);
    for (mutate, expect) in [
        (
            (|v: &mut Value| drop(v.as_object_mut().unwrap().remove("coupon"))) as fn(&mut Value),
            "/coupon",
        ),
        (|v| v["extra"] = json!(1), "/extra"),
        (
            |v| v["items"].as_array_mut().unwrap().push(json!({})),
            "/items",
        ),
        (
            |v| v["tags"].as_array_mut().unwrap().push(json!("a")),
            "/tags",
        ),
        (|v| v["id"] = json!("1"), "/id"),
    ] {
        let mut changed = base.clone();
        mutate(&mut changed);
        let mut d = vec![];
        diff(&RULES, "", &base, &changed, &mut d);
        assert_eq!(d, [expect]);
    }
    println!(
        "  {noise_quiet} noisy-but-equal pairs reported nothing; {caught}/3000 single injected faults reported at the exact path"
    );
}

// ---------------------------------------------------------------------------
// 3. shadow (dark launch) execution
// ---------------------------------------------------------------------------
/// Side effects go through this trait, so the shadow can be handed a sandbox instead of the real thing.
trait Effects {
    fn charge(&mut self, order: u64);
}
#[derive(Default)]
struct Real(Vec<u64>);
impl Effects for Real {
    fn charge(&mut self, order: u64) {
        self.0.push(order);
    }
}
#[derive(Default)]
struct Sandbox(Vec<u64>);
impl Effects for Sandbox {
    fn charge(&mut self, order: u64) {
        self.0.push(order);
    }
}
fn legacy_service(id: u64, fx: &mut dyn Effects) -> Result<Value, String> {
    fx.charge(id);
    Ok(order_response(id, 1))
}
/// The rewrite, with two planted behaviours: it panics on ids divisible by 97 and mis-totals ids divisible by 50.
fn new_service(id: u64, fx: &mut dyn Effects) -> Result<Value, String> {
    fx.charge(id);
    assert!(id % 97 != 0, "new service bug");
    let mut v = order_response(id, 2);
    if id % 50 == 0 {
        v["total"] = json!(v["total"].as_f64().unwrap() + 1.0);
    }
    Ok(v)
}
#[derive(Default)]
struct ShadowReport {
    compared: u64,
    mismatches: Vec<(u64, Vec<String>)>,
    shadow_failures: u64,
    dropped: u64,
}
/// The user path only ENQUEUES (bounded; drop when full). The comparison runs later, off the request path.
struct Shadower {
    queue: VecDeque<(u64, Value)>,
    capacity: usize,
    report: ShadowReport,
}
impl Shadower {
    fn serve(&mut self, id: u64, real: &mut Real) -> Result<Value, String> {
        let answer = legacy_service(id, real);
        if let Ok(v) = &answer {
            if self.queue.len() < self.capacity {
                self.queue.push_back((id, v.clone()));
            } else {
                self.report.dropped += 1;
            }
        }
        answer
    }
    fn drain(&mut self, n: usize, sandbox: &mut Sandbox) {
        for _ in 0..n {
            let Some((id, expected)) = self.queue.pop_front() else {
                return;
            };
            // a panic in the new code must not take the process down with it
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                new_service(id, sandbox)
            })) {
                Ok(Ok(got)) => {
                    self.report.compared += 1;
                    let mut d = vec![];
                    diff(&RULES, "", &expected, &got, &mut d);
                    if !d.is_empty() {
                        self.report.mismatches.push((id, d));
                    }
                }
                _ => self.report.shadow_failures += 1,
            }
        }
    }
}
fn shadow_test() {
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // the planted panics are expected; keep the output readable
    let (mut s, mut real, mut sandbox) = (
        Shadower {
            queue: VecDeque::new(),
            capacity: 64,
            report: ShadowReport::default(),
        },
        Real::default(),
        Sandbox::default(),
    );
    let mut r = Rng(3);
    for id in 1..=5000u64 {
        let answer = s.serve(id, &mut real);
        assert_eq!(
            answer.unwrap(),
            order_response(id, 1),
            "the user always gets the legacy answer"
        );
        // the shadow worker is slower than the traffic for a while, so the bounded queue must shed
        s.drain(
            if (1000..1500).contains(&id) {
                r.below(2) as usize
            } else {
                2
            },
            &mut sandbox,
        );
    }
    let backlog = s.capacity;
    s.drain(backlog, &mut sandbox);
    std::panic::set_hook(hook);
    let rep = &s.report;
    assert_eq!(
        real.0,
        (1..=5000).collect::<Vec<_>>(),
        "each order charged for real exactly once: the shadow had no real effects"
    );
    assert_eq!(rep.compared + rep.shadow_failures + rep.dropped, 5000);
    assert_eq!(sandbox.0.len() as u64, rep.compared + rep.shadow_failures);
    let shadowed: HashSet<u64> = sandbox.0.iter().copied().collect();
    let planted: Vec<u64> = (1..=5000)
        .filter(|i| i % 50 == 0 && i % 97 != 0 && shadowed.contains(i))
        .collect();
    assert_eq!(
        rep.mismatches.iter().map(|m| m.0).collect::<Vec<_>>(),
        planted,
        "exactly the planted wrong totals"
    );
    assert!(rep.mismatches.iter().all(|m| m.1 == ["/total"]));
    assert_eq!(
        rep.shadow_failures,
        shadowed.iter().filter(|&&i| i % 97 == 0).count() as u64
    );
    assert!(rep.dropped > 0);
    println!(
        "  5000 requests: {} compared, {} mismatches (all /total), {} shadow panics contained, {} shed",
        rep.compared,
        rep.mismatches.len(),
        rep.shadow_failures,
        rep.dropped
    );
}

// ---------------------------------------------------------------------------
// 4. canary analysis
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Default)]
struct Counts {
    requests: u64,
    errors: u64,
}
/// Two-proportion z statistic with a pooled variance; positive when the canary is worse. `None` when the variance is zero.
fn z_two_proportions(canary: Counts, control: Counts) -> Option<f64> {
    if canary.requests == 0 || control.requests == 0 {
        return None;
    }
    let (n1, n0) = (canary.requests as f64, control.requests as f64);
    let pooled = (canary.errors + control.errors) as f64 / (n1 + n0);
    let se = (pooled * (1.0 - pooled) * (1.0 / n1 + 1.0 / n0)).sqrt();
    (se > 0.0).then(|| (canary.errors as f64 / n1 - control.errors as f64 / n0) / se)
}
/// Pearson chi-square of the 2x2 table; for two groups it equals z squared (an independent formula used as an oracle).
fn chi_square(a: Counts, b: Counts) -> f64 {
    let t = [
        [a.errors, a.requests - a.errors],
        [b.errors, b.requests - b.errors],
    ]
    .map(|r| r.map(|x| x as f64));
    let n: f64 = t.iter().flatten().sum();
    let mut chi = 0.0;
    for i in 0..2 {
        for j in 0..2 {
            let expected = (t[i][0] + t[i][1]) * (t[0][j] + t[1][j]) / n;
            chi += (t[i][j] - expected).powi(2) / expected;
        }
    }
    chi
}
fn z_test_oracle() {
    let (c, k) = (
        Counts {
            requests: 1000,
            errors: 30,
        },
        Counts {
            requests: 10_000,
            errors: 100,
        },
    );
    assert!(
        (z_two_proportions(c, k).unwrap() - 5.580).abs() < 0.001,
        "hand-computed example"
    );
    let mut r = Rng(4);
    for _ in 0..2000 {
        let mut table = || {
            let requests = 10 + r.below(10_000);
            Counts {
                requests,
                errors: 1 + r.below(requests - 1),
            }
        };
        let (a, b) = (table(), table());
        let z = z_two_proportions(a, b).unwrap();
        assert!((z * z - chi_square(a, b)).abs() <= 1e-7 * (1.0 + z * z));
    }
    assert_eq!(
        z_two_proportions(
            Counts {
                requests: 5,
                errors: 0
            },
            Counts {
                requests: 5,
                errors: 0
            }
        ),
        None
    );
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Verdict {
    Continue,
    Rollback,
}
#[derive(Clone, Copy)]
struct Policy {
    z_critical: f64,
    min_difference: f64, // do not roll back for a difference too small to care about
    min_events: u64,     // normal approximation needs some expected errors in the canary arm
}
impl Policy {
    fn judge(&self, canary: Counts, control: Counts) -> Verdict {
        let pooled = (canary.errors + control.errors) as f64
            / (canary.requests + control.requests).max(1) as f64;
        if (pooled * canary.requests as f64) < self.min_events as f64 {
            return Verdict::Continue; // not enough evidence either way
        }
        let difference = canary.errors as f64 / canary.requests as f64
            - control.errors as f64 / control.requests.max(1) as f64;
        match z_two_proportions(canary, control) {
            Some(z) if z > self.z_critical && difference > self.min_difference => Verdict::Rollback,
            _ => Verdict::Continue,
        }
    }
}
const STAGES: [u64; 4] = [1, 5, 25, 50]; // percent of traffic to the canary; after the last stage: promote
const LOOKS_PER_STAGE: u32 = 10;
const REQUESTS_PER_LOOK: u64 = 4000;
/// Returns (rolled back?, share of canary traffic at the decision, bad requests served by the canary).
fn rollout(policy: Policy, base: f64, canary_rate: f64, r: &mut Rng) -> (bool, u64, u64) {
    let mut bad_served = 0;
    for pct in STAGES {
        // counts restart at each stage so that both arms always cover the same time window and traffic mix
        let (mut canary, mut control) = (Counts::default(), Counts::default());
        for _ in 0..LOOKS_PER_STAGE {
            for _ in 0..REQUESTS_PER_LOOK {
                let is_canary = r.below(100) < pct;
                let (arm, p) = if is_canary {
                    (&mut canary, canary_rate)
                } else {
                    (&mut control, base)
                };
                let failed = r.chance(p);
                arm.requests += 1;
                arm.errors += u64::from(failed);
                bad_served += u64::from(is_canary && failed);
            }
            if policy.judge(canary, control) == Verdict::Rollback {
                return (true, pct, bad_served);
            }
        }
    }
    (false, 100, bad_served)
}
fn canary_test() {
    z_test_oracle();
    // family-wise one-sided alpha 0.01 over 40 looks -> per look 0.00025 -> z = 3.48
    let corrected = Policy {
        z_critical: 3.48,
        min_difference: 0.003,
        min_events: 5,
    };
    let naive = Policy {
        z_critical: 1.645,
        min_difference: 0.0,
        min_events: 0,
    }; // "p < 0.05", checked 40 times
    println!(
        "  {:<44}{:>12}{:>18}",
        "1000 rollouts each (baseline error 1%)", "rolled back", "caught at <= 5%"
    );
    let mut results = vec![];
    for (name, policy, rate) in [
        ("corrected: good release (1.0%)", corrected, 0.01),
        ("naive p<0.05 at every look: good release", naive, 0.01),
        ("corrected: bad release (3.0%)", corrected, 0.03),
        ("corrected: catastrophic release (20%)", corrected, 0.20),
        ("corrected: slightly worse (1.2%)", corrected, 0.012),
    ] {
        let mut r = Rng(stable_hash(name));
        let (mut rolled, mut early) = (0, 0);
        for _ in 0..1000 {
            let (back, pct, _) = rollout(policy, 0.01, rate, &mut r);
            rolled += u32::from(back);
            early += u32::from(back && pct <= 5);
        }
        println!("  {name:<44}{rolled:>12}{early:>18}");
        results.push((rolled, early));
    }
    assert!(
        results[0].0 <= 10,
        "good releases must survive: {} false rollbacks",
        results[0].0
    );
    assert!(
        results[1].0 > 150,
        "peeking without correction fires often on a GOOD release"
    );
    assert!(results[2].0 >= 995, "a 3x error rate is rolled back");
    assert!(
        results[3].0 == 1000 && results[3].1 == 1000,
        "a catastrophe is stopped before it reaches 25% of traffic"
    );
    assert!(
        results[4].0 < 500,
        "a difference below the practical threshold is mostly let through: know what your canary CANNOT see"
    );
}

// ---------------------------------------------------------------------------
// 5. data migration: dual write + backfill + verification
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, PartialEq, Debug)]
struct Row {
    version: u64,
    value: Option<u64>, // None = tombstone
}
type Store = BTreeMap<u64, Row>;
fn apply(target: &mut Store, key: u64, row: Row, versioned: bool) {
    if versioned {
        if target.get(&key).is_none_or(|old| old.version < row.version) {
            target.insert(key, row);
        }
    } else if row.value.is_some() {
        target.insert(key, row);
    } else {
        target.remove(&key); // naive: a delete leaves no trace for later stale writes to lose against
    }
}
fn live(s: &Store) -> BTreeMap<u64, u64> {
    s.iter().filter_map(|(&k, r)| Some((k, r.value?))).collect()
}
/// One random schedule. Returns (legacy, new) after backfill and repair have finished.
fn migrate(seed: u64, versioned: bool) -> (Store, Store) {
    let mut r = Rng(seed);
    let mut legacy: Store = (0..40)
        .map(|k| {
            (
                k,
                Row {
                    version: k + 1,
                    value: Some(r.below(100)),
                },
            )
        })
        .collect();
    let (mut new, mut version) = (Store::new(), 41);
    let mut cursor = 0u64;
    let mut backfill_batch: Vec<(u64, Row)> = vec![]; // rows READ but not yet WRITTEN: the race window
    let mut repairs: VecDeque<u64> = VecDeque::new();
    let mut writes = 0;
    while writes < 150 || cursor < 60 || !backfill_batch.is_empty() || !repairs.is_empty() {
        match r.below(4) {
            0 if writes < 150 => {
                writes += 1;
                version += 1;
                let key = r.below(60);
                let row = Row {
                    version,
                    value: (r.below(4) != 0).then(|| r.below(100)),
                };
                // the legacy store stays authoritative; its tombstones are kept so repair can read them
                legacy.insert(key, row);
                if r.below(8) == 0 {
                    repairs.push_back(key); // the second write failed: remember the KEY, not the value
                } else {
                    apply(&mut new, key, row, versioned);
                }
            }
            1 if backfill_batch.is_empty() && cursor < 60 => {
                backfill_batch = legacy
                    .range(cursor..cursor + 6)
                    .filter(|(_, row)| row.value.is_some())
                    .map(|(&k, &row)| (k, row))
                    .collect();
                cursor += 6;
            }
            2 if !backfill_batch.is_empty() => {
                let (k, row) = backfill_batch.remove(0);
                apply(&mut new, k, row, versioned);
            }
            3 => {
                if let Some(k) = repairs.pop_front() {
                    apply(&mut new, k, legacy[&k], versioned); // re-read the current row
                }
            }
            _ => {}
        }
    }
    (legacy, new)
}
/// Compare in two passes like a real checker over a network: cheap per-bucket checksums, then rows only in unequal buckets.
fn verify(a: &BTreeMap<u64, u64>, b: &BTreeMap<u64, u64>) -> (Vec<u64>, usize) {
    const BUCKETS: u64 = 8;
    let digest = |s: &BTreeMap<u64, u64>, bucket| {
        s.iter()
            .filter(|(k, _)| *k % BUCKETS == bucket)
            .fold((0u64, 0u64), |(n, h), (&k, &v)| {
                (n + 1, h.wrapping_add(mix(mix(k) ^ v)))
            })
    };
    let (mut bad, mut fetched) = (BTreeSet::new(), 0);
    for bucket in 0..BUCKETS {
        if digest(a, bucket) != digest(b, bucket) {
            for k in a.keys().chain(b.keys()).filter(|k| *k % BUCKETS == bucket) {
                fetched += 1;
                if a.get(k) != b.get(k) {
                    bad.insert(*k);
                }
            }
        }
    }
    (bad.into_iter().collect(), fetched)
}
fn migration_test() {
    let (mut naive_bad, mut resurrected, mut versioned_bad) = (0, 0, 0);
    for seed in 0..2000 {
        let (legacy, new) = migrate(seed, false);
        let (bad, _) = verify(&live(&legacy), &live(&new));
        naive_bad += usize::from(!bad.is_empty());
        resurrected += bad
            .iter()
            .filter(|k| !live(&legacy).contains_key(k) && live(&new).contains_key(k))
            .count();
        let (legacy, new) = migrate(seed, true);
        versioned_bad += usize::from(!verify(&live(&legacy), &live(&new)).0.is_empty());
    }
    assert!(
        naive_bad > 200 && resurrected > 0,
        "{naive_bad} {resurrected}"
    );
    assert_eq!(versioned_bad, 0);
    // the checker itself: plant 1 to 3 corruptions and demand exactly those keys
    let mut r = Rng(5);
    for _ in 0..1000 {
        let a: BTreeMap<u64, u64> = (0..500).map(|k| (k, r.below(1000))).collect();
        let mut b = a.clone();
        let mut planted = BTreeSet::new();
        for _ in 0..1 + r.below(3) {
            let k = r.below(520);
            if !planted.insert(k) {
                continue;
            }
            match (b.get(&k).copied(), r.below(2)) {
                (Some(v), 0) => drop(b.insert(k, v + 1)),
                (Some(_), _) => drop(b.remove(&k)),
                (None, _) => drop(b.insert(k, 7)),
            }
        }
        let (bad, fetched) = verify(&a, &b);
        assert_eq!(bad, planted.into_iter().collect::<Vec<_>>());
        assert!(
            fetched <= 3 * 2 * 66,
            "only unequal buckets are read row by row: {fetched}"
        );
    }
    println!(
        "  2000 interleavings: naive dual write + backfill diverged in {naive_bad} ({resurrected} deleted rows resurrected); versioned + tombstones diverged in 0"
    );
    println!("  checksum verifier found exactly the planted corruptions in 1000/1000 trials");
}

// ---------------------------------------------------------------------------
fn main() {
    println!("== 1. routing facade ==");
    routing_test();
    println!("== 2. response diffing ==");
    diff_catches_injected();
    println!("== 3. shadow traffic ==");
    shadow_test();
    println!("== 4. canary analysis ==");
    canary_test();
    println!("== 5. dual write, backfill, verification ==");
    migration_test();
    println!("all migration checks passed");
}
