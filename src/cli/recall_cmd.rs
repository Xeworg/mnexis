//! `mnexis recall` — what a session was, in one answer (session-recall S1).
//!
//! `--session <id>` answers one session; `--previous` answers the session
//! before this one, which is the most recent finished session of the
//! namespace this workspace writes into. `mnexis session` keeps its shape: the
//! recall is a new question, not a new form of the old command.
//!
//! Read-only, and it follows the same scope authorization as every other read:
//! a session of a namespace the profile cannot read is NOT_FOUND.
//!
//! The answer also carries the state of the branch the session was anchored to,
//! validated when it is read (session-recall S3): `merged`, `not_merged` or
//! `unknown`, with the base it compared against and the instant of the check.
//!
//! The JSON answer is priced (extension-recall S1): the identity of every
//! memory in the window and a preview of its content by default, the whole
//! records with `--full` — the shape `mnexis list` has had since #24/#26.
//! The text rendering was priced by construction (a snippet per line) and
//! does not change.

use crate::application::git_state::BranchState;
use crate::application::service::session_recall::SessionRecall;
use crate::cli::output::CommandOutcome;
use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;

/// `mnexis recall --session <id>` / `mnexis recall --previous`.
pub fn recall(
    profile: &Profile,
    session: Option<String>,
    previous: bool,
    full: bool,
) -> Result<CommandOutcome, MnexisError> {
    let now = crate::cli::profile::now_rfc3339();
    let service = profile.service_ref();
    let recall = match (session, previous) {
        (Some(id), _) => service.recall_session(&id, &now)?,
        (None, true) => {
            // The namespace this workspace writes into. A workspace that
            // names none leaves the decision to the profile, exactly as a
            // write without a namespace does.
            let workspace = crate::application::workspace::current_workspace(&profile.data_dir)?;
            let namespace = workspace.and_then(|workspace| workspace.namespace);
            service.recall_previous_session(namespace.as_deref(), &now)?
        }
        (None, false) => {
            return Err(MnexisError::invalid_argument(
                "session",
                "give --session <id> or --previous",
            ));
        }
    };
    Ok(CommandOutcome::new(recall.payload(full), render(&recall)))
}

/// Text rendering: the session, what the user said in it in order, and the
/// memories of the namespace in its window — labelled as exactly that, never
/// as the session's own.
fn render(recall: &SessionRecall) -> String {
    let session = &recall.session;
    let ended = match session.ended_at.as_deref() {
        Some(ended_at) => format!("ended {ended_at}"),
        None => "still open".to_string(),
    };
    let mut lines = vec![format!(
        "session {} — {} (started {}, {ended})",
        session.id, session.namespace, session.started_at
    )];
    lines.push(format!(
        "directory: {}",
        if session.directory.is_empty() {
            "-"
        } else {
            session.directory.as_str()
        }
    ));
    lines.push(format!(
        "summary: {}",
        session.summary.as_deref().unwrap_or("-")
    ));
    lines.push(branch_line(recall.branch_state.as_ref()));

    lines.push("prompts:".to_string());
    if recall.prompts.is_empty() {
        lines.push("  none".to_string());
    }
    for prompt in &recall.prompts {
        lines.push(format!(
            "  #{} {} {}",
            prompt.id, prompt.created_at, prompt.content
        ));
    }

    let window = &recall.window;
    // An open session's window has not closed yet: it runs to the moment of
    // the read, and saying "now" is the honest way to show that.
    let closes = if window.open {
        "now".to_string()
    } else {
        window.to.clone()
    };
    lines.push(format!(
        "{} [{}, {} .. {}]:",
        window.label, window.namespace, window.from, closes
    ));
    if recall.memories_in_window.is_empty() {
        lines.push("  none".to_string());
    }
    for memory in &recall.memories_in_window {
        lines.push(crate::cli::output::memory_line(memory));
    }
    lines.join("\n")
}

/// The state of the anchored branch, in one line.
///
/// The state is derived when the session is read, so the line says which base
/// it compared against and when it checked: without them the verdict would
/// look like a fact about the session, and it is a fact about the repository at
/// that instant. A session that recorded no anchor has nothing to validate,
/// and the line says that too.
fn branch_line(branch_state: Option<&BranchState>) -> String {
    let Some(branch_state) = branch_state else {
        return "branch: none recorded".to_string();
    };
    // No base means no comparison: `-` is how the rest of this rendering says
    // "there is nothing here".
    let base = branch_state.base.as_deref().unwrap_or("-");
    let mut line = format!(
        "branch: {} (base {base}, checked {})",
        branch_state.state, branch_state.checked_at
    );
    if let Some(reason) = branch_state.reason.as_deref() {
        line.push_str(" — ");
        line.push_str(reason);
    }
    line
}
