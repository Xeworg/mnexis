//! One additive block of project state, carried by the door.
//!
//! The control fans out over the surfaces because each of them holds a part of
//! "where we left off". The block hands one surface the whole of it: the
//! `project_state` and `decision` records **with their content** (not the
//! priced preview the other surfaces return), newest first, plus the most
//! recent session summary — the session `mnexis recall --previous` answers
//! with.
//!
//! It rides one surface, and the experiment chose which one (call-budget S6):
//! measured against the shipped surfaces, the block on the door saved a median
//! round and a quarter of the tokens; the same block on `status` saved less and
//! never became the surface the agent answers from. So the door carries it and
//! `status` keeps its priced shape (#56). The bench switch is gone with the
//! experiment: the variable a bench arm set changes nothing now.
//!
//! The block is **outside the token budget**: the budgeted envelope is
//! `serialise_envelope` (label, tokenizer, items) and the block is not part of
//! it, so the budgeted hits keep their selection, their order and their count
//! whether the block is there or not.

use serde::{Deserialize, Serialize};

use crate::application::service::MemoryService;
use crate::domain::memory::Memory;
use crate::domain::repository::MemoryRepository;
use crate::domain::scope::NamespaceScope;

/// The kinds that are project state: where the project is, and what it chose.
/// Both are values of the closed set in [`crate::domain::memory_type`].
pub const STATE_TYPES: [&str; 2] = ["project_state", "decision"];

/// The most records the block carries. One answer, not an archive: the cap
/// every list command has, applied after the type filter.
const STATE_RECORD_LIMIT: usize = 100;

/// The block: this project's state, with its content.
///
/// An empty store is a valid block — an empty list and no session — never a
/// failure: the block is additive, so it can never take the command down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    /// The `project_state` and `decision` records of the namespace, newest
    /// first, each with its whole content.
    pub records: Vec<StateRecord>,
    /// The most recent finished session of the same namespace, when there is
    /// one.
    pub session: Option<SessionSummary>,
}

impl SessionState {
    /// The block of a project with nothing to say yet.
    pub fn empty() -> Self {
        Self {
            records: Vec::new(),
            session: None,
        }
    }
}

/// One state record, with the content the priced preview would have cut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateRecord {
    /// The memory's stable identifier, as `mnexis show` expects it.
    pub uuid: String,
    /// The kind of thing the fact is: `project_state` or `decision`.
    pub memory_type: String,
    /// What the fact *is*. Empty when it has no title.
    pub title: String,
    /// The whole content, never a preview.
    pub content: String,
    /// When the record was created: the block reads newest first.
    pub created_at: String,
}

impl StateRecord {
    fn of(memory: Memory) -> Self {
        Self {
            uuid: memory.uuid,
            memory_type: memory.memory_type,
            title: memory.title,
            content: memory.content,
            created_at: memory.created_at,
        }
    }
}

/// The session before this one: its identity, when it ended, and its summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    /// The caller's identifier for the session.
    pub id: String,
    /// The namespace the session belongs to.
    pub namespace: String,
    /// When it ended; `null` only for a session the store still calls open.
    pub ended_at: Option<String>,
    /// What the session was, as it was told when it ended.
    pub summary: Option<String>,
}

