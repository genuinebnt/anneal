//! Talking to the anneal web app from the CLI: `anneal course login` keeps a session, every `anneal course test` reports its run,
//! and `anneal course solutions` uploads each stage's solution diff (the reference is not in the public repo; the app serves
//! it only to the signed-in owner). Everything here is best effort: a learner without a login just works offline.
//!
//! HTTP goes through `curl` (always present on macOS and Linux), with the body on stdin so large uploads don't hit argv limits.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

const COOKIE: &str = "anneal_session";

#[derive(Debug, Serialize, Deserialize)]
struct Session {
    url: String,
    token: String,
}

fn session_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("anneal").join("course-session.json"))
}

/// The app you signed in to (`anneal course login <url>`), if any.
pub fn app_url() -> Option<String> {
    load_session().map(|s| s.url)
}

fn load_session() -> Option<Session> {
    serde_json::from_str(&fs::read_to_string(session_path()?).ok()?).ok()
}

/// One `curl` call. Returns (status, headers, body).
fn curl(method: &str, url: &str, token: Option<&str>, body: Option<&str>) -> anyhow::Result<(u16, String, String)> {
    curl_within(60, method, url, token, body)
}

/// `curl` that gives up after `secs` seconds.
fn curl_within(secs: u32, method: &str, url: &str, token: Option<&str>, body: Option<&str>) -> anyhow::Result<(u16, String, String)> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "--max-time", &secs.to_string(), "-i", "-X", method, url, "-H", "content-type: application/json"]);
    if let Some(t) = token {
        cmd.args(["-H", &format!("Cookie: {COOKIE}={t}")]);
    }
    if body.is_some() {
        cmd.args(["--data-binary", "@-"]);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("running curl (is it installed?)")?;
    if let Some(mut stdin) = child.stdin.take()
        && let Some(b) = body
    {
        stdin.write_all(b.as_bytes())?;
    }
    let out = child.wait_with_output()?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    // `-i` prints the headers first; a proxy's "100 Continue" block can come before the real one.
    let mut rest = text.as_str();
    loop {
        let (head, tail) = rest.split_once("\r\n\r\n").context("unexpected reply from the server")?;
        let status: u16 = head.split_whitespace().nth(1).and_then(|s| s.parse().ok()).context("no HTTP status")?;
        if status == 100 {
            rest = tail;
            continue;
        }
        return Ok((status, head.to_owned(), tail.to_owned()));
    }
}

/// `anneal course login <url>`: asks for the passphrase and keeps the session cookie in ~/.config/anneal.
pub fn login(url: &str) -> anyhow::Result<()> {
    let url = url.trim_end_matches('/').to_owned();
    let pass = match std::env::var("ANNEAL_PASSPHRASE") {
        Ok(p) if !p.is_empty() => p,
        _ => rpassword::prompt_password(format!("Passphrase for {url}: "))?,
    };
    let body = serde_json::json!({ "passphrase": pass }).to_string();
    let (status, head, text) = curl("POST", &format!("{url}/api/auth/login"), None, Some(&body))?;
    match status {
        200 | 204 => {}
        401 => bail!("wrong passphrase"),
        s => bail!("login failed ({s}): {text}"),
    }
    let token = head
        .lines()
        .filter(|l| l.to_ascii_lowercase().starts_with("set-cookie:"))
        .find_map(|l| l.split_once(&format!("{COOKIE}="))?.1.split(';').next().map(str::to_owned));
    let token = match token {
        Some(t) if !t.is_empty() => t,
        // Login is off on a local server: no cookie needed.
        _ if status == 204 => String::new(),
        _ => bail!("the server accepted the passphrase but sent no session"),
    };
    let path = session_path().context("no home directory")?;
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(&path, serde_json::to_string(&Session { url: url.clone(), token })? + "\n")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    println!("Signed in to {url}. Runs from `anneal course test` and pushes are reported there now.");
    Ok(())
}

#[derive(Serialize)]
struct TestReport<'a> {
    name: &'a str,
    ok: bool,
    detail: &'a str,
}

/// Where runs wait when the app cannot be reached: one JSON line per run, in the learner's repo.
fn outbox_path(repo: &Path) -> PathBuf {
    repo.join(".anneal").join("outbox.jsonl")
}

fn queued(repo: &Path) -> Vec<String> {
    fs::read_to_string(outbox_path(repo)).map(|t| t.lines().filter(|l| !l.trim().is_empty()).map(str::to_owned).collect()).unwrap_or_default()
}

