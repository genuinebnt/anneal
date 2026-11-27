use solution::*;

#[allow(unused_imports)]
use std::time::Duration;

#[allow(dead_code)]
pub fn run<F: std::future::Future>(f: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_time().start_paused(true).build().unwrap().block_on(f)
}

#[allow(unused_imports)]
use axum::Router;
#[allow(unused_imports)]
use axum::body::{Body, to_bytes};
#[allow(unused_imports)]
use axum::http::{HeaderMap, Request};
#[allow(unused_imports)]
use serde_json::{Value, json};
#[allow(unused_imports)]
use tower::ServiceExt;

#[allow(dead_code)]
pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub body: String,
}

#[allow(dead_code)]
impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or_else(|_| Value::String(self.body.clone()))
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers.get(name).map(|v| String::from_utf8_lossy(v.as_bytes()).into_owned())
    }

    /// (status, error code)
    pub fn error(&self) -> (u16, String) {
        (self.status, self.json()["error"]["code"].as_str().unwrap_or("<no error code>").to_string())
    }
}

#[allow(dead_code)]
pub fn failure(status: u16, code: &str) -> (u16, String) {
    (status, code.to_string())
}

#[allow(dead_code)]
pub async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let request = request.body(Body::from(body.to_string())).unwrap();
    let exchange = async {
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), 1 << 22).await.unwrap();
        Reply { status, headers, body: String::from_utf8_lossy(&bytes).into_owned() }
    };
    tokio::time::timeout(std::time::Duration::from_secs(3600), exchange).await.expect("the response did not finish")
}

#[allow(dead_code)]
pub async fn get(app: &Router, uri: &str) -> Reply {
    send(app, "GET", uri, &[], "").await
}

#[allow(dead_code)]
pub async fn post_job(app: &Router, key: &str, body: &str) -> Reply {
    send(app, "POST", "/v1/jobs", &[("idempotency-key", key)], body).await
}

#[allow(dead_code)]
pub fn job_json(source: &str, work_ms: u64) -> String {
    json!({"source": source, "width": 10, "height": 20, "work_ms": work_ms}).to_string()
}

#[allow(dead_code)]
pub fn ids(page: &Value) -> Vec<u64> {
    page["items"].as_array().map(|a| a.iter().map(|j| j["id"].as_u64().unwrap_or(0)).collect()).unwrap_or_default()
}

fn app() -> Router {
    router(JobStore::new())
}

async fn app_with(n: u64) -> Router {
    let app = app();
    for i in 1..=n {
        post_job(&app, &format!("key-{i}"), &job_json("a", 1)).await;
    }
    app
}

#[test]
fn only_a_new_job_has_location_and_only_a_retry_is_marked() {
    run(async {
        let app = app();
        let new = post_job(&app, "k", &job_json("a", 1)).await;
        let again = post_job(&app, "k", &job_json("a", 1)).await;
        check!("new job: idempotent-replayed", new.header("idempotent-replayed"), None);
        check!("retry: location", again.header("location"), None);
    });
}