impl<R: MemoryRepository> MemoryService<R> {
    /// The state block of `namespace`: the records of what the project is and
    /// what it chose, with their content, plus the most recent session
    /// summary of the same namespace.
    ///
    /// `namespace` is one the calling command already authorized: the door
    /// passes the namespace it searched, `status` the workspace's own (or the
    /// namespace a write without one would land in). A namespace the scope
    /// cannot read yields the empty block: the read allowlist is a boundary,
    /// and a write-only namespace never exposes its content here. The session
    /// is the one `mnexis recall --previous` answers with
    /// ([`MemoryRepository::previous_finished_session`]), so the block and the
    /// recall cannot disagree about where the project left off.
    ///
    /// A store that cannot answer leaves the block empty: it is additive, and
    /// it must never fail the command that carries it.
    pub fn session_state(&self, namespace: &str) -> SessionState {
        if !self.scope().can_read(namespace) {
            return SessionState::empty();
        }
        let types: Vec<String> = STATE_TYPES
            .iter()
            .map(|memory_type| (*memory_type).to_string())
            .collect();
        let records = self
            .repo
            .list_typed_recent(
                &NamespaceScope::Only(vec![namespace.to_string()]),
                &types,
                STATE_RECORD_LIMIT,
            )
            .map(|memories| memories.into_iter().map(StateRecord::of).collect())
            .unwrap_or_default();
        let session = self
            .repo
            .previous_finished_session(namespace)
            .ok()
            .flatten()
            .map(|session| SessionSummary {
                id: session.id,
                namespace: session.namespace,
                ended_at: session.ended_at,
                summary: session.summary,
            });
        SessionState { records, session }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::service::{SaveRequest, ServiceConfig};
    use crate::domain::memory::MemoryState;
    use crate::domain::scope::{AccessScope, Operation};
    use crate::storage::memory_repo::SqliteMemoryRepository;
    use crate::storage::sqlite::open_sqlite;
    use tempfile::tempdir_in;

    const NS: &str = "payments";

    type Service = MemoryService<SqliteMemoryRepository>;

    /// A service whose read list is `read`, whose write list and default
    /// namespace are [`NS`]: the write-only default of a profile that may
    /// write the namespace but not read it.
    fn service_reading(read: Vec<String>) -> (Service, tempfile::TempDir) {
        let dir = tempdir_in("target").expect("tempdir under target/");
        let conn = open_sqlite(&dir.path().join("session-state.db")).expect("open sqlite");
        let scope = AccessScope::new(
            read,
            vec![NS.to_string()],
            NS.to_string(),
            false,
            vec![Operation::Save],
        );
        (
            MemoryService::new(
                SqliteMemoryRepository::new(conn),
                scope,
                ServiceConfig::default(),
            ),
            dir,
        )
    }

    fn save_state(service: &mut Service) {
        let req = SaveRequest {
            uuid: "state-1".to_string(),
            title: "where we are".to_string(),
            topic_key: "state.where".to_string(),
            namespace: NS.to_string(),
            memory_type: Some("project_state".to_string()),
            content: "la puerta esta abierta".to_string(),
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
            valid_from: None,
            valid_until: None,
            tags: Vec::new(),
            keywords: Vec::new(),
            replace_keywords: false,
            idempotency_key: None,
            state: MemoryState::Active,
            now: "2026-10-26T10:00:00Z".to_string(),
        };
        service.save(req).expect("save a project_state record");
    }

    #[test]
    fn a_write_only_namespace_never_yields_a_block() {
        // A profile can write a namespace it cannot read (a write-only
        // default). The block carries whole content, so it must answer for a
        // readable namespace only: the read allowlist is a boundary, not a
        // hint.
        let (mut service, _dir) = service_reading(Vec::new());
        save_state(&mut service);
        let block = service.session_state(NS);
        assert!(
            block.records.is_empty(),
            "a write-only namespace yielded {} record(s)",
            block.records.len()
        );
        assert!(
            block.session.is_none(),
            "a write-only namespace yielded a session"
        );
    }

    #[test]
    fn a_readable_namespace_still_yields_the_block() {
        let (mut service, _dir) = service_reading(vec![NS.to_string()]);
        save_state(&mut service);
        let block = service.session_state(NS);
        assert_eq!(block.records.len(), 1, "the saved record must be there");
        assert_eq!(block.records[0].uuid, "state-1");
        assert_eq!(block.records[0].memory_type, "project_state");
        assert_eq!(block.records[0].content, "la puerta esta abierta");
    }

    #[test]
    fn an_empty_block_is_the_empty_list_and_no_session() {
        let block = SessionState::empty();
        assert!(block.records.is_empty());
        assert!(block.session.is_none());
        let json = serde_json::to_value(&block).expect("serialise the block");
        assert_eq!(json["records"], serde_json::json!([]));
        assert_eq!(json["session"], serde_json::Value::Null);
    }
}
