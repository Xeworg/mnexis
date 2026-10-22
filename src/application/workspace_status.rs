//! One status payload for every surface that has to answer "where may I
//! write, and what is already there?" (agent-memory-ergonomics S4).
//!
//! `mnexis status` and the MCP `memory_workspace` tool both render this
//! payload, so the CLI and the agent cannot drift: the workspace the process
//! runs in, the profile's read and write allowlists, how many memories the
//! scope can see, and the most recent of them.
//!
//! The workspace namespace is reported even when the scope cannot read it:
//! the agent still has to know where a write would land. In that case the
//! recent list falls back to every namespace the scope does read.

use std::path::Path;

use serde::Serialize;

use crate::application::MemoryService;
use crate::application::workspace::current_workspace;
use crate::domain::error::MnexisError;
use crate::domain::memory::Memory;
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
    /// The most recent memories the caller can read, newest first.
    pub recent: Vec<RecentMemory>,
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

/// One entry of the recent list: enough to recognize a memory and open it,
/// without the internal bookkeeping fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecentMemory {
    /// The memory's stable identifier, as `mnexis show` expects it.
    pub uuid: String,
    /// The namespace the memory lives in.
    pub namespace: String,
    /// The memory's content, verbatim.
    pub content: String,
    /// The memory's current revision.
    pub revision: i32,
    /// When the memory was last written (`RFC3339`).
    pub updated_at: String,
}

impl RecentMemory {
    fn from_memory(memory: &Memory) -> Self {
        Self {
            uuid: memory.uuid.clone(),
            namespace: memory.namespace.clone(),
            content: memory.content.clone(),
            revision: memory.revision,
            updated_at: memory.updated_at.clone(),
        }
    }
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
        recent: recent.iter().map(RecentMemory::from_memory).collect(),
    })
}
