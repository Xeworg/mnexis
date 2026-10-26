//! One additive block of project state, behind the bench switch (call-budget
//! S4, arms B and C).
//!
//! The control fans out over the surfaces because each of them holds a part of
//! "where we left off". The block hands one surface the whole of it: the
//! `project_state` and `decision` records **with their content** (not the
//! priced preview the other surfaces return), newest first, plus the most
//! recent session summary — the session `mnexis recall --previous` answers
//! with.
//!
//! The switch is a bench switch, not a product surface:
//! `MNEXIS_BENCH_SESSION_STATE` is `off`/unset (nothing changes anywhere),
//! `door` (the block rides the `mnexis context` answer) or `workspace` (it
//! rides the `mnexis status` payload). Any other value reads as unset, and an
//! explicit value wins over the environment — the `resolveLeanTools` shape:
//! the switch is read once, at the CLI edge, and injected downward, so a test
//! never mutates the process environment.
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

/// The variable the bench arms select the surface with.
pub const BENCH_SWITCH: &str = "MNEXIS_BENCH_SESSION_STATE";

/// The kinds that are project state: where the project is, and what it chose.
/// Both are values of the closed set in [`crate::domain::memory_type`].
pub const STATE_TYPES: [&str; 2] = ["project_state", "decision"];

/// The most records the block carries. One answer, not an archive: the cap
/// every list command has, applied after the type filter.
const STATE_RECORD_LIMIT: usize = 100;

/// Which answer carries the block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SessionStateMode {
    /// The shipped behaviour: no surface carries the block.
    #[default]
    Off,
    /// Arm B: `mnexis context` carries the block.
    Door,
    /// Arm C: `mnexis status` carries the block.
    Workspace,
}

impl SessionStateMode {
    /// The value of the switch that selects this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Door => "door",
            Self::Workspace => "workspace",
        }
    }

    /// `door` and `workspace`; everything else — `off`, an empty value, a typo
    /// — is `off`: an unknown value is treated as unset, never as an error.
    pub fn parse(value: &str) -> Self {
        match value.trim() {
            "door" => Self::Door,
            "workspace" => Self::Workspace,
            _ => Self::Off,
        }
    }

    /// Whether the block rides the `mnexis context` answer.
    pub fn is_door(self) -> bool {
        matches!(self, Self::Door)
    }

    /// Whether the block rides the `mnexis status` payload.
    pub fn is_workspace(self) -> bool {
        matches!(self, Self::Workspace)
    }
}

/// Resolve the bench switch: an explicit value wins, the environment is only
/// the default. Called once per process, at the CLI edge.
pub fn resolve_session_state_mode(explicit: Option<&str>) -> SessionStateMode {
    match explicit {
        Some(value) => SessionStateMode::parse(value),
        None => match std::env::var(BENCH_SWITCH) {
            Ok(value) => SessionStateMode::parse(&value),
            Err(_) => SessionStateMode::Off,
        },
    }
}

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
    /// namespace a write without one would land in). The session is the one
    /// `mnexis recall --previous` answers with
    /// ([`MemoryRepository::previous_finished_session`]), so the block and the
    /// recall cannot disagree about where the project left off.
    ///
    /// A store that cannot answer leaves the block empty: it is additive, and
    /// it must never fail the command that carries it.
    pub fn session_state(&self, namespace: &str) -> SessionState {
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

    #[test]
    fn the_two_states_are_the_two_named_values() {
        assert_eq!(SessionStateMode::parse("door"), SessionStateMode::Door);
        assert_eq!(
            SessionStateMode::parse("workspace"),
            SessionStateMode::Workspace
        );
        assert_eq!(SessionStateMode::parse("off"), SessionStateMode::Off);
    }

    #[test]
    fn an_unknown_value_is_treated_as_unset() {
        for value in ["", "  ", "DOOR", "Door", "turbo", "1", "on"] {
            assert_eq!(
                SessionStateMode::parse(value),
                SessionStateMode::Off,
                "`{value}` must read as unset"
            );
        }
    }

    #[test]
    fn an_explicit_value_wins_over_the_environment() {
        // The injection point: a caller that passes a value never consults the
        // environment, which is what makes the switch testable without a
        // mutated process environment.
        assert_eq!(
            resolve_session_state_mode(Some("workspace")),
            SessionStateMode::Workspace
        );
        assert_eq!(
            resolve_session_state_mode(Some("turbo")),
            SessionStateMode::Off
        );
    }

    #[test]
    fn the_mode_names_the_value_that_selects_it() {
        for mode in [
            SessionStateMode::Off,
            SessionStateMode::Door,
            SessionStateMode::Workspace,
        ] {
            assert_eq!(SessionStateMode::parse(mode.as_str()), mode);
        }
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
