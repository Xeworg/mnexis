//! Import / export use cases (PRD §13, e2-core §5/§7).
//!
//! The `MemoryService` owns the use-case surface; this module holds the
//! pure-data helpers (JSONL parsing, conflict classification) so they
//! can be unit-tested in isolation from SQLite.
//!
//! ## Wire format
//!
//! One JSON object per line, the canonical `Memory` shape (the same
//! envelope `serde_json::to_value(&mem)` produces). No manifest line, no
//! version header. Limits from PRD §8: 16 KiB per memory, 50 MiB per
//! import file, 100 000 records per file.
//!
//! ## Conflict resolution
//!
//! For each record, in order:
//! - **identical** (uuid exists locally and the canonical content
//!   matches): no-op.
//! - **uuid collision with different payload** (uuid exists locally and
//!   any canonical field differs): `Conflict::UuidClash`. Policy:
//!   - `reject` (default): error on the first clash, abort the import.
//!   - `skip`: count it, write nothing.
//!   - `remap`: generate a new uuid, treat the new record as fresh.
//! - **topic_key clash** (uuid is free but `(namespace, topic_key)` is
//!   already held by *another* record): `Conflict::TopicKey`. Policy:
//!   the policy applies, but **no policy remaps topic_keys** (the design
//!   prohibits silent rewriting of conceptual keys). On `reject` and
//!   `remap` this is still a hard error.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::domain::error::MnexisError;
use crate::domain::memory::{Memory, MemoryState};

/// Maximum bytes the importer will accept from a single file (PRD §13 + §8).
pub const MAX_IMPORT_BYTES: u64 = 50 * 1024 * 1024;
/// Maximum number of records a single import file may contain.
pub const MAX_IMPORT_RECORDS: usize = 100_000;

/// Conflict resolution policy for `import`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictPolicy {
    /// Abort on the first conflict.
    Reject,
    /// Skip conflicting records and keep going.
    Skip,
    /// Generate a new uuid for the conflicting record (uuid clash only).
    Remap,
}

impl ConflictPolicy {
    pub fn from_str(s: &str) -> Result<Self, MnexisError> {
        match s {
            "reject" => Ok(Self::Reject),
            "skip" => Ok(Self::Skip),
            "remap" => Ok(Self::Remap),
            other => Err(MnexisError::invalid_argument(
                "policy",
                format!("unknown policy `{other}` (expected reject|skip|remap)"),
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reject => "reject",
            Self::Skip => "skip",
            Self::Remap => "remap",
        }
    }
}

/// What kind of conflict a candidate record is presenting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conflict {
    /// uuid already exists locally and the candidate differs in any
    /// canonical field. Remap can heal this.
    UuidClash,
    /// uuid is free, but the `(namespace, topic_key)` pair is held by a
    /// different record. **No policy heals this**: it's a design-level
    /// clash on the conceptual key.
    TopicKey,
}

/// The outcome of importing a single candidate record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportDecision {
    /// Identical to the local record; nothing to do.
    Identical,
    /// New record (no clash, no duplicate). Write it.
    Fresh,
    /// Conflict resolved by skipping the record.
    Skipped(Conflict),
    /// Conflict resolved by remapping the uuid.
    Remapped(Conflict, String),
}

/// One record as it appears in the JSONL stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRecord {
    pub uuid: String,
    pub topic_key: String,
    pub namespace: String,
    pub memory_type: String,
    pub content: String,
    pub state: MemoryState,
    pub revision: i32,
    #[serde(default)]
    pub deleted: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub purge_after: Option<String>,
    #[serde(default)]
    pub valid_from: Option<String>,
    #[serde(default)]
    pub valid_until: Option<String>,
    pub actor: String,
    pub channel: String,
    pub assertion_basis: String,
}