fn save_queue(repo: &Path, lines: &[String]) {
    let path = outbox_path(repo);
    if lines.is_empty() {
        let _ = fs::remove_file(path);
    } else {
        let _ = fs::write(path, lines.join("\n") + "\n");
    }
}

/// How a send went: delivered, worth retrying later (no network, a server error, an expired session), or refused for good.
enum Sent {
    Delivered,
    Retry(String),
    Refused(String),
}

fn send(s: &Session, course: &str, body: &str) -> Sent {
    match curl("POST", &format!("{}/api/courses/{course}/runs", s.url), Some(&s.token), Some(body)) {
        Ok((200, ..)) => Sent::Delivered,
        Ok((401, ..)) => Sent::Retry(format!("the session at {} expired: run `anneal course login {}`", s.url, s.url)),
        Ok((404, ..)) => Sent::Refused(format!("{} doesn't know this stage; it may not be published there yet", s.url)),
        Ok((st, _, b)) if st >= 500 => Sent::Retry(format!("the server answered {st}: {}", b.trim())),
        Ok((st, _, b)) => Sent::Refused(format!("{st}: {}", b.trim())),
        Err(e) => Sent::Retry(format!("can't reach {}: {e}", s.url)),
    }
}

/// Sends the queued runs, oldest first, stopping at the first one that cannot be delivered right now. Returns (sent, refused, left).
fn flush(repo: &Path, s: &Session) -> (usize, usize, usize) {
    let lines = queued(repo);
    let (mut sent, mut refused) = (0, 0);
    let mut left: Vec<String> = Vec::new();
    for (k, line) in lines.iter().enumerate() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            refused += 1;
            continue;
        };
        let (course, body) = (v["course"].as_str().unwrap_or(""), v["body"].to_string());
        match send(s, course, &body) {
            Sent::Delivered => sent += 1,
            Sent::Refused(_) => refused += 1,
            Sent::Retry(_) => {
                left.extend(lines[k..].iter().cloned());
                break;
            }
        }
    }
    save_queue(repo, &left);
    (sent, refused, left.len())
}

/// Tells the app a run is starting, so the stage page shows "Running…" until the report arrives. Quiet, quick and never fails the caller.
pub fn report_start(course: &str, stage: &str) {
    let Some(s) = load_session() else { return };
    let body = serde_json::json!({ "stage_id": stage }).to_string();
    let _ = curl_within(4, "POST", &format!("{}/api/courses/{course}/runs/start", s.url), Some(&s.token), Some(&body));
}

/// Asks the app to forget progress (`{"all": true}`, `{"module": "1a"}` or `{"project": 1}`). `None` when nobody is signed in.
pub fn reset_remote(course: &str, scope: &serde_json::Value) -> Option<anyhow::Result<String>> {
    let s = load_session()?;
    Some(match curl("POST", &format!("{}/api/courses/{course}/reset", s.url), Some(&s.token), Some(&scope.to_string())) {
        Ok((200, _, b)) => Ok(b),
        Ok((401, ..)) => Err(anyhow::anyhow!("the session expired: run `anneal course login {}`", s.url)),
        Ok((st, _, b)) => Err(anyhow::anyhow!("the app refused ({st}): {}", b.trim())),
        Err(e) => Err(e),
    })
}

/// The repository URL the app remembers for a course: `None` when nobody is signed in, `Some(Ok(None))` when none was recorded.
pub fn repo_remote(course: &str) -> Option<anyhow::Result<Option<String>>> {
    let s = load_session()?;
    Some(match curl("GET", &format!("{}/api/courses/{course}/repo", s.url), Some(&s.token), None) {
        Ok((200, _, b)) => Ok(serde_json::from_str::<serde_json::Value>(&b).ok().and_then(|v| v["url"].as_str().map(str::to_owned))),
        Ok((401, ..)) => Err(anyhow::anyhow!("the session expired: run `anneal course login {}`", s.url)),
        Ok((st, _, b)) => Err(anyhow::anyhow!("the app refused ({st}): {}", b.trim())),
        Err(e) => Err(e),
    })
}

/// Tells the app which repository holds the learner's work. `None` when nobody is signed in.
pub fn set_repo_remote(course: &str, url: &str) -> Option<anyhow::Result<()>> {
    let s = load_session()?;
    let body = serde_json::json!({ "url": url }).to_string();
    Some(match curl("PUT", &format!("{}/api/courses/{course}/repo", s.url), Some(&s.token), Some(&body)) {
        Ok((200 | 204, ..)) => Ok(()),
        Ok((401, ..)) => Err(anyhow::anyhow!("the session expired: run `anneal course login {}`", s.url)),
        Ok((st, _, b)) => Err(anyhow::anyhow!("the app refused ({st}): {}", b.trim())),
        Err(e) => Err(e),
    })
}

