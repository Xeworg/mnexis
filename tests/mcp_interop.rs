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
        "memory_recall",
        "memory_context",
    ] {
        assert!(
            names.contains(&tool),
            "tools/list must contain '{tool}', got: {names:?}"
        );
    }

    // The door's schema names the arguments the agent has to send, so the
    // tool is callable without guessing.
    let context_tool = tools
        .iter()
        .find(|t| t.name.as_ref() == "memory_context")
        .expect("memory_context present in the tool list");
    let arguments = context_tool
        .input_schema
        .get("properties")
        .and_then(|props| props.as_object())
        .unwrap_or_else(|| {
            panic!(
                "memory_context must declare its arguments: {:?}",
                context_tool.input_schema
            )
        });
    for argument in ["task", "namespace", "token_budget", "max_items", "match"] {
        assert!(
            arguments.contains_key(argument),
            "memory_context must declare '{argument}', got: {:?}",
            arguments.keys().collect::<Vec<_>>()
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

// The workspace answer is priced: the recent list carries the identity and
// a preview, never the whole record. The tool description is how the model
// learns that, so it has to say it — the CLI text and the pi extension say
// it too, and the three cannot drift (#53).
#[tokio::test]
async fn the_workspace_tool_description_declares_the_pricing()
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
    let workspace = tools
        .iter()
        .find(|t| t.name.as_ref() == "memory_workspace")
        .expect("memory_workspace present in the tool list");
    let description = workspace
        .description
        .as_ref()
        .map(|d| d.as_ref())
        .unwrap_or_default();
    assert!(
        description.contains("preview"),
        "the description must say the recent list is priced, got: {description:?}"
    );
    assert!(
        description.contains("memory_get"),
        "the description must name the door to the full record, got: {description:?}"
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
    // The recent list is priced (#53): identity and a preview, never the
    // whole record — for a short content the preview is the full text.
    assert!(
        recent[0].get("content").is_none(),
        "the recent list must not paste the full record: {parsed}"
    );
    assert_eq!(
        recent[0]["preview"].as_str(),
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
// memory_update carries the kind of the fact (types B1 G1)
// ---------------------------------------------------------------------------

/// A client that reclassifies a record says so in the update itself: the tool
/// carries the kind to the service, the stored record becomes it, and the
/// answer names the kind the record now has. Before this, `memory_type` was
/// not one of the tool's arguments, so the revision bumped and the kind never
/// changed.
#[tokio::test]
async fn memory_update_carries_the_type_the_client_asks_for()
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

    // The tool's schema is a claim about the arguments: the kind is one of
    // them, and its doc names the kinds the caller can ask for.
    let tools = client.list_all_tools().await?;
    let schema = tools
        .iter()
        .find(|tool| tool.name.as_ref() == "memory_update")
        .expect("memory_update present in the tool list")
        .input_schema
        .clone();
    let kind_doc = schema
        .get("properties")
        .and_then(|properties| properties.get("memory_type"))
        .and_then(|prop| prop.get("description"))
        .and_then(|doc| doc.as_str())
        .unwrap_or_else(|| panic!("the schema must name memory_type: {schema:?}"))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        kind_doc.contains("decision"),
        "the arg doc must name the kinds the caller can ask for: {kind_doc:?}"
    );

    // A record saved without a kind is the generic one.
    let saved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({ "content": "el puerto del servicio es 8081" }),
        )
        .await?,
    )?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();
    let before: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(before["memory_type"], "fact", "{before}");

    // The update names the kind: the record becomes the decision it says.
    let updated: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_update",
            serde_json::json!({
                "uuid": uuid,
                "expected_revision": 1,
                "reason": "reclassify the port as a decision",
                "memory_type": "decision",
            }),
        )
        .await?,
    )?;
    assert_eq!(updated["revision"], 2, "{updated}");
    assert_eq!(
        updated["memory_type"], "decision",
        "the answer must carry the kind the record now has: {updated}"
    );
    assert_eq!(
        updated["state"], "active",
        "an update keeps the state it was not asked to change: {updated}"
    );

    // The stored record changed, not only the answer.
    let after: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(after["memory_type"], "decision", "{after}");
    assert_eq!(
        after["content"], "el puerto del servicio es 8081",
        "a type change touches nothing else: {after}"
    );

    client.cancel().await?;
    Ok(())
}

/// The anti-demotion guarantee: an update that says nothing about the kind
/// keeps the one the record has. Reading "no type" as "the default type"
/// would demote a decision on every content edit.
#[tokio::test]
async fn memory_update_without_a_type_keeps_the_stored_one()
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

    let saved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({
                "content": "el puerto del servicio es 8081",
                "memory_type": "decision",
            }),
        )
        .await?,
    )?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();

    // The update says nothing about the kind: only the content changes.
    let updated: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_update",
            serde_json::json!({
                "uuid": uuid,
                "expected_revision": 1,
                "reason": "the port moved to 8082",
                "content": "el puerto del servicio es 8082",
            }),
        )
        .await?,
    )?;
    assert_eq!(updated["revision"], 2, "{updated}");
    assert_eq!(
        updated["memory_type"], "decision",
        "an update that names no kind keeps the one the record has: {updated}"
    );

    let after: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(after["memory_type"], "decision", "{after}");
    assert_eq!(
        after["content"], "el puerto del servicio es 8082",
        "{after}"
    );
    assert_eq!(after["revision"], 2, "{after}");

    client.cancel().await?;
    Ok(())
}

