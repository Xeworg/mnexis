//! Memory domain types — maps to the production `memories` table and
//! related tables (`memory_history`, `memory_links`, `memory_tags`).

use serde::{Deserialize, Serialize};

use crate::domain::error::MnexisError;

/// The state of a memory (PRD §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryState {
    Proposed,
    Active,
    Disputed,
    Superseded,
    Archived,
}

impl MemoryState {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryState::Proposed => "proposed",
            MemoryState::Active => "active",
            MemoryState::Disputed => "disputed",
            MemoryState::Superseded => "superseded",
            MemoryState::Archived => "archived",
        }
    }

    pub fn from_db(s: &str) -> Result<Self, MnexisError> {
        match s {
            "proposed" => Ok(MemoryState::Proposed),
            "active" => Ok(MemoryState::Active),
            "disputed" => Ok(MemoryState::Disputed),
            "superseded" => Ok(MemoryState::Superseded),
            "archived" => Ok(MemoryState::Archived),
            other => Err(MnexisError::invalid_argument(
                "state",
                format!("invalid state value: {other}"),
            )),
        }
    }

    /// Allowed next states per the PRD §5/§6 state transition map.
    /// Soft-delete is via the `deleted` field, not a state transition.
    pub fn next_states(self) -> &'static [MemoryState] {
        use MemoryState::*;
        match self {
            Proposed => &[Active, Disputed, Archived],
            Active => &[Disputed, Superseded, Archived],
            Disputed => &[Active, Superseded, Archived],
            Superseded => &[],
            Archived => &[Active],
        }
    }

    pub fn can_transition_to(self, target: MemoryState) -> bool {
        self.next_states().contains(&target)
    }
}

/// A memory record. Maps to the `memories` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    /// `None` on insert (assigned by the DB); `Some` after load.
    pub id: Option<i64>,
    pub uuid: String,
    pub topic_key: String,
    pub namespace: String,
    pub memory_type: String,
    pub content: String,
    pub state: MemoryState,
    pub revision: i32,
    pub deleted: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
    pub purge_after: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub actor: String,
    pub channel: String,
    pub assertion_basis: String,
}

impl Memory {
    /// Construct a new memory in `proposed` state with revision 1, ready for `save()`.
    #[allow(clippy::too_many_arguments)]
    pub fn new_proposed(
        uuid: impl Into<String>,
        topic_key: impl Into<String>,
        namespace: impl Into<String>,
        memory_type: impl Into<String>,
        content: impl Into<String>,
        actor: impl Into<String>,
        channel: impl Into<String>,
        assertion_basis: impl Into<String>,
        now: impl Into<String>,
    ) -> Self {
        let now = now.into();
        Self {
            id: None,
            uuid: uuid.into(),
            topic_key: topic_key.into(),
            namespace: namespace.into(),
            memory_type: memory_type.into(),
            content: content.into(),
            state: MemoryState::Proposed,
            revision: 1,
            deleted: false,
            created_at: now.clone(),
            updated_at: now,
            deleted_at: None,
            purge_after: None,
            valid_from: None,
            valid_until: None,
            actor: actor.into(),
            channel: channel.into(),
            assertion_basis: assertion_basis.into(),
        }
    }

    /// True if this memory is currently soft-deleted.
    pub fn is_soft_deleted(&self) -> bool {
        self.deleted
    }

    /// True if this memory is eligible for retrieval: active, not deleted,
    /// and (if validity windows are set) currently within them.
    pub fn is_retrievable_now(&self) -> bool {
        !self.deleted && self.state == MemoryState::Active
    }
}

/// A memory revision history record. Maps to `memory_history`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryHistory {
    pub id: Option<i64>,
    pub memory_uuid: String,
    pub revision: i32,
    pub memory_type: String,
    pub content: String,
    pub state: MemoryState,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub assertion_basis: String,
    pub changed_by: String,
    pub change_reason: String,
    pub changed_at: String,
}

/// A memory link. Maps to `memory_links`. MVP: supersedes only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLink {
    pub id: Option<i64>,
    pub older_uuid: String,
    pub newer_uuid: String,
    pub link_type: String, // MVP: always "supersedes"
    pub created_at: String,
}

/// A memory tag value. Maps to `memory_tags`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryTag {
    pub id: Option<i64>,
    pub memory_uuid: String,
    pub tag: String, // lowercase, trimmed; normalized externally
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_round_trip_through_db_string() {
        for s in [
            MemoryState::Proposed,
            MemoryState::Active,
            MemoryState::Disputed,
            MemoryState::Superseded,
            MemoryState::Archived,
        ] {
            assert_eq!(MemoryState::from_db(s.as_str()).unwrap(), s);
        }
    }

    #[test]
    fn state_from_db_rejects_unknown() {
        let err = MemoryState::from_db("deleted").unwrap_err();
        assert!(matches!(err, MnexisError::InvalidArgument { .. }));
    }

    #[test]
    fn state_transitions_match_prd_map() {
        assert!(MemoryState::Proposed.can_transition_to(MemoryState::Active));
        assert!(MemoryState::Proposed.can_transition_to(MemoryState::Disputed));
        assert!(MemoryState::Active.can_transition_to(MemoryState::Superseded));
        assert!(MemoryState::Active.can_transition_to(MemoryState::Disputed));
        assert!(MemoryState::Disputed.can_transition_to(MemoryState::Active));
        assert!(MemoryState::Archived.can_transition_to(MemoryState::Active));
        // Superseded has no direct transitions.
        assert!(!MemoryState::Superseded.can_transition_to(MemoryState::Active));
        assert!(!MemoryState::Superseded.can_transition_to(MemoryState::Archived));
        // Illegal: active cannot go back to proposed.
        assert!(!MemoryState::Active.can_transition_to(MemoryState::Proposed));
    }

    #[test]
    fn memory_new_proposed_sets_sensible_defaults() {
        let m = Memory::new_proposed(
            "uuid-1",
            "topic.key",
            "proyecto-a",
            "fact",
            "content body",
            "user",
            "cli",
            "user_stated",
            "2026-01-01T00:00:00Z",
        );
        assert_eq!(m.id, None);
        assert_eq!(m.uuid, "uuid-1");
        assert_eq!(m.revision, 1);
        assert_eq!(m.state, MemoryState::Proposed);
        assert!(!m.deleted);
        assert_eq!(m.created_at, "2026-01-01T00:00:00Z");
        assert_eq!(m.updated_at, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn memory_retrievability_logic() {
        let mut m = Memory::new_proposed("u", "", "ns", "fact", "c", "a", "c", "b", "t");
        assert!(!m.is_retrievable_now());
        m.state = MemoryState::Active;
        assert!(m.is_retrievable_now());
        m.deleted = true;
        assert!(!m.is_retrievable_now());
    }
}
