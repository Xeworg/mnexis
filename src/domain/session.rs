//! The session the agent runs in, and what the user said.
//!
//! A **session** is the unit of work: the caller names it (the agent never
//! invents one), it belongs to a namespace, it starts, and it ends with a
//! summary. The engine never guesses that a session is over.
//!
//! A **user prompt** is what the user said. It is stored, searchable, and
//! attached to the session it happened in when there is one. The
//! conversation is the one thing a repository cannot hold.

use serde::Serialize;

/// A session of work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Session {
    /// The caller's identifier for the session. The agent never invents one.
    pub id: String,
    /// The namespace the session belongs to.
    pub namespace: String,
    /// Where the session ran (informational).
    pub directory: String,
    /// When the session started (`RFC3339`).
    pub started_at: String,
    /// When the session ended, when it did.
    pub ended_at: Option<String>,
    /// What the session was about, when it was told.
    pub summary: Option<String>,
    /// The branch the session ran on, when it ran in a repository.
    pub branch: Option<String>,
    /// The commit the session started from, when it ran in a repository.
    pub branch_sha: Option<String>,
    /// The canonical key of the repository the session ran in
    /// (`git:<normalized>`) — never the raw URL, which can carry
    /// credentials.
    pub branch_remote: Option<String>,
}

/// One thing the user said.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserPrompt {
    /// The prompt's row id.
    pub id: i64,
    /// The session the prompt belongs to, when there is one.
    pub session_id: Option<String>,
    /// The namespace the prompt lives in.
    pub namespace: String,
    /// What the user said, verbatim.
    pub content: String,
    /// When the prompt was captured (`RFC3339`).
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_carries_its_boundaries() {
        let session = Session {
            id: "01JX".to_string(),
            namespace: "payments".to_string(),
            directory: "/work/project".to_string(),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            ended_at: Some("2026-01-01T01:00:00Z".to_string()),
            summary: Some("what the session did".to_string()),
            branch: Some("main".to_string()),
            branch_sha: Some("0123456789abcdef".to_string()),
            branch_remote: Some("git:github.com-xeworg-mnexis".to_string()),
        };
        assert_eq!(session.id, "01JX");
        assert!(session.ended_at.is_some());
        assert_eq!(session.summary.as_deref(), Some("what the session did"));
        assert_eq!(session.branch.as_deref(), Some("main"));
        assert_eq!(
            session.branch_remote.as_deref(),
            Some("git:github.com-xeworg-mnexis")
        );
    }

    #[test]
    fn a_prompt_may_have_no_session() {
        let prompt = UserPrompt {
            id: 1,
            session_id: None,
            namespace: "payments".to_string(),
            content: "el puerto es 8081".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(prompt.session_id, None);
        assert_eq!(prompt.content, "el puerto es 8081");
    }
}
