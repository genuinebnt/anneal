//! Drives real rust-analyzer through the WebSocket bridge. Needs `rust-analyzer` on PATH
//! (it ships with rustup) and the compose Postgres.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anneal_api::auth::AuthConfig;
use anneal_api::lsp::LspConfig;
use anneal_api::{AppState, app};
use anneal_content::Catalog;
use anneal_runner::{Runner, RunnerConfig, Sandbox};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio_tungstenite::tungstenite::Message;

const LIB: &str = "file:///workspace/src/lib.rs";

#[sqlx::test(migrator = "anneal_api::MIGRATOR")]
async fn rust_analyzer_hovers_and_reports_check_errors(db: PgPool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../content/fixtures/runnable");
    let catalog = Catalog::load(&root).unwrap().catalog;
    let work = Path::new(env!("CARGO_TARGET_TMPDIR")).join("anneal-lsp");
    let state = AppState {
        catalog: Arc::new(catalog),
        runner: Arc::new(Runner::new(RunnerConfig::new(Sandbox::Host, &work))),
        db,
        lsp: LspConfig::new("rust-analyzer", &work),
        auth: AuthConfig::disabled(),
        courses: Arc::new(Vec::new()),
        meaning: anneal_api::meaning::Meaning::off(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app(state, None)).await.unwrap() });

    let (mut ws, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/api/lsp/d9-network-delay-time"))
            .await
            .unwrap();
    let solution = std::fs::read_to_string(
        root.join("tracks/d9-graphs/problems/network-delay-time/solution.rs"),
    )
    .unwrap();

    let send = |v: Value| Message::Text(v.to_string().into());
    ws.send(send(json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "processId": null, "rootUri": "file:///workspace", "capabilities": {} } }))).await.unwrap();

    // Collects messages until `pred` matches one, or gives up.
    async fn until(
        ws: &mut (impl StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin),
        secs: u64,
        pred: impl Fn(&Value) -> bool,
    ) -> Option<Value> {
        tokio::time::timeout(Duration::from_secs(secs), async {
            while let Some(Ok(msg)) = ws.next().await {
                if let Message::Text(t) = msg {
                    let v: Value = serde_json::from_str(&t).unwrap();
                    assert!(
                        !t.contains("/lsp-"),
                        "a real path leaked to the browser: {t}"
                    );
                    if pred(&v) {
                        return Some(v);
                    }
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    let init = until(&mut ws, 30, |v| v["id"] == 1)
        .await
        .expect("initialize response");
    assert!(
        init["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap_or(true)
    );
    ws.send(send(
        json!({ "jsonrpc": "2.0", "method": "initialized", "params": {} }),
    ))
    .await
    .unwrap();
    ws.send(send(json!({ "jsonrpc": "2.0", "method": "textDocument/didOpen", "params": { "textDocument": { "uri": LIB, "languageId": "rust", "version": 1, "text": solution } } }))).await.unwrap();

    // Hover on `BinaryHeap` in `let mut heap = BinaryHeap::new();` once indexing has caught up.
    let (line, col) = solution
        .lines()
        .enumerate()
        .find_map(|(i, l)| l.find("BinaryHeap::new").map(|c| (i, c + 2)))
        .unwrap();
    let mut hovered = None;
    for id in 100..160 {
        ws.send(send(json!({ "jsonrpc": "2.0", "id": id, "method": "textDocument/hover", "params": { "textDocument": { "uri": LIB }, "position": { "line": line, "character": col } } }))).await.unwrap();
        let resp = until(&mut ws, 10, |v| v["id"] == id)
            .await
            .expect("hover response");
        if resp["result"]["contents"]
            .to_string()
            .contains("BinaryHeap")
        {
            hovered = Some(resp);
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(hovered.is_some(), "rust-analyzer never answered the hover");

    // Saving triggers cargo check; its errors come back as diagnostics for the virtual path.
    let broken = solution.replace("dist[k] = 0;", "dist[k] = \"zero\";");
    ws.send(send(
        json!({ "jsonrpc": "2.0", "method": "anneal/save", "params": { "text": broken } }),
    ))
    .await
    .unwrap();
    let diags = until(&mut ws, 90, |v| {
        v["method"] == "textDocument/publishDiagnostics"
            && v["params"]["uri"] == LIB
            && v["params"]["diagnostics"]
                .as_array()
                .is_some_and(|d| d.iter().any(|d| d["code"] == "E0308"))
    })
    .await;
    assert!(diags.is_some(), "no E0308 from cargo check");
}
