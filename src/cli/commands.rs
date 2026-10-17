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
use crate::diagnostics::doctor::purge_eligible_count;
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
        // Dry-run (the default): count eligible records without purging.
        let conn = profile.explain_connection()?;
        let eligible = purge_eligible_count(&conn, before.as_deref(), all)?;
        Ok(CommandOutcome::new(
            json!({
                "dry_run": true,
                "purged": 0,
                "eligible": eligible,
                "before": before,
                "all": all
            }),
            format!(
                "dry-run: {} eligible, 0 purged (before={}, all={}); pass --apply to purge",
                eligible,
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

// --- doctor ---

use crate::diagnostics::doctor::{self, Check, RepairReport};

/// Run every diagnostic check against the profile's database, optionally
/// dropping and rebuilding the FTS5 index. Exit codes follow e2-core §8:
/// 0 = healthy, 6 = INDEX_UNAVAILABLE, 9 = UNSUPPORTED_VERSION, 1 = other.
pub fn doctor(profile: &Profile, repair: bool) -> Result<CommandOutcome, MnexisError> {
    // Doctor reads the on-disk schema state through its own connection.
    // We use the regular `open_sqlite` rather than the profile's service
    // connection so a dropped-and-unrepaired FTS table is correctly
    // reported (the migrations are gated by `applied < N` and never
    // recreate tables that were dropped out from under us).
    let conn = crate::storage::sqlite::open_sqlite(&profile.db_path())?;

    if repair {
        let (repair_report, after_report) = doctor::repair_and_rerun(&conn)?;
        let json = serde_json::json!({
            "checks": after_report.checks,
            "healthy": after_report.healthy,
            "repaired": repair_report,
        });
        let text = render_doctor_text(&after_report.checks, Some(&repair_report));
        let exit_code = doctor::failure_exit_code(&after_report)
            .map(|(c, _)| c)
            .unwrap_or(0);
        Ok(CommandOutcome::new(json, text).with_exit_code(exit_code))
    } else {
        let report = doctor::run_checks(&conn);
        let json = serde_json::json!({
            "checks": report.checks,
            "healthy": report.healthy,
        });
        let text = render_doctor_text(&report.checks, None);
        let exit_code = doctor::failure_exit_code(&report)
            .map(|(c, _)| c)
            .unwrap_or(0);
        Ok(CommandOutcome::new(json, text).with_exit_code(exit_code))
    }
}

/// Human-readable rendering of the doctor report (text mode).
fn render_doctor_text(checks: &[Check], repaired: Option<&RepairReport>) -> String {
    let mut lines: Vec<String> = checks.iter().map(doctor::check_line).collect();
    if let Some(r) = repaired {
        lines.push(format!(
            "  repaired: fts={}, triggers={}, reindexed={}, healthy_after={}",
            r.fts_recreated, r.triggers_recreated, r.reindexed, r.healthy_after
        ));
    }
    let all_ok = checks.iter().all(|c| c.ok);
    lines.push(if all_ok {
        "all checks passed".into()
    } else {
        "one or more checks failed".into()
    });
    lines.join("\n")
}

// --- export / import ---

use crate::application::io::ConflictPolicy;

/// Read all bytes from `--input FILE` (or stdin if absent) into a Vec.
/// 50 MiB cap mirrors the import side; export doesn't enforce the limit
/// (a single profile's snapshot rarely exceeds 50 MiB anyway, and a
/// bounded output is a UX downgrade).
fn read_input(path: Option<&std::path::Path>) -> Result<Vec<u8>, MnexisError> {
    use std::io::Read;
    match path {
        Some(p) => std::fs::read(p).map_err(|e| {
            MnexisError::invalid_argument("input", format!("cannot read {}: {e}", p.display()))
        }),
        None => {
            let mut buf = Vec::new();
            std::io::stdin()
                .read_to_end(&mut buf)
                .map_err(|e| MnexisError::invalid_argument("input", format!("stdin: {e}")))?;
            Ok(buf)
        }
    }
}

fn write_output(path: Option<&std::path::Path>, body: &str) -> Result<(), MnexisError> {
    match path {
        Some(p) => std::fs::write(p, body).map_err(|e| {
            MnexisError::invalid_argument("output", format!("cannot write {}: {e}", p.display()))
        }),
        None => {
            print!("{body}");
            Ok(())
        }
    }
}

/// `mnexis context TASK` — the PRD §9 budgeted context pipeline.
#[allow(clippy::too_many_arguments)]
pub fn context(
    profile: &Profile,
    namespace: Option<String>,
    token_budget: i64,
    tokenizer_id: Option<String>,
    max_items: usize,
    budget_mode: &str,
    task: String,
) -> Result<CommandOutcome, MnexisError> {
    use crate::application::budget::BudgetMode;
    use crate::application::{ContextRequest, SearchFilter};

    let mode = match budget_mode {
        "estimated" => BudgetMode::Estimated,
        "exact" => BudgetMode::Exact,
        other => {
            return Err(MnexisError::invalid_argument(
                "budget-mode",
                format!("unknown mode `{other}` (expected estimated|exact)"),
            ));
        }
    };
    let max_items = max_items.clamp(1, 100);
    let req = ContextRequest {
        task,
        namespace,
        token_budget,
        tokenizer_id,
        max_items: Some(max_items),
        budget_mode: mode,
        search: SearchFilter::default(),
        now: crate::cli::profile::now_rfc3339(),
    };
    let response = profile.service_ref().context(&req)?;
    let json = serde_json::to_value(&response)
        .map_err(|e| MnexisError::invalid_argument("context", format!("serialise: {e}")))?;
    let count = response.items.len();
    Ok(CommandOutcome::new(
        json,
        if count == 0 {
            "no items selected".to_string()
        } else {
            format!("selected {count} items (tokens={})", response.tokens_used)
        },
    ))
}

pub fn export(
    profile: &Profile,
    include_deleted: bool,
    output: Option<std::path::PathBuf>,
) -> Result<CommandOutcome, MnexisError> {
    let memories = profile.service_ref().export(include_deleted)?;
    let mut lines: Vec<String> = Vec::with_capacity(memories.len());
    for m in &memories {
        let record = crate::application::io::ImportRecord::from(m);
        let line = serde_json::to_string(&record)
            .map_err(|e| MnexisError::invalid_argument("export", format!("serialise: {e}")))?;
        lines.push(line);
    }
    let body = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    write_output(output.as_deref(), &body)?;
    Ok(CommandOutcome::new(
        json!({
            "count": memories.len(),
            "include_deleted": include_deleted,
            "output": output.as_ref().map(|p| p.display().to_string()),
        }),
        if memories.is_empty() {
            "no memories to export".to_string()
        } else {
            format!("exported {} memories", memories.len())
        },
    ))
}

pub fn import(
    profile: &mut Profile,
    dry_run: bool,
    policy: &str,
    input: Option<std::path::PathBuf>,
) -> Result<CommandOutcome, MnexisError> {
    let policy = ConflictPolicy::from_str(policy)?;
    let body = read_input(input.as_deref())?;
    let summary = profile.service_mut().import(&body, policy, dry_run)?;
    let conflicts: Vec<serde_json::Value> = summary
        .conflicts
        .iter()
        .map(|c| {
            let kind = match c.kind {
                crate::application::io::Conflict::UuidClash => "uuid_clash",
                crate::application::io::Conflict::TopicKey => "topic_key",
            };
            json!({ "uuid": c.uuid, "kind": kind, "action": c.action })
        })
        .collect();
    let written = if dry_run {
        0
    } else {
        summary.fresh + summary.remapped
    };
    Ok(CommandOutcome::new(
        json!({
            "dry_run": dry_run,
            "policy": summary.policy,
            "records": summary.records,
            "identical": summary.identical,
            "fresh": summary.fresh,
            "skipped": summary.skipped,
            "remapped": summary.remapped,
            "imported": written,
            "would_import": summary.fresh + summary.remapped,
            "conflicts": conflicts,
            "remapped_uuids": summary.remapped_uuids,
        }),
        if dry_run {
            format!(
                "dry-run: would import {} ({} fresh, {} remapped); {} identical, {} skipped",
                summary.fresh + summary.remapped,
                summary.fresh,
                summary.remapped,
                summary.identical,
                summary.skipped
            )
        } else {
            format!(
                "imported {} ({identical} identical, {skipped} skipped, {remapped} remapped) from {records} records",
                written,
                identical = summary.identical,
                skipped = summary.skipped,
                remapped = summary.remapped,
                records = summary.records
            )
        },
    ))
}
