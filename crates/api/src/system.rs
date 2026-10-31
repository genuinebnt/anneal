//! The server's own numbers for the Analytics page: the host (CPU, load, memory, swap, disk, uptime) read from `/proc`, the containers
//! from the Docker CLI, the database's size, and the API's own requests. Samples and requests are kept in memory, in rings of fixed size:
//! a day of one-minute samples and the last few hundred requests. A restart starts them again; nothing here is stored.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::Json;
use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use serde_json::Value;
use tokio::process::Command;

use crate::AppState;
use crate::error::ApiResult;

/// A day of one-minute samples.
const SAMPLES: usize = 24 * 60;
/// The requests kept for the live log.
const HITS: usize = 300;
/// A day of one-minute request counts.
const MINUTES: usize = 24 * 60;
/// How long a Docker command may take before its answer is left out.
const DOCKER_WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Serialize)]
pub struct Sample {
    /// Unix seconds.
    t: i64,
    /// Busy share of all CPUs over the minute's measuring window, 0 to 100.
    cpu: Option<f32>,
    load1: Option<f32>,
    mem_used: Option<u64>,
    swap_used: Option<u64>,
}

#[derive(Clone, Serialize)]
struct Hit {
    t: i64,
    method: String,
    path: String,
    status: u16,
    ms: u32,
}

#[derive(Clone, Serialize)]
struct Minute {
    t: i64,
    n: u32,
    errors: u32,
    ms_sum: u64,
    ms_max: u32,
}

struct Monitor {
    started: Instant,
    samples: Mutex<VecDeque<Sample>>,
    hits: Mutex<VecDeque<Hit>>,
    minutes: Mutex<VecDeque<Minute>>,
    totals: Mutex<(u64, u64)>,
}

static MONITOR: OnceLock<Monitor> = OnceLock::new();

fn monitor() -> &'static Monitor {
    MONITOR.get_or_init(|| Monitor {
        started: Instant::now(),
        samples: Mutex::new(VecDeque::with_capacity(SAMPLES)),
        hits: Mutex::new(VecDeque::with_capacity(HITS)),
        minutes: Mutex::new(VecDeque::with_capacity(MINUTES)),
        totals: Mutex::new((0, 0)),
    })
}

fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Pushes onto a ring, dropping the oldest when it is full.
fn push<T>(ring: &mut VecDeque<T>, cap: usize, v: T) {
    if ring.len() == cap {
        ring.pop_front();
    }
    ring.push_back(v);
}

/// Starts the one-minute sampler, once per process. Called from [`crate::app`]; outside a Tokio runtime it does nothing.
pub fn start() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if tokio::runtime::Handle::try_current().is_err() || STARTED.set(()).is_err() {
        return;
    }
    tokio::spawn(async {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            let s = sample().await;
            push(&mut monitor().samples.lock().unwrap_or_else(|e| e.into_inner()), SAMPLES, s);
        }
    });
}

/// Records every API request for the live log and the per-minute counts.
pub async fn record(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_owned();
    let start = Instant::now();
    let res = next.run(req).await;
    let ms = u32::try_from(start.elapsed().as_millis()).unwrap_or(u32::MAX);
    let status = res.status().as_u16();
    let t = now_secs();
    let m = monitor();
    {
        let mut totals = m.totals.lock().unwrap_or_else(|e| e.into_inner());
        totals.0 += 1;
        if status >= 500 {
            totals.1 += 1;
        }
    }
    {
        let mut minutes = m.minutes.lock().unwrap_or_else(|e| e.into_inner());
        let minute = t - t.rem_euclid(60);
        match minutes.back_mut() {
            Some(last) if last.t == minute => {
                last.n += 1;
                last.errors += u32::from(status >= 500);
                last.ms_sum += u64::from(ms);
                last.ms_max = last.ms_max.max(ms);
            }
            _ => push(&mut minutes, MINUTES, Minute { t: minute, n: 1, errors: u32::from(status >= 500), ms_sum: u64::from(ms), ms_max: ms }),
        }
    }
    push(&mut m.hits.lock().unwrap_or_else(|e| e.into_inner()), HITS, Hit { t, method, path, status, ms });
    res
}

// ---------------------------------------------------------------- the host, from /proc

