//! `mnexis timeline <uuid>` — the life of one memory in one answer.
//!
//! Identity, the revision events in order, and the supersede links in both
//! directions. Read-only, and it follows the same scope authorization as
//! every other read: a memory of a namespace the profile cannot read is
//! NOT_FOUND, not a leak.

use serde_json::json;

use crate::cli::output::CommandOutcome;
use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;

/// `mnexis timeline <uuid>`.
pub fn timeline(profile: &mut Profile, uuid: String) -> Result<CommandOutcome, MnexisError> {
    let timeline = profile.service_ref().timeline(&uuid)?;

    // The links, split by direction. The repo's semantics: `older` is the
    // one that was replaced, `newer` is the one that replaced it. So a
    // memory that is `older` in a link was superseded BY the other side,
    // and a memory that is `newer` SUPERSEDES the other side.
    let mut supersedes = Vec::new();
    let mut superseded_by = Vec::new();
    for link in &timeline.links {
        if link.older_uuid == uuid {
            superseded_by.push(json!({
                "uuid": link.newer_uuid,
                "relation": "was replaced by",
                "reason": link.reason,
                "at": link.created_at,
            }));
        } else {
            supersedes.push(json!({
                "uuid": link.older_uuid,
                "relation": "supersedes",
                "reason": link.reason,
                "at": link.created_at,
            }));
        }
    }

    let events: Vec<_> = timeline
        .events
        .iter()
        // The history table reads newest-first; the life reads oldest-first.
        .rev()
        .map(|e| {
            json!({
                "revision": e.revision,
                "state": e.state.as_str(),
                "memory_type": e.memory_type,
                "content": e.content,
                "changed_by": e.changed_by,
                "change_reason": e.change_reason,
                "at": e.changed_at,
            })
        })
        .collect();

    let json = json!({
        "memory": {
            "uuid": timeline.memory.uuid,
            "title": timeline.memory.title,
            "topic_key": timeline.memory.topic_key,
            "keywords": timeline.memory.keywords,
            "namespace": timeline.memory.namespace,
            "memory_type": timeline.memory.memory_type,
            "state": timeline.memory.state.as_str(),
            "revision": timeline.memory.revision,
            "deleted": timeline.memory.deleted,
            "created_at": timeline.memory.created_at,
            "updated_at": timeline.memory.updated_at,
            "deleted_at": timeline.memory.deleted_at,
        },
        "events": events,
        "supersedes": supersedes,
        "superseded_by": superseded_by,
    });

    let text = render(&timeline.memory, &events, &supersedes, &superseded_by);
    Ok(CommandOutcome::new(json, text))
}

/// Text rendering: the identity, then the events, then the links.
fn render(
    memory: &crate::domain::memory::Memory,
    events: &[serde_json::Value],
    supersedes: &[serde_json::Value],
    superseded_by: &[serde_json::Value],
) -> String {
    let mut lines = vec![format!(
        "{} — {} ({} · {} · revision {})",
        memory.uuid,
        if memory.title.is_empty() {
            "(no title)"
        } else {
            &memory.title
        },
        memory.namespace,
        memory.state.as_str(),
        memory.revision
    )];
    if !events.is_empty() {
        lines.push("events:".into());
        for e in events {
            lines.push(format!(
                "  r{:<4} {:<11} {} — {}",
                e["revision"].as_i64().unwrap_or(0),
                e["state"].as_str().unwrap_or(""),
                e["changed_at"].as_str().unwrap_or(""),
                e["change_reason"].as_str().unwrap_or("")
            ));
        }
    }
    if !supersedes.is_empty() {
        lines.push("supersedes:".into());
        for side in supersedes {
            lines.push(format!("  {}", side["uuid"].as_str().unwrap_or("")));
        }
    }
    if !superseded_by.is_empty() {
        lines.push("superseded by:".into());
        for side in superseded_by {
            lines.push(format!("  {}", side["uuid"].as_str().unwrap_or("")));
        }
    }
    lines.join("\n")
}
