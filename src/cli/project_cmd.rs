//! `mnexis project` — the workspace → namespace binding registry
//! (project-bindings S2).
//!
//! The registry records which namespace a workspace writes into. The
//! workspace identity (the key) is computed by the agent: a git remote
//! when the workspace is a repository, a canonical path otherwise. This
//! command never touches memories: unbinding a project leaves the
//! memories exactly where they are.

use std::path::PathBuf;

use crate::application::workspace::{canonical_workspace_key, current_workspace};
use crate::cli::output::CommandOutcome;
use crate::config::projects::{BindOutcome, ProjectRegistry, registry_path};
use crate::domain::error::MnexisError;

/// `mnexis project bind <ns> --key <key> [--path <path>]` — record or
/// update the binding for a workspace key.
pub fn bind(
    data_dir: PathBuf,
    namespace: String,
    key: String,
    path: Option<String>,
    force: bool,
) -> Result<CommandOutcome, MnexisError> {
    let file = registry_path(&data_dir);
    let mut registry = ProjectRegistry::load(&file)?;
    // The key is stored in its canonical form: the human may write the raw
    // remote, the agent always computes the normalized one, and both have
    // to name the same workspace.
    let key = canonical_workspace_key(&key);
    let outcome = registry.bind(&key, &namespace, path.as_deref(), force)?;
    registry.save(&file)?;

    let (text, action) = match outcome {
        BindOutcome::Added => (
            format!("Bound {key} to namespace {namespace} in {}", file.display()),
            "added",
        ),
        BindOutcome::Unchanged => (
            format!(
                "{key} was already bound to namespace {namespace} in {}",
                file.display()
            ),
            "unchanged",
        ),
        BindOutcome::Rebound => (
            format!(
                "Rebound {key} to namespace {namespace} in {}",
                file.display()
            ),
            "rebound",
        ),
    };
    Ok(CommandOutcome::new(
        serde_json::json!({
            "action": action,
            "key": key,
            "namespace": namespace,
            "path": path,
            "registry_path": file.display().to_string(),
        }),
        text,
    ))
}

/// `mnexis project list` — show the recorded bindings.
pub fn list(data_dir: PathBuf) -> Result<CommandOutcome, MnexisError> {
    let file = registry_path(&data_dir);
    let registry = ProjectRegistry::load(&file)?;
    // The bindings are shown in their canonical form, and the canonical key
    // of the current workspace travels with them: an agent that asks the
    // registry for the key gets the engine's answer, not its own guess.
    let canonical: Vec<_> = registry
        .bindings
        .iter()
        .map(|b| crate::config::projects::ProjectBinding {
            key: canonical_workspace_key(&b.key),
            namespace: b.namespace.clone(),
            path: b.path.clone(),
        })
        .collect();
    let current_key = current_workspace(&data_dir)
        .ok()
        .flatten()
        .map(|w| w.key)
        .unwrap_or_default();
    let text = if canonical.is_empty() {
        "No project bindings recorded.".to_string()
    } else {
        canonical
            .iter()
            .map(|b| match &b.path {
                Some(p) => format!("{} -> {} ({})", b.key, b.namespace, p),
                None => format!("{} -> {}", b.key, b.namespace),
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let json = serde_json::json!({
        "bindings": canonical
            .iter()
            .map(|b| serde_json::json!({
                "key": b.key,
                "namespace": b.namespace,
                "path": b.path,
            }))
            .collect::<Vec<_>>(),
        "current_key": current_key,
    });
    Ok(CommandOutcome::new(json, text))
}

/// `mnexis project unbind <ns>` — remove the bindings for a namespace.
pub fn unbind(data_dir: PathBuf, namespace: String) -> Result<CommandOutcome, MnexisError> {
    let file = registry_path(&data_dir);
    let mut registry = ProjectRegistry::load(&file)?;
    let removed = registry.unbind(&namespace);
    if removed {
        registry.save(&file)?;
    }
    let text = if removed {
        format!("Unbound namespace {namespace} in {}", file.display())
    } else {
        format!("No binding for namespace {namespace} in {}", file.display())
    };
    Ok(CommandOutcome::new(
        serde_json::json!({
            "action": if removed { "unbound" } else { "unchanged" },
            "namespace": namespace,
            "registry_path": file.display().to_string(),
        }),
        text,
    ))
}