impl From<&Memory> for ImportRecord {
    fn from(m: &Memory) -> Self {
        Self {
            uuid: m.uuid.clone(),
            topic_key: m.topic_key.clone(),
            namespace: m.namespace.clone(),
            memory_type: m.memory_type.clone(),
            content: m.content.clone(),
            state: m.state,
            revision: m.revision,
            deleted: m.deleted,
            created_at: m.created_at.clone(),
            updated_at: m.updated_at.clone(),
            deleted_at: m.deleted_at.clone(),
            purge_after: m.purge_after.clone(),
            valid_from: m.valid_from.clone(),
            valid_until: m.valid_until.clone(),
            actor: m.actor.clone(),
            channel: m.channel.clone(),
            assertion_basis: m.assertion_basis.clone(),
        }
    }
}

impl From<ImportRecord> for Memory {
    fn from(r: ImportRecord) -> Self {
        Memory {
            id: None,
            uuid: r.uuid,
            topic_key: r.topic_key,
            namespace: r.namespace,
            memory_type: r.memory_type,
            content: r.content,
            state: r.state,
            revision: r.revision,
            deleted: r.deleted,
            created_at: r.created_at,
            updated_at: r.updated_at,
            deleted_at: r.deleted_at,
            purge_after: r.purge_after,
            valid_from: r.valid_from,
            valid_until: r.valid_until,
            actor: r.actor,
            channel: r.channel,
            assertion_basis: r.assertion_basis,
        }
    }
}

/// True when every canonical field of two records matches. Used to detect
/// "identical payload, no-op" imports without re-running the full upsert.
pub fn records_canonical_match(a: &Memory, b: &ImportRecord) -> bool {
    a.uuid == b.uuid
        && a.topic_key == b.topic_key
        && a.namespace == b.namespace
        && a.memory_type == b.memory_type
        && a.content == b.content
        && a.state == b.state
        && a.revision == b.revision
        && a.deleted == b.deleted
        && a.created_at == b.created_at
        && a.updated_at == b.updated_at
        && a.deleted_at == b.deleted_at
        && a.purge_after == b.purge_after
        && a.valid_from == b.valid_from
        && a.valid_until == b.valid_until
        && a.actor == b.actor
        && a.channel == b.channel
        && a.assertion_basis == b.assertion_basis
}

/// Parse the JSONL stream from bytes into structured records.
///
/// - Returns each non-empty line as either a record or a parse error with
///   the 1-indexed line number (so the user can locate bad data).
/// - Rejects input that exceeds `MAX_IMPORT_BYTES` or `MAX_IMPORT_RECORDS`.
/// - Validates the per-memory content length against `max_content_bytes`.
pub fn parse_jsonl(
    body: &[u8],
    max_content_bytes: usize,
) -> Result<Vec<ImportRecord>, MnexisError> {
    if body.len() as u64 > MAX_IMPORT_BYTES {
        return Err(MnexisError::invalid_argument(
            "import",
            format!(
                "input exceeds {} MiB limit ({} bytes)",
                MAX_IMPORT_BYTES / 1024 / 1024,
                body.len()
            ),
        ));
    }
    let text = std::str::from_utf8(body)
        .map_err(|e| MnexisError::invalid_argument("import", format!("non-utf8 input: {e}")))?;
    let mut out = Vec::new();
    for (idx, line) in text.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        if out.len() >= MAX_IMPORT_RECORDS {
            return Err(MnexisError::invalid_argument(
                "import",
                format!("exceeds {} records limit", MAX_IMPORT_RECORDS),
            ));
        }
        let record: ImportRecord = serde_json::from_str(line).map_err(|e| {
            MnexisError::invalid_argument("import", format!("line {}: invalid JSON: {e}", idx + 1))
        })?;
        if record.content.len() > max_content_bytes {
            return Err(MnexisError::invalid_argument(
                "import",
                format!(
                    "line {}: content exceeds {} bytes ({} bytes)",
                    idx + 1,
                    max_content_bytes,
                    record.content.len()
                ),
            ));
        }
        out.push(record);
    }
    Ok(out)
}

/// Classify one candidate against the local state. The local state is
/// summarised as the index built once per import call (see
/// [`classify_all`]).
#[derive(Debug, Default, Clone)]
pub struct LocalIndex {
    /// uuid -> canonical Memory (None for non-existent uuids is impossible;
    /// this map only contains uuid-present records).
    pub by_uuid: BTreeMap<String, Memory>,
    /// (namespace, topic_key) -> uuid, only for non-empty topic_keys.
    pub topic_keys: BTreeMap<(String, String), String>,
}

