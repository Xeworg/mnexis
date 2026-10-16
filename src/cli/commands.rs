//! Command handlers (e2-core §7). Each handler is a thin wrapper over
//! `MemoryService`: it maps CLI flags to a request, calls one or two
//! service methods, and renders the dual-format outcome. No business
//! logic lives here — validation, scopes, and state transitions are the
//! service's job.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde_json::json;

use crate::application::{ListFilter, SaveRequest, SupersedeRequest, UpdateRequest};
use crate::cli::output::CommandOutcome;
use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;
use crate::domain::idempotency::IdempotencyKey;
use crate::domain::memory::MemoryState;
use crate::retrieval::explain;
use crate::retrieval::fts5::{DeletedFilter, NamespaceFilter, StateFilter};

pub const DEFAULT_LIMIT: usize = 20;
pub const MAX_LIMIT: usize = 100;

pub fn clamp_limit(limit: usize) -> usize {
    limit.min(MAX_LIMIT)
}

pub fn parse_state(value: &str) -> Result<MemoryState, MnexisError> {
    MemoryState::from_db(value)
}

fn parse_tags(raw: Option<&String>) -> Vec<String> {
    match raw {
        Some(s) => s
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    }
}

// --- save ---

#[allow(clippy::too_many_arguments)]
pub fn save(
    profile: &mut Profile,
    namespace: Option<String>,
    memory_type: Option<String>,
    basis: Option<String>,
    topic_key: Option<String>,
    idempotency_key: Option<String>,
    valid_from: Option<String>,
    valid_until: Option<String>,
    tags: Option<String>,
    state: Option<String>,
    content: String,
) -> Result<CommandOutcome, MnexisError> {
    let state = parse_state(state.as_deref().unwrap_or("active"))?;
    let uuid = crate::cli::profile::uuid_v4()?;
    let now = crate::cli::profile::now_rfc3339();
    let req = SaveRequest {
        uuid: uuid.clone(),
        topic_key: topic_key.unwrap_or_default(),
        namespace: namespace.unwrap_or_else(|| profile.default_namespace()),
        memory_type: memory_type.unwrap_or_else(|| "fact".to_string()),
        content,
        actor: "user".to_string(),
        channel: "cli".to_string(),
        assertion_basis: basis.unwrap_or_else(|| "user_stated".to_string()),
        valid_from,
        valid_until,
        tags: parse_tags(tags.as_ref()),
        idempotency_key: idempotency_key.map(IdempotencyKey::new),
        state,
        now,
    };
    let uuid_out = req.uuid.clone();
    let row_id = profile.service_mut().save(req)?;
    Ok(CommandOutcome::new(
        json!({ "uuid": uuid_out, "row_id": row_id }),
        format!("saved {uuid_out} (row {row_id})"),
    ))
}

// --- update ---

#[allow(clippy::too_many_arguments)]
pub fn update(
    profile: &mut Profile,
    uuid: String,
    content: Option<String>,
    state: Option<String>,
    tags: Option<String>,
    basis: Option<String>,
    expected_revision: i32,
    reason: String,
) -> Result<CommandOutcome, MnexisError> {
    let now = crate::cli::profile::now_rfc3339();
    let req = UpdateRequest {
        uuid: uuid.clone(),
        expected_revision,
        content,
        state: state.as_deref().map(parse_state).transpose()?,
        tags: Some(parse_tags(tags.as_ref())),
        assertion_basis: basis,
        reason,
        now,
    };
    let updated = profile.service_mut().update(req)?;
    Ok(CommandOutcome::new(
        json!({
            "uuid": updated.uuid,
            "revision": updated.revision,
            "state": updated.state.as_str(),
        }),
        format!(
            "updated {} to revision {} ({})",
            updated.uuid,
            updated.revision,
            updated.state.as_str()
        ),
    ))
}

// --- supersede ---

