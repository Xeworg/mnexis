//! Output formatting for every CLI result type (e2-core §7 "Output format").
//!
//! Text is the default; `--json` switches to `serde_json::to_string_pretty`.
//! Errors never include memory content: only the stable `error_code` label,
//! the field names, and revision numbers (e2-core §8 "Logging constraints").

use serde_json::{Value, to_string_pretty};

use crate::domain::error::MnexisError;
use crate::domain::memory::{Memory, MemoryHistory};
use crate::retrieval::explain::ExplainedHit;
use crate::retrieval::fts5::SearchHit;

/// Which renderer a command uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Text,
    Json,
}

impl OutputMode {
    pub fn from_flag(json: bool) -> Self {
        if json {
            OutputMode::Json
        } else {
            OutputMode::Text
        }
    }
}

/// A command's dual-format result: the canonical JSON envelope and the
/// human text rendering of the same data.
pub struct CommandOutcome {
    pub json: Value,
    pub text: String,
    /// Process exit code. Defaults to 0.
    pub exit_code: i32,
}

impl CommandOutcome {
    pub fn new(json: Value, text: impl Into<String>) -> Self {
        Self {
            json,
            text: text.into(),
            exit_code: 0,
        }
    }

    /// Set a non-zero exit code (e.g. from a diagnostics failure).
    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    pub fn render(&self, mode: OutputMode) -> String {
        match mode {
            OutputMode::Text => self.text.clone(),
            OutputMode::Json => to_string_pretty(&self.json).unwrap_or_default(),
        }
    }
}

/// Stable error labels (PRD §8 "Errores estables"; e2-core §8).
/// Delegates to [`MnexisError::error_code_label`](crate::domain::error::MnexisError::error_code_label)
/// to keep the canonical mapping in the domain layer.
pub fn error_code_label(err: &MnexisError) -> &'static str {
    err.error_code_label()
}

/// Exit codes 2–10 (e2-core §7 "Exit codes"); 1 is any other failure.
pub fn exit_code(err: &MnexisError) -> i32 {
    match err {
        MnexisError::InvalidArgument { .. } => 2,
        MnexisError::NotFound { .. } => 3,
        MnexisError::Forbidden { .. } => 4,
        MnexisError::RevisionConflict { .. } | MnexisError::TopicConflict { .. } => 5,
        MnexisError::IndexUnavailable { .. } => 6,
        MnexisError::BudgetTooSmall { .. } => 7,
        MnexisError::CursorExpired { .. } => 8,
        MnexisError::UnsupportedVersion { .. } => 9,
        MnexisError::Busy { .. } => 10,
    }
}

/// The stderr rendering for a failed command. JSON mode emits a structured
/// envelope; text mode a single `error[LABEL]` line. Neither ever carries
/// memory content.
pub fn render_error(err: &MnexisError, mode: OutputMode) -> String {
    match mode {
        OutputMode::Text => format!("error[{}]: {err}", error_code_label(err)),
        OutputMode::Json => {
            let envelope = serde_json::json!({
                "error": {
                    "error_code": error_code_label(err),
                    "message": err.to_string(),
                }
            });
            to_string_pretty(&envelope).unwrap_or_default()
        }
    }
}

// --- text formatters ---

const SNIPPET: usize = 60;

pub(crate) fn snippet(content: &str) -> String {
    let mut s: String = content.chars().take(SNIPPET).collect();
    if content.chars().count() > SNIPPET {
        s.push('…');
    }
    s.replace('\n', " ")
}

/// One line per memory, matching the e2-core §7 sample:
/// `[ns] uuid type/state "content" rev=N date`
pub fn memory_line(m: &Memory) -> String {
    format!(
        "  [{}] {} {}/{} \"{}\" rev={} {}",
        m.namespace,
        m.uuid,
        m.memory_type,
        m.state.as_str(),
        snippet(&m.content),
        m.revision,
        m.created_at
    )
}