/// An argument the tool does not have is an error, not a silent no-op: a
/// typo'd field used to bump the revision and change nothing, and the caller
/// was never told.
#[tokio::test]
async fn memory_update_refuses_an_unknown_argument_instead_of_ignoring_it()
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

    let saved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({ "content": "el puerto del servicio es 8081" }),
        )
        .await?,
    )?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();

    // `memory_typo` is not an argument of the tool. rmcp answers a parameter
    // it cannot deserialize as an error result (is_error), so the caller
    // sees the refusal in the answer instead of a bumped revision.
    let refused = client
        .call_tool(
            CallToolRequestParams::new("memory_update").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": 1,
                    "reason": "reclassify with a typo'd field",
                    "memory_typo": "decision",
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await
        .expect("the refusal is an answer, not a transport failure");
    assert_eq!(
        refused.is_error,
        Some(true),
        "an unknown argument must be an error result, not a silent no-op: {refused:?}"
    );
    let message = refused
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("the refusal carries a message");
    assert!(
        message.contains("unknown field"),
        "the refusal must name the problem: {message}"
    );
    assert!(
        message.contains("memory_typo"),
        "the refusal must name the argument it did not recognise: {message}"
    );

    // Nothing was written: the revision did not move and the kind is intact.
    let intact: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(intact["revision"], 1, "{intact}");
    assert_eq!(intact["memory_type"], "fact", "{intact}");

    // The same update, spelled right, still works.
    let updated: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_update",
            serde_json::json!({
                "uuid": uuid,
                "expected_revision": 1,
                "reason": "reclassify as a decision",
                "memory_type": "decision",
            }),
        )
        .await?,
    )?;
    assert_eq!(updated["revision"], 2, "{updated}");
    assert_eq!(updated["memory_type"], "decision", "{updated}");

    client.cancel().await?;
    Ok(())
}

