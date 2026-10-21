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
/// profile opens reads across namespaces (project-isolation S3).
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

// ---------------------------------------------------------------------------
// Tool presence (S6)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tools_list_contains_all_9_memory_tools_and_no_ping()
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

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
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
