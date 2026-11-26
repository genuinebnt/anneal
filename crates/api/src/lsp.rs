//! rust-analyzer for the browser editor, over a WebSocket.
//!
//! Each connection gets its own throwaway cargo workspace (the same package a run
//! builds) and its own `rust-analyzer` process. The bridge relays JSON-RPC between
//! the socket (one message per text frame) and rust-analyzer's stdio
//! (`Content-Length` framing), and:
//!
//! - shows the browser a virtual root, `file:///workspace`, never the real path;
//! - answers server-to-client requests itself, since the browser client ignores them;
//! - handles an `anneal/save` notification by writing the buffer to disk and sending
//!   `didSave`, which triggers rust-analyzer's `cargo check`. That's where borrow-checker
//!   errors like E0499 come from. It runs on the host, not in the sandbox: the package has
//!   no dependencies, build scripts or proc macros, so compiling it runs none of the user's code.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, close_code};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::sync::{Semaphore, mpsc};
use tokio::time::Instant;

/// Why a fourth editor gets no rust-analyzer. A close reason holds at most 123 bytes.
pub const BUSY: &str = "already running for 3 editors; close one";

/// How often the browser is pinged, and how long it may stay silent before its session is ended and its slot freed
/// (a sleeping laptop or a dropped network never closes the socket itself).
const PING_EVERY: Duration = Duration::from_secs(20);
const SILENT_FOR: Duration = Duration::from_secs(70);

/// Closes a new socket, saying why.
pub async fn refuse(mut socket: WebSocket, reason: &'static str) {
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: close_code::AGAIN,
            reason: reason.into(),
        })))
        .await;
}

/// What the browser sees as the workspace root.
pub const VIRTUAL_ROOT: &str = "file:///workspace";

#[derive(Clone)]
pub struct LspConfig {
    pub binary: PathBuf,
    /// Session workspaces live under `<work_root>/lsp`.
    pub work_root: PathBuf,
    /// Caps concurrent rust-analyzer processes; each one indexes std on start.
    pub slots: Arc<Semaphore>,
}

impl LspConfig {
    pub fn new(binary: impl Into<PathBuf>, work_root: impl Into<PathBuf>) -> Self {
        LspConfig {
            binary: binary.into(),
            work_root: work_root.into(),
            slots: Arc::new(Semaphore::new(3)),
        }
    }
}

/// Settings sent as initializationOptions and returned for workspace/configuration. With `clippy`, the check that
/// runs on save is clippy (its lints show in the editor as you work) instead of `cargo check`.
fn settings(clippy: bool) -> Value {
    json!({
        "cargo": { "buildScripts": { "enable": false }, "targetDir": true },
        "procMacro": { "enable": false },
        "checkOnSave": true,
        "check": { "command": if clippy { "clippy" } else { "check" }, "allTargets": true },
        "cachePriming": { "enable": false },
        "inlayHints": {
            "typeHints": { "enable": true },
            "parameterHints": { "enable": false },
            "chainingHints": { "enable": false },
            "closingBraceHints": { "enable": false }
        },
        "lens": { "enable": false }
    })
}

pub struct SessionFiles {
    pub lib_rs: String,
    pub visible_tests: String,
    /// The problem's crates; rust-analyzer resolves them from crates.io on the host.
    pub crates: Vec<String>,
    /// The problem's `[perf]`, so the prelude rust-analyzer sees has the same helpers.
    pub perf: anneal_runner::Perf,
    /// Check with clippy rather than `cargo check` (the editor's "Live clippy" setting).
    pub clippy: bool,
}

/// Runs one editor session until either side goes away.
pub async fn session(
    mut socket: WebSocket,
    cfg: LspConfig,
    files: SessionFiles,
) -> anyhow::Result<()> {
    let parent = cfg.work_root.join("lsp");
    std::fs::create_dir_all(&parent)?;
    let dir = tempfile::Builder::new()
        .prefix("lsp-")
        .tempdir_in(&parent)?;
    // rust-analyzer reports canonical paths (/private/var on macOS), so rewrite those.
    let root = dir.path().canonicalize()?;
    anneal_runner::write_project(
        &root,
        &anneal_runner::Submission {
            lib_rs: &files.lib_rs,
            visible_tests: &files.visible_tests,
            hidden_tests: None,
            crates: &files.crates,
            perf: files.perf,
        },
        false,
    )?;
    let real_root = format!("file://{}", root.display());

    let mut child = Command::new(&cfg.binary)
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");

    // rust-analyzer → bridge: messages for the browser, or replies we owe the server.
    let (tx, mut rx) = mpsc::channel::<FromServer>(64);
    let reader_root = real_root.clone();
    let clippy = files.clippy;
    tokio::spawn(async move {
        let mut out = BufReader::new(stdout);
        while let Ok(Some(text)) = read_frame(&mut out).await {
            if tx.send(from_server(&text, &reader_root, clippy)).await.is_err() {
                break;
            }
        }
    });

    let mut ping = tokio::time::interval(PING_EVERY);
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if heard.elapsed() > SILENT_FOR
                    || socket.send(Message::Ping(Default::default())).await.is_err()
                {
                    break;
                }
            },
            received = socket.recv() => {
                heard = Instant::now();
                match received {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(frame) = from_client(&text, &root, &real_root, files.clippy) {
                            write_frame(&mut stdin, &frame).await?;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {}
                }
            }
            out = rx.recv() => match out {
                Some(FromServer::Forward(text)) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Some(FromServer::Reply(text)) => write_frame(&mut stdin, &text).await?,
                None => break,
            },
        }
    }
    let _ = child.kill().await;
    Ok(())
}