/// `memory_save` refuses an argument it does not have for the same reason
/// `memory_update` does: `type` is not the name of the kind (`memory_type`
/// is), so a save that sends it used to be read as a save that said nothing
/// about the kind and answered with a bumped revision — a silent no-op on
/// the tool an agent uses most (audit round 2, A11).
#[tokio::test]
async fn memory_save_refuses_an_unknown_argument_instead_of_ignoring_it()
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

    // The decision the caller means to re-save, under the topic key the
    // re-save will name.
    let saved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({
                "content": "el puerto del servicio es 8081",
                "memory_type": "decision",
                "topic_key": "puerto.servicio",
            }),
        )
        .await?,
    )?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();

    // `type` is not an argument of the tool. rmcp answers a parameter it
    // cannot deserialize as an error result (is_error), and the store is
    // never reached.
    let refused = client
        .call_tool(
            CallToolRequestParams::new("memory_save").with_arguments(
                serde_json::json!({
                    "content": "el puerto del servicio es 8082",
                    "topic_key": "puerto.servicio",
                    "type": "decision",
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await
        .expect("the refusal is an answer, not a transport failure");
    assert_eq!(
        refused.is_error,
        Some(true),
        "an unknown argument must be an error result, not a silent no-op: {refused:?}"
    );
    let message = refused
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("the refusal carries a message");
    assert!(
        message.contains("unknown field"),
        "the refusal must name the problem: {message}"
    );
    assert!(
        message.contains("type"),
        "the refusal must name the argument it did not recognise: {message}"
    );

    // Nothing was written: the record is still the decision it was, at the
    // revision it had, with the content it had.
    let intact: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(intact["revision"], 1, "{intact}");
    assert_eq!(intact["memory_type"], "decision", "{intact}");
    assert_eq!(
        intact["content"], "el puerto del servicio es 8081",
        "{intact}"
    );

    // The same save, spelled right, still works and still carries the kind.
    let resaved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({
                "content": "el puerto del servicio es 8082",
                "memory_type": "decision",
                "topic_key": "puerto.servicio",
            }),
        )
        .await?,
    )?;
    assert_eq!(
        resaved["uuid"], intact["uuid"],
        "the re-save lands on the same record: {resaved}"
    );
    let after: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(after["revision"], 2, "{after}");
    assert_eq!(after["memory_type"], "decision", "{after}");
    assert_eq!(
        after["content"], "el puerto del servicio es 8082",
        "{after}"
    );

    client.cancel().await?;
    Ok(())
}

/// The set of kinds is closed: a value outside it is refused with the stable
/// domain label, and the record keeps the kind it had.
#[tokio::test]
async fn memory_update_refuses_a_kind_outside_the_closed_set()
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

    let saved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({ "content": "el puerto del servicio es 8081" }),
        )
        .await?,
    )?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();

    let error = client
        .call_tool(
            CallToolRequestParams::new("memory_update").with_arguments(
                serde_json::json!({
                    "uuid": uuid,
                    "expected_revision": 1,
                    "reason": "reclassify as something nobody can filter",
                    "memory_type": "meeting",
                })
                .as_object()
                .expect("static object")
                .clone(),
            ),
        )
        .await
        .expect_err("a kind outside the closed set must fail the tool call");
    let message = format!("{error}");
    for expected in [
        "-32050",
        "INVALID_ARGUMENT",
        "unknown memory type 'meeting'",
    ] {
        assert!(
            message.contains(expected),
            "the refusal must carry {expected}: {message}"
        );
    }

    // The record is untouched: same revision, same kind.
    let intact: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(intact["revision"], 1, "{intact}");
    assert_eq!(intact["memory_type"], "fact", "{intact}");

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_history after update → 1 entry, holding the replaced revision (S6, S9)
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
    // The trail records the state the update replaced: revision 1, with the
    // content the record had before the change.
    assert_eq!(
        items[0]["revision"].as_i64().expect("revision"),
        1,
        "history entry must record the replaced revision: {items:?}"
    );
    assert_eq!(
        items[0]["content"].as_str().expect("content"),
        "original",
        "history entry must record the pre-update content: {items:?}"
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
async fn memory_suggest_topic_key_asks_in_the_workspace_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    // The profile's default namespace is not the one the workspace writes
    // into: only the workspace's own namespace holds the record below, and
    // only the binding opens it for reading.
    std::fs::write(
        data_dir.join("mnexis.toml"),
        "default_namespace = \"default\"\nread_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\", \"mnexis\"]\nallow_read_all = false\n\
         allow_global = false\nexistence_disclosure = \"not_found\"\n\
         allowed_operations = [\"all\"]\n",
    )?;

    // A workspace bound to `mnexis`, exactly like a real project.
    let workspace = tempfile::tempdir_in("target")?;
    let git = |args: &[&str]| -> Result<(), Box<dyn std::error::Error>> {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(workspace.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        assert!(status.success(), "git {args:?} failed");
        Ok(())
    };
    git(&["init", "-q", "."])?;
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/Xeworg/mnexis.git",
    ])?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let bound = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "project",
            "bind",
            "mnexis",
            "--key",
            "git:github.com/Xeworg/mnexis",
        ])
        .output()?;
    assert!(
        bound.status.success(),
        "bind failed: {}",
        String::from_utf8_lossy(&bound.stderr)
    );

    // One titled record, in the namespace the workspace writes into.
    let saved = std::process::Command::new(&binary)
        .current_dir(workspace.path())
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "--json",
            "save",
            "--namespace",
            "mnexis",
            "--title",
            "puerto del servicio",
            "El servicio escucha en el puerto 8081",
        ])
        .output()?;
    assert!(
        saved.status.success(),
        "save failed: {}",
        String::from_utf8_lossy(&saved.stderr)
    );
    let saved_json: serde_json::Value = serde_json::from_slice(&saved.stdout)?;
    let uuid = saved_json["uuid"].as_str().expect("uuid").to_string();

    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir")
                .arg(data_dir)
                .arg("mcp")
                .arg("serve")
                .current_dir(workspace.path());
        }))?;
    let client = ().serve(transport).await?;

    // Without a namespace the tool asks about the workspace's own namespace —
    // the one a save lands in — and the answer names it. Before this rule it
    // answered about the profile's default namespace, silently, and reported
    // the key as free.
    let text = call_tool_text(
        &client,
        "memory_suggest_topic_key",
        serde_json::json!({ "title": "puerto del servicio" }),
    )
    .await?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        json["namespace"], "mnexis",
        "the answer names the namespace it asked about: {json}"
    );
    assert_eq!(json["topic_key"], "puerto-del-servicio", "{json}");
    assert_eq!(
        json["recorded_by"], uuid,
        "the record of the workspace's namespace holds the key: {json}"
    );

    // A key nobody holds is free in that same namespace, and says so.
    let text = call_tool_text(
        &client,
        "memory_suggest_topic_key",
        serde_json::json!({ "title": "otro asunto" }),
    )
    .await?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(json["namespace"], "mnexis", "{json}");
    assert_eq!(json["recorded_by"], serde_json::Value::Null, "{json}");

    // A namespace the caller names is still the one asked about.
    let text = call_tool_text(
        &client,
        "memory_suggest_topic_key",
        serde_json::json!({ "title": "puerto del servicio", "namespace": "default" }),
    )
    .await?;
    let json: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(json["namespace"], "default", "{json}");
    assert_eq!(json["recorded_by"], serde_json::Value::Null, "{json}");

    // The description and the arg doc say which namespace the tool asks
    // about: the workspace's, not the profile's default one (S6).
    let tools = client.list_all_tools().await?;
    let tool = tools
        .iter()
        .find(|t| t.name.as_ref() == "memory_suggest_topic_key")
        .expect("memory_suggest_topic_key present in the tool list");
    let description = tool.description.as_deref().unwrap_or_default().to_string();
    assert!(
        description.contains("this workspace writes into"),
        "the description must say which namespace is asked about: {description:?}"
    );
    let namespace_doc = tool
        .input_schema
        .get("properties")
        .and_then(|props| props.get("namespace"))
        .and_then(|prop| prop.get("description"))
        .and_then(|doc| doc.as_str())
        .unwrap_or_else(|| {
            panic!(
                "the schema must describe namespace: {:?}",
                tool.input_schema
            )
        });
    // The schema keeps the doc's line breaks: compare the words.
    let namespace_doc = namespace_doc
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        namespace_doc.contains("the namespace this workspace writes into"),
        "the arg doc must say which namespace is asked about: {namespace_doc:?}"
    );
    assert!(
        !namespace_doc.contains("Namespace (default: the profile's default namespace)"),
        "the arg doc must not claim the profile's default namespace: {namespace_doc:?}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn memory_suggest_topic_key_refuses_a_namespace_the_profile_cannot_read()
-> Result<(), Box<dyn std::error::Error>> {
    // A profile whose `default_namespace` is NOT one of its readable
    // namespaces. The suggestion falls back to the scope's default when the
    // workspace names none, and that fallback is a read: the scope must
    // refuse it, exactly as it refuses every other read. The tool must not
    // answer about a namespace the profile may not read — not even to say
    // the key is free.
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;
    std::fs::write(
        data_dir.join("mnexis.toml"),
        "default_namespace = \"secreto\"\nread_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\"]\nallow_read_all = false\nallow_global = false\n\
         existence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    )?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    let result = client
        .call_tool(
            CallToolRequestParams::new("memory_suggest_topic_key".to_string()).with_arguments(
                serde_json::json!({ "title": "puerto del servidor" })
                    .as_object()
                    .expect("object")
                    .clone(),
            ),
        )
        .await;
    let error = match result {
        Ok(ok) => panic!("the tool must refuse an unreadable namespace: {ok:?}"),
        Err(error) => error,
    };
    let message = error.to_string();
    assert!(
        message.contains("not found: namespace:secreto"),
        "the refusal names the namespace and the route: {message}"
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
        recent[0]["preview"], "El servicio escucha en el puerto 8081",
        "the recent list is priced: a short content travels whole as the preview, and the full record is one memory_get away (#53): {parsed}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn memory_workspace_prices_the_recent_memories() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // A content longer than the preview cut, so the pricing is observable:
    // the first answer must carry the identity and the preview, never the
    // whole record (#53).
    let long = format!(
        "la memoria larga empieza acá {} y sigue un buen rato más",
        "palabra ".repeat(40)
    );
    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": long,
            "title": "la memoria larga del proyecto"
        }),
    )
    .await?;

    let text = call_tool_text(&client, "memory_workspace", serde_json::json!({})).await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    let recent = parsed["recent"]
        .as_array()
        .expect("the payload must carry the recent list");
    assert!(
        !recent.is_empty(),
        "the payload must carry the recent memories: {parsed}"
    );
    let entry = &recent[0];
    assert_eq!(
        entry["title"], "la memoria larga del proyecto",
        "the recent list names what each memory is: {parsed}"
    );
    assert!(
        entry["preview"].as_str().is_some_and(|p| p.ends_with('…')),
        "the recent list carries the preview, cut with an ellipsis when it was cut: {parsed}"
    );
    assert!(
        entry.get("content").is_none(),
        "the recent list is priced: the full record stays one mnexis_show away (#53): {parsed}"
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
        .find(|item| item["preview"] == "listen on port 8081")
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

// ---------------------------------------------------------------------------
// memory_recall: what a session was (session-recall S1)
// ---------------------------------------------------------------------------

/// The one sentence the answer uses for what a window can and cannot claim.
const WINDOW_LABEL: &str = "the memories of the namespace in the session's window";

#[tokio::test]
async fn memory_recall_answers_what_a_session_was() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // No finished session yet: NOT_FOUND, and the message says there is
    // none rather than answering with an empty session.
    let no_previous = client
        .call_tool(
            CallToolRequestParams::new("memory_recall").with_arguments(
                serde_json::json!({ "previous": true })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect_err("no finished session must be NOT_FOUND");
    let err_text = format!("{no_previous}");
    assert!(err_text.contains("-32050"), "{err_text}");
    assert!(err_text.contains("NOT_FOUND"), "{err_text}");
    assert!(
        err_text.contains("no finished session"),
        "the answer says there is none: {err_text}"
    );

    // One session, with what the user said in it and one memory written
    // while it was open.
    call_tool_text(
        &client,
        "memory_session_start",
        serde_json::json!({ "session_id": "01JX", "directory": "/work/project" }),
    )
    .await?;
    call_tool_text(
        &client,
        "memory_save_prompt",
        serde_json::json!({ "content": "el puerto es 8081", "session_id": "01JX" }),
    )
    .await?;
    let saved = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "el servicio escucha en 8081" }),
    )
    .await?;
    let saved: serde_json::Value = serde_json::from_str(&saved)?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();
    call_tool_text(
        &client,
        "memory_session_end",
        serde_json::json!({ "session_id": "01JX", "summary": "el puerto del servicio" }),
    )
    .await?;

    // `previous`: the session before this one.
    let text = call_tool_text(
        &client,
        "memory_recall",
        serde_json::json!({ "previous": true }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed["session"]["id"], "01JX", "{parsed}");
    assert_eq!(parsed["session"]["namespace"], "default", "{parsed}");
    assert_eq!(parsed["session"]["directory"], "/work/project", "{parsed}");
    assert_eq!(parsed["session"]["summary"], "el puerto del servicio");
    assert!(parsed["session"]["ended_at"].is_string(), "{parsed}");
    assert_eq!(
        parsed["prompts"][0]["content"], "el puerto es 8081",
        "{parsed}"
    );
    assert_eq!(
        parsed["window"]["label"], WINDOW_LABEL,
        "the answer says what the memories are: {parsed}"
    );
    assert_eq!(parsed["window"]["namespace"], "default", "{parsed}");
    assert_eq!(
        parsed["window"]["to"], parsed["session"]["ended_at"],
        "a finished session closes its window where it ended: {parsed}"
    );
    assert_eq!(parsed["window"]["open"], false, "{parsed}");
    assert_eq!(
        parsed["memories_in_window"][0]["uuid"],
        uuid.as_str(),
        "{parsed}"
    );

    // `session`: the same answer, asked by id.
    let text = call_tool_text(
        &client,
        "memory_recall",
        serde_json::json!({ "session": "01JX" }),
    )
    .await?;
    let by_id: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        by_id, parsed,
        "the two ways ask the same question and get the same answer"
    );

    // An unknown session is NOT_FOUND, not an empty answer.
    let unknown = client
        .call_tool(
            CallToolRequestParams::new("memory_recall").with_arguments(
                serde_json::json!({ "session": "NO-EXISTE" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect_err("an unknown session must be NOT_FOUND");
    let err_text = format!("{unknown}");
    assert!(err_text.contains("-32050"), "{err_text}");
    assert!(err_text.contains("NOT_FOUND"), "{err_text}");

    client.cancel().await?;
    Ok(())
}

/// `memory_recall` refuses an argument it does not have, exactly like the
/// other tools: `type` is not one of its arguments (recall narrows nothing
/// by kind), so a recall that sends it used to be read as a recall that
/// said nothing extra and answered the whole session — a silent no-op on
/// the last argument object still open (post-audit debts S3).
#[tokio::test]
async fn memory_recall_refuses_an_unknown_argument_instead_of_ignoring_it()
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

    // One finished session with one memory in its window, so a valid recall
    // has an answer to give.
    call_tool_text(
        &client,
        "memory_session_start",
        serde_json::json!({ "session_id": "01JY", "directory": "/work/project" }),
    )
    .await?;
    let saved: serde_json::Value = serde_json::from_str(
        &call_tool_text(
            &client,
            "memory_save",
            serde_json::json!({ "content": "el puerto del servicio es 8081" }),
        )
        .await?,
    )?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();
    call_tool_text(
        &client,
        "memory_session_end",
        serde_json::json!({ "session_id": "01JY", "summary": "el puerto del servicio" }),
    )
    .await?;

    // `type` is not an argument of the tool: rmcp answers a parameter it
    // cannot deserialize as an error result (is_error) and the recall never
    // runs, instead of dropping the field and answering as if it had not
    // been sent.
    let refused = client
        .call_tool(
            CallToolRequestParams::new("memory_recall").with_arguments(
                serde_json::json!({ "previous": true, "type": "decision" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await
        .expect("the refusal is an answer, not a transport failure");
    assert_eq!(
        refused.is_error,
        Some(true),
        "an unknown argument must be an error result, not a silent no-op: {refused:?}"
    );
    let message = refused
        .content
        .iter()
        .find_map(|b| match b {
            ContentBlock::Text(t) => Some(t.text.as_str()),
            _ => None,
        })
        .expect("the refusal carries a message");
    assert!(
        message.contains("unknown field"),
        "the refusal must name the problem: {message}"
    );
    assert!(
        message.contains("type"),
        "the refusal must name the argument it did not recognise: {message}"
    );

    // The refusal changed nothing: the record is still at its first revision,
    // with the content it was saved with.
    let intact: serde_json::Value = serde_json::from_str(
        &call_tool_text(&client, "memory_get", serde_json::json!({ "uuid": uuid })).await?,
    )?;
    assert_eq!(intact["revision"], 1, "{intact}");
    assert_eq!(
        intact["content"], "el puerto del servicio es 8081",
        "{intact}"
    );

    // The same recall, spelled right, still answers the session.
    let text = call_tool_text(
        &client,
        "memory_recall",
        serde_json::json!({ "previous": true }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(parsed["session"]["id"], "01JY", "{parsed}");
    assert_eq!(
        parsed["memories_in_window"][0]["uuid"],
        uuid.as_str(),
        "{parsed}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_session_start: the session lands in the workspace's namespace
// (memory-types A1)
// ---------------------------------------------------------------------------

/// A git repository bound to `namespace` in the profile's registry, and the
/// namespace's key: the workspace the MCP server is spawned in, so the
/// server's own resolution — the same rule the CLI applies to its writes —
/// finds the binding.
fn bind_workspace(
    data_dir: &std::path::Path,
    namespace: &str,
) -> Result<tempfile::TempDir, Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir_in("target")?;
    let remote = format!("https://github.com/Xeworg/{namespace}.git");
    let key = format!("git:github.com-xeworg-{namespace}");
    for args in [
        vec!["init", "-q", "."],
        vec!["remote", "add", "origin", remote.as_str()],
    ] {
        let output = std::process::Command::new("git")
            .args(&args)
            .current_dir(dir.path())
            .output()?;
        if !output.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
    }
    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let output = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args(["project", "bind", namespace, "--key", key.as_str()])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "project bind failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(dir)
}

/// The session answers in the namespace the workspace writes into, not in the
/// profile's default one, so the two axes of the store are the same axis.
#[tokio::test]
async fn memory_session_start_lands_in_the_workspace_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;
    // Two writable namespaces, and the default is deliberately NOT the
    // workspace's: a session that ignores the workspace lands in `default`.
    std::fs::write(
        data_dir.join("mnexis.toml"),
        "default_namespace = \"default\"\nread_namespaces = [\"default\", \"equipo\"]\nwrite_namespaces = [\"default\", \"equipo\"]\nallow_read_all = false\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    )?;
    let workspace = bind_workspace(data_dir, "equipo")?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir")
                .arg(data_dir)
                .arg("mcp")
                .arg("serve")
                .current_dir(workspace.path());
        }))?;
    let client = ().serve(transport).await?;

    let started = call_tool_text(
        &client,
        "memory_session_start",
        serde_json::json!({ "session_id": "01JX", "directory": "/work/project" }),
    )
    .await?;
    let started: serde_json::Value = serde_json::from_str(&started)?;
    assert_eq!(
        started["namespace"], "equipo",
        "the session must land in the namespace this workspace's memories land in: {started}"
    );

    let saved = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "el servicio escucha en 8081", "namespace": "equipo" }),
    )
    .await?;
    let saved: serde_json::Value = serde_json::from_str(&saved)?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();
    call_tool_text(
        &client,
        "memory_session_end",
        serde_json::json!({ "session_id": "01JX", "summary": "el puerto del servicio" }),
    )
    .await?;

    // The session is now where `--previous` looks for it, so the window it
    // opens holds what was saved during it.
    let text = call_tool_text(
        &client,
        "memory_recall",
        serde_json::json!({ "previous": true }),
    )
    .await?;
    let recall: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(recall["session"]["id"], "01JX", "{recall}");
    assert_eq!(recall["session"]["namespace"], "equipo", "{recall}");
    assert_eq!(
        recall["memories_in_window"][0]["uuid"],
        uuid.as_str(),
        "the window holds the memories saved during the session: {recall}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_save_prompt: the prompt lands in the workspace's namespace
// (memory-types A2)
// ---------------------------------------------------------------------------

/// A prompt with no namespace lands where this workspace's memories land, and
/// one the caller names keeps it: the same rule A1 gave the session, on the
/// surface a host without the capture hook uses.
#[tokio::test]
async fn memory_save_prompt_lands_in_the_workspace_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile(data_dir)?;
    // The default is deliberately NOT the workspace's: a prompt that ignores
    // the workspace lands in `default`, and the answer would show it.
    std::fs::write(
        data_dir.join("mnexis.toml"),
        "default_namespace = \"default\"\nread_namespaces = [\"default\", \"equipo\", \"otro\"]\nwrite_namespaces = [\"default\", \"equipo\", \"otro\"]\nallow_read_all = false\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    )?;
    let workspace = bind_workspace(data_dir, "equipo")?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir")
                .arg(data_dir)
                .arg("mcp")
                .arg("serve")
                .current_dir(workspace.path());
        }))?;
    let client = ().serve(transport).await?;

    // The schema is a claim about behaviour, and the old one — "the profile's
    // default namespace" — is how the gap survived unread.
    let tools = client.list_all_tools().await?;
    let schema = tools
        .iter()
        .find(|tool| tool.name == "memory_save_prompt")
        .unwrap_or_else(|| panic!("memory_save_prompt is listed"))
        .input_schema
        .clone();
    let namespace_doc = schema
        .get("properties")
        .and_then(|properties| properties.get("namespace"))
        .and_then(|prop| prop.get("description"))
        .and_then(|doc| doc.as_str())
        .unwrap_or_else(|| panic!("the schema must describe namespace: {schema:?}"))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        namespace_doc.contains("the namespace this workspace writes into"),
        "the arg doc must say which namespace a prompt lands in: {namespace_doc:?}"
    );

    let stored = call_tool_text(
        &client,
        "memory_save_prompt",
        serde_json::json!({ "content": "el puerto es 8081", "session_id": "01JX" }),
    )
    .await?;
    let stored: serde_json::Value = serde_json::from_str(&stored)?;
    assert_eq!(
        stored["namespace"], "equipo",
        "a prompt must land in the namespace this workspace's memories land in: {stored}"
    );
    assert_eq!(stored["session_id"], "01JX", "{stored}");
    assert_eq!(stored["content"], "el puerto es 8081", "{stored}");

    // The workspace's prompts are what a search from the workspace finds.
    let found = call_tool_text(
        &client,
        "memory_search_prompts",
        serde_json::json!({ "query": "puerto", "namespace": "equipo" }),
    )
    .await?;
    let found: serde_json::Value = serde_json::from_str(&found)?;
    assert_eq!(found["count"], 1, "{found}");
    assert_eq!(found["items"][0]["content"], "el puerto es 8081", "{found}");

    // A caller that names a namespace keeps it.
    let explicit = call_tool_text(
        &client,
        "memory_save_prompt",
        serde_json::json!({ "content": "una nota para el otro", "namespace": "otro" }),
    )
    .await?;
    let explicit: serde_json::Value = serde_json::from_str(&explicit)?;
    assert_eq!(
        explicit["namespace"], "otro",
        "the workspace is a default, never an override: {explicit}"
    );

    client.cancel().await?;
    Ok(())
}

#[tokio::test]
async fn memory_recall_is_priced_preview_by_default_full_on_request()
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
        "memory_session_start",
        serde_json::json!({ "session_id": "01JX", "directory": "/work/project" }),
    )
    .await?;
    let saved = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({ "content": "el servicio escucha en 8081" }),
    )
    .await?;
    let saved: serde_json::Value = serde_json::from_str(&saved)?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();
    call_tool_text(
        &client,
        "memory_session_end",
        serde_json::json!({ "session_id": "01JX", "summary": "el puerto" }),
    )
    .await?;

    // Default: identity and preview, the shape `list` and `memory_search`
    // already have (extension-recall S1).
    let text = call_tool_text(
        &client,
        "memory_recall",
        serde_json::json!({ "previous": true }),
    )
    .await?;
    let parsed: serde_json::Value = serde_json::from_str(&text)?;
    let memories = parsed["memories_in_window"]
        .as_array()
        .expect("memories_in_window array");
    assert_eq!(memories.len(), 1, "{parsed}");
    assert_eq!(memories[0]["uuid"], uuid.as_str(), "{parsed}");
    assert!(
        memories[0]["preview"].is_string(),
        "the default answer carries a preview: {parsed}"
    );
    assert!(
        memories[0].get("content").is_none(),
        "the default answer carries no full content: full asks for it: {parsed}"
    );

    // `full: true` asks for the whole records.
    let text = call_tool_text(
        &client,
        "memory_recall",
        serde_json::json!({ "previous": true, "full": true }),
    )
    .await?;
    let full: serde_json::Value = serde_json::from_str(&text)?;
    let memories = full["memories_in_window"]
        .as_array()
        .expect("memories_in_window array");
    assert_eq!(memories.len(), 1, "{full}");
    assert_eq!(
        memories[0]["content"], "el servicio escucha en 8081",
        "the full record is one argument away: {full}"
    );

    client.cancel().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// memory_context: the door in the MCP surface (mcp-door S1, S2, S3)
