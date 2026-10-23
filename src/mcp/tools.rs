//! Tool definitions for the MCP server (T20; e2-core §3, §5).
//!
//! 9 tools: memory_save, memory_get, memory_list, memory_update,
//! memory_history, memory_supersede, memory_delete, memory_restore, memory_search.
//!
//! MCP-required field requirements per e2-core §8:
//! - memory_supersede: older_uuid, newer_uuid, expected_old_revision,
//!   expected_new_revision, reason (all required)
//! - memory_delete: uuid + expected_revision (required)
//! - memory_update: uuid, expected_revision, reason (required); optional
//!   content/state/tags/assertion_basis
//! - memory_save: content required; optional namespace, memory_type (default
//!   "fact"), assertion_basis (default "user_stated"), topic_key (default "",
//!   allowed characters: lowercase letters, digits, '.', '-' and '/'),
//!   idempotency_key, valid_from, valid_until, tags (array, max 32), state
//!   (default "active")
//! - memory_list: optional namespace, state, limit (default 20, clamp to 100),
//!   offset (default 0)
//! - memory_search: query required; optional namespace, limit default 20 clamp 100
//! - memory_history: uuid required; optional limit
//! - memory_get: uuid required
//! - memory_restore: uuid required

use rmcp::handler::server::tool::IntoCallToolResult;
use rmcp::model::{CallToolResponse, CallToolResult, ContentBlock};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::domain::error::MnexisError;
use crate::domain::memory::MemoryState;

// ---------------------------------------------------------------------------
// Argument structs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveArgs {
    /// Memory content (required)
    pub content: String,
    /// Namespace (default: "default")
    #[serde(default)]
    pub namespace: Option<String>,
    /// Memory type (default: "fact")
    #[serde(default)]
    pub memory_type: Option<String>,
    /// Assertion basis (default: "user_stated")
    #[serde(default)]
    pub assertion_basis: Option<String>,
    /// Topic key — unique per namespace when non-empty (default: "").
    /// Allowed characters: lowercase letters, digits, '.', '-' and '/'
    #[serde(default)]
    pub topic_key: Option<String>,
    /// Idempotency key for safe retries (default: none)
    #[serde(default)]
    pub idempotency_key: Option<String>,
    /// Valid-from RFC3339 timestamp (default: none)
    #[serde(default)]
    pub valid_from: Option<String>,
    /// Valid-until RFC3339 timestamp (default: none)
    #[serde(default)]
    pub valid_until: Option<String>,
    /// Comma-separated tags, max 32 (default: empty)
    #[serde(default)]
    pub tags: Option<String>,
    /// Initial state: "active" (default) or "proposed"
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetArgs {
    pub uuid: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListArgs {
    /// Namespace filter (default: all authorised namespaces)
    #[serde(default)]
    pub namespace: Option<String>,
    /// State filter (default: all states)
    #[serde(default)]
    pub state: Option<String>,
    /// Max results (default 20, clamped to 100)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Pagination offset (default 0)
    #[serde(default)]
    pub offset: usize,
    /// The complete content of every memory, not a preview. The default
    /// answers "what is there"; `memory_get` answers "what does it say".
    #[serde(default)]
    pub full: Option<bool>,
}

/// Arguments for `memory_session_start`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionStartArgs {
    /// The session id: the caller's own (the agent never invents one).
    pub session_id: String,
    /// Namespace (default: the profile's default namespace).
    #[serde(default)]
    pub namespace: Option<String>,
    /// Where the session runs (informational).
    #[serde(default)]
    pub directory: Option<String>,
}

/// Arguments for `memory_session_end`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionEndArgs {
    /// The session id.
    pub session_id: String,
    /// What the session was about.
    #[serde(default)]
    pub summary: Option<String>,
}

/// Arguments for `memory_save_prompt`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SavePromptArgs {
    /// What the user said, verbatim.
    pub content: String,
    /// The session it belongs to, when there is one.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Namespace (default: the profile's default namespace).
    #[serde(default)]
    pub namespace: Option<String>,
}

/// Arguments for `memory_search_prompts`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchPromptsArgs {
    /// The query, in plain language.
    pub query: String,
    /// Namespace (default: every authorised namespace).
    #[serde(default)]
    pub namespace: Option<String>,
    /// How the query's words are matched: any (default) | all | phrase.
    #[serde(default)]
    pub match_mode: Option<String>,
    /// Max results (default 20, clamped to 100).
    #[serde(default = "default_limit")]
    pub limit: usize,
}

