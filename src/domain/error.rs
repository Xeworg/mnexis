//! The stable error set (PRD §8).
//! Every application-level failure maps to one of these variants.

use thiserror::Error;

/// The application's stable error type. Maps to MCP JSON-RPC code `-32050`
/// (server error) with the specific label carried in `data.error_code`,
/// and to the CLI exit codes 2–10.
///
/// The `-32602` (Invalid params) code is reserved for protocol-level
/// malformed requests and is not represented here.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MnexisError {
    #[error("invalid argument: {field}: {message}")]
    InvalidArgument { field: String, message: String },

    #[error("not found: {resource}")]
    NotFound { resource: String },

    #[error("forbidden: {operation}: {reason}")]
    Forbidden { operation: String, reason: String },

    #[error("revision conflict: current is {current_revision}")]
    RevisionConflict { current_revision: i64 },

    #[error("topic conflict: {topic_key} in {namespace}")]
    TopicConflict {
        topic_key: String,
        namespace: String,
    },

    #[error("busy: {message}")]
    Busy { message: String },

    #[error("index unavailable: {reason}")]
    IndexUnavailable { reason: String },

    /// Units are bytes in E2 (the byte-counting proxy for `budget_enforcement:
    /// estimated`).
    #[error("budget too small: {budget} (minimum envelope {minimum_envelope})")]
    BudgetTooSmall { budget: i64, minimum_envelope: i64 },

    #[error("cursor expired: {reason}")]
    CursorExpired { reason: String },

    #[error("unsupported version: {version}")]
    UnsupportedVersion { version: i64 },
}

impl MnexisError {
    /// The stable error label used in JSON-RPC `data.error_code` (e2-core §8)
    /// and in the CLI JSON error envelope.
    pub fn error_code_label(&self) -> &'static str {
        match self {
            MnexisError::InvalidArgument { .. } => "INVALID_ARGUMENT",
            MnexisError::NotFound { .. } => "NOT_FOUND",
            MnexisError::Forbidden { .. } => "FORBIDDEN",
            MnexisError::RevisionConflict { .. } => "REVISION_CONFLICT",
            MnexisError::TopicConflict { .. } => "TOPIC_CONFLICT",
            MnexisError::Busy { .. } => "BUSY",
            MnexisError::IndexUnavailable { .. } => "INDEX_UNAVAILABLE",
            MnexisError::BudgetTooSmall { .. } => "BUDGET_TOO_SMALL",
            MnexisError::CursorExpired { .. } => "CURSOR_EXPIRED",
            MnexisError::UnsupportedVersion { .. } => "UNSUPPORTED_VERSION",
        }
    }

    pub fn invalid_argument(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            field: field.into(),
            message: message.into(),
        }
    }
    pub fn not_found(resource: impl Into<String>) -> Self {
        Self::NotFound {
            resource: resource.into(),
        }
    }
    pub fn forbidden(operation: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Forbidden {
            operation: operation.into(),
            reason: reason.into(),
        }
    }
    pub fn revision_conflict(current_revision: i64) -> Self {
        Self::RevisionConflict { current_revision }
    }
    pub fn topic_conflict(topic_key: impl Into<String>, namespace: impl Into<String>) -> Self {
        Self::TopicConflict {
            topic_key: topic_key.into(),
            namespace: namespace.into(),
        }
    }
    pub fn busy(message: impl Into<String>) -> Self {
        Self::Busy {
            message: message.into(),
        }
    }
    pub fn index_unavailable(reason: impl Into<String>) -> Self {
        Self::IndexUnavailable {
            reason: reason.into(),
        }
    }
    pub fn budget_too_small(budget: i64, minimum_envelope: i64) -> Self {
        Self::BudgetTooSmall {
            budget,
            minimum_envelope,
        }
    }
    pub fn cursor_expired(reason: impl Into<String>) -> Self {
        Self::CursorExpired {
            reason: reason.into(),
        }
    }
    pub fn unsupported_version(version: i64) -> Self {
        Self::UnsupportedVersion { version }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages_are_stable() {
        assert_eq!(
            MnexisError::invalid_argument("content", "too long").to_string(),
            "invalid argument: content: too long"
        );
        assert_eq!(
            MnexisError::not_found("memory:abc").to_string(),
            "not found: memory:abc"
        );
        assert_eq!(
            MnexisError::RevisionConflict {
                current_revision: 5
            }
            .to_string(),
            "revision conflict: current is 5"
        );
        assert_eq!(
            MnexisError::BudgetTooSmall {
                budget: 100,
                minimum_envelope: 200
            }
            .to_string(),
            "budget too small: 100 (minimum envelope 200)"
        );
    }

    #[test]
    fn constructors_produce_expected_variants() {
        assert!(matches!(
            MnexisError::invalid_argument("f", "m"),
            MnexisError::InvalidArgument { .. }
        ));
        assert!(matches!(
            MnexisError::not_found("r"),
            MnexisError::NotFound { .. }
        ));
        assert!(matches!(
            MnexisError::forbidden("op", "reason"),
            MnexisError::Forbidden { .. }
        ));
        assert!(matches!(
            MnexisError::revision_conflict(3),
            MnexisError::RevisionConflict { .. }
        ));
        assert!(matches!(
            MnexisError::topic_conflict("tk", "ns"),
            MnexisError::TopicConflict { .. }
        ));
        assert!(matches!(MnexisError::busy("db"), MnexisError::Busy { .. }));
        assert!(matches!(
            MnexisError::index_unavailable("fts5"),
            MnexisError::IndexUnavailable { .. }
        ));
        assert!(matches!(
            MnexisError::budget_too_small(0, 50),
            MnexisError::BudgetTooSmall { .. }
        ));
        assert!(matches!(
            MnexisError::cursor_expired("r"),
            MnexisError::CursorExpired { .. }
        ));
        assert!(matches!(
            MnexisError::unsupported_version(99),
            MnexisError::UnsupportedVersion { .. }
        ));
    }

    #[test]
    fn error_is_clone_and_eq() {
        let a = MnexisError::invalid_argument("f", "m");
        let b = a.clone();
        assert_eq!(a, b);
    }
}