pub fn memory_block(m: &Memory) -> String {
    format!(
        "uuid:        {}\nnamespace:   {}\ntype:        {}\nstate:       {}\nrevision:    {}\ncreated_at:  {}\nupdated_at:  {}\nvalid_from:  {}\nvalid_until: {}\nbasis:       {}\ncontent:\n  {}",
        m.uuid,
        m.namespace,
        m.memory_type,
        m.state.as_str(),
        m.revision,
        m.created_at,
        m.updated_at,
        m.valid_from.as_deref().unwrap_or("-"),
        m.valid_until.as_deref().unwrap_or("-"),
        m.assertion_basis,
        m.content
    )
}

pub fn history_entry_line(h: &MemoryHistory) -> String {
    format!(
        "  rev {}  {}  by {}  [{}]  reason: {}",
        h.revision,
        h.changed_at,
        h.changed_by,
        h.state.as_str(),
        h.change_reason
    )
}

pub fn hit_line(hit: &SearchHit, position: usize) -> String {
    format!(
        "  #{} rank={:.3} [{}] {} {}/{} rev={}",
        position, hit.rank, hit.namespace, hit.uuid, hit.memory_type, hit.state, hit.revision
    )
}

pub fn explained_hit_line(hit: &ExplainedHit, position: usize) -> String {
    format!(
        "  #{} rank={:.3} [{}] {} {}/{} rev={} terms: {}",
        position,
        hit.rank,
        hit.namespace,
        hit.uuid,
        hit.memory_type,
        hit.state,
        hit.revision,
        hit.matched_terms.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_follow_the_stable_map() {
        assert_eq!(exit_code(&MnexisError::invalid_argument("f", "m")), 2);
        assert_eq!(exit_code(&MnexisError::not_found("r")), 3);
        assert_eq!(exit_code(&MnexisError::forbidden("op", "why")), 4);
        assert_eq!(exit_code(&MnexisError::revision_conflict(3)), 5);
        assert_eq!(exit_code(&MnexisError::topic_conflict("k", "ns")), 5);
        assert_eq!(exit_code(&MnexisError::index_unavailable("r")), 6);
        assert_eq!(exit_code(&MnexisError::budget_too_small(1, 2)), 7);
        assert_eq!(exit_code(&MnexisError::cursor_expired("r")), 8);
        assert_eq!(exit_code(&MnexisError::unsupported_version(3)), 9);
        assert_eq!(exit_code(&MnexisError::busy("db")), 10);
    }

    #[test]
    fn json_error_envelope_has_code_and_message() {
        let err = MnexisError::invalid_argument("state", "invalid state value: bogus");
        let rendered = render_error(&err, OutputMode::Json);
        assert!(rendered.contains("INVALID_ARGUMENT"), "{rendered}");
        // §8 shape: the envelope carries the code and the message.
        assert!(rendered.contains("\"error_code\""));
        assert!(rendered.contains("\"message\""));
        assert!(rendered.contains("invalid state value: bogus"));
    }

    #[test]
    fn text_error_carries_the_label() {
        let err = MnexisError::not_found("memory:uuid-1");
        assert_eq!(
            render_error(&err, OutputMode::Text),
            "error[NOT_FOUND]: not found: memory:uuid-1"
        );
    }

    #[test]
    fn snippet_truncates_long_content() {
        let long = "x".repeat(200);
        let mem = test_memory(&long);
        let line = memory_line(&mem);
        // The snippet sits mid-line; the ellipsis marks the truncation.
        assert!(line.contains('\u{2026}'), "truncated: {line}");
        let content_run = line.split('"').nth(1).expect("quoted content");
        assert_eq!(content_run.chars().count(), 61); // 60 chars + ellipsis
    }

    fn test_memory(content: &str) -> Memory {
        Memory {
            id: Some(1),
            uuid: "u".into(),
            title: String::new(),
            keywords: Vec::new(),
            topic_key: String::new(),
            namespace: "ns".into(),
            memory_type: "fact".into(),
            content: content.into(),
            state: crate::domain::memory::MemoryState::Active,
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
}
