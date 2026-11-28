use solution::*;

use axum::body::{Body, to_bytes};
use axum::http::Request;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use tower::ServiceExt;

/// Each request takes the next latency from the script: `micros()` returns a start, then the start plus it.
#[derive(Default)]
struct Ticking {
    seconds: AtomicU64,
    calls: AtomicU64,
    script: Mutex<VecDeque<u64>>,
    started: Mutex<u64>,
}

impl Clock for Ticking {
    fn seconds(&self) -> u64 {
        self.seconds.load(Ordering::SeqCst)
    }
    fn micros(&self) -> u64 {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let mut started = self.started.lock().unwrap();
        if call.is_multiple_of(2) {
            *started = 1_000_000 * call;
            *started
        } else {
            *started + self.script.lock().unwrap().pop_front().unwrap_or(100)
        }
    }
}

fn setup() -> (axum::Router, Arc<Service>, Arc<Ticking>) {
    let service = Arc::new(Service::new(64, Box::new(classify)));
    let clock = Arc::new(Ticking::default());
    (router(service.clone(), clock.clone()), service, clock)
}

fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
}

async fn send(app: &axum::Router, method: &str, path: &str, body: &str) -> (u16, Value) {
    let request = Request::builder().method(method).uri(path).header("content-type", "application/json");
    let response = app.clone().oneshot(request.body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status().as_u16();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let body = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, body)
}

/// The value of one metrics line.
async fn metric(app: &axum::Router, name: &str) -> Option<u64> {
    let (status, body) = send(app, "GET", "/metrics", "").await;
    assert_eq!(status, 200);
    let text = body.as_str().expect("plain text").to_string();
    assert!(text.ends_with('\n'), "no newline at the end");
    let values: Vec<u64> = text
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix(name)?.strip_prefix(' ')?.parse().ok())
        .collect();
    assert_eq!(values.len(), 1, "want exactly one line for {name} in:\n{text}");
    values.first().copied()
}

fn refused(got: &(u16, Value), status: u16) -> bool {
    got.0 == status && got.1["ok"] == json!(false) && got.1["error"].as_str().is_some_and(|e| !e.is_empty())
}

fn query(text: &str) -> String {
    text.bytes().map(|b| format!("%{b:02X}")).collect()
}

fn listed(body: &Value) -> Vec<String> {
    body["suggestions"].as_array().expect("suggestions").iter().map(|s| s["term"].as_str().unwrap_or("").to_string()).collect()
}

#[test]
fn defaults_and_the_range_of_limit() {
    run(async {
        let (app, service, _) = setup();
        let words: Vec<String> = (0..30).map(|i| format!("item{i:02}")).collect();
        service.add(&words.iter().map(|w| (w.as_str(), 1)).collect::<Vec<_>>(), 0);
        let got = send(&app, "GET", "/suggest", "").await;
        check!("no q", (got.0, got.1["q"].clone(), got.1["key"].clone(), listed(&got.1).len()), (200, json!(""), json!(""), 5));
        for (limit, n) in [("0", 1), ("7", 7), ("21", 20), ("18446744073709551615", 20)] {
            let got = send(&app, "GET", &format!("/suggest?q=item&limit={limit}"), "").await;
            check!(format!("limit={limit}"), (got.0, listed(&got.1).len()), (200, n));
        }
        for bad in ["-1", "2.5", "", "18446744073709551616"] {
            let status = send(&app, "GET", &format!("/suggest?q=item&limit={bad}"), "").await.0;
            check!(format!("limit={bad:?} is {status}"), (400..500).contains(&status), true);
        }
    });
}