#[test]
fn replay_compares_jobs_not_text() {
    run(async {
        let app = app();
        post_job(&app, "k", r#"{"source":"a","width":1,"height":2}"#).await;
        let same = post_job(&app, "k", r#"{ "height": 2, "width": 1, "format": "png", "work_ms": 20, "source": "a" }"#).await;
        check!("the same job written differently", same.status, 200);
        let other = post_job(&app, "k", r#"{"source":"a","width":1,"height":3}"#).await;
        check!("another job", other.error(), failure(409, "idempotency_key_reuse"));
    });
}

#[test]
fn bad_posts_leave_no_job() {
    run(async {
        let app = app();
        check!("junk", post_job(&app, "a", "{nope").await.error(), failure(422, "invalid_json"));
        let r = post_job(&app, "a", r#"{"source":"","width":0,"height":1}"#).await;
        check!("invalid", r.error(), failure(422, "validation_failed"));
        check!("both details", r.json()["error"]["details"].as_array().map(Vec::len), Some(2));
        check!("empty key", send(&app, "POST", "/v1/jobs", &[("idempotency-key", "")], &job_json("a", 1)).await.error(), failure(400, "missing_idempotency_key"));
        check!("no jobs", ids(&get(&app, "/v1/jobs").await.json()), Vec::<u64>::new());
        let r = post_job(&app, "a", &job_json("a", 1)).await;
        check!("the key was not used up", (r.status, r.json()["id"].clone(), r.json()["created_at"].clone()), (201, json!(1), json!(1)));
    });
}

#[test]
fn a_posted_array_is_not_a_job() {
    run(async {
        let r = post_job(&app(), "k", r#"["a.png",1,1,"png",20]"#).await;
        check!("POST a filled array", r.error(), failure(422, "invalid_json"));
    });
}

#[test]
fn the_key_is_checked_before_the_body() {
    run(async {
        let r = send(&app(), "POST", "/v1/jobs", &[], "{nope").await;
        check!("no key and a bad body", r.error(), failure(400, "missing_idempotency_key"));
    });
}

#[test]
fn the_content_type_does_not_matter() {
    run(async {
        let r = send(&app(), "POST", "/v1/jobs", &[("idempotency-key", "k"), ("content-type", "text/plain")], &job_json("a", 1)).await;
        check!("text/plain", r.status, 201);
    });
}

#[test]
fn ids_that_are_not_jobs() {
    run(async {
        let app = app_with(1).await;
        for raw in ["0", "2", "abc", "-1", "1.0", "18446744073709551615", "18446744073709551616", "%FF"] {
            check!(format!("GET /v1/jobs/{raw}"), get(&app, &format!("/v1/jobs/{raw}")).await.error(), failure(404, "job_not_found"));
        }
    });
}

#[test]
fn routes_and_methods() {
    run(async {
        let app = app();
        for uri in ["/", "/v1", "/v1/job", "/v1/jobs/1/2", "/v2/jobs"] {
            check!(format!("GET {uri}"), get(&app, uri).await.error(), failure(404, "no_route"));
        }
        for (method, uri) in [("DELETE", "/v1/jobs"), ("PUT", "/v1/jobs/1"), ("POST", "/v1/jobs/1")] {
            check!(format!("{method} {uri}"), send(&app, method, uri, &[], "").await.error(), failure(405, "method_not_allowed"));
        }
    });
}

#[test]
fn default_and_clamped_limits() {
    run(async {
        let app = app_with(120).await;
        for (uri, want) in [("/v1/jobs", 10), ("/v1/jobs?limit=0", 1), ("/v1/jobs?limit=1", 1), ("/v1/jobs?limit=100", 100), ("/v1/jobs?limit=101", 100), ("/v1/jobs?limit=18446744073709551615", 100)] {
            let page = get(&app, uri).await.json();
            check!(format!("GET {uri}: (number of items, next_after)"), (ids(&page).len(), page["next_after"].clone()), (want, json!(want)));
        }
    });
}

#[test]
fn next_after_only_when_more_follow() {
    run(async {
        let app = app_with(4).await;
        for (uri, want_ids, next) in [("/v1/jobs?limit=4", vec![1, 2, 3, 4], Value::Null), ("/v1/jobs?limit=3", vec![1, 2, 3], json!(3)), ("/v1/jobs?after=2&limit=2", vec![3, 4], Value::Null), ("/v1/jobs?after=4", vec![], Value::Null), ("/v1/jobs?after=1000", vec![], Value::Null)] {
            let page = get(&app, uri).await.json();
            check!(format!("GET {uri}"), (ids(&page), page["next_after"].clone()), (want_ids, next));
        }
    });
}

#[test]
fn the_largest_cursor() {
    run(async {
        let app = app_with(3).await;
        let r = get(&app, "/v1/jobs?after=18446744073709551615").await;
        check!("after = u64::MAX: (status, body)", (r.status, r.json()), (200, json!({"items": [], "next_after": null})));
        check!("the service still works", get(&app, "/v1/jobs/3").await.status, 200);
    });
}

#[test]
fn bad_queries() {
    run(async {
        let app = app_with(2).await;
        for query in ["limit=abc", "limit=-1", "limit=", "limit=1.5", "after=x", "after=-1", "after=", "after=18446744073709551616", "limit=2&after=no", "limit=99999999999999999999"] {
            check!(format!("GET /v1/jobs?{query}"), get(&app, &format!("/v1/jobs?{query}")).await.error(), failure(400, "bad_query"));
        }
        check!("unknown parameters are ignored", ids(&get(&app, "/v1/jobs?sort=new&limit=1").await.json()), vec![1]);
    });
}

#[test]
fn the_list_shows_whole_jobs() {
    run(async {
        let app = app_with(1).await;
        let one = get(&app, "/v1/jobs/1").await.json();
        check!("item equals the job", get(&app, "/v1/jobs").await.json()["items"][0].clone(), one);
    });
}

#[test]
fn the_router_serves_the_store_it_is_given() {
    run(async {
        let store = JobStore::new();
        let app = router(store.clone());
        post_job(&app, "k", &job_json("a", 1)).await;
        store.start(1);
        check!("a change made on the store shows over HTTP", get(&app, "/v1/jobs/1").await.json()["status"].clone(), json!("running"));
    });
}

#[test]
fn walks_with_new_jobs_arriving() {
    let mut rng = anneal_prelude::Rng::new(22003);
    for _ in 0..40 {
        let start = rng.below(25) as u64;
        let limits: Vec<u64> = rng.vec(40, 1, 7);
        run(async {
            let app = app_with(start).await;
            let mut total = start;
            let mut after = 0;
            let mut seen = Vec::new();
            for (turn, limit) in limits.iter().enumerate() {
                let page = get(&app, &format!("/v1/jobs?limit={limit}&after={after}")).await.json();
                let want: Vec<u64> = (after + 1..=total).take(*limit as usize).collect();
                let more = total > after + *limit;
                let input = format!("{total} jobs, limit {limit}, after {after}");
                check!(input, (ids(&page), page["next_after"].clone()), (want.clone(), if more { json!(want[want.len() - 1]) } else { Value::Null }));
                seen.extend(want);
                match page["next_after"].as_u64() {
                    Some(next) => after = next,
                    None => break,
                }
                if turn < 5 {
                    total += 1;
                    post_job(&app, &format!("late-{turn}"), &job_json("late", 1)).await;
                }
            }
            check!("every job once, in order", seen, (1..=total).collect::<Vec<u64>>());
        });
    }
}
