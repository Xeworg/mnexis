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
//! import file, 100 000 records per file. The `memory_type` of every record
//! must be one of the seven known types: the file is refused, by line and
//! uuid, before a single row is written (see [`parse_jsonl`]).
//!
//! Every timestamp the import writes (`created_at`, `updated_at`, and the
//! optional `deleted_at`, `purge_after`, `valid_from`, `valid_until`) must be
//! the one fixed-width UTC shape the engine writes,
//! `YYYY-MM-DDTHH:MM:SSZ`: the reads compare those values as strings, so an
//! offset or a fractional second would sort in the wrong place forever. Such a
//! record is refused by line and uuid, never silently rewritten.
//!
//! The record carries the memory's `title` (`#[serde(default)]`, so files
//! written before the field existed still parse): the title is part of the
//! fact — it is the label a human reads, and the topic key derives from it —
//! so an export that dropped it would lose it on the round trip (PRD §331).
//! The record's keywords are not carried yet: a save normalizes and bounds
//! them, and the importer has no equivalent rule.
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
use std::str::FromStr;

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

impl FromStr for ConflictPolicy {
    type Err = MnexisError;

    fn from_str(s: &str) -> Result<Self, MnexisError> {
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
}

impl ConflictPolicy {
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
    /// The short, stable label of the fact. Defaulted so a file written
    /// before the field existed still parses as a record with no title.
    #[serde(default)]
    pub title: String,
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
            title: m.title.clone(),
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
            title: r.title,
            keywords: Vec::new(),
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
        && a.title == b.title
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
/// - Validates the record's `memory_type` against the closed set, the same
///   way a save does. The type is the one thing an import could otherwise
///   carry verbatim into a column with no `CHECK`, and a value outside the
///   set matches no `--type` filter and no state block: the record would be
///   stored and then never selectable again. So the file is refused, by
///   line and uuid, instead of writing a row nobody can find.
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
        // The type is part of the record's identity on every surface that
        // reads it. `validate_memory_type` owns the set and the wording of
        // the refusal; this adds the one thing it cannot know — which line
        // and which record the value came in on.
        if let Err(error) = crate::domain::memory_type::validate_memory_type(&record.memory_type) {
            return Err(match error {
                MnexisError::InvalidArgument { message, .. } => MnexisError::invalid_argument(
                    "type",
                    format!("import line {} (uuid {}): {message}", idx + 1, record.uuid),
                ),
                other => other,
            });
        }
        // The timestamps the engine writes are one fixed-width UTC shape, and
        // its reads compare them as strings. A value the engine did not write
        // would sort in the wrong place forever, so the file is refused by
        // line and uuid — never silently rewritten.
        for (field, value) in [
            ("created_at", Some(record.created_at.as_str())),
            ("updated_at", Some(record.updated_at.as_str())),
            ("deleted_at", record.deleted_at.as_deref()),
            ("purge_after", record.purge_after.as_deref()),
            ("valid_from", record.valid_from.as_deref()),
            ("valid_until", record.valid_until.as_deref()),
        ] {
            if let Some(value) = value {
                validate_canonical_timestamp(field, value, idx + 1, &record.uuid)?;
            }
        }
        out.push(record);
    }
    Ok(out)
}

/// The one timestamp shape the engine writes, and the one its reads compare.
///
/// `memories_in_window` (recall) and the door's tiebreak compare these values
/// as strings — SQLite stores them as TEXT and nothing here parses them — so
/// the ordering matches the clock only while every value is the same
/// fixed-width UTC shape. `2026-01-01T00:00:00.123Z` and
/// `2026-01-01T00:00:00+02:00` are valid RFC3339 timestamps that sort in the
/// wrong place forever, so an import refuses them instead of rewriting a value
/// the operator can see.
pub const CANONICAL_TIMESTAMP_SHAPE: &str = "YYYY-MM-DDTHH:MM:SSZ";

/// Whether `value` is exactly `YYYY-MM-DDTHH:MM:SSZ` with a plausible calendar
/// and clock part. `2026-13-45T99:99:99Z` is already impossible to have been
/// written by the engine, and accepting it would let a bad record in through
/// the one check that exists to keep the string comparison honest.
fn is_canonical_timestamp(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != CANONICAL_TIMESTAMP_SHAPE.len() {
        return false;
    }
    if bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let slice = value.get(range)?;
        if slice.bytes().all(|b| b.is_ascii_digit()) {
            slice.parse().ok()
        } else {
            None
        }
    };
    matches!(
        (
            digits(0..4),
            digits(5..7),
            digits(8..10),
            digits(11..13),
            digits(14..16),
            digits(17..19),
        ),
        (Some(_), Some(month), Some(day), Some(hour), Some(minute), Some(second))
            if (1..=12).contains(&month)
                && (1..=31).contains(&day)
                && hour < 24
                && minute < 60
                && second < 60
    )
}

