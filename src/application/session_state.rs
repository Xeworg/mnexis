//! One additive block of project state, carried by the door.
//!
//! The control fans out over the surfaces because each of them holds a part of
//! "where we left off". The block hands one surface the whole of it: the
//! **live** `project_state` and `decision` records (not the priced preview the
//! other surfaces return), newest first, plus the most recent session summary —
//! the session `mnexis recall --previous` answers with.
//!
//! It rides one surface, and the experiment chose which one (call-budget S6):
//! measured against the shipped surfaces, the block on the door saved a median
//! round and a quarter of the tokens; the same block on `status` saved less and
//! never became the surface the agent answers from. So the door carries it and
//! `status` keeps its priced shape (#56). The bench switch is gone with the
//! experiment: the variable a bench arm set changes nothing now.
//!
//! The block is **outside the token budget of the door**: the budgeted envelope
//! is `serialise_envelope` (label, tokenizer, items) and the block is not part
//! of it, so the budgeted hits keep their selection, their order and their
//! count whether the block is there or not.
//!
//! It carries a budget of its own, because an unbudgeted resident mass never
//! decays (state-block-budget S4). The resident part — the live records with
//! their whole content — fits [`SESSION_STATE_TOKEN_BUDGET`]; the live records
//! that do not fit ride [`SessionState::index`], identity and title without
//! content, so the agent sees that they exist and pulls one with
//! `mnexis show <uuid>`. The fill is greedy, deterministic and newest-first:
//! the recursion's order is the decision, and a decision that depends on the
//! budget is not reproducible without one.
//!
//! The history — `superseded`, `archived` and `proposed` — is excluded
//! silently (state-block-budget S2), and the filter is in the SQL
//! ([`MemoryRepository::list_typed_state_recent`]): a filter applied after the
//! query would let superseded records crowd the live ones out of the limit.

use serde::{Deserialize, Serialize};

use crate::application::budget::token_count_v1;
use crate::application::service::MemoryService;
use crate::domain::memory::Memory;
use crate::domain::repository::MemoryRepository;
use crate::domain::scope::NamespaceScope;

/// The kinds that are project state: where the project is, and what it chose.
/// Both are values of the closed set in [`crate::domain::memory_type`].
pub const STATE_TYPES: [&str; 2] = ["project_state", "decision"];

/// The states of a record that is still live: what the project is, and what it
/// still argues about. The history is everything else.
pub const LIVE_STATES: [&str; 2] = ["active", "disputed"];

/// The most records the block carries. One answer, not an archive: the cap
/// every list command has, applied after the type and state filters.
///
/// Twenty-five candidates feed the fill; the budget decides how many of them
/// keep their content and how many are indexed.
const STATE_RECORD_LIMIT: usize = 25;

/// The budget of the block's resident part, in tokens: the live records that
/// fit carry their whole content, the rest are indexed.
///
/// It is the block's own budget — the door's budgeted items never see it — and
/// it is measured with the same estimator ([`token_count_v1`]) the door's
/// budget uses.
pub const SESSION_STATE_TOKEN_BUDGET: i64 = 2048;

/// The block: this project's live state, with the content that fits.
///
/// An empty store is a valid block — an empty list, an empty index and no
/// session — never a failure: the block is additive, so it can never take the
/// command down.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    /// The live records that fit the block's budget: whole content, newest
    /// first.
    pub records: Vec<StateRecord>,
    /// The live records that did not fit: identity and title, no content. The
    /// agent sees that they exist and pulls one with `mnexis show <uuid>`.
    pub index: Vec<StateIndexEntry>,
    /// The budget the resident part may use, in tokens.
    pub budget: i64,
    /// What the resident part costs, in tokens.
    pub resident_tokens: i64,
    /// What the whole block costs (resident + index + session), in tokens.
    /// This is what the agent pays.
    pub tokens: i64,
    /// The most recent finished session of the same namespace, when there is
    /// one.
    pub session: Option<SessionSummary>,
}

