//! Real-client MCP interoperability smoke (T10; ODD L77).
//!
//! Drives the actual `mnexis mcp serve` binary with rmcp 3.3.0's official
//! client over the child-process stdio transport. A green run proves the
//! full session — initialize, initialized, tools/list, tools/call and
//! shutdown — works across a real process boundary, and that the server's
//! stdout carries only protocol frames: any stray non-protocol output
//! would break JSON-RPC framing and fail the session.

use rmcp::{
    ServiceExt,
    model::{CallToolRequestParams, ContentBlock},
    transport::{ConfigureCommandExt, TokioChildProcess},
};

#[tokio::test]
async fn official_rmcp_client_completes_stdio_session_against_the_real_binary()
-> Result<(), Box<dyn std::error::Error>> {
    let binary = std::env::var("CARGO_BIN_EXE_mnexis")?;
    let transport =
        TokioChildProcess::new(tokio::process::Command::new(&binary).configure(|cmd| {
            cmd.arg("mcp").arg("serve");
        }))?;

    // `().serve(...)` runs the initialize/initialized handshake with a
    // default no-op client handler; failing here means the server never
    // completed the handshake on its real stdio.
    let client = ().serve(transport).await?;

    // tools/list must advertise the `ping` tool.
    let tools = client.list_all_tools().await?;
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    assert!(
        names.contains(&"ping"),
        "tools/list must contain 'ping', got: {names:?}"
    );

    // tools/call ping(text="smoke") must return "pong: smoke".
    let result = client
        .call_tool(
            CallToolRequestParams::new("ping").with_arguments(
                serde_json::json!({ "text": "smoke" })
                    .as_object()
                    .expect("static object")
                    .clone(),
            ),
        )
        .await?;
    assert!(
        !result.is_error.unwrap_or(false),
        "the ping tool must not report an error: {result:?}"
    );
    let texts: Vec<&str> = result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("pong: smoke")),
        "tool result must contain 'pong: smoke', got: {texts:?}"
    );

    // Graceful shutdown: `cancel` consumes the service and resolves once
    // the session has fully ended (stdin closes -> serve_stdio returns ->
    // the child process exits -> the transport task joins), proving the
    // real server process terminated.
    client.cancel().await?;
    Ok(())
}
