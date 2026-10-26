//! One status payload for every surface that has to answer "where may I
//! write, and what is already there?" (agent-memory-ergonomics S4).
//!
//! `mnexis status` and the MCP `memory_workspace` tool both render this
//! payload, so the CLI and the agent cannot drift: the workspace the process
//! runs in, the profile's read and write allowlists, how many memories the
//! scope can see, and the most recent of them — each with its identity and
//! a preview of its content (#53). The first answer a session reads is
//! priced like every other read: the full record is one `mnexis show` away.
//!
//! The workspace namespace is reported even when the scope cannot read it:
//! the agent still has to know where a write would land. In that case the
//! recent list falls back to every namespace the scope does read.

use std::path::Path;

use serde::Serialize;

use crate::application::MemoryService;
use crate::application::preview::MemoryPreview;
use crate::application::service::session_state::SessionState;
use crate::application::workspace::current_workspace;
use crate::domain::error::MnexisError;
use crate::domain::repository::MemoryRepository;

/// The workspace → namespace and scope → recent memories payload shared by
/// the CLI command and the MCP tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkspaceStatus {
    /// The workspace the process runs in and the namespace it writes into.
    pub workspace: StatusWorkspace,
    /// The profile's default namespace, allowlists, and permission flags.
    pub scope: StatusScope,
    /// How many memories the scope can see, in every namespace it reads.
    pub memory_count: i64,
    /// The most recent memories the caller can read, newest first — the
    /// priced projection (`MemoryPreview`): identity and a preview, the same
    /// shape `list` answers with. The full record is one `mnexis show` away.
    pub recent: Vec<MemoryPreview>,
    /// The session-state block, when a bench arm asks for it (call-budget
    /// S4, arm C). Absent by default, and absent from the JSON when absent:
    /// the shipped payload has no such key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_state: Option<SessionState>,
}

/// The workspace the process runs in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusWorkspace {
    /// The workspace key: `git:<normalized remote>` or `path:<directory>`.
    /// Empty only when the working directory could not be read at all.
    pub key: String,
    /// The namespace the workspace is bound to (or names with a marker);
    /// `None` when the workspace resolves to none and the profile allowlist
    /// keeps deciding.
    pub namespace: Option<String>,
}

/// The read and write allowlists the process runs under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StatusScope {
    /// The namespace a save without `--namespace` lands in.
    pub default_namespace: String,
    /// The namespaces the profile allows reading.
    pub read_namespaces: Vec<String>,
    /// The namespaces the profile allows writing.
    pub write_namespaces: Vec<String>,
    /// Whether the reserved `global` namespace is allowed.
    pub allow_global: bool,
    /// Whether reads are open across every namespace.
    pub allow_read_all: bool,
}

/// Build the status payload: where may I write, and what is already there?
///
/// `data_dir` is the profile directory the workspace bindings live in (next
/// to `mnexis.db`), and `recent_limit` bounds the recent list.
pub fn workspace_status<R: MemoryRepository>(
    service: &MemoryService<R>,
    data_dir: &Path,
    recent_limit: usize,
) -> Result<WorkspaceStatus, MnexisError> {
    let (key, namespace) = match current_workspace(data_dir)? {
        Some(workspace) => (workspace.key, workspace.namespace),
        // A working directory the process cannot read leaves the key empty:
        // the payload keeps its shape and the allowlists keep deciding.
        None => (String::new(), None),
    };
    let scope = service.scope();
    // The workspace namespace is the agent's home when it can read it.
    // Otherwise the recent list falls back to the readable scope while the
    // workspace field still names the namespace a write would land in.
    let recent = match namespace.as_deref() {
        Some(ns) if scope.can_read(ns) => service.recent(Some(ns), recent_limit)?,
        _ => service.recent(None, recent_limit)?,
    };
    Ok(WorkspaceStatus {
        workspace: StatusWorkspace { key, namespace },
        scope: StatusScope {
            default_namespace: scope.default_namespace.clone(),
            read_namespaces: scope.read_namespaces.clone(),
            write_namespaces: scope.write_namespaces.clone(),
            allow_global: scope.allow_global,
            allow_read_all: scope.allow_read_all,
        },
        memory_count: service.count()?,
        recent: crate::application::previews(&recent),
        session_state: None,
    })
}