/// Total and idle jiffies across all CPUs, from the first line of `/proc/stat`.
fn cpu_times() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let line = stat.lines().next()?.strip_prefix("cpu ")?;
    let v: Vec<u64> = line.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let idle = v.get(3)? + v.get(4).copied().unwrap_or(0);
    Some((v.iter().sum(), idle))
}

/// The busy share of the CPUs over a short window.
async fn cpu_percent() -> Option<f32> {
    let (t0, i0) = cpu_times()?;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (t1, i1) = cpu_times()?;
    let total = t1.checked_sub(t0)?;
    let idle = i1.checked_sub(i0)?;
    (total > 0).then(|| 100.0 * (total - idle.min(total)) as f32 / total as f32)
}

fn meminfo() -> Option<[u64; 4]> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let field = |name: &str| -> Option<u64> {
        let line = text.lines().find(|l| l.starts_with(name))?;
        line.split_whitespace().nth(1)?.parse::<u64>().ok().map(|kb| kb * 1024)
    };
    Some([field("MemTotal:")?, field("MemAvailable:")?, field("SwapTotal:").unwrap_or(0), field("SwapFree:").unwrap_or(0)])
}

fn loadavg() -> Option<[f32; 3]> {
    let text = std::fs::read_to_string("/proc/loadavg").ok()?;
    let mut it = text.split_whitespace().filter_map(|x| x.parse::<f32>().ok());
    Some([it.next()?, it.next()?, it.next()?])
}

fn uptime() -> Option<u64> {
    let text = std::fs::read_to_string("/proc/uptime").ok()?;
    text.split_whitespace().next()?.parse::<f64>().ok().map(|s| s as u64)
}

async fn sample() -> Sample {
    let cpu = cpu_percent().await;
    let mem = meminfo();
    Sample { t: now_secs(), cpu, load1: loadavg().map(|l| l[0]), mem_used: mem.map(|m| m[0].saturating_sub(m[1])), swap_used: mem.map(|m| m[2].saturating_sub(m[3])) }
}

#[derive(Serialize)]
struct Disk {
    path: String,
    total: u64,
    used: u64,
    free: u64,
}

/// The disk holding the work directory (a host mount in production), from `df`.
async fn disk() -> Option<Disk> {
    let path = std::env::var("ANNEAL_WORK_ROOT").ok().filter(|p| std::path::Path::new(p).is_dir()).unwrap_or_else(|| "/".into());
    let out = tokio::time::timeout(DOCKER_WAIT, Command::new("df").args(["-Pk", &path]).output()).await.ok()?.ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let cols: Vec<&str> = text.lines().nth(1)?.split_whitespace().collect();
    let kb = |i: usize| cols.get(i).and_then(|v| v.parse::<u64>().ok()).map(|v| v * 1024);
    Some(Disk { path, total: kb(1)?, used: kb(2)?, free: kb(3)? })
}

// ---------------------------------------------------------------- containers, from the Docker CLI

/// Runs `docker <args>` and parses each output line as JSON. None when Docker is not reachable or too slow.
async fn docker_lines(args: &[&str]) -> Option<Vec<Value>> {
    let out = tokio::time::timeout(DOCKER_WAIT, Command::new("docker").args(args).output()).await.ok()?.ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).lines().filter_map(|l| serde_json::from_str(l).ok()).collect())
}

#[derive(Serialize)]
struct Container {
    name: String,
    image: String,
    state: String,
    status: String,
    cpu: Option<String>,
    mem: Option<String>,
    mem_pct: Option<String>,
    net: Option<String>,
    block: Option<String>,
}

