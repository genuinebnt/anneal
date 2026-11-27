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

/// Where reports wait when the app cannot be reached: one JSON line each, in the learner's repo. A line is a run
/// (`{"kind":"run","course":..,"body":{..}}`; lines written before kinds existed have no `kind` and are runs) or a hint opened
/// offline (`{"kind":"hint","course":..,"stage":..,"n":..}`).
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

/// One line of the outbox.
#[derive(Debug, PartialEq)]
enum Queued {
    /// A test run: the body of `POST /api/courses/<course>/runs`.
    Run { course: String, body: String },
    /// Hints 1 to `n` of a stage were opened while the app could not be asked.
    Hint { course: String, stage: String, n: usize },
}

fn run_line(course: &str, body: &serde_json::Value) -> String {
    serde_json::json!({ "kind": "run", "course": course, "body": body }).to_string()
}

fn hint_line(course: &str, stage: &str, n: usize) -> String {
    serde_json::json!({ "kind": "hint", "course": course, "stage": stage, "n": n }).to_string()
}

/// Reads one outbox line; `None` for a line that is not JSON or names an unknown kind.
fn parse_line(line: &str) -> Option<Queued> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let course = v["course"].as_str().unwrap_or("").to_owned();
    match v.get("kind").and_then(|k| k.as_str()) {
        None | Some("run") => Some(Queued::Run { course, body: v["body"].to_string() }),
        Some("hint") => Some(Queued::Hint { course, stage: v["stage"].as_str()?.to_owned(), n: usize::try_from(v["n"].as_u64()?).ok()? }),
        Some(_) => None,
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

/// What a flush of the outbox did.
#[derive(Debug, Default, PartialEq)]
struct Flushed {
    runs: usize,
    hints: usize,
    refused: usize,
    /// The lines still waiting, in the order they will be sent.
    left: Vec<String>,
    /// Why the first line still waiting could not be sent.
    why: Option<String>,
}

impl Flushed {
    fn hint_waiting(&self) -> bool {
        self.left.iter().any(|l| matches!(parse_line(l), Some(Queued::Hint { .. })))
    }
}

/// Sends queued lines, stopping at the first one that cannot be delivered right now. Hint opens go first, then runs, each oldest
/// first: a stage's "helped" mark must reach the app before a run that passes it, or the app would count the stage as passed without help.
fn flush_lines(lines: &[String], send_hint: &mut dyn FnMut(&str, &str, usize) -> Sent, send_run: &mut dyn FnMut(&str, &str) -> Sent) -> Flushed {
    let mut ordered: Vec<(&String, Queued)> = Vec::new();
    let mut f = Flushed::default();
    for line in lines {
        match parse_line(line) {
            Some(q) => ordered.push((line, q)),
            None => f.refused += 1,
        }
    }
    ordered.sort_by_key(|(_, q)| matches!(q, Queued::Run { .. }));
    for (k, (_, q)) in ordered.iter().enumerate() {
        let sent = match q {
            Queued::Hint { course, stage, n } => send_hint(course, stage, *n),
            Queued::Run { course, body } => send_run(course, body),
        };
        match sent {
            Sent::Delivered if matches!(q, Queued::Hint { .. }) => f.hints += 1,
            Sent::Delivered => f.runs += 1,
            Sent::Refused(_) => f.refused += 1,
            Sent::Retry(why) => {
                f.left = ordered[k..].iter().map(|(l, _)| (*l).clone()).collect();
                f.why = Some(why);
                break;
            }
        }
    }
    f
}

/// Sends the queued hint opens and runs (see `flush_lines`) and keeps what is left in the outbox.
fn flush(repo: &Path, s: &Session) -> Flushed {
    let f = flush_lines(
        &queued(repo),
        &mut |course, stage, n| match reveal_hints(s, course, stage, n, false) {
            Ok(_) => Sent::Delivered,
            Err(e) => e,
        },
        &mut |course, body| send(s, course, body),
    );
    save_queue(repo, &f.left);
    f
}

/// Remembers that hints 1 to `n` of `stage` were read from the learner's copy, so the next sync tells the app the stage was helped.
/// Nothing is added when a waiting line already covers it.
pub fn queue_hint(repo: &Path, course: &str, stage: &str, n: usize) {
    let mut q = queued(repo);
    let covered = q.iter().any(|l| matches!(parse_line(l), Some(Queued::Hint { course: c, stage: st, n: m }) if c == course && st == stage && m >= n));
    if !covered {
        q.push(hint_line(course, stage, n));
        save_queue(repo, &q);
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
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
/// `.anneal/outbox.jsonl` and goes out with the next report or `anneal course sync`. So is a run while a hint opened offline is still
/// waiting: the hint has to reach the app first.
pub fn report_run(repo: &Path, course: &str, stage: &str, tests: &[(String, bool, String)], problem: Option<&str>, commit: &str, ms: u64) {
    let Some(s) = load_session() else { return };
    let tests: Vec<TestReport> = tests.iter().map(|(n, ok, d)| TestReport { name: n.rsplit("::").next().unwrap_or(n), ok: *ok, detail: d }).collect();
    let body = serde_json::json!({ "stage_id": stage, "tests": tests, "problem": problem, "commit": commit, "duration_ms": ms });
    let earlier = flush(repo, &s);
    let sent = match &earlier.why {
        Some(why) if earlier.hint_waiting() => Sent::Retry(format!("a hint opened offline has to reach {} first ({why})", s.url)),
        _ => send(&s, course, &body.to_string()),
    };
    match sent {
        Sent::Delivered => {
            let mut also = Vec::new();
            if earlier.runs > 0 {
                also.push(format!("{} earlier queued", plural(earlier.runs, "run", "runs")));
            }
            if earlier.hints > 0 {
                also.push(plural(earlier.hints, "hint opened offline", "hints opened offline"));
            }
            let more = if also.is_empty() { String::new() } else { format!(" (and {})", also.join(" and ")) };
            println!("\nReported to {}{more}", s.url);
        }
        Sent::Retry(why) => {
            let mut q = queued(repo);
            q.push(run_line(course, &body));
            let n = q.len();
            save_queue(repo, &q);
            println!("\nNot reported yet: {why}.\nQueued ({n} waiting); `anneal course sync` or your next test run sends them.");
        }
        Sent::Refused(why) => println!("\nNot reported: {why}"),
    }
}

/// `anneal course sync`: sends what is queued (hint opens first, then runs).
pub fn sync(repo: &Path) -> anyhow::Result<()> {
    let s = load_session().context("not signed in: run `anneal course login <url>` first")?;
    let waiting = queued(repo).len();
    if waiting == 0 {
        println!("Nothing waiting to be reported.");
        return Ok(());
    }
    let f = flush(repo, &s);
    let mut sent = vec![plural(f.runs, "run", "runs")];
    if f.hints > 0 {
        sent.push(plural(f.hints, "hint opened offline", "hints opened offline"));
    }
    println!("Sent {} of {waiting} waiting to {}: {}.", f.runs + f.hints, s.url, sent.join(" and "));
    if f.refused > 0 {
        println!("{} the server refused and dropped (an unknown stage, for example).", f.refused);
    }
    if !f.left.is_empty() {
        bail!("{} still waiting: {}", f.left.len(), f.why.as_deref().unwrap_or("the server can't be reached"));
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

/// For `anneal course doctor`: how many runs and hint opens wait to be reported.
pub fn queued_runs(repo: &Path) -> usize {
    queued(repo).len()
}

/// What `anneal course show --hint` got: the hint from the app, which recorded that the stage was helped, or why the app could not be
/// asked (nobody is signed in, no network, a server error, an expired session), in which case the caller reads the learner's copy.
pub enum HintOpen {
    Online { title: String, md: String, opened: bool },
    Offline(String),
}

/// Opens hints up to the `n`th (1-based) through the server, which records that the stage was helped, and returns the `n`th with whether
/// this call opened it. A refusal from the app (no such stage, no such hint) is an error.
pub fn open_hint(course: &str, stage: &str, n: usize) -> anyhow::Result<HintOpen> {
    let Some(s) = load_session() else { return Ok(HintOpen::Offline("not signed in".into())) };
    match reveal_hints(&s, course, stage, n, true) {
        Ok((page, opened)) => {
            let hint = &page["hints"]["revealed"][n - 1];
            Ok(HintOpen::Online { title: hint["title"].as_str().unwrap_or("").to_owned(), md: hint["md"].as_str().unwrap_or("").to_owned(), opened })
        }
        Err(Sent::Retry(why)) => Ok(HintOpen::Offline(why)),
        Err(Sent::Refused(why)) => bail!("{why}"),
        Err(Sent::Delivered) => unreachable!("an error is never a delivery"),
    }
}

/// Asks the app to reveal the hints of `stage` until at least `n` are revealed: it reveals them one at a time, in order, and records
/// that the stage was helped. Reads the current count first, so hints already revealed are not asked for again. Returns the stage page
/// as the app sends it and whether this call revealed any. With `exact`, an `n` past the stage's hints is refused; without it (a hint
/// opened offline, from a copy of the stage that may differ from the app's), every hint up to `n` that the app has is revealed.
fn reveal_hints(s: &Session, course: &str, stage: &str, n: usize, exact: bool) -> Result<(serde_json::Value, bool), Sent> {
    let base = format!("{}/api/courses/{course}/stages/{stage}", s.url.trim_end_matches('/'));
    let read = |reply: anyhow::Result<(u16, String, String)>| -> Result<serde_json::Value, Sent> {
        match reply {
            Ok((200, _, body)) => serde_json::from_str(&body).map_err(|e| Sent::Retry(format!("the server sent a page that does not read: {e}"))),
            Ok((401, ..)) => Err(Sent::Retry(format!("the session at {} expired: run `anneal course login {}`", s.url, s.url))),
            Ok((404, ..)) => Err(Sent::Refused(format!("the server does not publish stage {stage} yet"))),
            Ok((st, _, body)) if st >= 500 => Err(Sent::Retry(format!("the server answered {st}: {}", body.trim()))),
            Ok((st, _, body)) => Err(Sent::Refused(format!("the server answered {st}: {}", body.trim()))),
            Err(e) => Err(Sent::Retry(format!("can't reach {}: {e}", s.url))),
        }
    };
    let mut page = read(curl("GET", &base, Some(&s.token), None))?;
    let total = page["hints"]["total"].as_u64().unwrap_or(0) as usize;
    if total == 0 {
        return Err(Sent::Refused(format!("stage {stage} has no hints written yet")));
    }
    if exact && (n == 0 || n > total) {
        return Err(Sent::Refused(format!("stage {stage} has {total} hint{}: pick 1 to {total}", if total == 1 { "" } else { "s" })));
    }
    let goal = n.clamp(1, total);
    let revealed = |p: &serde_json::Value| p["hints"]["revealed"].as_array().map_or(0, Vec::len);
    let mut opened = false;
    while revealed(&page) < goal {
        let before = revealed(&page);
        page = read(curl("POST", &format!("{base}/hints"), Some(&s.token), Some("{}")))?;
        opened = true;
        if revealed(&page) <= before {
            return Err(Sent::Refused(format!("the server did not reveal hint {} of stage {stage}", before + 1)));
        }
    }
    Ok((page, opened))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbox_lines_name_their_kind_and_old_run_lines_still_read_as_runs() {
        let old = r#"{"course":"bustub","body":{"stage_id":"1a-01","tests":[],"problem":null,"commit":"abc","duration_ms":5}}"#;
        match parse_line(old) {
            Some(Queued::Run { course, body }) => {
                assert_eq!(course, "bustub");
                let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                assert_eq!(v["stage_id"], "1a-01", "the body is sent as it was queued");
            }
            other => panic!("an old line is a run: {other:?}"),
        }
        let body = serde_json::json!({ "stage_id": "1a-02" });
        let run = run_line("bustub", &body);
        assert!(run.contains(r#""kind":"run""#), "{run}");
        assert_eq!(parse_line(&run), Some(Queued::Run { course: "bustub".into(), body: body.to_string() }));
        let hint = hint_line("bustub", "1a-03", 2);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&hint).unwrap()["kind"], "hint");
        assert_eq!(parse_line(&hint), Some(Queued::Hint { course: "bustub".into(), stage: "1a-03".into(), n: 2 }));
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line(r#"{"kind":"later","course":"bustub"}"#), None, "a kind from a newer anneal is not taken for a run");
    }

    #[test]
    fn queued_hint_opens_are_sent_before_queued_runs() {
        let lines = vec![
            run_line("bustub", &serde_json::json!({ "stage_id": "1a-01" })),
            hint_line("bustub", "1a-02", 1),
            r#"{"course":"bustub","body":{"stage_id":"1a-02"}}"#.to_owned(),
            hint_line("bustub", "1a-03", 2),
        ];
        let calls = std::cell::RefCell::new(Vec::<String>::new());
        let f = flush_lines(
            &lines,
            &mut |_, stage, n| {
                calls.borrow_mut().push(format!("hint {stage} {n}"));
                Sent::Delivered
            },
            &mut |_, body| {
                let v: serde_json::Value = serde_json::from_str(body).unwrap();
                calls.borrow_mut().push(format!("run {}", v["stage_id"].as_str().unwrap()));
                Sent::Delivered
            },
        );
        assert_eq!(calls.into_inner(), ["hint 1a-02 1", "hint 1a-03 2", "run 1a-01", "run 1a-02"]);
        assert_eq!((f.hints, f.runs, f.refused, f.left.len()), (2, 2, 0, 0));
    }

    #[test]
    fn a_hint_that_cannot_be_sent_holds_back_every_run() {
        let lines = vec![run_line("bustub", &serde_json::json!({ "stage_id": "1a-02" })), hint_line("bustub", "1a-02", 1)];
        let mut runs = 0;
        let f = flush_lines(&lines, &mut |_, _, _| Sent::Retry("offline".into()), &mut |_, _| {
            runs += 1;
            Sent::Delivered
        });
        assert_eq!(runs, 0, "a passing run must not reach the app before the stage's helped mark");
        assert_eq!(f.left, [lines[1].clone(), lines[0].clone()], "both stay, the hint first");
        assert!(f.hint_waiting());
        assert_eq!(f.why.as_deref(), Some("offline"));
    }

    #[test]
    fn a_hint_opened_offline_is_queued_once() {
        let repo = std::env::temp_dir().join(format!("anneal-outbox-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join(".anneal")).unwrap();
        fs::write(outbox_path(&repo), r#"{"course":"bustub","body":{"stage_id":"1a-01"}}"#.to_owned() + "\n").unwrap();
        queue_hint(&repo, "bustub", "1a-02", 2);
        queue_hint(&repo, "bustub", "1a-02", 1);
        queue_hint(&repo, "bustub", "1a-02", 2);
        queue_hint(&repo, "bustub", "1a-02", 3);
        let q = queued(&repo);
        assert_eq!(q.len(), 3, "the old run, hint 2, then hint 3 (hint 1 and a second hint 2 are covered): {q:?}");
        assert!(matches!(parse_line(&q[0]), Some(Queued::Run { .. })), "the old line is kept as it was");
        assert_eq!(parse_line(&q[2]), Some(Queued::Hint { course: "bustub".into(), stage: "1a-02".into(), n: 3 }));
        let _ = fs::remove_dir_all(&repo);
    }
}