enum FromServer {
    Forward(String),
    Reply(String),
}

/// Rewrites a browser message for rust-analyzer, or handles it here.
fn from_client(text: &str, root: &Path, real_root: &str, clippy: bool) -> Option<String> {
    let mut v: Value = serde_json::from_str(text).ok()?;
    match v["method"].as_str() {
        Some("anneal/save") => {
            let code = v["params"]["text"].as_str()?;
            std::fs::write(root.join("src/lib.rs"), code).ok()?;
            let saved = json!({
                "jsonrpc": "2.0",
                "method": "textDocument/didSave",
                "params": { "textDocument": { "uri": format!("{real_root}/src/lib.rs") } }
            });
            return Some(saved.to_string());
        }
        Some("initialize") => {
            v["params"]["initializationOptions"] = settings(clippy);
            v["params"]["rootUri"] = json!(VIRTUAL_ROOT);
            v["params"]["workspaceFolders"] = json!([{ "uri": VIRTUAL_ROOT, "name": "solution" }]);
        }
        _ => {}
    }
    Some(v.to_string().replace(VIRTUAL_ROOT, real_root))
}

/// Answers server-to-client requests; forwards everything else with paths hidden.
fn from_server(text: &str, real_root: &str, clippy: bool) -> FromServer {
    if let Ok(v) = serde_json::from_str::<Value>(text)
        && let (Some(id), Some(method)) = (v.get("id"), v["method"].as_str())
    {
        let result = match method {
            "workspace/configuration" => {
                let n = v["params"]["items"].as_array().map_or(1, Vec::len);
                Value::Array(vec![settings(clippy); n])
            }
            _ => Value::Null,
        };
        return FromServer::Reply(
            json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string(),
        );
    }
    FromServer::Forward(text.replace(real_root, VIRTUAL_ROOT))
}

async fn read_frame<R: AsyncBufReadExt + AsyncReadExt + Unpin>(
    r: &mut R,
) -> std::io::Result<Option<String>> {
    let mut len = None;
    loop {
        let mut line = String::new();
        if r.read_line(&mut line).await? == 0 {
            return Ok(None);
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            len = v.trim().parse::<usize>().ok();
        }
    }
    let len = len.ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "frame without Content-Length",
        )
    })?;
    let mut body = vec![0; len];
    r.read_exact(&mut body).await?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

async fn write_frame(w: &mut ChildStdin, body: &str) -> std::io::Result<()> {
    w.write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .await?;
    w.write_all(body.as_bytes()).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_gets_settings_and_the_real_root() {
        let msg = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "rootUri": "file:///workspace", "capabilities": {} } });
        let out: Value = serde_json::from_str(
            &from_client(&msg.to_string(), Path::new("/tmp/x"), "file:///tmp/x", true).unwrap(),
        )
        .unwrap();
        assert_eq!(out["params"]["rootUri"], "file:///tmp/x");
        assert_eq!(out["params"]["initializationOptions"]["checkOnSave"], true);
    }

    #[test]
    fn server_requests_are_answered_not_forwarded() {
        let req = json!({ "jsonrpc": "2.0", "id": 7, "method": "workspace/configuration", "params": { "items": [{}, {}] } });
        match from_server(&req.to_string(), "file:///tmp/x", true) {
            FromServer::Reply(r) => {
                let r: Value = serde_json::from_str(&r).unwrap();
                assert_eq!(r["id"], 7);
                assert_eq!(r["result"].as_array().unwrap().len(), 2);
            }
            FromServer::Forward(_) => panic!("should be answered"),
        }
        let note = json!({ "jsonrpc": "2.0", "method": "textDocument/publishDiagnostics", "params": { "uri": "file:///tmp/x/src/lib.rs" } });
        match from_server(&note.to_string(), "file:///tmp/x", true) {
            FromServer::Forward(t) => assert!(t.contains("file:///workspace/src/lib.rs")),
            FromServer::Reply(_) => panic!("should be forwarded"),
        }
    }
}