#[test]
fn long_text_is_refused_by_characters() {
    run(async {
        let (app, service, _) = setup();
        let fits = "é".repeat(128);
        let long = "é".repeat(129);
        check!("128 characters", send(&app, "GET", &format!("/suggest?q={}", query(&fits)), "").await.0, 200);
        let got = send(&app, "GET", &format!("/suggest?q={}", query(&long)), "").await;
        check!(format!("129 characters: {got:?}"), refused(&got, 400), true);
        let got = send(&app, "POST", "/events", &json!({"term": long}).to_string()).await;
        check!(format!("long event: {got:?}"), refused(&got, 400), true);
        let got = send(&app, "POST", "/events", &json!({"term": fits}).to_string()).await;
        check!(format!("unknown: {got:?}"), refused(&got, 404), true);
        let got = send(&app, "POST", "/index", &json!({"terms":[{"term": fits}]}).to_string()).await;
        check!("128 characters indexed", got, (200, json!({"added":1,"existing":0})));
        check!("len", service.len(), 1);
    });
}

#[test]
fn a_refused_index_request_adds_nothing() {
    run(async {
        let (app, service, _) = setup();
        let long = "x".repeat(129);
        let got = send(&app, "POST", "/index", &json!({"terms":[{"term":"first"},{"term":"second"},{"term":long}]}).to_string()).await;
        check!(format!("one long term: {got:?}"), refused(&got, 400), true);
        check!("nothing added", service.len(), 0);
        let many: Vec<Value> = (0..1001).map(|i| json!({"term": format!("t{i}")})).collect();
        let got = send(&app, "POST", "/index", &json!({"terms": many}).to_string()).await;
        check!(format!("1001 pairs: {got:?}"), refused(&got, 400), true);
        check!("nothing added", service.len(), 0);
        let got = send(&app, "POST", "/index", &json!({"terms": many[..1000]}).to_string()).await;
        check!("1000 pairs", got, (200, json!({"added":1000,"existing":0})));
        check!("counter", metric(&app, "typeahead_terms_added_total").await, Some(1000));
    });
}