/// Refuse one timestamp that is not the shape the engine writes, naming the
/// field, the line, the record and the expected shape.
fn validate_canonical_timestamp(
    field: &str,
    value: &str,
    line: usize,
    uuid: &str,
) -> Result<(), MnexisError> {
    if is_canonical_timestamp(value) {
        return Ok(());
    }
    Err(MnexisError::invalid_argument(
        field,
        format!(
            "import line {line} (uuid {uuid}): `{value}` is not the canonical UTC \
             timestamp shape {CANONICAL_TIMESTAMP_SHAPE} the engine writes and its reads \
             compare; an offset or a fractional second sorts in the wrong place, so the \
             value is refused instead of rewritten"
        ),
    ))
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
        let mut index = Self::default();
        for m in memories {
            index.insert(&m);
        }
        index
    }

    /// Fold a record into the index.
    ///
    /// The index is built from the store and then grows with the records the
    /// run accepts, so the next candidate of the same file is judged against
    /// the store the run is building and not only against the store it started
    /// from: two records claiming one topic key, or one uuid, become conflicts
    /// the policy can answer instead of storage constraint errors nobody
    /// planned for.
    pub fn insert(&mut self, m: &Memory) {
        if !m.topic_key.is_empty() {
            self.topic_keys
                .insert((m.namespace.clone(), m.topic_key.clone()), m.uuid.clone());
        }
        self.by_uuid.insert(m.uuid.clone(), m.clone());
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
                    && occupier != &new
                {
                    return ImportDecision::Skipped(Conflict::TopicKey);
                }
                return ImportDecision::Remapped(Conflict::UuidClash, new);
            }
        }
    }

    // uuid is free: only a topic_key conflict can stop us.
    if !candidate.topic_key.is_empty()
        && let Some(occupier) = local
            .topic_keys
            .get(&(candidate.namespace.clone(), candidate.topic_key.clone()))
        && occupier != &candidate.uuid
    {
        // No policy remaps topic_keys.
        return ImportDecision::Skipped(Conflict::TopicKey);
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
            title: String::new(),
            keywords: Vec::new(),
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
            title: String::new(),
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

    #[test]
    fn parse_jsonl_refuses_a_type_outside_the_closed_set() {
        // A canonical record except for the type: `Decision` is not one of
        // the seven, so no `--type` filter and no state block can ever
        // select the record it would create.
        let body = "{\"uuid\":\"u-mayuscula\",\"topic_key\":\"\",\"namespace\":\"ns\",\
                    \"memory_type\":\"Decision\",\"content\":\"a\",\"state\":\"active\",\
                    \"revision\":1,\"deleted\":false,\
                    \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
                    \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}\n";
        let error = parse_jsonl(body.as_bytes(), 16_384).expect_err("a bad type is refused");
        assert!(
            matches!(&error, MnexisError::InvalidArgument { field, .. } if field == "type"),
            "the refusal is an invalid argument about the type: {error}"
        );
        let message = error.to_string();
        assert!(
            message.contains("import line 1 (uuid u-mayuscula)"),
            "the error names the line and the record: {message}"
        );
        assert!(
            message.contains("unknown memory type 'Decision'"),
            "the error names the value: {message}"
        );
        assert!(
            message.contains(
                "valid types: knowledge, decision, preference, procedure, project_state, \
                 episode, fact"
            ),
            "the error names the valid set: {message}"
        );
    }

    #[test]
    fn parse_jsonl_keeps_a_type_inside_the_closed_set() {
        let body = "{\"uuid\":\"u-decision\",\"topic_key\":\"\",\"namespace\":\"ns\",\
                    \"memory_type\":\"decision\",\"content\":\"a\",\"state\":\"active\",\
                    \"revision\":1,\"deleted\":false,\
                    \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
                    \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}\n";
        let records = parse_jsonl(body.as_bytes(), 16_384).expect("a valid record parses");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].memory_type, "decision");
    }

    #[test]
    fn parse_jsonl_refuses_a_timestamp_that_is_not_the_canonical_shape() {
        // The reads compare these values as strings, so a valid RFC3339 value
        // the engine never writes sorts in the wrong place forever. The field
        // is named, and so is the shape that was expected.
        for (field, value) in [
            ("created_at", "2026-01-01T00:00:00.123Z"),
            ("updated_at", "2026-01-01T00:00:00+02:00"),
            ("valid_from", "2026-01-01"),
        ] {
            let mut fixture =
                serde_json::to_value(record("u-time", "ns", "", "a")).expect("record JSON");
            fixture[field] = serde_json::json!(value);
            let body = format!("{fixture}\n");
            let error = parse_jsonl(body.as_bytes(), 16_384)
                .expect_err("a timestamp the engine cannot compare is refused");
            assert!(
                matches!(&error, MnexisError::InvalidArgument { field: refusal, .. } if refusal == field),
                "{field}: the refusal is about the field that carried the value: {error}"
            );
            let message = error.to_string();
            assert!(
                message.contains("import line 1 (uuid u-time)"),
                "{field}: the error names the line and the record: {message}"
            );
            assert!(
                message.contains(CANONICAL_TIMESTAMP_SHAPE),
                "{field}: the error names the shape it expected: {message}"
            );
        }
    }

    #[test]
    fn a_canonical_timestamp_is_kept_as_it_arrived() {
        // The shape the engine writes round-trips byte for byte: nothing here
        // normalizes or reformats a value.
        assert!(is_canonical_timestamp("2026-01-01T00:00:00Z"));
        assert!(
            !is_canonical_timestamp("2026-13-01T00:00:00Z"),
            "a month no calendar has is not a timestamp the engine wrote"
        );
        assert!(!is_canonical_timestamp("2026-01-01T24:00:00Z"));
        let body = format!(
            "{}\n",
            serde_json::to_string(&record("u-time", "ns", "", "a")).expect("record JSON")
        );
        let records = parse_jsonl(body.as_bytes(), 16_384).expect("a canonical record parses");
        assert_eq!(records[0].created_at, "2026-01-01T00:00:00Z");
    }
}