pub fn supersede(
    profile: &mut Profile,
    older: String,
    newer: String,
    expected_old_revision: Option<i32>,
    expected_new_revision: Option<i32>,
    reason: String,
) -> Result<CommandOutcome, MnexisError> {
    let now = crate::cli::profile::now_rfc3339();
    // Omitted revision flags resolve to the current revisions; the service
    // still enforces the optimistic check against them.
    let old_rev = match expected_old_revision {
        Some(r) => r,
        None => current_revision(profile, &older)?,
    };
    let new_rev = match expected_new_revision {
        Some(r) => r,
        None => current_revision(profile, &newer)?,
    };
    let req = SupersedeRequest {
        older_uuid: older.clone(),
        newer_uuid: newer.clone(),
        expected_old_revision: old_rev,
        expected_new_revision: new_rev,
        reason,
        now,
    };
    profile.service_mut().supersede(req)?;
    Ok(CommandOutcome::new(
        json!({ "older": older, "newer": newer, "ok": true }),
        format!("superseded {older} with {newer}"),
    ))
}

fn current_revision(profile: &Profile, uuid: &str) -> Result<i32, MnexisError> {
    profile
        .service_ref()
        .get(uuid)?
        .map(|m| m.revision)
        .ok_or_else(|| MnexisError::not_found(format!("memory:{uuid}")))
}

// --- search ---