#[test]
fn bad_bodies_are_client_errors() {
    run(async {
        let (app, service, _) = setup();
        for (path, body) in [("/events", "{"), ("/events", "{}"), ("/events", r#"{"term":5}"#), ("/index", r#"{"terms":[{"term":"a","count":-1}]}"#), ("/index", r#"{"terms":[{"term":"a","count":4294967296}]}"#), ("/index", "[]x")] {
            let status = send(&app, "POST", path, body).await.0;
            check!(format!("{path} {body} gave {status}"), (400..500).contains(&status), true);
        }
        check!("nothing added", service.len(), 0);
        check!("no events counted", (metric(&app, "typeahead_events_total").await, metric(&app, "typeahead_unknown_events_total").await), (Some(0), Some(0)));
    });
}

#[test]
fn seconds_come_from_the_clock() {
    run(async {
        let (app, _, clock) = setup();
        clock.seconds.store(1000, Ordering::SeqCst);
        send(&app, "POST", "/index", r#"{"terms":[{"term":"rust","count":9},{"term":"ruby","count":6}]}"#).await;
        clock.seconds.store(1000 + 3600, Ordering::SeqCst);
        let got = send(&app, "GET", "/suggest?q=ru", "").await;
        check!("halved", (got.1["suggestions"][0]["score"].clone(), got.1["suggestions"][1]["score"].clone()), (json!(4.5), json!(3.0)));
        clock.seconds.store(1000 + 3600 + 1200, Ordering::SeqCst);
        send(&app, "POST", "/events", r#"{"term":"ruby"}"#).await;
        let got = send(&app, "GET", "/suggest?q=ru&limit=1", "").await;
        // 6 * 0.5^(4/3) + 1 = 3.381; rust is at 9 * 0.5^(4/3) = 3.572
        check!("rounded to three places", got.1["suggestions"][0].clone(), json!({"term":"rust","score":3.572,"kind":"prefix","dist":0}));
    });
}

#[test]
fn counters_count_only_what_was_answered() {
    run(async {
        let (app, service, clock) = setup();
        check!("empty quantile", (metric(&app, "typeahead_suggest_latency_us{quantile=\"0.99\"}").await, metric(&app, "typeahead_suggest_latency_us_count").await), (Some(0), Some(0)));
        service.add(&[("rust", 1)], 0);
        for path in ["/suggest?q=ru", "/suggest?q=ru", "/suggest?q=ust", "/suggest?q=RU", "/suggest?q=ru&limit=x"] {
            send(&app, "GET", path, "").await;
        }
        send(&app, "GET", &format!("/suggest?q={}", "a".repeat(500)), "").await;
        check!("micros calls", clock.calls.load(Ordering::SeqCst), 8);
        for term in ["rust", "zig", "go", "rust", "RUST"] {
            send(&app, "POST", "/events", &json!({"term": term}).to_string()).await;
        }
        send(&app, "POST", "/index", r#"{"terms":[{"term":"rust"},{"term":"go"},{"term":"go"}]}"#).await;
        let want = [
            ("typeahead_suggest_requests_total", 4),
            ("typeahead_cache_hits_total", 2),
            ("typeahead_cache_misses_total", 2),
            ("typeahead_events_total", 3),
            ("typeahead_unknown_events_total", 2),
            ("typeahead_terms_added_total", 1),
            ("typeahead_indexed_terms", 2),
            ("typeahead_suggest_latency_us_count", 4),
            ("typeahead_suggest_latency_us_sum", 400),
        ];
        for (name, value) in want {
            check!(name.to_string(), metric(&app, name).await, Some(value));
        }
        check!("no micros for other routes", clock.calls.load(Ordering::SeqCst), 8);
    });
}

#[test]
fn quantiles_are_close_to_the_exact_ones() {
    run(async {
        let mut rng = anneal_prelude::Rng::new(15205);
        for round in 0..12 {
            let (app, _, clock) = setup();
            let n = 1 + rng.below(150);
            let mut latencies: Vec<u64> = (0..n)
                .map(|_| {
                    let shift = rng.below(24);
                    rng.below(2 + (1 << shift)) as u64
                })
                .collect();
            if round == 0 {
                latencies = vec![10; 99];
                latencies.push(5_000_000);
            }
            clock.script.lock().unwrap().extend(&latencies);
            for _ in &latencies {
                send(&app, "GET", "/suggest?q=a", "").await;
            }
            latencies.sort_unstable();
            for (q, p) in [("0.5", 0.5f64), ("0.99", 0.99)] {
                let rank = ((p * latencies.len() as f64).ceil() as usize).max(1);
                let exact = latencies[rank - 1];
                let got = metric(&app, &format!("typeahead_suggest_latency_us{{quantile=\"{q}\"}}")).await.unwrap_or(0);
                let ok = got >= exact && got <= exact + exact / 8 && got <= latencies[latencies.len() - 1];
                check!(format!("quantile {q} of {} latencies: got {got}, exact {exact}", latencies.len()), ok, true);
            }
        }
    });
}

#[test]
fn the_cached_flag_and_outside_writes() {
    run(async {
        let (app, service, _) = setup();
        service.add(&[("javascript", 5)], 0);
        let first = send(&app, "GET", "/suggest?q=script", "").await;
        check!("first", (first.1["cached"].clone(), listed(&first.1)), (json!(false), vec!["javascript".to_string()]));
        check!("second", send(&app, "GET", "/suggest?q=script", "").await.1["cached"].clone(), json!(true));
        service.add(&[("TypeScript", 9)], 0);
        let after = send(&app, "GET", "/suggest?q=script", "").await;
        check!("fresh", listed(&after.1), vec!["TypeScript".to_string(), "javascript".to_string()]);
        check!("infix", after.1["suggestions"][0]["kind"].clone(), json!("infix"));
    });
}

#[test]
fn q_comes_back_as_sent_and_key_is_its_search_key() {
    run(async {
        let (app, service, _) = setup();
        service.add(&[("Crème Brûlée", 2), ("Straße", 1)], 0);
        let typed = "  CRE\u{301}ME\t Br";
        let got = send(&app, "GET", &format!("/suggest?q={}", query(typed)), "").await;
        check!("q and key", (got.0, got.1["q"].clone(), got.1["key"].clone(), listed(&got.1)), (200, json!(typed), json!("creme br"), vec!["Crème Brûlée".to_string()]));
        let got = send(&app, "GET", &format!("/suggest?q={}", query("STRASS")), "").await;
        check!("ß", (got.1["key"].clone(), listed(&got.1)), (json!("strass"), vec!["Straße".to_string()]));
    });
}
