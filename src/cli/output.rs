//! Output formatting for every CLI result type (e2-core §7 "Output format").
//!
//! Text is the default; `--json` switches to `serde_json::to_string_pretty`.
//! Errors never include memory content: only the stable `error_code` label,
//! the field names, and revision numbers (e2-core §8 "Logging constraints").

use serde_json::{Value, to_string_pretty};

use crate::application::{ContextItem, ContextResponse, Coverage, SessionState};
use crate::domain::error::MnexisError;
use crate::domain::memory::{Memory, MemoryHistory};
use crate::domain::scope::ReadScopeMode;
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
    // The same one-line policy as the status renderer: a content with a
    // carriage return must not send the terminal back to column 0.
    one_line(&s)
}

/// One physical line out of any text: the line breaks become spaces.
///
/// A title or a preview can carry a line break (the store accepts them),
/// and a renderer that prints one fact per line must not be broken by a
/// title that has two. The carriage return goes too: on its own it does not
/// end the line, but a terminal reads it as "back to the start of the line"
/// and the rest of the fact overwrites what came before.
///
/// The policy, in full: everything that can end a line becomes a space
/// (LF, CR, NEL, the Unicode line and paragraph separators), and so does
/// every other control character — the escape sequences a terminal would
/// otherwise obey and paint over the line with. A tab becomes a space for
/// the same reason: it is whitespace, and one geometry is easier to read.
/// A zero-width character is left alone: it does not break the line, and
/// dropping it would change what the text says.
pub(crate) fn one_line(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\n' | '\r' | '\u{0085}' | '\u{2028}' | '\u{2029}' | '\t' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect()
}

// --- read scope (read-scope-disclosure S2) ---

/// The one line that says how far a read reached.
///
/// It lives here so `search`, `list` and `context` say the same thing in the
/// same words: a bounded answer that stays silent reads as "there are no
/// memories" when the truth is "you may not read them". The mode is the
/// whole declaration: the line never names a namespace nor counts the ones
/// the profile cannot read.
pub fn read_scope_line(mode: ReadScopeMode) -> String {
    let meaning = match mode {
        ReadScopeMode::All => "every namespace is readable",
        ReadScopeMode::ExceptGlobal => "the reserved namespace is not readable",
        ReadScopeMode::Restricted => "this profile may not read every namespace",
    };
    format!("read scope: {} ({meaning})", mode.as_str())
}

/// Close the human text of a read with the scope line, so a bounded answer
/// never reads like a complete one.
pub fn with_read_scope(text: impl Into<String>, mode: ReadScopeMode) -> String {
    format!("{}\n{}", text.into(), read_scope_line(mode))
}

/// The door's text answer: the items, then the session-state block.
///
/// The CLI's stdout is the machine channel of every agent that has no tool
/// (this extension, opencode, and the codex and claude code integrations
/// still to come), so the text is the answer and not a header: the items
/// with their content, and the block with the live state that fits its
/// budget and the index of the live records that did not fit.
///
/// One physical line per fact, always: the identity before the text, the
/// text folded with [`one_line`].
pub fn context_answer(response: &ContextResponse, mode: ReadScopeMode) -> String {
    let mut lines: Vec<String> = Vec::new();
    // The text says where the door looked, the same way the envelope
    // does: a human reading "no items selected" has to know whether the
    // project has no memories or the search looked elsewhere.
    let searched = if response.searched_namespace.is_empty() {
        "none".to_string()
    } else {
        response.searched_namespace.clone()
    };
    if response.items.is_empty() {
        lines.push(format!("no items selected (namespace={searched})"));
    } else {
        lines.push(format!(
            "selected {} items (tokens={}, namespace={searched})",
            response.items.len(),
            response.tokens_used
        ));
    }
    for (index, item) in response.items.iter().enumerate() {
        lines.push(context_item_line(index + 1, item));
    }
    // What the answer did not include: the agent's cross-check round is a
    // coverage problem, and it needs something to look for.
    lines.extend(coverage_lines(&response.coverage));
    if let Some(state) = &response.session_state {
        // A block with nothing in it is not worth the lines the agent
        // pays for: a project that recorded no state answers exactly what
        // the door always answered.
        if !state.records.is_empty() || !state.index.is_empty() || state.session.is_some() {
            lines.extend(session_state_lines(state));
        }
    }
    with_read_scope(lines.join("\n"), mode)
}