// ---------------------------------------------------------------------------

/// The door's answer is the one the CLI prints with `--json`: the priced
/// items, the session-state block, the coverage declaration and the
/// declarations of the namespace it searched, the tokens it spent and how far
/// the read reached (mcp-door S1).
#[tokio::test]
async fn memory_context_answers_the_door_with_its_items_state_and_coverage()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    bootstrap_profile_with_scope(data_dir, "default", "default", true, false)?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir").arg(data_dir).arg("mcp").arg("serve");
        }))?;
    let client = ().serve(transport).await?;

    // A live state record, a fact that does not match the task, and a
    // finished session: the block has something to carry.
    let saved = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "el puerto del servicio es 8081",
            "title": "donde estamos",
            "memory_type": "project_state",
        }),
    )
    .await?;
    let saved: serde_json::Value = serde_json::from_str(&saved)?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();
    call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "el despliegue corre en docker",
            "title": "despliegue",
        }),
    )
    .await?;
    call_tool_text(
        &client,
        "memory_session_start",
        serde_json::json!({ "session_id": "S1" }),
    )
    .await?;
    call_tool_text(
        &client,
        "memory_session_end",
        serde_json::json!({ "session_id": "S1", "summary": "el puerto del servicio" }),
    )
    .await?;

    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "puerto" }),
    )
    .await?;
    let door: serde_json::Value = serde_json::from_str(&text)?;

    // The whole facts: the item carries the record's whole text, never a cut
    // one (the budget decides which facts fit, not how much of a fact).
    let items = door["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "only the fact of the task: {door}");
    let item = &items[0];
    assert_eq!(item["uuid"], uuid.as_str(), "{door}");
    assert_eq!(item["title"], "donde estamos", "{door}");
    assert_eq!(
        item["content"], "el puerto del servicio es 8081",
        "the item carries the whole fact: {door}"
    );
    assert_eq!(item["namespace"], "default", "{door}");
    assert_eq!(item["revision"], 1, "{door}");
    assert!(
        item.get("preview").is_none(),
        "the door never cuts a fact: {door}"
    );

    // The declarations of the namespace it searched, the tokens it spent and
    // how far the read reached (#57, read-scope-disclosure S4).
    assert_eq!(door["searched_namespace"], "default", "{door}");
    assert_eq!(door["read_scope"], "except_global", "{door}");
    assert!(
        door["tokens_used"].as_i64().is_some(),
        "the answer declares the tokens it spent: {door}"
    );
    assert!(
        door["bytes_used"].as_i64().is_some(),
        "the answer declares the bytes it spent: {door}"
    );
    assert!(
        door["tokenizer_id"].is_string(),
        "the answer names its tokenizer: {door}"
    );
    assert_eq!(door["budget_enforcement"], "estimated", "{door}");
    assert!(door["retrieval_mode"].is_string(), "{door}");
    assert!(door["index_status"].is_string(), "{door}");
    assert!(door["warnings"].is_array(), "{door}");

    // The session-state block rides the same answer: the live records with
    // their whole content, the index of what did not fit, the previous
    // session's summary (call-budget S6).
    let block = door["session_state"]
        .as_object()
        .unwrap_or_else(|| panic!("the door carries the block: {door}"));
    let records = block["records"].as_array().expect("records array");
    assert_eq!(records.len(), 1, "the live record of the namespace: {door}");
    assert_eq!(records[0]["uuid"], uuid.as_str(), "{door}");
    assert_eq!(
        records[0]["content"], "el puerto del servicio es 8081",
        "the block carries the whole content, not the preview: {door}"
    );
    assert_eq!(records[0]["memory_type"], "project_state", "{door}");
    assert_eq!(
        block["index"],
        serde_json::json!([]),
        "nothing was left out of the block: {door}"
    );
    assert_eq!(block["session"]["id"], "S1", "{door}");
    assert_eq!(
        block["session"]["summary"], "el puerto del servicio",
        "{door}"
    );

    // The coverage declaration: what it saw, what it carries, what it left
    // out (board item 12, M4).
    assert_eq!(door["coverage"]["candidates"], 1, "{door}");
    assert_eq!(door["coverage"]["carried"], 1, "{door}");
    assert_eq!(door["coverage"]["left_out"], 0, "{door}");
    assert_eq!(
        door["coverage"]["left_out_items"],
        serde_json::json!([]),
        "{door}"
    );

    // A budget that cannot carry the candidates declares them, with the size
    // that explains why they did not fit.
    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "el", "token_budget": 60 }),
    )
    .await?;
    let tight: serde_json::Value = serde_json::from_str(&text)?;
    let coverage = &tight["coverage"];
    assert!(
        coverage["candidates"].as_u64().unwrap_or(0) >= 2,
        "the door saw both facts: {tight}"
    );
    assert!(
        coverage["left_out"].as_u64().unwrap_or(0) >= 1,
        "the budget left something out and the answer says so: {tight}"
    );
    let left_out = coverage["left_out_items"]
        .as_array()
        .expect("left_out_items array");
    assert!(
        !left_out.is_empty(),
        "the declaration names what it left out: {tight}"
    );
    assert!(left_out[0]["uuid"].is_string(), "{tight}");
    assert!(
        left_out[0]["bytes"].as_u64().is_some_and(|bytes| bytes > 0),
        "the entry says how big the fact was: {tight}"
    );

    // The `match` argument decides how the task's words are matched, and an
    // answer that carries nothing still declares where it looked (#57).
    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "docker 8081", "match": "all" }),
    )
    .await?;
    let strict: serde_json::Value = serde_json::from_str(&text)?;
    assert!(
        strict["items"].as_array().expect("items array").is_empty(),
        "no single fact carries both words: {strict}"
    );
    assert_eq!(strict["searched_namespace"], "default", "{strict}");
    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "docker 8081" }),
    )
    .await?;
    let loose: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        loose["items"].as_array().expect("items array").len(),
        2,
        "the default match counts every term: {loose}"
    );

    client.cancel().await?;
    Ok(())
}

