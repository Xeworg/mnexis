//! `mnexis recall` — what a session was, in one answer (session-recall S1).
//!
//! `--session <id>` answers one session; `--previous` answers the session
//! before this one, which is the most recent finished session of the
//! namespace this workspace writes into. `mnexis session` keeps its shape: the
//! recall is a new question, not a new form of the old command.
//!
//! Read-only, and it follows the same scope authorization as every other read:
//! a session of a namespace the profile cannot read is NOT_FOUND.

use crate::application::service::session_recall::SessionRecall;
use crate::cli::output::CommandOutcome;
use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;

/// `mnexis recall --session <id>` / `mnexis recall --previous`.
pub fn recall(
    profile: &Profile,
    session: Option<String>,
    previous: bool,
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
    Ok(CommandOutcome::new(
        serde_json::to_value(&recall).unwrap_or(serde_json::Value::Null),
        render(&recall),
    ))
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