/// Reports one stage run, if a session exists. Never fails the caller. A run that cannot be delivered now is queued in
/// `.anneal/outbox.jsonl` and goes out with the next report or `anneal course sync`.
pub fn report_run(repo: &Path, course: &str, stage: &str, tests: &[(String, bool, String)], problem: Option<&str>, commit: &str, ms: u64) {
    let Some(s) = load_session() else { return };
    let tests: Vec<TestReport> = tests.iter().map(|(n, ok, d)| TestReport { name: n.rsplit("::").next().unwrap_or(n), ok: *ok, detail: d }).collect();
    let body = serde_json::json!({ "stage_id": stage, "tests": tests, "problem": problem, "commit": commit, "duration_ms": ms });
    let (earlier, ..) = flush(repo, &s);
    match send(&s, course, &body.to_string()) {
        Sent::Delivered => {
            let more = if earlier > 0 { format!(" (and {earlier} earlier queued run{})", if earlier == 1 { "" } else { "s" }) } else { String::new() };
            println!("\nReported to {}{more}", s.url);
        }
        Sent::Retry(why) => {
            let mut q = queued(repo);
            q.push(serde_json::json!({ "course": course, "body": body }).to_string());
            let n = q.len();
            save_queue(repo, &q);
            println!("\nNot reported yet: {why}.\nQueued ({n} waiting); `anneal course sync` or your next test run sends them.");
        }
        Sent::Refused(why) => println!("\nNot reported: {why}"),
    }
}

/// `anneal course sync`: sends what is queued.
pub fn sync(repo: &Path) -> anyhow::Result<()> {
    let s = load_session().context("not signed in: run `anneal course login <url>` first")?;
    let waiting = queued(repo).len();
    if waiting == 0 {
        println!("Nothing waiting to be reported.");
        return Ok(());
    }
    let (sent, refused, left) = flush(repo, &s);
    println!("Sent {sent} of {waiting} queued run{} to {}.", if waiting == 1 { "" } else { "s" }, s.url);
    if refused > 0 {
        println!("{refused} the server refused and dropped (an unknown stage, for example).");
    }
    if left > 0 {
        bail!("{left} still waiting: the server can't be reached or the session expired (`anneal course login {}`)", s.url);
    }
    Ok(())
}

#[derive(Serialize)]
pub struct SolutionFile {
    path: String,
    lines: Vec<String>,
}

/// For each stage: the diff between the files rendered with the stage undone and done.
///
/// `render(cutoff)` returns every reference file rendered for "stages up to `cutoff` are done", keyed by path.
pub fn solution_diffs(ranks: &HashMap<String, usize>, render: &dyn Fn(usize) -> anyhow::Result<BTreeMap<String, String>>) -> anyhow::Result<BTreeMap<String, Vec<SolutionFile>>> {
    let mut by_rank: Vec<(&String, usize)> = ranks.iter().map(|(k, v)| (k, *v)).collect();
    by_rank.sort_by_key(|(_, r)| *r);
    let mut out = BTreeMap::new();
    let mut before = render(0)?;
    for (id, rank) in by_rank {
        let after = render(rank)?;
        let mut files = Vec::new();
        for (path, new) in &after {
            let old = before.get(path).map(String::as_str).unwrap_or("");
            if old == new {
                continue;
            }
            let diff = TextDiff::from_lines(old, new);
            let mut lines = Vec::new();
            for (gi, group) in diff.grouped_ops(3).iter().enumerate() {
                if gi > 0 {
                    lines.push(" ⋯".to_owned());
                }
                for op in group {
                    for ch in diff.iter_changes(op) {
                        let sign = match ch.tag() {
                            ChangeTag::Equal => ' ',
                            ChangeTag::Insert => '+',
                            ChangeTag::Delete => '-',
                        };
                        lines.push(format!("{sign}{}", ch.value().trim_end_matches('\n')));
                    }
                }
            }
            files.push(SolutionFile { path: path.clone(), lines });
        }
        out.insert(id.clone(), files);
        before = after;
    }
    Ok(out)
}