/// One implementation, two surfaces: the tool calls the same
/// `MemoryService::context` the CLI calls, so the two answers are the same
/// envelope, not two projections of the same facts (mcp-door S2).
#[tokio::test]
async fn memory_context_answers_the_same_envelope_as_the_cli()
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
            "content": "el puerto del servicio es 8081",
            "title": "donde estamos",
            "memory_type": "project_state",
        }),
    )
    .await?;

    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "puerto" }),
    )
    .await?;
    let door: serde_json::Value = serde_json::from_str(&text)?;

    // The same question through the CLI's `--json` envelope.
    let cli = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args(["--json", "context", "puerto"])
        .output()?;
    assert!(
        cli.status.success(),
        "context failed: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let cli_door: serde_json::Value = serde_json::from_slice(&cli.stdout)?;

    assert_eq!(
        door, cli_door,
        "the door answers the same envelope in both surfaces"
    );

    client.cancel().await?;
    Ok(())
}

/// The namespace is the workspace's own — the one a save lands in — never
/// another project's; a caller may still name one the scope may read
/// (mcp-door S3).
#[tokio::test]
async fn memory_context_searches_the_workspaces_own_namespace()
-> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir_in("target")?;
    let data_dir = tmp.path();
    // The profile's default namespace is not the one the workspace writes
    // into: only the workspace's own namespace holds the record below, and
    // only the binding opens it for reading.
    bootstrap_profile_with_scope(data_dir, "default", "mnexis", false, false)?;

    // A workspace bound to `mnexis`, exactly like a real project.
    let workspace = tempfile::tempdir_in("target")?;
    let git = |args: &[&str]| -> Result<(), Box<dyn std::error::Error>> {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(workspace.path())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()?;
        assert!(status.success(), "git {args:?} failed");
        Ok(())
    };
    git(&["init", "-q", "."])?;
    git(&[
        "remote",
        "add",
        "origin",
        "https://github.com/Xeworg/mnexis.git",
    ])?;

    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let bound = std::process::Command::new(&binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args([
            "project",
            "bind",
            "mnexis",
            "--key",
            "git:github.com/Xeworg/mnexis",
        ])
        .output()?;
    assert!(
        bound.status.success(),
        "bind failed: {}",
        String::from_utf8_lossy(&bound.stderr)
    );

    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("--data-dir")
                .arg(data_dir)
                .arg("mcp")
                .arg("serve")
                .current_dir(workspace.path());
        }))?;
    let client = ().serve(transport).await?;

    // One record, in the namespace the workspace writes into.
    let saved = call_tool_text(
        &client,
        "memory_save",
        serde_json::json!({
            "content": "el puerto del servicio es 8081",
            "namespace": "mnexis",
        }),
    )
    .await?;
    let saved: serde_json::Value = serde_json::from_str(&saved)?;
    let uuid = saved["uuid"].as_str().expect("uuid").to_string();

    // Without a namespace the door searches the workspace's own namespace —
    // the one a save lands in — and the answer names it. Before this rule the
    // engine searched the profile's default namespace and answered another
    // project's memories, in silence (#57).
    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "puerto" }),
    )
    .await?;
    let door: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        door["searched_namespace"], "mnexis",
        "the door names the namespace it searched: {door}"
    );
    let items = door["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "the record of the workspace: {door}");
    assert_eq!(items[0]["uuid"], uuid.as_str(), "{door}");
    assert_eq!(items[0]["namespace"], "mnexis", "{door}");
    assert_eq!(
        door["read_scope"], "restricted",
        "the read scope still bounds the answer: {door}"
    );

    // A namespace the caller names is the one searched, and the answer names
    // it too: the other project holds nothing with this task.
    let text = call_tool_text(
        &client,
        "memory_context",
        serde_json::json!({ "task": "puerto", "namespace": "default" }),
    )
    .await?;
    let door: serde_json::Value = serde_json::from_str(&text)?;
    assert_eq!(
        door["searched_namespace"], "default",
        "the caller's namespace wins over the workspace's: {door}"
    );
    assert!(
        door["items"].as_array().expect("items array").is_empty(),
        "the other project has no record of this task: {door}"
    );

    client.cancel().await?;
    Ok(())
}