async fn containers() -> Option<Vec<Container>> {
    let (ps, stats) = tokio::join!(docker_lines(&["ps", "-a", "--format", "{{json .}}"]), docker_lines(&["stats", "--no-stream", "--format", "{{json .}}"]));
    let ps = ps?;
    let stats = stats.unwrap_or_default();
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
    // sandboxes come and go; the page lists the long-running services
    let mut out: Vec<Container> = ps
        .iter()
        .filter(|c| !s(c, "Image").contains("anneal-runner"))
        .map(|c| {
            let name = s(c, "Names");
            let st = stats.iter().find(|x| s(x, "Name") == name);
            let get = |k: &str| st.map(|x| s(x, k)).filter(|v| !v.is_empty());
            Container { image: s(c, "Image"), state: s(c, "State"), status: s(c, "Status"), cpu: get("CPUPerc"), mem: get("MemUsage"), mem_pct: get("MemPerc"), net: get("NetIO"), block: get("BlockIO"), name }
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Some(out)
}

#[derive(Serialize, Clone)]
struct HostInfo {
    hostname: Option<String>,
    os: Option<String>,
    kernel: Option<String>,
    cpus: Option<u64>,
    mem_total: Option<u64>,
}

/// What Docker knows about the host. It changes only on a reboot, so it is asked once.
async fn host_info() -> HostInfo {
    static INFO: tokio::sync::OnceCell<HostInfo> = tokio::sync::OnceCell::const_new();
    INFO.get_or_init(|| async {
        let v = docker_lines(&["info", "--format", "{{json .}}"]).await.and_then(|mut l| l.pop());
        let s = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(Value::as_str).map(str::to_owned);
        let n = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(Value::as_u64);
        HostInfo { hostname: s("Name"), os: s("OperatingSystem"), kernel: s("KernelVersion"), cpus: n("NCPU"), mem_total: n("MemTotal") }
    })
    .await
    .clone()
}

// ---------------------------------------------------------------- the database

#[derive(Serialize)]
struct Db {
    size: i64,
    connections: i64,
    tables: Vec<(String, i64)>,
}

async fn db(pool: &sqlx::PgPool) -> ApiResult<Db> {
    let size: i64 = sqlx::query_scalar("SELECT pg_database_size(current_database())::bigint").fetch_one(pool).await?;
    let connections: i64 = sqlx::query_scalar("SELECT count(*)::bigint FROM pg_stat_activity WHERE datname = current_database()").fetch_one(pool).await?;
    let tables: Vec<(String, i64)> = sqlx::query_as("SELECT relname::text, n_live_tup::bigint FROM pg_stat_user_tables ORDER BY n_live_tup DESC LIMIT 8").fetch_all(pool).await?;
    Ok(Db { size, connections, tables })
}

// ---------------------------------------------------------------- GET /api/system

/// `GET /api/system`: the host now and over the day, the containers, the database and the API's own requests.
pub async fn get(State(s): State<AppState>) -> ApiResult<Json<Value>> {
    let (now, disk, containers, info, db) = tokio::join!(sample(), disk(), containers(), host_info(), db(&s.db));
    let db = db?;
    let mem = meminfo();
    let m = monitor();
    // copy what is needed out of each ring, so no lock is held while the answer is built
    let samples: Vec<Sample> = m.samples.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
    let minutes: Vec<Minute> = m.minutes.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned().collect();
    let recent: Vec<Hit> = m.hits.lock().unwrap_or_else(|e| e.into_inner()).iter().rev().take(60).cloned().collect();
    let (requests, errors) = *m.totals.lock().unwrap_or_else(|e| e.into_inner());
    let app_image = containers.as_ref().and_then(|cs| cs.iter().find(|c| c.name == "anneal-app").map(|c| (c.image.clone(), c.status.clone())));
    Ok(Json(serde_json::json!({
        "host": {
            "hostname": info.hostname,
            "os": info.os,
            "kernel": info.kernel,
            "cpus": info.cpus.or_else(|| std::thread::available_parallelism().ok().map(|n| n.get() as u64)),
            "uptime": uptime(),
        },
        "now": {
            "cpu": now.cpu,
            "load": loadavg(),
            "mem": mem.map(|m| serde_json::json!({ "total": m[0], "available": m[1], "used": m[0].saturating_sub(m[1]) })),
            "swap": mem.map(|m| serde_json::json!({ "total": m[2], "used": m[2].saturating_sub(m[3]) })),
            "disk": disk,
        },
        "samples": samples,
        "containers": containers,
        "db": db,
        "app": {
            "version": env!("CARGO_PKG_VERSION"),
            "image": app_image.as_ref().map(|a| &a.0),
            "status": app_image.as_ref().map(|a| &a.1),
            "up": m.started.elapsed().as_secs(),
            "requests": requests,
            "errors": errors,
            "minutes": minutes,
            "recent": recent,
        },
    })))
}