/// Uploads `stages` (stage id -> files) to the signed-in app.
pub fn push_solutions(course: &str, stages: &BTreeMap<String, Vec<SolutionFile>>, only: Option<&[String]>) -> anyhow::Result<()> {
    let s = load_session().context("not signed in: run `anneal course login <url>` first")?;
    let mut sel: BTreeMap<&String, &Vec<SolutionFile>> = BTreeMap::new();
    for (k, v) in stages {
        if v.is_empty() || only.is_some_and(|o| !o.contains(k)) {
            continue;
        }
        sel.insert(k, v);
    }
    // The server rejects ids it doesn't publish; upload one stage at a time and report which were refused.
    let (mut ok, mut skipped) = (0, Vec::new());
    for (id, files) in sel {
        let body = serde_json::json!({ "stages": { id: files } }).to_string();
        match curl("PUT", &format!("{}/api/courses/{course}/solutions", s.url), Some(&s.token), Some(&body))? {
            (200, ..) => ok += 1,
            (401, ..) => bail!("the session expired: run `anneal course login {}`", s.url),
            (404, ..) => skipped.push(id.clone()),
            (st, _, b) => bail!("uploading {id} failed ({st}): {}", b.trim()),
        }
    }
    println!("Uploaded {ok} stage solutions to {}.", s.url);
    if !skipped.is_empty() {
        println!("Skipped {} the server doesn't publish yet: {}", skipped.len(), skipped.join(", "));
    }
    Ok(())
}

/// For `anneal course doctor`: the signed-in app's address, and whether it answers.
pub fn session_status() -> Option<(String, anyhow::Result<u16>)> {
    let s = load_session()?;
    let answer = curl("GET", &format!("{}/api/courses/bustub", s.url), Some(&s.token), None).map(|(st, ..)| st);
    Some((s.url, answer))
}

/// For `anneal course doctor`: how many runs wait to be reported.
pub fn queued_runs(repo: &Path) -> usize {
    queued(repo).len()
}

/// Opens hints up to the `n`th (1-based) through the server, which records that the stage was helped, and returns the `n`th as
/// (title, markdown) with whether this call opened it. Needs the sign-in from `anneal course login`.
pub fn open_hint(course: &str, stage: &str, n: usize) -> anyhow::Result<(String, String, bool)> {
    let s = load_session().context("opening a hint is recorded on the website: sign in first with `anneal course login <url>`")?;
    let base = format!("{}/api/courses/{course}/stages/{stage}", s.url.trim_end_matches('/'));
    let read = |status: u16, body: &str| -> anyhow::Result<serde_json::Value> {
        match status {
            200 => Ok(serde_json::from_str(body)?),
            401 => bail!("the session expired: run `anneal course login {}`", s.url),
            404 => bail!("the server does not publish stage {stage} yet"),
            _ => bail!("the server answered {status}: {}", body.trim()),
        }
    };
    let (status, _, body) = curl("GET", &base, Some(&s.token), None)?;
    let mut page = read(status, &body)?;
    let total = page["hints"]["total"].as_u64().unwrap_or(0) as usize;
    if total == 0 {
        bail!("stage {stage} has no hints written yet");
    }
    if n == 0 || n > total {
        bail!("stage {stage} has {total} hint{}: pick 1 to {total}", if total == 1 { "" } else { "s" });
    }
    let mut opened = false;
    while page["hints"]["revealed"].as_array().map_or(0, Vec::len) < n {
        let (status, _, body) = curl("POST", &format!("{base}/hints"), Some(&s.token), Some("{}"))?;
        page = read(status, &body)?;
        opened = true;
    }
    let hint = &page["hints"]["revealed"][n - 1];
    Ok((hint["title"].as_str().unwrap_or("").to_owned(), hint["md"].as_str().unwrap_or("").to_owned(), opened))
}

/// `anneal course adopt`: opens the stage's solution in the app (which marks the stage as helped) and returns its files: (path, diff lines).
pub fn open_solution(course: &str, stage: &str) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let s = load_session().context("not signed in: `anneal course adopt` fetches our solution from the app; run `anneal course login <url>` first")?;
    let (status, _, body) = curl("POST", &format!("{}/api/courses/{course}/stages/{stage}/solution", s.url), Some(&s.token), Some("{}"))?;
    match status {
        200 => {}
        401 => bail!("the session expired: run `anneal course login {}`", s.url),
        404 => bail!("the app has no solution for {stage} (a boss or a challenge has none, or it was not uploaded yet)"),
        st => bail!("the app refused ({st}): {}", body.trim()),
    }
    let page: serde_json::Value = serde_json::from_str(&body).context("reading the app's reply")?;
    let files = page["solution"]["files"].as_array().context("the app sent no solution files")?;
    Ok(files
        .iter()
        .map(|f| {
            let path = f["path"].as_str().unwrap_or("").to_owned();
            let lines = f["lines"].as_array().map(|l| l.iter().map(|x| x.as_str().unwrap_or("").to_owned()).collect()).unwrap_or_default();
            (path, lines)
        })
        .collect())
}
