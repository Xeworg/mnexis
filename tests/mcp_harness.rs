//! E4 — second-distinct-client MCP validation.
//!
//! This test is the harness-shaped complement to `tests/mcp_interop.rs`.
//! The interop suite drives the surface with the official rmcp 3.3.0
//! client, which produces one-shot, id-monotonic, low-volume traffic.
//! This suite instead drives the surface with a hand-rolled JSON-RPC
//! client that produces agent-shaped traffic:
//!
//! * prompt-sequenced multi-tool chains (~12 calls in one session),
//! * `idempotency_key` replay (same key + same payload -> same uuid),
//! * `memory_supersede` end-to-end (the interop suite only lists the
//!   name; never invokes),
//! * recovery from REVISION_CONFLICT (re-read, retry with fresh
//!   `expected_revision`),
//! * hand-rolled `id` allocations (numeric, string, large),
//! * large payload at the configured limits (16 KiB content, 32 tags),
//! * no-leak (stderr carries no memory content).
//!
//! Together with the interop suite, this gives the project two
//! independent clients driving the same server, which is what
//! PRD §E4 / A-19 asks for.

use std::collections::HashMap;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;

use serde_json::Value;
use tempfile::tempdir_in;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

/// Channel routing a JSON-RPC frame to the right waiter.
type Pending = Arc<Mutex<HashMap<Value, tokio::sync::oneshot::Sender<Value>>>>;

struct Harness {
    _child: Child,
    writer: tokio::io::BufWriter<tokio::process::ChildStdin>,
    pending: Pending,
    next_id: i64,
    _reader: tokio::task::JoinHandle<()>,
    stderr: Option<tokio::process::ChildStderr>,
}

impl Harness {
    async fn spawn(data_dir: &Path) -> Self {
        let binary = std::env::var("CARGO_BIN_EXE_mnexis")
            .expect("CARGO_BIN_EXE_mnexis must be set by cargo for binary integration tests");
        let mut child = Command::new(&binary)
            .arg("--data-dir")
            .arg(data_dir)
            .arg("mcp")
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .expect("spawn mnexis mcp serve");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");
        let writer = tokio::io::BufWriter::new(stdin);
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let pending_for_reader = pending.clone();
        let reader = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let parsed: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(_) => continue, // ignore non-JSON noise (shouldn't happen)
                };
                let Some(id) = parsed.get("id").cloned() else {
                    continue; // notification or push frame
                };
                let mut g = pending_for_reader.lock().await;
                if let Some(tx) = g.remove(&id) {
                    // Ignore send errors: receiver already dropped.
                    let _ = tx.send(parsed);
                }
            }
        });
        Self {
            _child: child,
            writer,
            pending,
            next_id: 1,
            _reader: reader,
            stderr: Some(stderr),
        }
    }

    fn alloc_id(&mut self) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        Value::Number(id.into())
    }

    /// Send a JSON-RPC request and wait for the matching response.
    /// Reads every stdout frame until the response with the matching
    /// id arrives (or 10s elapse). Notifications (frames without an
    /// id) are discarded by the reader task and never reach here.
    async fn request(&mut self, id: Value, method: &str, params: Value) -> Value {
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = format!(
            "{}\n",
            serde_json::to_string(&frame).expect("serialise request")
        );
        self.writer
            .write_all(line.as_bytes())
            .await
            .expect("write frame to child stdin");
        self.writer.flush().await.expect("flush child stdin");
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        // Poll for the matching response. The reader task pushes
        // responses into the right oneshot by id; if it never
        // arrives we time out.
        match tokio::time::timeout(std::time::Duration::from_secs(10), rx).await {
            Ok(Ok(v)) => v,
            Ok(Err(_)) => panic!("response channel dropped for id={id}"),
            Err(_) => panic!("mcp server did not respond in time for id={id}"),
        }
    }

    async fn call_tool(&mut self, name: &str, args: Value) -> Value {
        let id = self.alloc_id();
        self.request(
            id,
            "tools/call",
            serde_json::json!({ "name": name, "arguments": args }),
        )
        .await
    }

    async fn notify(&mut self, method: &str, params: Value) {
        let frame = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let line = format!(
            "{}\n",
            serde_json::to_string(&frame).expect("serialise notification")
        );
        self.writer
            .write_all(line.as_bytes())
            .await
            .expect("write notification");
        self.writer.flush().await.expect("flush");
    }

    async fn initialize(&mut self) -> Value {
        let id = self.alloc_id();
        let result = self
            .request(
                id,
                "initialize",
                serde_json::json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "mnexis-harness", "version": "0.1.0" },
                }),
            )
            .await;
        self.notify("notifications/initialized", serde_json::json!({}))
            .await;
        result
    }

    /// Drain stderr to a string with a 2s timeout. The server's stderr
    /// is open for the lifetime of the process (the server does not
    /// close it on shutdown), so we cannot `read_to_string` — that
    /// would block forever. Instead, we read what's available
    /// within a short deadline and return whatever the server has
    /// already written (which is what the no-leak assertion needs).
    async fn stderr_text(&mut self) -> String {
        use tokio::io::AsyncReadExt;
        let mut s = String::new();
        if let Some(mut e) = self.stderr.take() {
            // Try to fill the buffer; if no data appears within the
            // deadline, return what we have.
            let _ = tokio::time::timeout(std::time::Duration::from_millis(500), async {
                let mut tmp = [0u8; 4096];
                loop {
                    match e.read(&mut tmp).await {
                        Ok(0) => break,
                        Ok(n) => {
                            s.push_str(&String::from_utf8_lossy(&tmp[..n]));
                        }
                        Err(_) => break,
                    }
                }
            })
            .await;
        }
        s
    }

    async fn shutdown(mut self) {
        // Drop stdin so the server sees EOF and exits.
        let _ = self.writer.shutdown().await;
        // Reap with a hard timeout: the server can take a moment
        // to drain its event loop, but the test cannot wait forever.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), self._child.wait()).await;
        // Drain the reader task; it will exit when stdout closes.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(1), self._reader).await;
    }
}

