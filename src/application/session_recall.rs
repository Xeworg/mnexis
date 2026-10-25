//! The recall of one session: what it was, in one answer (session-recall S1).
//!
//! A session is the unit of work: it starts, the user says things in it, and
//! it ends with a summary. The recall answers what it was — the session row,
//! its prompts in order, and the memories of its namespace created inside its
//! window — so the session after it does not start from nothing.
//!
//! **The window is a window, not a link.** `memories` carries no `session_id`
//! and the engine does not know which session is active in a namespace (the
//! MCP server is one process, the CLI another), so it cannot know which
//! session wrote which memory. The recall answers "the memories of the
//! namespace in the session's window" and says exactly that
//! ([`WINDOW_LABEL`]); a memory of another session that overlapped the window
//! is in the list, and that is the honest answer.
//!
//! The module is declared by `service.rs`, which is where the session reads it
//! composes live; its path is `crate::application::service::session_recall`.

use serde::Serialize;

use crate::application::service::MemoryService;
use crate::domain::error::MnexisError;
use crate::domain::memory::Memory;
use crate::domain::repository::MemoryRepository;
use crate::domain::session::{Session, UserPrompt};

/// The words the answer uses for what it can and cannot claim.
///
/// One constant, so the CLI rendering and the MCP payload say the same thing:
/// the engine does not know which session wrote a memory, so it never says a
/// session wrote one.
pub const WINDOW_LABEL: &str = "the memories of the namespace in the session's window";

/// How many prompts and how many memories one recall carries.
///
/// A recall is a session, not an archive: the cap keeps one answer readable,
/// and it is the same ceiling every other list command has.
const RECALL_LIMIT: usize = 100;

/// What a session was: the session itself, what the user said in it, and the
/// memories of its namespace inside its window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionRecall {
    /// The session itself.
    pub session: Session,
    /// What the user said in the session, oldest first.
    pub prompts: Vec<UserPrompt>,
    /// The window the memories were read through, and what they are.
    pub window: RecallWindow,
    /// The memories of the session's namespace created inside the window.
    ///
    /// They are **not** the session's memories: the engine cannot tell which
    /// session wrote which record, and [`RecallWindow::label`] says so.
    pub memories_in_window: Vec<Memory>,
}

/// The window a session opens on its namespace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecallWindow {
    /// The namespace the window belongs to.
    pub namespace: String,
    /// Where the window opens: the session's start (`RFC3339`).
    pub from: String,
    /// Where the window closes: the session's end, or the read's clock when
    /// the session is still open.
    pub to: String,
    /// Whether the session is still open, so `to` is the moment of the read.
    pub open: bool,
    /// What the memories in the window are: [`WINDOW_LABEL`].
    pub label: String,
}

impl<R: MemoryRepository> MemoryService<R> {
    /// The recall of one session, by its id.
    ///
    /// A session of a namespace the profile cannot read is NOT_FOUND, the
    /// same answer every other read gives.
    pub fn recall_session(
        &self,
        session_id: &str,
        now: &str,
    ) -> Result<SessionRecall, MnexisError> {
        let session = self
            .repo
            .get_session(session_id)?
            .ok_or_else(|| MnexisError::not_found(format!("session:{session_id}")))?;
        self.recall_of(session, now)
    }

    /// The recall of the session before this one: the most recent finished
    /// session of `namespace`.
    ///
    /// `namespace` is the namespace the workspace writes into; without one
    /// the profile's default namespace decides, which is where a write
    /// without a namespace lands. A namespace with no finished session is
    /// NOT_FOUND, and the message says there is none: an empty answer would
    /// read as "the previous session did nothing".
    pub fn recall_previous_session(
        &self,
        namespace: Option<&str>,
        now: &str,
    ) -> Result<SessionRecall, MnexisError> {
        let ns = match namespace {
            Some(ns) => ns.to_string(),
            None => self.scope.default_namespace.clone(),
        };
        self.scope.authorize_read(&ns)?;
        let finished = self.repo.previous_finished_session(&ns)?.ok_or_else(|| {
            MnexisError::not_found(format!(
                "previous session in namespace {ns}: no finished session yet"
            ))
        })?;
        self.recall_of(finished, now)
    }