pub fn search(
    profile: &mut Profile,
    query: String,
    namespace: Option<String>,
    limit: usize,
    explain_hits: bool,
) -> Result<CommandOutcome, MnexisError> {
    let limit = clamp_limit(limit);
    // service.search validates the query and authorizes the namespace.
    let hits = profile
        .service_ref()
        .search(&query, namespace.as_deref(), limit)?;

    if explain_hits {
        // Same filters as the repository's search path, on a second
        // connection so the service keeps owning its own.
        let ns_filter = match &namespace {
            Some(ns) => NamespaceFilter::Namespace(ns.clone()),
            None => NamespaceFilter::Any,
        };
        let conn = profile.explain_connection()?;
        let explained = explain::explain(
            &conn,
            &query,
            ns_filter,
            StateFilter::default(),
            DeletedFilter::default(),
            None,
            limit,
        )?;
        let items: Vec<_> = explained
            .iter()
            .map(|h| {
                json!({
                    "uuid": h.uuid,
                    "rank": h.rank,
                    "namespace": h.namespace,
                    "memory_type": h.memory_type,
                    "state": h.state,
                    "revision": h.revision,
                    "created_at": h.created_at,
                    "matched_terms": h.matched_terms,
                })
            })
            .collect();
        let text = explained
            .iter()
            .enumerate()
            .map(|(i, h)| crate::cli::output::explained_hit_line(h, i + 1))
            .collect::<Vec<_>>()
            .join("\n");
        return Ok(CommandOutcome::new(
            json!({
                "query": query,
                "namespace": namespace,
                "count": explained.len(),
                "items": items,
            }),
            if text.is_empty() {
                format!("query `{query}` matched 0 memories")
            } else {
                text
            },
        ));
    }

    let items: Vec<_> = hits
        .iter()
        .map(|h| {
            json!({
                "uuid": h.uuid,
                "rank": h.rank,
                "namespace": h.namespace,
                "memory_type": h.memory_type,
                "state": h.state,
                "revision": h.revision,
                "created_at": h.created_at,
                "valid_from": h.valid_from,
                "valid_until": h.valid_until,
            })
        })
        .collect();
    let text = hits
        .iter()
        .enumerate()
        .map(|(i, h)| crate::cli::output::hit_line(h, i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(CommandOutcome::new(
        json!({
            "query": query,
            "namespace": namespace,
            "count": hits.len(),
            "items": items,
        }),
        if text.is_empty() {
            format!("query `{query}` matched 0 memories")
        } else {
            text
        },
    ))
}

// --- list ---

pub fn list(
    profile: &mut Profile,
    namespace: Option<String>,
    state: Option<String>,
    limit: usize,
    offset: usize,
) -> Result<CommandOutcome, MnexisError> {
    let filter = ListFilter {
        namespace,
        state: state.as_deref().map(parse_state).transpose()?,
        limit: clamp_limit(limit),
        offset,
    };
    let memories = profile.service_ref().list(filter)?;
    let text = memories
        .iter()
        .map(crate::cli::output::memory_line)
        .collect::<Vec<_>>()
        .join("\n");
    Ok(CommandOutcome::new(
        json!({
            "count": memories.len(),
            "items": memories,
        }),
        if text.is_empty() {
            "no memories".to_string()
        } else {
            text
        },
    ))
}

// --- show ---

pub fn show(
    profile: &mut Profile,
    uuid: String,
    revision: Option<i32>,
) -> Result<CommandOutcome, MnexisError> {
    match revision {
        None => {
            let mem = profile
                .service_ref()
                .get(&uuid)?
                .ok_or_else(|| MnexisError::not_found(format!("memory:{uuid}")))?;
            Ok(CommandOutcome::new(
                serde_json::to_value(&mem).unwrap_or_default(),
                crate::cli::output::memory_block(&mem),
            ))
        }
        Some(rev) => {
            let entries = profile.service_ref().history(&uuid)?;
            let entry = entries
                .iter()
                .find(|h| h.revision == rev)
                .ok_or_else(|| MnexisError::not_found(format!("memory:{uuid} revision:{rev}")))?;
            Ok(CommandOutcome::new(
                json!({
                    "uuid": entry.memory_uuid,
                    "revision": entry.revision,
                    "memory_type": entry.memory_type,
                    "state": entry.state.as_str(),
                    "content": entry.content,
                    "assertion_basis": entry.assertion_basis,
                    "valid_from": entry.valid_from,
                    "valid_until": entry.valid_until,
                    "changed_by": entry.changed_by,
                    "change_reason": entry.change_reason,
                    "changed_at": entry.changed_at,
                }),
                crate::cli::output::memory_block(&crate::domain::memory::Memory {
                    id: None,
                    uuid: entry.memory_uuid.clone(),
                    topic_key: String::new(),
                    namespace: String::new(),
                    memory_type: entry.memory_type.clone(),
                    content: entry.content.clone(),
                    state: entry.state,
                    revision: entry.revision,
                    deleted: false,
                    created_at: entry.changed_at.clone(),
                    updated_at: entry.changed_at.clone(),
                    deleted_at: None,
                    purge_after: None,
                    valid_from: entry.valid_from.clone(),
                    valid_until: entry.valid_until.clone(),
                    actor: entry.changed_by.clone(),
                    channel: String::new(),
                    assertion_basis: entry.assertion_basis.clone(),
                }),
            ))
        }
    }
}

// --- history ---

pub fn history(
    profile: &mut Profile,
    uuid: String,
    limit: Option<usize>,
) -> Result<CommandOutcome, MnexisError> {
    let entries = profile.service_ref().history(&uuid)?;
    let shown: Vec<_> = match limit {
        Some(l) => entries.iter().rev().take(l).collect(),
        None => entries.iter().collect(),
    };
    let items: Vec<_> = shown
        .iter()
        .map(|h| {
            json!({
                "revision": h.revision,
                "memory_type": h.memory_type,
                "state": h.state.as_str(),
                "content": h.content,
                "changed_by": h.changed_by,
                "change_reason": h.change_reason,
                "changed_at": h.changed_at,
            })
        })
        .collect();
    let text = shown
        .iter()
        .map(|h| crate::cli::output::history_entry_line(h))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(CommandOutcome::new(
        json!({ "uuid": uuid, "count": items.len(), "items": items }),
        if text.is_empty() {
            format!("no history for {uuid}")
        } else {
            text
        },
    ))
}

// --- delete / restore ---

pub fn delete(
    profile: &mut Profile,
    uuid: String,
    force: bool,
    expected_revision: Option<i32>,
) -> Result<CommandOutcome, MnexisError> {
    let current = profile
        .service_ref()
        .get(&uuid)?
        .ok_or_else(|| MnexisError::not_found(format!("memory:{uuid}")))?;
    if !force && !confirm(&format!("soft-delete {}", uuid))? {
        return Err(MnexisError::invalid_argument("confirm", "aborted"));
    }
    let revision = expected_revision.unwrap_or(current.revision);
    profile.service_mut().soft_delete(&uuid, revision)?;
    Ok(CommandOutcome::new(
        json!({ "deleted": uuid, "revision": revision }),
        format!("soft-deleted {uuid} (was revision {revision})"),
    ))
}

fn confirm(prompt: &str) -> Result<bool, MnexisError> {
    let mut stderr = std::io::stderr();
    write!(stderr, "{prompt}? [y/N] ")
        .and_then(|_| stderr.flush())
        .map_err(|e| MnexisError::invalid_argument("confirm", format!("cannot prompt: {e}")))?;
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| MnexisError::invalid_argument("confirm", format!("cannot read stdin: {e}")))?;
    let answer = line.trim().to_ascii_lowercase();
    Ok(answer == "y" || answer == "yes")
}

pub fn restore(profile: &mut Profile, uuid: String) -> Result<CommandOutcome, MnexisError> {
    profile.service_mut().restore(&uuid)?;
    Ok(CommandOutcome::new(
        json!({ "restored": uuid }),
        format!("restored {uuid}"),
    ))
}

// --- purge ---

pub fn purge(
    profile: &mut Profile,
    apply: bool,
    before: Option<String>,
    all: bool,
) -> Result<CommandOutcome, MnexisError> {
    if apply {
        let purged = profile.service_mut().purge(before.as_deref(), all)?;
        Ok(CommandOutcome::new(
            json!({ "dry_run": false, "purged": purged, "before": before, "all": all }),
            format!("purged {purged} memories"),
        ))
    } else {
        // Dry-run (the default): report the scope, touch nothing. The
        // affected-record count needs a service-level eligibility query
        // that arrives with the diagnostics task (T21).
        Ok(CommandOutcome::new(
            json!({ "dry_run": true, "purged": 0, "before": before, "all": all }),
            format!(
                "dry-run: no records purged (before={}, all={}); pass --apply to purge",
                before.as_deref().unwrap_or("-"),
                all
            ),
        ))
    }
}

// --- reindex ---

pub fn reindex(profile: &mut Profile) -> Result<CommandOutcome, MnexisError> {
    let count = profile.service_mut().reindex()?;
    Ok(CommandOutcome::new(
        json!({ "reindexed": count }),
        format!("reindexed {count} memories"),
    ))
}

// --- init ---

pub fn init(data_dir: PathBuf) -> Result<CommandOutcome, MnexisError> {
    std::fs::create_dir_all(&data_dir).map_err(|e| {
        MnexisError::invalid_argument(
            "data-dir",
            format!("cannot create {}: {e}", data_dir.display()),
        )
    })?;
    let db_path = data_dir.join("mnexis.db");
    let config_path = data_dir.join("mnexis.toml");
    let initialized = !db_path.exists();
    if initialized {
        // open_sqlite creates the file and runs every migration.
        crate::storage::sqlite::open_sqlite(&db_path)?;
    }
    let (settings, _) = crate::config::ProfileSettings::load_or_default(&config_path)?;
    if !config_path.exists() {
        std::fs::write(&config_path, settings.to_toml()).map_err(|e| {
            MnexisError::invalid_argument("mnexis.toml", format!("cannot write: {e}"))
        })?;
    }
    Ok(CommandOutcome::new(
        json!({
            "initialized": initialized,
            "data_dir": data_dir.display().to_string(),
            "db_path": db_path.display().to_string(),
            "config_path": config_path.display().to_string(),
        }),
        if initialized {
            format!("initialized profile at {}", data_dir.display())
        } else {
            format!("profile already initialized at {}", data_dir.display())
        },
    ))
}

// --- version ---

pub fn version() -> CommandOutcome {
    let v = env!("CARGO_PKG_VERSION");
    CommandOutcome::new(
        json!({ "name": "mnexis", "version": v }),
        format!("mnexis {v}"),
    )
}