/// One line per item of the door's answer: the identity, then the text.
///
/// The text is the fact whole: the door never cuts one, so the line needs no
/// handle to pull the rest — the declaration in [`coverage_lines`] is where a
/// record the budget could not keep names its uuid.
pub fn context_item_line(position: usize, item: &ContextItem) -> String {
    let mut line = format!("{position}. ");
    if !item.topic_key.is_empty() {
        line.push_str(&format!("[{}] ", item.topic_key));
    }
    if !item.title.is_empty() {
        line.push_str(&format!("{} — ", item.title));
    }
    line.push_str(&one_line(&item.content));
    line
}

/// The declaration of what the answer left out: the candidates the budget
/// could not keep, with the size that explains why.
///
/// Nothing is declared when the answer left nothing out: a declaration that
/// says "nothing was left out" on every answer is noise the agent pays for
/// on every call.
pub fn coverage_lines(coverage: &Coverage) -> Vec<String> {
    if coverage.left_out == 0 && coverage.left_out_items.is_empty() {
        return Vec::new();
    }
    let plural = |count: usize| {
        if count == 1 {
            "candidate"
        } else {
            "candidates"
        }
    };
    let mut lines = vec![format!(
        "not included: {} {} did not fit the budget (the least relevant first)",
        coverage.left_out,
        plural(coverage.left_out)
    )];
    for entry in &coverage.left_out_items {
        lines.push(format!(
            "- [{}] {} — {} ({} bytes)",
            entry.memory_type, entry.title, entry.uuid, entry.bytes
        ));
    }
    lines
}

/// The block's lines: the live records that fit the budget, the index of
/// the ones that did not, and the session before this one.
///
/// The index is what the budget left out, and it is never silent: the
/// agent sees that the state exists and pulls one with `mnexis show <uuid>`.
pub fn session_state_lines(state: &SessionState) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let plural = |count: usize| if count == 1 { "record" } else { "records" };
    lines.push(format!(
        "session state: {} {}, {} of {} tokens",
        state.records.len(),
        plural(state.records.len()),
        state.resident_tokens,
        state.budget
    ));
    for record in &state.records {
        lines.push(format!(
            "- [{}] {} — {}",
            record.memory_type,
            record.title,
            one_line(&record.content)
        ));
    }
    if !state.index.is_empty() {
        lines.push(format!(
            "indexed: {} {} did not fit the budget (pull one with mnexis show <uuid>)",
            state.index.len(),
            plural(state.index.len())
        ));
        for entry in &state.index {
            lines.push(format!(
                "- [{}] {} — {} ({})",
                entry.memory_type, entry.title, entry.uuid, entry.created_at
            ));
        }
    }
    if let Some(session) = &state.session {
        let mut line = format!("session: {}", session.id);
        if let Some(ended) = &session.ended_at {
            line.push_str(&format!(" ended {ended}"));
        }
        if let Some(summary) = &session.summary
            && !summary.is_empty()
        {
            line.push_str(&format!(" — {}", one_line(summary)));
        }
        lines.push(line);
    }
    lines
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
    fn the_read_scope_line_names_the_mode_and_its_meaning() {
        assert_eq!(
            read_scope_line(ReadScopeMode::All),
            "read scope: all (every namespace is readable)"
        );
        assert_eq!(
            read_scope_line(ReadScopeMode::ExceptGlobal),
            "read scope: except_global (the reserved namespace is not readable)"
        );
        assert_eq!(
            read_scope_line(ReadScopeMode::Restricted),
            "read scope: restricted (this profile may not read every namespace)"
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
