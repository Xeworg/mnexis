//! Real-client MCP interoperability suite (T20; e2-core §10).
//!
//! Drives the actual `mnexis mcp serve` binary with rmcp 3.3.0's official
//! client over the child-process stdio transport. A green run proves the
//! full session — initialize, initialized, tools/list, tools/call and
//! shutdown — works across a real process boundary.

use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ContentBlock},
    transport::{ConfigureCommandExt, TokioChildProcess},
};

/// Bootstrap a temporary profile directory by running `mnexis init`.
fn bootstrap_profile(data_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let output = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("init")
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "init failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// A profile whose reads stay scoped to its own namespaces: the default
/// profile scopes the reads too (closed-default), and this one says so
/// explicitly (project-isolation S3).
fn bootstrap_scoped_profile(data_dir: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    bootstrap_profile(data_dir)?;
    let path = data_dir.join("mnexis.toml");
    let src = std::fs::read_to_string(&path)?;
    std::fs::write(
        &path,
        src.replace("allow_read_all = true", "allow_read_all = false"),
    )?;
    Ok(())
}

/// A profile whose reads stay inside `read` while writes reach `write`, so a
/// test can hold data the read scope hides (read-scope-disclosure S4).
fn bootstrap_profile_with_scope(
    data_dir: &std::path::Path,
    read: &str,
    write: &str,
    allow_read_all: bool,
    allow_global: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    bootstrap_profile(data_dir)?;
    std::fs::write(
        data_dir.join("mnexis.toml"),
        format!(
            "default_namespace = \"{read}\"\nread_namespaces = [\"{read}\"]\nwrite_namespaces = [\"{write}\"]\nallow_read_all = {allow_read_all}\nallow_global = {allow_global}\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n"
        ),
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tool presence (S6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tools_list_contains_all_the_memory_tools_and_no_ping()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let tools = client.list_all_tools().await?;
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    for tool in [
        "memory_save",
        "memory_get",
        "memory_list",
        "memory_update",
        "memory_history",
        "memory_supersede",
        "memory_delete",
        "memory_restore",
        "memory_search",
        "memory_workspace",
        "memory_session_start",
        "memory_session_end",
        "memory_save_prompt",
        "memory_search_prompts",
    ] {
        assert!(
            names.contains(&tool),
            "tools/list must contain '{tool}', got: {names:?}"
        );
    }

    // ping is retired (spike scaffolding gone)
    assert!(
        !names.contains(&"ping"),
        "ping must not appear in the tool list, got: {names:?}"
    );

    // S3: the schema an agent reads must document the topic_key charset, so
    // a guessed key like "snake/design" never needs a failing save to learn it.
    let save_tool = tools
        .iter()
        .find(|t| t.name.as_ref() == "memory_save")
        .expect("memory_save present in the tool list");
    let description = save_tool
        .input_schema
        .get("properties")
        .and_then(|props| props.get("topic_key"))
        .and_then(|prop| prop.get("description"))
        .and_then(|description| description.as_str())
        .unwrap_or_else(|| {
            panic!(
                "memory_save schema must describe topic_key: {:?}",
                save_tool.input_schema
            )
        });
    assert!(
        description.contains("lowercase letters, digits, '.', '-' and '/'"),
        "memory_save topic_key schema must document the charset, got: {description:?}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_workspace: where may I write, and what is already there?
// (agent-memory-ergonomics S4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_workspace_reports_the_namespace_the_allowlists_and_the_recent_memories()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Two memories, so the newest-first order is observable.
    for content in [
        "la primera memoria del proyecto",
        "la segunda memoria del proyecto",
    ] {
        let result = client
            .call_tool(
                CallToolRequestParams::new("memory_save").with_arguments(
                    serde_json::json!({ "content": content })
                        .as_object()
                        .expect("static object")
                        .clone(),
                ),
            )
            .await?;
        assert!(
            !result.is_error.unwrap_or(false),
            "memory_save must not report an error: {result:?}"
        );
    }

    let result = client
        .call_tool(CallToolRequestParams::new("memory_workspace"))
        .await?;
    assert!(
        !result.is_error.unwrap_or(false),
        "memory_workspace must not report an error: {result:?}"
    );
    let text = result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("at least one text block");
    let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON");

    // The scope an agent needs to know before it writes.
    assert!(
        parsed["scope"]["default_namespace"].is_string(),
        "the payload must name the default namespace: {parsed}"
    );
    assert!(
        parsed["scope"]["read_namespaces"].is_array(),
        "the payload must carry the read allowlist: {parsed}"
    );
    assert!(
        parsed["scope"]["write_namespaces"].is_array(),
        "the payload must carry the write allowlist: {parsed}"
    );
    assert!(
        parsed["workspace"]["key"].is_string(),
        "the payload must name the workspace key: {parsed}"
    );
    assert!(
        parsed["memory_count"].as_i64().unwrap_or(0) >= 2,
        "the payload must count the memories: {parsed}"
    );

    // The most recent memory comes first.
    let recent = parsed["recent"]
        .as_array()
        .expect("the payload must carry the recent list");
    assert!(
        !recent.is_empty(),
        "the payload must carry the recent memories: {parsed}"
    );
    assert_eq!(
        recent[0]["content"].as_str(),
        Some("la segunda memoria del proyecto"),
        "the newest memory must come first: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_save round-trip (S6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_save_returns_uuid_and_row_id() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({
                    "content": "RustaceanCon 2025 confirmed for March"
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;

    assert!(
        !result.is_error.unwrap_or(false),
        "memory_save must not report an error: {result:?}"
    );

    // Parse uuid and row_id from the JSON text content.
    let text = result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("at least one text block");
    let parsed: serde_json::Value = serde_json::from_str(text).expect("valid JSON");
    assert!(
        parsed.get("uuid").is_some(),
        "save result must have 'uuid': {parsed}"
    );
    assert!(
        parsed.get("row_id").is_some(),
        "save result must have 'row_id': {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_get (S6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_get_returns_full_memory_object() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Save first.
    let save_result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({
                    "content": "SQLite WAL mode enabled by default"
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;
    assert!(!save_result.is_error.unwrap_or(false));
    let save_text = save_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text block");
    let save_parsed: serde_json::Value = serde_json::from_str(save_text).expect("JSON");
    let uuid = save_parsed["uuid"].as_str().expect("uuid string");

    // Get it back.
    let get_result = client
        .call_tool(
            CallToolRequestParams::new("memory_get").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(!get_result.is_error.unwrap_or(false));
    let get_text = get_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text block");
    let get_parsed: serde_json::Value = serde_json::from_str(get_text).expect("JSON");

    // Content matches what we saved.
    assert_eq!(
        get_parsed["content"].as_str().expect("content"),
        "SQLite WAL mode enabled by default"
    );
    assert_eq!(get_parsed["uuid"].as_str().expect("uuid"), uuid);
    assert_eq!(get_parsed["revision"].as_i64().expect("revision"), 1);

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_search on a distinctive term (S6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_search_finds_saved_memory_by_content() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Use a unique term derived from timestamp to avoid collisions.
    let unique_term = format!(
        "t20_interop_search_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );

    // Save with a unique term.
    client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({
                    "content": format!("MCP interop test using term {}", unique_term)
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;

    // Search for the unique term.
    let search_result = client
        .call_tool(
            CallToolRequestParams::new("memory_search").with_arguments(
                serde_json::json!({ "query": unique_term })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(!search_result.is_error.unwrap_or(false));
    let search_text = search_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text block");
    let search_parsed: serde_json::Value = serde_json::from_str(search_text).expect("JSON");

    assert_eq!(
        search_parsed["count"].as_i64().expect("count"),
        1,
        "search should find 1 result for unique term: {search_parsed}"
    );
    assert!(
        search_parsed["items"]
            .as_array()
            .expect("items array")
            .len()
            == 1,
        "should have exactly one item: {search_parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_update with wrong expected_revision → REVISION_CONFLICT (S6, S8)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_update_wrong_revision_returns_error() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Save.
    let save_result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({ "content": "original content" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let save_text = save_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let save_parsed: serde_json::Value = serde_json::from_str(save_text).expect("JSON");
    let uuid = save_parsed["uuid"].as_str().expect("uuid");

    // Update with WRONG expected_revision (the service starts at rev 1; we pass 99).
    // e2-core 8: the domain error travels as a JSON-RPC server error,
    // code -32050 with data.error_code; rmcp surfaces it as
    // ServiceError::McpError whose Display carries code+message+data.
    let update_err = client
        .call_tool(
            CallToolRequestParams::new("memory_update").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": 99,
                    "reason": "test conflict",
                    "content": "updated content"
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await
        .expect_err("wrong expected_revision must fail the tool call");
    let err_text = format!("{update_err}");
    assert!(
        err_text.contains("-32050"),
        "code must reach the client: {err_text}"
    );
    assert!(
        err_text.contains("revision conflict"),
        "message must reach the client: {err_text}"
    );
    assert!(
        err_text.contains("REVISION_CONFLICT"),
        "data.error_code must reach the client: {err_text}"
    );

    // Verify the original content is intact by getting the memory.
    let get_result = client
        .call_tool(
            CallToolRequestParams::new("memory_get").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let get_text = get_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let get_parsed: serde_json::Value = serde_json::from_str(get_text).expect("JSON");
    assert_eq!(
        get_parsed["content"].as_str().expect("content"),
        "original content",
        "content must be unchanged after conflict"
    );
    assert_eq!(
        get_parsed["revision"].as_i64().expect("revision"),
        1,
        "revision must still be 1 after conflict"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_update with correct revision → revision 2 (S6, S8)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_update_correct_revision_increments() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Save.
    let save_result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({ "content": "v1 content" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let save_text = save_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let save_parsed: serde_json::Value = serde_json::from_str(save_text).expect("JSON");
    let uuid = save_parsed["uuid"].as_str().expect("uuid");

    // Update with correct expected_revision = 1.
    let update_result = client
        .call_tool(
            CallToolRequestParams::new("memory_update").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": 1,
                    "reason": "test increment",
                    "content": "v2 content"
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;
    assert!(!update_result.is_error.unwrap_or(false));
    let update_text = update_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let update_parsed: serde_json::Value = serde_json::from_str(update_text).expect("JSON");
    assert_eq!(
        update_parsed["revision"].as_i64().expect("revision"),
        2,
        "revision must be 2 after first update: {update_parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_history after update → 1 entry (S6, S9)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_history_returns_update_revisions() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Save.
    let save_result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({ "content": "original" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let save_text = save_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let save_parsed = serde_json::from_str::<serde_json::Value>(save_text).expect("JSON");
    let uuid = save_parsed["uuid"].as_str().expect("uuid");

    // Update → rev 2.
    client
        .call_tool(
            CallToolRequestParams::new("memory_update").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": 1,
                    "reason": "update rev 2",
                    "content": "updated"
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;

    // History.
    let hist_result = client
        .call_tool(
            CallToolRequestParams::new("memory_history").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(!hist_result.is_error.unwrap_or(false));
    let hist_text = hist_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let hist_parsed: serde_json::Value = serde_json::from_str(hist_text).expect("JSON");
    assert_eq!(
        hist_parsed["count"].as_i64().expect("count"),
        1,
        "history has 1 entry after one update: {hist_parsed}"
    );
    let items = hist_parsed["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "history items: {items:?}");
    // History records the post-update state of the memory at revision 2.
    assert_eq!(
        items[0]["content"].as_str().expect("content"),
        "updated",
        "history entry must record post-update content: {items:?}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_delete then memory_get → NOT_FOUND (S6, S12)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_delete_then_get_returns_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Save.
    let save_result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({ "content": "to be deleted" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let save_text = save_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let save_parsed: serde_json::Value = serde_json::from_str(save_text).expect("JSON");
    let uuid = save_parsed["uuid"].as_str().expect("uuid");

    // Get to find the current revision.
    let get_result = client
        .call_tool(
            CallToolRequestParams::new("memory_get").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let get_text = get_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let revision: i32 =
        serde_json::from_str::<serde_json::Value>(get_text).expect("JSON")["revision"]
            .as_i64()
            .expect("i64") as i32;

    // Delete with the correct expected_revision.
    let delete_result = client
        .call_tool(
            CallToolRequestParams::new("memory_delete").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": revision
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;
    assert!(!delete_result.is_error.unwrap_or(false));

    // Get after delete must return NOT_FOUND as a JSON-RPC -32050 error
    // (e2-core 8). The service's default disclosure does not echo the uuid.
    let get_after = client
        .call_tool(
            CallToolRequestParams::new("memory_get").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect_err("memory_get after delete must fail with NOT_FOUND");
    let err_text = format!("{get_after}");
    assert!(
        err_text.contains("-32050"),
        "code must reach the client: {err_text}"
    );
    assert!(
        err_text.contains("not found"),
        "NOT_FOUND must reach the client: {err_text}"
    );
    assert!(
        err_text.contains("NOT_FOUND"),
        "data.error_code must reach the client: {err_text}"
    );
    assert!(
        !err_text.contains(uuid),
        "the denial must not echo the uuid: {err_text}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_restore → memory_search finds it again (S6, S12)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_restore_restores_and_search_finds_it() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let unique_term = format!(
        "t20_restore_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );

    // Save with unique term.
    let save_result = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({
                    "content": format!("memory_restore test with {}", unique_term)
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;
    let save_text = save_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let save_parsed = serde_json::from_str::<serde_json::Value>(save_text).expect("JSON");
    let uuid = save_parsed["uuid"].as_str().expect("uuid");

    // Get revision for delete.
    let get_result = client
        .call_tool(
            CallToolRequestParams::new("memory_get").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let revision: i32 = get_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .expect("parsed JSON")["revision"]
        .as_i64()
        .expect("i64") as i32;

    // Delete.
    client
        .call_tool(
            CallToolRequestParams::new("memory_delete").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": revision
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;

    // Verify search does NOT find it.
    let search_before = client
        .call_tool(
            CallToolRequestParams::new("memory_search").with_arguments(
                serde_json::json!({ "query": unique_term })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let search_before_text = search_before
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let search_before_parsed: serde_json::Value =
        serde_json::from_str(search_before_text).expect("JSON");
    assert_eq!(
        search_before_parsed["count"].as_i64().expect("count"),
        0,
        "search must return 0 after delete: {search_before_parsed}"
    );

    // Restore.
    let restore_result = client
        .call_tool(
            CallToolRequestParams::new("memory_restore").with_arguments(
                serde_json::json!({ "uuid": uuid })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(
        !restore_result.is_error.unwrap_or(false),
        "restore must succeed: {restore_result:?}"
    );

    // Search must find it again.
    let search_after = client
        .call_tool(
            CallToolRequestParams::new("memory_search").with_arguments(
                serde_json::json!({ "query": unique_term })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    let search_after_text = search_after
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let search_after_parsed: serde_json::Value =
        serde_json::from_str(search_after_text).expect("JSON");
    assert_eq!(
        search_after_parsed["count"].as_i64().expect("count"),
        1,
        "search must return 1 after restore: {search_after_parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// S10: out-of-scope UUID returns NOT_FOUND without leaking content
// ---------------------------------------------------------------------------

#[tokio::test]
async fn get_unknown_uuid_returns_not_found_without_uuid_leak()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Try to get a UUID that does not exist: JSON-RPC -32050 NOT_FOUND
    // (e2-core 8); the denial does not echo the unknown uuid.
    let err = client
        .call_tool(
            CallToolRequestParams::new("memory_get").with_arguments(
                serde_json::json!({ "uuid": "00000000-0000-0000-0000-000000000000" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect_err("unknown uuid must fail with NOT_FOUND");
    let err_text = format!("{err}");
    assert!(
        err_text.contains("-32050"),
        "code must reach the client: {err_text}"
    );
    assert!(
        err_text.contains("not found"),
        "unknown uuid must return NOT_FOUND: {err_text}"
    );
    assert!(
        err_text.contains("NOT_FOUND"),
        "data.error_code must reach the client: {err_text}"
    );
    assert!(
        !err_text.contains("00000000-0000-0000-0000-000000000000"),
        "NOT_FOUND must not echo back the unknown uuid"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// S16: list respects namespace scope; out-of-scope namespace denied
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_respects_scope_and_denies_out_of_scope_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_scoped_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Save a memory in the default namespace.
    client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({
                    "content": "visible in default namespace"
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await?;

    // List with default namespace must return it.
    let list_result = client
        .call_tool(
            CallToolRequestParams::new("memory_list").with_arguments(
                serde_json::json!({ "namespace": "default" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(!list_result.is_error.unwrap_or(false));
    let list_text = list_result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let list_parsed: serde_json::Value = serde_json::from_str(list_text).expect("JSON");
    assert!(
        list_parsed["count"].as_i64().expect("count") >= 1,
        "list must return >= 1 memory in default namespace: {list_parsed}"
    );

    // Listing an out-of-scope namespace: JSON-RPC -32050 NOT_FOUND via the
    // profile-configured default disclosure (deny without content leak).
    let list_other = client
        .call_tool(
            CallToolRequestParams::new("memory_list").with_arguments(
                serde_json::json!({ "namespace": "definitely-not-in-scope-xyz" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect_err("out-of-scope list must deny");
    let err_text = format!("{list_other}");
    assert!(
        err_text.contains("-32050"),
        "code must reach the client: {err_text}"
    );
    assert!(
        err_text.contains("not found"),
        "list on out-of-scope namespace must deny: {err_text}"
    );
    assert!(
        err_text.contains("NOT_FOUND"),
        "data.error_code must reach the client: {err_text}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// S16: reads are open across namespaces when the profile allows them
// ---------------------------------------------------------------------------

#[tokio::test]
async fn list_reaches_another_namespace_when_reads_are_open()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;
    // The default profile scopes the reads (closed-default), so this test
    // opens them the way an operator would: with the explicit switch.
    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let opened = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("config")
        .arg("allow-read-all")
        .output()?;
    assert!(
        opened.status.success(),
        "allow-read-all failed: {}",
        String::from_utf8_lossy(&opened.stderr)
    );

    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let list_other = client
        .call_tool(
            CallToolRequestParams::new("memory_list").with_arguments(
                serde_json::json!({ "namespace": "otro-proyecto" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect("reads open across namespaces must not deny");
    assert!(!list_other.is_error.unwrap_or(false));
    let text = list_other
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("text");
    let parsed: serde_json::Value = serde_json::from_str(text).expect("JSON");
    assert_eq!(parsed["count"], 0, "no memory lives in that namespace yet");

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// S5: the server refuses writes that leave the workspace
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_server_refuses_a_write_into_another_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    // The workspace names its namespace with a marker, and the human
    // authorized that namespace for writes.
    let workspace = tempfile::tempdir_in("target")?;
    std::fs::write(workspace.path().join(".mnexis-namespace"), "ns-marcado\n")?;
    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("config")
        .arg("allow-namespace")
        .arg("ns-marcado")
        .output()?;

    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir")
                .arg(data_dir)
                .arg("mcp")
                .arg("serve")
                .current_dir(workspace.path());
        }))?;
    let client = ().serve(transport).await?;

    let denied = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({ "content": "into another project", "namespace": "otro-proyecto" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect_err("a write outside the workspace must be refused");
    let text = format!("{denied}");
    assert!(
        text.contains("FORBIDDEN"),
        "the error must be FORBIDDEN: {text}"
    );
    assert!(
        text.contains("ns-marcado"),
        "the message names the allowed namespace: {text}"
    );

    // The workspace's own namespace is accepted.
    let allowed = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({ "content": "in its own project", "namespace": "ns-marcado" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(!allowed.is_error.unwrap_or(false));

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Graceful shutdown (S6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancel_gracefully_shuts_down_the_server() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // Any tool call proves the server is alive.
    let result = client
        .call_tool(
            CallToolRequestParams::new("memory_list").with_arguments(
                serde_json::json!({})
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(!result.is_error.unwrap_or(false));

    // cancel() waits for the real server process to exit.
    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_suggest_topic_key and the relations (topic-continuity)
// ---------------------------------------------------------------------------

async fn call_tool_text(
    client: &rmcp::service::RunningService<rmcp::service::RoleClient, ()>,
    tool: &str,
    args: serde_json::Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let result = client
        .call_tool(
            CallToolRequestParams::new(tool.to_string())
                .with_arguments(args.as_object().expect("object").clone()),
        )
        .await?;
    let text = result
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .unwrap_or_default();
    Ok(text)
}

#[tokio::test]
async fn memory_suggest_topic_key_answers_from_the_title() -> Result<(), Box<dyn std::error::Error>>
{
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // The same title always gives the same key, in any language.
    let text = call_tool_text(
        &client,
        "memory_suggest_topic_key",
        serde_json::json!({ "title": "Puerto del Servicio" }),
    )
    .await?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(json["topic_key"], "puerto-del-servicio", "{json}");
    assert_eq!(
        json["recorded_by"],
        serde_json::Value::Null,
        "the key is free: {json}"
    );

    // A title in another language yields a usable key too.
    let text = call_tool_text(
        &client,
        "memory_suggest_topic_key",
        serde_json::json!({ "title": "サービスのポート" }),
    )
    .await?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(json["topic_key"], "サービスのポート", "{json}");

    // Once the fact is recorded, the key says who holds it.
    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "El servicio escucha en el puerto 8081",
            "title": "puerto del servicio",
        }),
    )
    .await?;
    let text = call_tool_text(
        &client,
        "memory_suggest_topic_key",
        serde_json::json!({ "title": "puerto del servicio" }),
    )
    .await?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    assert!(
        json["recorded_by"].is_string(),
        "the key names the record that holds it: {json}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn the_list_carries_the_relations_between_the_memories()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let older = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "the port was 8081", "topic_key": "svc/port" }),
    )
    .await?;
    let older_json: serde_json::Value = serde_json::from_str(&older)?;
    let older_uuid = older_json["uuid"].as_str().expect("uuid").to_string();

    let newer = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "the port is 9090" }),
    )
    .await?;
    let newer_json: serde_json::Value = serde_json::from_str(&newer)?;
    let newer_uuid = newer_json["uuid"].as_str().expect("uuid").to_string();

    call_tool_text(
        &client,
        "memory_supersede",
        serde_json::json!({
            "older_uuid": older_uuid,
            "newer_uuid": newer_uuid,
            "expected_old_revision": 1,
            "expected_new_revision": 1,
            "reason": "the port changed"
        }),
    )
    .await?;

    let listed = call_tool_text(&client, "memory_list", serde_json::json!({})).await?;
    let json: serde_json::Value = serde_json::from_str(&listed)?;
    let relations = json["relations"].as_array().expect("relations array");
    assert_eq!(relations.len(), 1, "one relation is reported: {json}");
    assert_eq!(relations[0]["older"], older_uuid.as_str());
    assert_eq!(relations[0]["newer"], newer_uuid.as_str());

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// S1: the title travels with what the agent reads (identity-visibility T3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn memory_search_carries_the_title_of_each_hit() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "El servicio escucha en el puerto 8081",
            "title": "puerto del servicio"
        }),
    )
    .await?;

    let text = call_tool_text(
        &client,
        "memory_search",
        serde_json::json!({ "query": "puerto" }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed["count"], 1, "search: {parsed}");
    assert_eq!(
        parsed["items"][0]["title"], "puerto del servicio",
        "a hit says what it is: {parsed}"
    );
    assert!(
        parsed["items"][0]["uuid"].is_string(),
        "and keeps the identity it carried: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn memory_list_carries_the_title_and_keeps_the_preview()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "listable content", "title": "contenido listable" }),
    )
    .await?;
    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "sin titulo" }),
    )
    .await?;

    let text = call_tool_text(&client, "memory_list", serde_json::json!({})).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    let items = parsed["items"].as_array().expect("items array");

    let titled = items
        .iter()
        .find(|item| item["preview"] == "listable content")
        .unwrap_or_else(|| panic!("the titled memory is listed: {parsed}"));
    assert_eq!(
        titled["title"], "contenido listable",
        "the identity travels with the preview: {parsed}"
    );
    assert!(
        titled.get("content").is_none(),
        "the compact item stays compact: {parsed}"
    );

    let untitled = items
        .iter()
        .find(|item| item["preview"] == "sin titulo")
        .unwrap_or_else(|| panic!("the untitled memory is listed: {parsed}"));
    assert_eq!(
        untitled["title"], "",
        "no title is the empty string, never null: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn memory_workspace_carries_the_title_of_the_recent_memories()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "El servicio escucha en el puerto 8081",
            "title": "puerto del servicio"
        }),
    )
    .await?;

    let text = call_tool_text(&client, "memory_workspace", serde_json::json!({})).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    let recent = parsed["recent"]
        .as_array()
        .expect("the payload must carry the recent list");
    assert_eq!(
        recent[0]["title"], "puerto del servicio",
        "the recent list names what each memory is: {parsed}"
    );
    assert_eq!(
        recent[0]["content"], "El servicio escucha en el puerto 8081",
        "additive: the content stays: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// S1, S4: the words of a fact (agent-keywords T3)
// ---------------------------------------------------------------------------
//
// The agent supplies the vocabulary of the fact through the tool, the words
// accumulate on the record across sessions, `replace_keywords` replaces
// them, and every read payload carries them back — the empty list when the
// memory has none, never `null`.

#[tokio::test]
async fn memory_save_answers_with_the_accumulated_words() -> Result<(), Box<dyn std::error::Error>>
{
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let first = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "listen on port 8081",
            "title": "listen port",
            "keywords": "puerto, port"
        }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&first)?;
    assert_eq!(
        parsed["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the save's answer carries the words: {parsed}"
    );

    // A new session does not remember the old words, so a save adds its own.
    let second = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "listen on port 9090",
            "title": "listen port",
            "keywords": "listen port"
        }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&second)?;
    assert_eq!(
        parsed["keywords"],
        serde_json::json!(["listen port", "puerto", "port"]),
        "the words accumulate, the requested ones first: {parsed}"
    );

    // A word that is wrong is replaced instead of accumulated.
    let replaced = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "listen on port 7070",
            "title": "listen port",
            "keywords": "puerto",
            "replace_keywords": true
        }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&replaced)?;
    assert_eq!(
        parsed["keywords"],
        serde_json::json!(["puerto"]),
        "replace_keywords replaces the accumulated words: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn a_save_without_words_answers_with_the_empty_list() -> Result<(), Box<dyn std::error::Error>>
{
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let text = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "sin palabras" }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        parsed["keywords"],
        serde_json::json!([]),
        "no words is the empty list, never null: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn the_read_payloads_carry_the_words() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "listen on port 8081",
            "title": "listen port",
            "keywords": "puerto, port"
        }),
    )
    .await?;
    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "sin palabras" }),
    )
    .await?;

    // Search: the hit carries the words of the record it points at.
    let text = call_tool_text(
        &client,
        "memory_search",
        serde_json::json!({ "query": "listen" }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed["count"], 1, "search: {parsed}");
    assert_eq!(
        parsed["items"][0]["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the hit carries the words: {parsed}"
    );

    // List: the compact item carries them beside the preview, and the
    // memory with no words says so with the empty list.
    let text = call_tool_text(&client, "memory_list", serde_json::json!({})).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    let items = parsed["items"].as_array().expect("items array");
    let worded = items
        .iter()
        .find(|item| item["preview"] == "listen on port 8081")
        .unwrap_or_else(|| panic!("the worded memory is listed: {parsed}"));
    assert_eq!(
        worded["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the words travel with the preview: {parsed}"
    );
    let wordless = items
        .iter()
        .find(|item| item["preview"] == "sin palabras")
        .unwrap_or_else(|| panic!("the wordless memory is listed: {parsed}"));
    assert_eq!(
        wordless["keywords"],
        serde_json::json!([]),
        "no words is the empty list, never null: {parsed}"
    );

    // Workspace: the first payload a session reads carries them too.
    let text = call_tool_text(&client, "memory_workspace", serde_json::json!({})).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    let recent = parsed["recent"]
        .as_array()
        .expect("the payload must carry the recent list");
    let worded = recent
        .iter()
        .find(|item| item["content"] == "listen on port 8081")
        .unwrap_or_else(|| panic!("the worded memory is recent: {parsed}"));
    assert_eq!(
        worded["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the recent list carries the words: {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// The scope a read was bounded by (read-scope-disclosure S4)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_mcp_read_tools_declare_the_scope_that_bounded_them()
-> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    for (allow_read_all, allow_global, expected) in [
        (false, false, "restricted"),
        (true, false, "except_global"),
        (true, true, "all"),
    ] {
        let tmp = tempfile::tempdir_in("target")?;
        let data_dir = tmp.path();
        bootstrap_profile_with_scope(data_dir, "ns-a", "ns-a", allow_read_all, allow_global)?;
        let transport =
            TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
                cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
            }))?;
        let client = ().serve(transport).await?;

        call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({ "content": "memoria legible", "namespace": "ns-a" }),
        )
        .await?;

        let text = call_tool_text(
            &client,
            "memory_search",
            serde_json::json!({ "query": "memoria" }),
        )
        .await?;
        let parsed: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(
            parsed["read_scope"], expected,
            "memory_search ({expected}): {parsed}"
        );
        // Additive: the answer keeps the fields it had.
        assert_eq!(parsed["count"], 1, "memory_search: {parsed}");
        assert!(parsed["items"].is_array(), "memory_search: {parsed}");
        assert!(parsed["relations"].is_array(), "memory_search: {parsed}");

        let text = call_tool_text(&client, "memory_list", serde_json::json!({})).await?;
        let parsed: serde_json::Value = serde_json::from_str(&text)?;
        assert_eq!(
            parsed["read_scope"], expected,
            "memory_list ({expected}): {parsed}"
        );
        assert_eq!(parsed["count"], 1, "memory_list: {parsed}");

        client.cancel().await?;
    }
    Ok(())
}

#[tokio::test]
async fn the_mcp_read_scope_names_no_namespace_and_counts_nothing()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    // `ns-b` holds a memory this profile may not read: the declaration must
    // not announce it, not even by counting it.
    bootstrap_profile_with_scope(data_dir, "ns-a", "ns-b", false, false)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "memoria escondida", "namespace": "ns-b" }),
    )
    .await?;

    let text = call_tool_text(
        &client,
        "memory_search",
        serde_json::json!({ "query": "memoria" }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        parsed["count"], 0,
        "the scoped read returns fewer rows: {parsed}"
    );
    let scope = parsed["read_scope"].as_str().expect("read_scope is a word");
    assert_eq!(scope, "restricted", "{parsed}");
    assert!(
        !scope.contains("ns-a") && !scope.contains("ns-b"),
        "the mode names no namespace: {scope}"
    );
    assert!(
        !scope.chars().any(|c| c.is_ascii_digit()),
        "the mode carries no count: {scope}"
    );

    let text = call_tool_text(&client, "memory_list", serde_json::json!({})).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed["count"], 0, "{parsed}");
    assert_eq!(parsed["read_scope"], "restricted", "{parsed}");

    client.cancel().await?;
    Ok(())
}