    /// One session, its prompts in order, and the memories of its namespace
    /// created inside its window.
    ///
    /// The session and everything read through it obey the scope: a session
    /// of a namespace the profile cannot read is NOT_FOUND, like every other
    /// read, and the prompts are read through the same namespace filter as
    /// `mnexis prompts`.
    fn recall_of(&self, session: Session, now: &str) -> Result<SessionRecall, MnexisError> {
        self.scope.authorize_read(&session.namespace)?;
        let scope = self.prompt_scope(Some(&session.namespace))?;
        // `list_prompts` reads newest first, like every list; the recall
        // reads as a timeline, so the session's prompts are flipped back into
        // the order they were said.
        let mut prompts = self
            .repo
            .list_prompts(&scope, Some(&session.id), RECALL_LIMIT)?;
        prompts.reverse();
        // The window is a window: the memories are read by namespace and
        // time, never by a session id the engine does not record.
        let to = session.ended_at.clone().unwrap_or_else(|| now.to_string());
        let memories_in_window = self.repo.memories_in_window(
            &session.namespace,
            &session.started_at,
            &to,
            RECALL_LIMIT,
        )?;
        Ok(SessionRecall {
            window: RecallWindow {
                namespace: session.namespace.clone(),
                from: session.started_at.clone(),
                to,
                open: session.ended_at.is_none(),
                label: WINDOW_LABEL.to_string(),
            },
            session,
            prompts,
            memories_in_window,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::service::{SaveRequest, ServiceConfig};
    use crate::domain::memory::MemoryState;
    use crate::domain::scope::{AccessScope, Operation};
    use crate::storage::memory_repo::SqliteMemoryRepository;
    use crate::storage::sqlite::open_sqlite;
    use tempfile::tempdir_in;

    const NS: &str = "payments";

    type Service = MemoryService<SqliteMemoryRepository>;

    fn service() -> (Service, tempfile::TempDir) {
        let dir = tempdir_in("target").expect("tempdir under target/");
        let conn = open_sqlite(&dir.path().join("recall.db")).expect("open sqlite");
        let scope = AccessScope::new(
            vec![NS.to_string()],
            vec![NS.to_string()],
            NS.to_string(),
            false,
            vec![Operation::Save, Operation::List, Operation::Search],
        );
        (
            MemoryService::new(
                SqliteMemoryRepository::new(conn),
                scope,
                ServiceConfig::default(),
            ),
            dir,
        )
    }

    fn save_at(service: &mut Service, uuid: &str, at: &str) {
        let req = SaveRequest {
            uuid: uuid.to_string(),
            title: String::new(),
            topic_key: format!("topic.{uuid}"),
            namespace: NS.to_string(),
            memory_type: "fact".to_string(),
            content: format!("memory {uuid}"),
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
            valid_from: None,
            valid_until: None,
            tags: Vec::new(),
            keywords: Vec::new(),
            replace_keywords: false,
            idempotency_key: None,
            state: MemoryState::Active,
            now: at.to_string(),
        };
        service.save(req).expect("save a memory at a known time");
    }

    fn window_uuids(recall: &SessionRecall) -> Vec<String> {
        recall
            .memories_in_window
            .iter()
            .map(|memory| memory.uuid.clone())
            .collect()
    }

    #[test]
    fn the_window_holds_the_memories_created_inside_it_and_nothing_else() {
        let (mut service, _dir) = service();
        service
            .start_session("S1", Some(NS), "/work", "2026-01-01T10:00:00Z")
            .expect("start");
        service
            .end_session("S1", Some("lo que hizo"), "2026-01-01T11:00:00Z")
            .expect("end");

        // One before the window, one at each edge, one inside, one after the
        // session ended: the edges belong to the window, the rest does not.
        save_at(&mut service, "before", "2026-01-01T09:59:59Z");
        save_at(&mut service, "opening", "2026-01-01T10:00:00Z");
        save_at(&mut service, "inside", "2026-01-01T10:30:00Z");
        save_at(&mut service, "closing", "2026-01-01T11:00:00Z");
        save_at(&mut service, "after", "2026-01-01T11:00:01Z");

        let recall = service
            .recall_session("S1", "2026-01-02T00:00:00Z")
            .expect("recall");

        assert_eq!(
            window_uuids(&recall),
            vec!["opening", "inside", "closing"],
            "the window is from started_at to ended_at, edges included"
        );
        assert_eq!(recall.window.from, "2026-01-01T10:00:00Z");
        assert_eq!(recall.window.to, "2026-01-01T11:00:00Z");
        assert!(!recall.window.open);
        assert_eq!(recall.window.namespace, NS);
        assert_eq!(recall.window.label, WINDOW_LABEL);
    }

    #[test]
    fn an_open_session_reads_its_window_up_to_now() {
        let (mut service, _dir) = service();
        service
            .start_session("S1", Some(NS), "/work", "2026-01-01T10:00:00Z")
            .expect("start");
        save_at(&mut service, "early", "2026-01-01T10:05:00Z");
        save_at(&mut service, "late", "2026-01-01T12:00:00Z");

        let recall = service
            .recall_session("S1", "2026-01-01T11:00:00Z")
            .expect("recall while open");
        assert_eq!(window_uuids(&recall), vec!["early"]);
        assert!(recall.window.open);
        assert_eq!(recall.window.to, "2026-01-01T11:00:00Z");

        let later = service
            .recall_session("S1", "2026-01-01T13:00:00Z")
            .expect("recall later");
        assert_eq!(window_uuids(&later), vec!["early", "late"]);
    }

    #[test]
    fn the_window_is_a_window_and_says_so() {
        // Two sessions overlap. The memory was written while both were open
        // and carries no session id, so both windows hold it — and the answer
        // says it is the namespace in the window, never that a session wrote
        // it.
        let (mut service, _dir) = service();
        service
            .start_session("S1", Some(NS), "/work", "2026-01-01T10:00:00Z")
            .expect("start S1");
        service
            .start_session("S2", Some(NS), "/work", "2026-01-01T10:30:00Z")
            .expect("start S2");
        save_at(&mut service, "shared", "2026-01-01T10:45:00Z");
        service
            .end_session("S2", None, "2026-01-01T10:50:00Z")
            .expect("end S2");

        for id in ["S1", "S2"] {
            let recall = service
                .recall_session(id, "2026-01-01T12:00:00Z")
                .expect("recall");
            assert_eq!(window_uuids(&recall), vec!["shared"], "{id}");
            assert_eq!(recall.window.label, WINDOW_LABEL, "{id}");
        }
    }

    #[test]
    fn the_prompts_are_the_session_ones_in_the_order_they_were_said() {
        let (mut service, _dir) = service();
        service
            .start_session("S1", Some(NS), "/work", "2026-01-01T10:00:00Z")
            .expect("start");
        service
            .save_prompt("primero", Some("S1"), Some(NS), "2026-01-01T10:01:00Z")
            .expect("first prompt");
        service
            .save_prompt("segundo", Some("S1"), Some(NS), "2026-01-01T10:02:00Z")
            .expect("second prompt");
        service
            .save_prompt(
                "de otra sesion",
                Some("S2"),
                Some(NS),
                "2026-01-01T10:03:00Z",
            )
            .expect("prompt of another session");

        let recall = service
            .recall_session("S1", "2026-01-01T12:00:00Z")
            .expect("recall");
        let said: Vec<&str> = recall
            .prompts
            .iter()
            .map(|prompt| prompt.content.as_str())
            .collect();
        assert_eq!(said, vec!["primero", "segundo"]);
    }

    #[test]
    fn the_previous_session_is_the_most_recent_finished_one() {
        let (mut service, _dir) = service();
        service
            .start_session("vieja", Some(NS), "/work", "2026-01-01T10:00:00Z")
            .expect("start old");
        service
            .end_session("vieja", Some("primera"), "2026-01-01T10:30:00Z")
            .expect("end old");
        service
            .start_session("nueva", Some(NS), "/work", "2026-01-01T11:00:00Z")
            .expect("start new");
        service
            .end_session("nueva", Some("segunda"), "2026-01-01T12:00:00Z")
            .expect("end new");
        // A session that has not finished is not the one before this one.
        service
            .start_session("abierta", Some(NS), "/work", "2026-01-01T13:00:00Z")
            .expect("start open");

        let recall = service
            .recall_previous_session(Some(NS), "2026-01-01T14:00:00Z")
            .expect("previous");
        assert_eq!(recall.session.id, "nueva");
        assert_eq!(recall.session.summary.as_deref(), Some("segunda"));
        assert!(!recall.window.open);
    }

    #[test]
    fn no_finished_session_is_not_found_and_says_there_is_none() {
        let (mut service, _dir) = service();
        service
            .start_session("abierta", Some(NS), "/work", "2026-01-01T10:00:00Z")
            .expect("start");

        let err = service
            .recall_previous_session(Some(NS), "2026-01-01T11:00:00Z")
            .expect_err("no finished session is NOT_FOUND");
        assert!(matches!(err, MnexisError::NotFound { .. }), "{err}");
        assert!(err.to_string().contains("no finished session"), "{err}");
    }

    #[test]
    fn an_unknown_session_is_not_found() {
        let (service, _dir) = service();
        let err = service
            .recall_session("NO-EXISTE", "2026-01-01T00:00:00Z")
            .expect_err("an unknown session is NOT_FOUND");
        assert!(matches!(err, MnexisError::NotFound { .. }), "{err}");
    }
}