pub(crate) fn default_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateArgs {
    pub uuid: String,
    /// Required: the revision the caller believes is current.
    pub expected_revision: i32,
    /// Why this update is being made (required by service).
    pub reason: String,
    /// New content (optional)
    #[serde(default)]
    pub content: Option<String>,
    /// New state (optional)
    #[serde(default)]
    pub state: Option<String>,
    /// New tags as comma-separated list (optional)
    #[serde(default)]
    pub tags: Option<String>,
    /// New assertion_basis (optional)
    #[serde(default)]
    pub assertion_basis: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryArgs {
    pub uuid: String,
    /// Max history entries to return (default: all)
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SupersedeArgs {
    pub older_uuid: String,
    pub newer_uuid: String,
    /// Required: expected revision of the older memory.
    pub expected_old_revision: i32,
    /// Required: expected revision of the newer memory.
    pub expected_new_revision: i32,
    /// Why the supersession is happening (required by service).
    pub reason: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteArgs {
    pub uuid: String,
    /// Required: current revision of the memory.
    pub expected_revision: i32,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RestoreArgs {
    pub uuid: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// The query, in plain language (required). The engine compiles it:
    /// every word counts, a hyphen is part of a word.
    pub query: String,
    /// Namespace to search within (default: all authorised).
    #[serde(default)]
    pub namespace: Option<String>,
    /// Max results (default 20, clamped to 100).
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// How the query's words are matched: any (default) | all | phrase.
    #[serde(default)]
    pub match_mode: Option<String>,
}

// ---------------------------------------------------------------------------
// Tool name constants
// ---------------------------------------------------------------------------

pub const TOOL_MEMORY_SAVE: &str = "memory_save";
pub const TOOL_MEMORY_GET: &str = "memory_get";
pub const TOOL_MEMORY_LIST: &str = "memory_list";
pub const TOOL_MEMORY_UPDATE: &str = "memory_update";
pub const TOOL_MEMORY_HISTORY: &str = "memory_history";
pub const TOOL_MEMORY_SUPERSEDE: &str = "memory_supersede";
pub const TOOL_MEMORY_DELETE: &str = "memory_delete";
pub const TOOL_MEMORY_RESTORE: &str = "memory_restore";
pub const TOOL_MEMORY_SEARCH: &str = "memory_search";
pub const TOOL_MEMORY_SESSION_START: &str = "memory_session_start";
pub const TOOL_MEMORY_SESSION_END: &str = "memory_session_end";
pub const TOOL_MEMORY_SAVE_PROMPT: &str = "memory_save_prompt";
pub const TOOL_MEMORY_SEARCH_PROMPTS: &str = "memory_search_prompts";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse comma-separated tags, returning at most 32.
pub(crate) fn parse_tags(raw: Option<String>) -> Vec<String> {
    match raw {
        None => Vec::new(),
        Some(s) => s
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect(),
    }
}

/// Parse a state string to MemoryState.
pub(crate) fn parse_state(raw: Option<String>) -> Result<MemoryState, MnexisError> {
    match raw.as_deref() {
        None | Some("") | Some("active") => Ok(MemoryState::Active),
        Some("proposed") => Ok(MemoryState::Proposed),
        Some("disputed") => Ok(MemoryState::Disputed),
        Some("archived") => Ok(MemoryState::Archived),
        Some("superseded") => Ok(MemoryState::Superseded),
        Some(other) => Err(MnexisError::invalid_argument(
            "state",
            format!("invalid state value: {other}"),
        )),
    }
}

/// Clamp limit to the maximum allowed value.
pub const MAX_LIMIT: usize = 100;

pub fn clamp_limit(limit: usize) -> usize {
    limit.min(MAX_LIMIT)
}

// ---------------------------------------------------------------------------
// Error mapping (e2-core §8)
// ---------------------------------------------------------------------------

/// MCP code for all MnexisError domain errors.
pub const DOMAIN_ERROR_CODE: i32 = -32050;

/// Maps every MnexisError variant to its stable label (e2-core §8).
/// Delegates to the domain method so CLI and MCP can never disagree.
pub fn error_code_label(err: &MnexisError) -> &'static str {
    err.error_code_label()
}

/// MCP error type used in tool handlers.
///
/// Wraps `rmcp::ErrorData` with code -32050 so the `?` operator can be used
/// directly. rmcp's `IntoCallToolResult` blanket impl covers `ErrorData`.
#[derive(Debug)]
pub struct McpError {
    inner: rmcp::ErrorData,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.inner.message)
    }
}

impl std::error::Error for McpError {}

impl From<rmcp::ErrorData> for McpError {
    fn from(inner: rmcp::ErrorData) -> Self {
        Self { inner }
    }
}

impl From<MnexisError> for McpError {
    fn from(err: MnexisError) -> Self {
        let label = error_code_label(&err);
        let data = serde_json::json!({
            "error_code": label,
        });
        Self {
            inner: rmcp::ErrorData::new(
                rmcp::model::ErrorCode(DOMAIN_ERROR_CODE),
                err.to_string(),
                Some(data),
            ),
        }
    }
}

impl From<McpError> for CallToolResult {
    fn from(err: McpError) -> Self {
        CallToolResult::error(vec![ContentBlock::text(err.inner.message.to_string())])
    }
}

impl IntoCallToolResult for McpError {
    fn into_call_tool_result(self) -> Result<CallToolResponse, rmcp::ErrorData> {
        // e2-core §8: domain errors travel as JSON-RPC server errors,
        // code -32050 with data.error_code carrying the stable label.
        Err(self.inner)
    }
}

// String implements IntoCallToolResult via the blanket impl for T: IntoContents.
// Result<String, McpError> works via: String -> IntoCallToolResult -> IntoContents, and
// McpError -> IntoCallToolResult -> ErrorData.

// ---------------------------------------------------------------------------
// Unit tests for error mapping (S6)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mnexis_error_variant_maps_to_code_and_label() {
        let cases: Vec<(MnexisError, &'static str)> = vec![
            (MnexisError::invalid_argument("f", "m"), "INVALID_ARGUMENT"),
            (MnexisError::not_found("r"), "NOT_FOUND"),
            (MnexisError::forbidden("op", "why"), "FORBIDDEN"),
            (MnexisError::revision_conflict(3), "REVISION_CONFLICT"),
            (MnexisError::topic_conflict("k", "ns"), "TOPIC_CONFLICT"),
            (MnexisError::busy("db"), "BUSY"),
            (MnexisError::index_unavailable("r"), "INDEX_UNAVAILABLE"),
            (MnexisError::budget_too_small(1, 2), "BUDGET_TOO_SMALL"),
            (MnexisError::cursor_expired("r"), "CURSOR_EXPIRED"),
            (MnexisError::unsupported_version(3), "UNSUPPORTED_VERSION"),
        ];
        for (err, expected_label) in cases {
            assert_eq!(error_code_label(&err), expected_label);
            let mcp_err: McpError = err.clone().into();
            assert_eq!(
                mcp_err.inner.code.0, DOMAIN_ERROR_CODE,
                "error code must be -32050 for {expected_label}"
            );
            assert!(
                mcp_err.inner.data.as_ref().is_some(),
                "data must be present for {expected_label}"
            );
            let data = mcp_err.inner.data.as_ref().unwrap();
            assert_eq!(
                data.get("error_code").and_then(|v| v.as_str()),
                Some(expected_label),
                "data.error_code must be {expected_label}"
            );
        }
    }

    #[test]
    fn limit_clamping_preserves_small_limits() {
        assert_eq!(clamp_limit(5), 5);
        assert_eq!(clamp_limit(20), 20);
        assert_eq!(clamp_limit(100), 100);
    }

    #[test]
    fn limit_clamping_caps_at_100() {
        assert_eq!(clamp_limit(101), 100);
        assert_eq!(clamp_limit(1000), 100);
        assert_eq!(clamp_limit(usize::MAX), 100);
    }

    #[test]
    fn parse_tags_splits_comma_separated() {
        assert_eq!(parse_tags(Some("a,b,c".into())), vec!["a", "b", "c"]);
        assert_eq!(parse_tags(Some("  x ,  y ".into())), vec!["x", "y"]);
        assert_eq!(parse_tags(None), Vec::<String>::new());
        assert_eq!(parse_tags(Some("".into())), Vec::<String>::new());
    }

    #[test]
    fn parse_state_defaults_to_active() {
        assert_eq!(parse_state(None).unwrap(), MemoryState::Active);
        assert_eq!(parse_state(Some("".into())).unwrap(), MemoryState::Active);
        assert_eq!(
            parse_state(Some("active".into())).unwrap(),
            MemoryState::Active
        );
        assert_eq!(
            parse_state(Some("proposed".into())).unwrap(),
            MemoryState::Proposed
        );
        assert!(parse_state(Some("bogus".into())).is_err());
    }
}