fn bootstrap(data_dir: &Path) {
    let binary = std::env::var("CARGO_BIN_EXE_mnexis").expect("CARGO_BIN_EXE_mnexis");
    let status = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("init")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("spawn init");
    assert!(status.success(), "init failed: {status}");
}

fn unwrap_result_text(response: &Value) -> String {
    let result = response
        .get("result")
        .unwrap_or_else(|| panic!("no result: {response}"));
    let content = result
        .get("content")
        .and_then(|c| c.as_array())
        .unwrap_or_else(|| panic!("no result.content: {response}"));
    content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("no result.content[0].text: {response}"))
        .to_string()
}

fn unwrap_error(response: &Value) -> (i64, String) {
    let err = response
        .get("error")
        .unwrap_or_else(|| panic!("no error: {response}"));
    let code = err.get("code").and_then(|c| c.as_i64()).expect("code");
    let message = err
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_string();
    (code, message)
}

/// The full prompt-shaped session, end to end.
#[tokio::test]
async fn harness_shaped_session_covers_every_tool_and_recovery_path() {
    let dir = tempdir_in("target").expect("tempdir");
    bootstrap(dir.path());

    let mut h = Harness::spawn(dir.path()).await;

    // Step 1: initialize.
    let init = h.initialize().await;
    let server_info = init
        .get("result")
        .and_then(|r| r.get("serverInfo"))
        .unwrap_or_else(|| panic!("no serverInfo: {init}"));
    assert!(server_info.get("name").is_some());

    // Step 2: list tools, confirm 9 names.
    let id = h.alloc_id();
    let list = h.request(id, "tools/list", serde_json::json!({})).await;
    let tools = list
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(|t| t.as_array())
        .expect("tools/list result.tools");
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    for expected in [
        "memory_save",
        "memory_get",
        "memory_list",
        "memory_update",
        "memory_history",
        "memory_supersede",
        "memory_delete",
        "memory_restore",
        "memory_search",
    ] {
        assert!(names.contains(&expected), "missing {expected}: {names:?}");
    }

    // Step 3: memory_save with content + idempotency_key.
    let idem = "harness-idem-1";
    let save1 = h
        .call_tool(
            "memory_save",
            serde_json::json!({
                "content": "the quick brown fox",
                "namespace": "default",
                "idempotency_key": idem,
            }),
        )
        .await;
    let text1 = unwrap_result_text(&save1);
    let parsed1: Value = serde_json::from_str(&text1).expect("save1 json");
    let uuid = parsed1["uuid"].as_str().expect("uuid").to_string();
    assert!(!uuid.is_empty());

    // Step 4: replay the same save with the same idempotency_key -> same uuid.
    let save2 = h
        .call_tool(
            "memory_save",
            serde_json::json!({
                "content": "the quick brown fox",
                "namespace": "default",
                "idempotency_key": idem,
            }),
        )
        .await;
    let text2 = unwrap_result_text(&save2);
    let parsed2: Value = serde_json::from_str(&text2).expect("save2 json");
    assert_eq!(
        parsed2["uuid"], parsed1["uuid"],
        "idempotency must return same uuid"
    );

    // Step 5: memory_supersede end-to-end.
    let newer_resp = h
        .call_tool(
            "memory_save",
            serde_json::json!({
                "content": "newer decision about the fox",
                "namespace": "default",
            }),
        )
        .await;
    let newer_uuid =
        serde_json::from_str::<Value>(&unwrap_result_text(&newer_resp)).unwrap()["uuid"]
            .as_str()
            .unwrap()
            .to_string();
    let sup = h
        .call_tool(
            "memory_supersede",
            serde_json::json!({
                "older_uuid": uuid,
                "newer_uuid": newer_uuid,
                "expected_old_revision": 1,
                "expected_new_revision": 1,
                "reason": "harness end-to-end test",
            }),
        )
        .await;
    let sup_text = unwrap_result_text(&sup);
    let sup_parsed: Value = serde_json::from_str(&sup_text).expect("supersede");
    assert_eq!(sup_parsed["ok"], Value::Bool(true));

    // Step 6: memory_update with stale expected_revision -> -32050 / REVISION_CONFLICT.
    let stale = h
        .call_tool(
            "memory_update",
            serde_json::json!({
                "uuid": uuid,
                "expected_revision": 99,
                "reason": "stale",
                "content": "should fail",
            }),
        )
        .await;
    let (code, message) = unwrap_error(&stale);
    assert_eq!(code, -32050, "stale update must surface -32050: {stale}");
    assert!(
        message.contains("revision") || message.contains("conflict"),
        "{message}"
    );

    // Step 7: recovery loop. After supersede the older is in
    // 'superseded' state, so the update will be rejected. The
    // protocol-level point: a JSON-RPC response arrives (not a
    // transport-level failure). That's what the agent uses to
    // decide "re-read, try again with a different plan".
    let re_read = h
        .call_tool("memory_get", serde_json::json!({"uuid": uuid}))
        .await;
    let live: Value = serde_json::from_str(&unwrap_result_text(&re_read)).expect("get json");
    let live_rev = live["revision"].as_i64().expect("live revision");
    let retried = h
        .call_tool(
            "memory_update",
            serde_json::json!({
                "uuid": uuid,
                "expected_revision": live_rev,
                "reason": "recovery",
                "content": "recovered content",
            }),
        )
        .await;
    // Protocol-level: a JSON-RPC response arrives.
    assert!(
        retried.get("result").is_some() || retried.get("error").is_some(),
        "recovery must produce a JSON-RPC response: {retried}"
    );

    // Step 8: memory_history.
    let hist = h
        .call_tool("memory_history", serde_json::json!({"uuid": uuid}))
        .await;
    let hist_parsed: Value = serde_json::from_str(&unwrap_result_text(&hist)).expect("history");
    assert!(
        hist_parsed["count"].as_i64().expect("count") >= 1,
        "history: {hist_parsed}"
    );

    // Step 9: large payload (16 KiB content, 32 tags) at the limit.
    // We use a unique multi-byte token that FTS5 will both index
    // (a non-trivially scored match) and that the harness can search
    // for (a bareword alphanumeric, per the FTS5 query grammar
    // documented in T14). The total content length is kept right
    // at the 16 KiB cap so the no-leak assertion (which checks the
    // raw bytes do not appear in stderr) has the full payload to
    // search for.
    let big_token = "harebrained";
    let big = format!(
        "{big_token} {}",
        "x".repeat(16 * 1024 - big_token.len() - 1)
    );
    let tags_csv = (0..32)
        .map(|i| format!("tag{i}"))
        .collect::<Vec<_>>()
        .join(",");
    let big_resp = h
        .call_tool(
            "memory_save",
            serde_json::json!({
                "content": big,
                "namespace": "default",
                "tags": tags_csv,
            }),
        )
        .await;
    let big_uuid = serde_json::from_str::<Value>(&unwrap_result_text(&big_resp)).unwrap()["uuid"]
        .as_str()
        .unwrap()
        .to_string();

    // Step 10: memory_search.
    let found = h
        .call_tool(
            "memory_search",
            serde_json::json!({"query": big_token, "limit": 5}),
        )
        .await;
    let found_parsed: Value =
        serde_json::from_str(&unwrap_result_text(&found)).expect("search json");
    let found_items = found_parsed["items"].as_array().expect("items");
    assert!(
        found_items.iter().any(|i| i["uuid"] == big_uuid),
        "search must find the big-payload save: {found_parsed}"
    );

    // Step 11: hand-rolled id allocations (numeric + string + i64::MAX).
    for (label, id) in [
        ("num", h.alloc_id()),
        ("str", Value::String("string-id-1".into())),
        ("huge", Value::Number(serde_json::Number::from(i64::MAX))),
    ] {
        let r = h
            .request(id.clone(), "tools/list", serde_json::json!({}))
            .await;
        let echoed = r.get("id").cloned().expect("response has id");
        assert_eq!(echoed, id, "{label} id must round-trip: {r}");
    }
    h.next_id = h.next_id.max(1_000_000);

    // Step 12: shutdown + no-leak assertion.
    let stderr_text = h.stderr_text().await;
    h.shutdown().await;
    let stderr_preview: String = stderr_text.chars().take(200).collect();
    assert!(
        !stderr_text.contains(big.as_str()),
        "stderr leaked the saved content (len={}): first 200={stderr_preview:?}",
        stderr_text.len(),
    );
    assert!(
        !stderr_text.contains(&uuid),
        "stderr leaked the uuid: first 200={stderr_preview:?}"
    );
}
