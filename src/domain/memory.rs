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
    /// The short, stable label of the fact. The topic key derives from it,
    /// so a fact keeps one record when the value inside the content changes.
    #[serde(default)]
    pub title: String,
    /// The vocabulary of the fact: the words the same fact is called in
    /// another language, in another jargon, in another document. The agent
    /// supplies them when it saves; the record keeps them.
    #[serde(default)]
    pub keywords: Vec<String>,
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
            title: String::new(),
            keywords: Vec::new(),
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

    /// The words of the fact as the `memories.keywords` column stores them:
    /// comma-separated, one `, ` between words, no spaces inside a word.
    ///
    /// The column is the one ASCII-ish shape every read agrees on; the words
    /// themselves stay in the language they were written in.
    pub fn keywords_column(words: &[String]) -> String {
        words.join(", ")
    }

    /// Read a `memories.keywords` column back into words. An empty column —
    /// every memory saved before the words existed — reads back as no words.
    pub fn keywords_from_column(text: &str) -> Vec<String> {
        text.split(',')
            .map(str::trim)
            .filter(|word| !word.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// True if this memory is eligible for retrieval: active, not deleted,
    /// and (if validity windows are set) currently within them.
    pub fn is_retrievable_now(&self) -> bool {
        !self.deleted && self.state == MemoryState::Active
    }
}

/// What a memory *is*, without what it says: the two fields a read surface
/// resolves per hit (identity-visibility S1, agent-keywords S4).
///
/// A search hit ranks a row and carries neither the title nor the words: they
/// live in the canonical record. Resolving them is a projection, not a record
/// read — the content of a hit is the biggest column the table has and the
/// caller is not going to look at it (post-audit-debts C5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryIdentity {
    pub uuid: String,
    /// The namespace the record lives in. Carried so the read surface can
    /// apply the same boundary it applies to a whole record.
    pub namespace: String,
    pub title: String,
    pub keywords: Vec<String>,
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

/// The whole life of one memory in one answer: its identity, its revision
/// events (oldest first), and its supersede links. Built by the service for
/// `mnexis timeline`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryTimeline {
    pub memory: Memory,
    pub events: Vec<MemoryHistory>,
    pub links: Vec<MemoryLink>,
}

/// A memory link. Maps to `memory_links`. MVP: supersedes only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryLink {
    pub id: Option<i64>,
    pub older_uuid: String,
    pub newer_uuid: String,
    pub link_type: String, // MVP: always "supersedes"
    /// Why the supersede happened. Persisted since migration 003;
    /// pre-migration rows read back as "" (no backfill, S2).
    #[serde(default)]
    pub reason: String,
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
        assert_eq!(m.title, "", "a new memory has no title yet");
        assert_eq!(m.keywords, Vec::<String>::new(), "no words yet");
    }

    #[test]
    fn the_keywords_column_is_comma_separated_and_round_trips() {
        let words = vec![
            "puerto".to_string(),
            "puerto de escucha".to_string(),
            "port".to_string(),
        ];
        assert_eq!(
            Memory::keywords_column(&words),
            "puerto, puerto de escucha, port"
        );
        assert_eq!(
            Memory::keywords_from_column("puerto, puerto de escucha, port"),
            words
        );
        assert_eq!(
            Memory::keywords_from_column(""),
            Vec::<String>::new(),
            "a memory saved before the words existed has none"
        );
        assert_eq!(
            Memory::keywords_from_column("puerto,, port ,"),
            vec!["puerto".to_string(), "port".to_string()],
            "an empty word is not a word"
        );
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
