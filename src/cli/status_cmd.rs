//! `mnexis status` — one call that answers "where may I write, and what is
//! already there?" (agent-memory-ergonomics S4).
//!
//! The payload is built by
//! [`crate::application::workspace_status::workspace_status`], the same
//! function the MCP `memory_workspace` tool renders, so the CLI and the agent
//! cannot drift. Like `mnexis list`, the command needs an initialised profile
//! and never writes to it.

use std::path::PathBuf;

use serde_json::json;

use crate::application::WorkspaceStatus;
use crate::application::workspace_status::workspace_status;
use crate::cli::output::{CommandOutcome, one_line};
use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;

/// How many recent memories the status line lists.
pub const RECENT_LIMIT: usize = 3;

/// `mnexis status`: the workspace namespace, the allowlists, the memory
/// count, and the most recent memories, newest first.
///
pub fn status(data_dir: PathBuf) -> Result<CommandOutcome, MnexisError> {
    // A missing profile is the usual NOT_FOUND (exit 3), exactly like
    // `mnexis list`: the command reports on a profile, it does not create one.
    let profile = Profile::open(data_dir)?;
    let service = profile.service_ref();
    let status = workspace_status(service, &profile.data_dir, RECENT_LIMIT)?;
    Ok(CommandOutcome::new(json!(status), render_text(&status)))
}

/// One line per fact: where the process runs, what the scope allows, and
/// what the most recent memories are.
fn render_text(status: &WorkspaceStatus) -> String {
    let workspace = &status.workspace;
    let key = if workspace.key.is_empty() {
        "(unknown)"
    } else {
        workspace.key.as_str()
    };
    let namespace = workspace
        .namespace
        .as_deref()
        .unwrap_or("(unbound: the profile allowlist decides)");
    let mut text = format!(
        "workspace: {key} -> {namespace}\n\
         scope: default={}, read={}, write={}, global={}, read_all={}",
        status.scope.default_namespace,
        quoted_list(&status.scope.read_namespaces),
        quoted_list(&status.scope.write_namespaces),
        status.scope.allow_global,
        status.scope.allow_read_all,
    );
    if status.recent.is_empty() {
        text.push_str(&format!(
            "\nmemories: {} (no recent memories)",
            status.memory_count
        ));
        return text;
    }
    text.push_str(&format!(
        "\nmemories: {} ({} most recent, previews)",
        status.memory_count,
        status.recent.len()
    ));
    for memory in &status.recent {
        text.push_str(&format!(
            "\n  [{}] {} \"{}\" — {}",
            memory.namespace,
            memory.uuid,
            one_line(&memory.title),
            one_line(&memory.preview)
        ));
    }
    text.push_str("\nfull record: mnexis show <uuid>");
    text
}

/// `["a","b"]`: the shape the scope line documents, one fact per line.
fn quoted_list(names: &[String]) -> String {
    format!(
        "[{}]",
        names
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(",")
    )
}