impl SessionState {
    /// The block of a project with nothing to say yet.
    pub fn empty() -> Self {
        Self {
            records: Vec::new(),
            index: Vec::new(),
            budget: SESSION_STATE_TOKEN_BUDGET,
            resident_tokens: 0,
            tokens: 0,
            session: None,
        }
        .with_counts()
    }

    /// Declare what the block costs, with the estimator the door's budget uses.
    ///
    /// `resident_tokens` is the token count of the resident part's serialised
    /// form, and `tokens` the token count of the whole block: the declaration
    /// itself is counted zeroed, so the number does not measure its own digits.
    fn with_counts(mut self) -> Self {
        self.resident_tokens = token_count_v1(&serialise_records(&self.records));
        let mut counted = self.clone();
        counted.tokens = 0;
        self.tokens =
            token_count_v1(&serde_json::to_string(&counted).expect("the block serialises"));
        self
    }
}

/// The resident part's serialised form: the records, as the wire carries them,
/// in the block's order.
fn serialise_records(records: &[StateRecord]) -> String {
    serde_json::to_string(records).expect("the resident part serialises")
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
    /// The record's lifecycle state. The block carries only the live ones, so
    /// this is `active` or `disputed`.
    pub state: String,
}

impl StateRecord {
    fn of(memory: Memory) -> Self {
        Self {
            uuid: memory.uuid,
            memory_type: memory.memory_type,
            title: memory.title,
            content: memory.content,
            created_at: memory.created_at,
            state: memory.state.as_str().to_string(),
        }
    }
}

/// A live record the budget left out: identity and title, no content.
///
/// The agent sees that the record exists and pulls it whole with
/// `mnexis show <uuid>`, so the uuid it carries is the full one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateIndexEntry {
    /// The memory's stable identifier, as `mnexis show` expects it.
    pub uuid: String,
    /// The kind of thing the fact is: `project_state` or `decision`.
    pub memory_type: String,
    /// What the fact *is*. Empty when it has no title.
    pub title: String,
    /// When the record was created: the block reads newest first.
    pub created_at: String,
    /// The record's lifecycle state: `active` or `disputed`.
    pub state: String,
}

impl StateIndexEntry {
    fn of(record: StateRecord) -> Self {
        Self {
            uuid: record.uuid,
            memory_type: record.memory_type,
            title: record.title,
            created_at: record.created_at,
            state: record.state,
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
    /// The state block of `namespace`: the live records of what the project is
    /// and what it chose, newest first, with the content its budget has room
    /// for, plus the most recent session summary of the same namespace.
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
        let states: Vec<String> = LIVE_STATES
            .iter()
            .map(|state| (*state).to_string())
            .collect();
        let candidates = self
            .repo
            .list_typed_state_recent(
                &NamespaceScope::Only(vec![namespace.to_string()]),
                &types,
                &states,
                STATE_RECORD_LIMIT,
            )
            .unwrap_or_default();

        // Greedy, deterministic, newest-first: a candidate keeps its content
        // while the resident part stays within the budget, and the first one
        // that does not fit — with every one after it — is indexed instead.
        let mut records: Vec<StateRecord> = Vec::new();
        let mut index: Vec<StateIndexEntry> = Vec::new();
        let mut full = false;
        for memory in candidates {
            if full {
                index.push(StateIndexEntry::of(StateRecord::of(memory)));
                continue;
            }
            records.push(StateRecord::of(memory));
            if token_count_v1(&serialise_records(&records)) > SESSION_STATE_TOKEN_BUDGET {
                let record = records.pop().expect("the record just pushed");
                index.push(StateIndexEntry::of(record));
                full = true;
            }
        }

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
        SessionState {
            records,
            index,
            budget: SESSION_STATE_TOKEN_BUDGET,
            resident_tokens: 0,
            tokens: 0,
            session,
        }
        .with_counts()
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