impl LocalIndex {
    pub fn from_memories(memories: Vec<Memory>) -> Self {
        let mut by_uuid = BTreeMap::new();
        let mut topic_keys = BTreeMap::new();
        for m in memories {
            if !m.topic_key.is_empty() {
                topic_keys.insert((m.namespace.clone(), m.topic_key.clone()), m.uuid.clone());
            }
            by_uuid.insert(m.uuid.clone(), m);
        }
        Self {
            by_uuid,
            topic_keys,
        }
    }
}

/// Apply a single candidate to a [`LocalIndex`] and return what to do.
///
/// `new_uuid` is a closure that mints a fresh uuid (only called under
/// `remap`); receiving the closure keeps this module free of IO.
pub fn classify(
    candidate: ImportRecord,
    local: &LocalIndex,
    policy: ConflictPolicy,
    new_uuid: impl FnOnce() -> String,
) -> ImportDecision {
    if let Some(existing) = local.by_uuid.get(&candidate.uuid) {
        // Same uuid: either identical or a clash.
        if records_canonical_match(existing, &candidate) {
            return ImportDecision::Identical;
        }
        match policy {
            ConflictPolicy::Reject => {
                // Surface as a UuidClash; the caller is expected to surface
                // the error on the first one.
                return ImportDecision::Skipped(Conflict::UuidClash);
            }
            ConflictPolicy::Skip => return ImportDecision::Skipped(Conflict::UuidClash),
            ConflictPolicy::Remap => {
                let mut remapped = candidate;
                let new = new_uuid();
                remapped.uuid = new.clone();
                // A remap turns the candidate into a fresh write, but the
                // topic_key still has to clear the index.
                if let Some(occupier) = local
                    .topic_keys
                    .get(&(remapped.namespace.clone(), remapped.topic_key.clone()))
                {
                    if occupier != &new {
                        return ImportDecision::Skipped(Conflict::TopicKey);
                    }
                }
                return ImportDecision::Remapped(Conflict::UuidClash, new);
            }
        }
    }

    // uuid is free: only a topic_key conflict can stop us.
    if !candidate.topic_key.is_empty() {
        if let Some(occupier) = local
            .topic_keys
            .get(&(candidate.namespace.clone(), candidate.topic_key.clone()))
        {
            if occupier != &candidate.uuid {
                // No policy remaps topic_keys.
                return ImportDecision::Skipped(Conflict::TopicKey);
            }
        }
    }
    ImportDecision::Fresh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::memory::MemoryState;

    fn memory(uuid: &str, ns: &str, topic: &str, content: &str) -> Memory {
        Memory {
            id: Some(1),
            uuid: uuid.into(),
            topic_key: topic.into(),
            namespace: ns.into(),
            memory_type: "fact".into(),
            content: content.into(),
            state: MemoryState::Active,
            revision: 1,
            deleted: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            deleted_at: None,
            purge_after: None,
            valid_from: None,
            valid_until: None,
            actor: "user".into(),
            channel: "cli".into(),
            assertion_basis: "user_stated".into(),
        }
    }

    fn record(uuid: &str, ns: &str, topic: &str, content: &str) -> ImportRecord {
        ImportRecord {
            uuid: uuid.into(),
            topic_key: topic.into(),
            namespace: ns.into(),
            memory_type: "fact".into(),
            content: content.into(),
            state: MemoryState::Active,
            revision: 1,
            deleted: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            deleted_at: None,
            purge_after: None,
            valid_from: None,
            valid_until: None,
            actor: "user".into(),
            channel: "cli".into(),
            assertion_basis: "user_stated".into(),
        }
    }

    #[test]
    fn identical_record_returns_identical_decision() {
        let local = LocalIndex::from_memories(vec![memory("u1", "ns", "tk", "same")]);
        let cand = record("u1", "ns", "tk", "same");
        let d = classify(cand, &local, ConflictPolicy::Reject, || "new".into());
        assert_eq!(d, ImportDecision::Identical);
    }

    #[test]
    fn fresh_record_returns_fresh() {
        let local = LocalIndex::from_memories(vec![]);
        let d = classify(
            record("u1", "ns", "tk", "hello"),
            &local,
            ConflictPolicy::Reject,
            || "new".into(),
        );
        assert_eq!(d, ImportDecision::Fresh);
    }

    #[test]
    fn reject_on_uuid_clash_does_not_call_new_uuid() {
        let local = LocalIndex::from_memories(vec![memory("u1", "ns", "tk", "local")]);
        let d = classify(
            record("u1", "ns", "tk", "different"),
            &local,
            ConflictPolicy::Reject,
            || panic!("must not mint on reject"),
        );
        assert_eq!(d, ImportDecision::Skipped(Conflict::UuidClash));
    }

    #[test]
    fn skip_on_uuid_clash() {
        let local = LocalIndex::from_memories(vec![memory("u1", "ns", "tk", "local")]);
        let d = classify(
            record("u1", "ns", "tk", "different"),
            &local,
            ConflictPolicy::Skip,
            || "unused".into(),
        );
        assert_eq!(d, ImportDecision::Skipped(Conflict::UuidClash));
    }

    #[test]
    fn remap_on_uuid_clash_mints_new_uuid_when_topic_key_is_free() {
        // Local has (u1, ns, "", "local"); candidate arrives with same uuid
        // but different content and no topic_key. Remap can mint a new uuid
        // because the topic_key slot is free on the new record.
        let local = LocalIndex::from_memories(vec![memory("u1", "ns", "", "local")]);
        let d = classify(
            record("u1", "ns", "", "different"),
            &local,
            ConflictPolicy::Remap,
            || "fresh-uuid".into(),
        );
        assert_eq!(
            d,
            ImportDecision::Remapped(Conflict::UuidClash, "fresh-uuid".into())
        );
    }

    #[test]
    fn topic_key_clash_is_not_healed_by_remap() {
        // The local holds topic_key "shared" on uuid_a; candidate u_new
        // arrives with the same (ns, shared). uuid_new is free.
        let local = LocalIndex::from_memories(vec![memory("u_a", "ns", "shared", "local")]);
        let d = classify(
            record("u_new", "ns", "shared", "incoming"),
            &local,
            ConflictPolicy::Remap,
            || "should-not-be-called".into(),
        );
        assert_eq!(d, ImportDecision::Skipped(Conflict::TopicKey));
    }

    #[test]
    fn empty_topic_key_never_clashes() {
        let local = LocalIndex::from_memories(vec![memory("u1", "ns", "", "x")]);
        let d = classify(
            record("u2", "ns", "", "y"),
            &local,
            ConflictPolicy::Reject,
            || panic!(),
        );
        assert_eq!(d, ImportDecision::Fresh);
    }

    #[test]
    fn parse_jsonl_rejects_oversized_input() {
        // 1 MiB over the limit
        let body = vec![b'x'; (MAX_IMPORT_BYTES + 1) as usize];
        let err = parse_jsonl(&body, 16_384).unwrap_err();
        assert!(matches!(err, MnexisError::InvalidArgument { .. }));
    }

    #[test]
    fn parse_jsonl_rejects_oversized_content() {
        let body = format!(
            "{{\"uuid\":\"u\",\"content\":\"{}\",\"state\":\"active\",\"revision\":1,\
             \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
             \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n",
            "x".repeat(20)
        );
        let err = parse_jsonl(body.as_bytes(), 16).unwrap_err();
        assert!(matches!(err, MnexisError::InvalidArgument { .. }));
    }

    #[test]
    fn parse_jsonl_reports_bad_line_numbers() {
        // Two lines: line 1 is a complete valid record, line 2 is garbage.
        let body = "{\"uuid\":\"u1\",\"topic_key\":\"\",\"namespace\":\"ns\",\"memory_type\":\"fact\",\
                    \"content\":\"a\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
                    \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
                    \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}\n\
                    not json\n";
        let err = parse_jsonl(body.as_bytes(), 16_384).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("line 2"), "{msg}");
    }
}
