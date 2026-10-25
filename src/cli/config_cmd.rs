//! `mnexis config` — the effective configuration and scope
//! (e2-core §7 "Admin commands"). Prints what the process would run
//! with; never any memory content. `allow-namespace` authorizes a
//! namespace in the profile, `allow-read-namespace` authorizes reading
//! only; `allow-global` and `allow-read-all` move the two permission
//! flags the human owns.

use std::path::PathBuf;

use serde_json::json;

use crate::cli::output::CommandOutcome;
use crate::config::ProfileSettings;
use crate::domain::error::MnexisError;

/// The two permission flags a profile carries besides the allowlists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// The reserved `global` namespace (read and write).
    Global,
    /// Reads across every namespace.
    ReadAll,
}

impl Permission {
    fn key(self) -> &'static str {
        match self {
            Permission::Global => "allow_global",
            Permission::ReadAll => "allow_read_all",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Permission::Global => "the reserved 'global' namespace",
            Permission::ReadAll => "reads across namespaces",
        }
    }
}

/// `mnexis config allow-global [--revoke]` and
/// `mnexis config allow-read-all [--revoke]` — move a permission flag in
/// the profile (project-isolation S10).
///
/// The change lands in `mnexis.toml`. The profile must already exist
/// (`mnexis init`): the command edits an existing profile instead of
/// creating one, and it never touches the namespace allowlists.
pub fn set_permission(
    data_dir: PathBuf,
    permission: Permission,
    enable: bool,
) -> Result<CommandOutcome, MnexisError> {
    let config_path = data_dir.join("mnexis.toml");
    let src = std::fs::read_to_string(&config_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            MnexisError::not_found(format!(
                "profile at {} (run `mnexis init` first)",
                data_dir.display()
            ))
        } else {
            MnexisError::invalid_argument("mnexis.toml", format!("cannot read: {e}"))
        }
    })?;

    let mut settings = ProfileSettings::from_toml(&src)?;
    let previous = match permission {
        Permission::Global => settings.allow_global,
        Permission::ReadAll => settings.allow_read_all,
    };
    match permission {
        Permission::Global => settings.allow_global = enable,
        Permission::ReadAll => settings.allow_read_all = enable,
    }
    let changed = previous != enable;
    if changed {
        settings.save_to(&config_path)?;
    }

    let verb = if enable { "Allowed" } else { "Revoked" };
    let text = if changed {
        format!("{verb} {} in {}", permission.label(), config_path.display())
    } else {
        format!(
            "{} was already {} in {}",
            permission.label(),
            if enable { "allowed" } else { "revoked" },
            config_path.display()
        )
    };
    Ok(CommandOutcome::new(
        json!({
            "permission": permission.key(),
            "value": enable,
            "changed": changed,
            "config_path": config_path.display().to_string(),
        }),
        text,
    ))
}

/// Reject the namespaces the allowlists cannot carry. Both namespace
/// commands share it so the two spellings refuse the same input the same
/// way.
fn validate_namespace(namespace: &str) -> Result<(), MnexisError> {
    if namespace.trim().is_empty() {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "namespace must not be empty",
        ));
    }
    if namespace.contains('\n') || namespace.contains('\r') {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "namespace must not contain line breaks",
        ));
    }
    // The reserved `global` namespace is governed by its own permission
    // (`allow_global`). Putting it in the namespace allowlists would open
    // it through a door that does not check that permission.
    if namespace.trim() == "global" {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "the reserved 'global' namespace is governed by \
             `mnexis config allow-global`, not by the namespace allowlists",
        ));
    }
    Ok(())
}

/// `mnexis config allow-namespace <ns>` — authorize a namespace in the
/// profile (project-bindings S3).
///
/// The change lands in `mnexis.toml`: the namespace joins the read and
/// write allowlists. The allowlists only grow; nothing is revoked. The
/// profile must already exist (`mnexis init`), because the command edits
/// an existing profile rather than creating one.
pub fn allow_namespace(
    data_dir: PathBuf,
    namespace: String,
) -> Result<CommandOutcome, MnexisError> {
    validate_namespace(&namespace)?;

    let config_path = data_dir.join("mnexis.toml");
    let src = std::fs::read_to_string(&config_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            MnexisError::not_found(format!(
                "profile at {} (run `mnexis init` first)",
                data_dir.display()
            ))
        } else {
            MnexisError::invalid_argument("mnexis.toml", format!("cannot read: {e}"))
        }
    })?;

    let mut settings = ProfileSettings::from_toml(&src)?;
    let changed = settings.allow_namespace(&namespace);
    if changed {
        settings.save_to(&config_path)?;
    }

    let (text, changed_json) = if changed {
        (
            format!(
                "Allowed namespace {namespace} for read and write in {}",
                config_path.display()
            ),
            true,
        )
    } else {
        (
            format!(
                "Namespace {namespace} was already allowed in {}",
                config_path.display()
            ),
            false,
        )
    };
    Ok(CommandOutcome::new(
        json!({
            "namespace": namespace,
            "changed": changed_json,
            "config_path": config_path.display().to_string(),
            "read_namespaces": settings.read_namespaces,
            "write_namespaces": settings.write_namespaces,
        }),
        text,
    ))
}

/// `mnexis config allow-read-namespace <ns>` — authorize a namespace for
/// reading only in the profile (read-granularity S1).
///
/// The change lands in `mnexis.toml`: the namespace joins the read
/// allowlist and the write allowlist is not touched. The allowlists only
/// grow; nothing is revoked. The profile must already exist
/// (`mnexis init`), because the command edits an existing profile rather
/// than creating one.
pub fn allow_read_namespace(
    data_dir: PathBuf,
    namespace: String,
) -> Result<CommandOutcome, MnexisError> {
    validate_namespace(&namespace)?;

    let config_path = data_dir.join("mnexis.toml");
    let src = std::fs::read_to_string(&config_path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            MnexisError::not_found(format!(
                "profile at {} (run `mnexis init` first)",
                data_dir.display()
            ))
        } else {
            MnexisError::invalid_argument("mnexis.toml", format!("cannot read: {e}"))
        }
    })?;

    let mut settings = ProfileSettings::from_toml(&src)?;
    let changed = settings.allow_read_namespace(&namespace);
    if changed {
        settings.save_to(&config_path)?;
    }

    let (text, changed_json) = if changed {
        (
            format!(
                "Allowed namespace {namespace} for reading only in {}",
                config_path.display()
            ),
            true,
        )
    } else {
        (
            format!(
                "Namespace {namespace} was already allowed for reading only in {}",
                config_path.display()
            ),
            false,
        )
    };
    Ok(CommandOutcome::new(
        json!({
            "namespace": namespace,
            "changed": changed_json,
            "read_only": true,
            "config_path": config_path.display().to_string(),
            "read_namespaces": settings.read_namespaces,
            "write_namespaces": settings.write_namespaces,
        }),
        text,
    ))
}

pub fn show(data_dir: PathBuf) -> Result<CommandOutcome, MnexisError> {
    let config_path = data_dir.join("mnexis.toml");
    let (settings, fell_back) = ProfileSettings::load_or_default(&config_path)?;
    let scope = settings.to_access_scope()?;
    let limits = settings.to_service_config();

    let ops: Vec<&str> = if settings.allowed_operations.iter().any(|o| o == "all") {
        vec!["all"]
    } else {
        settings
            .allowed_operations
            .iter()
            .map(String::as_str)
            .collect()
    };

    let disclosure = match scope.existence_disclosure {
        crate::domain::scope::ExistenceDisclosure::NotFound => "not_found",
        crate::domain::scope::ExistenceDisclosure::Forbid => "forbid",
    };

    Ok(CommandOutcome::new(
        json!({
            "data_dir": data_dir.display().to_string(),
            "db_path": data_dir.join("mnexis.db").display().to_string(),
            "config_path": config_path.display().to_string(),
            "config_file_present": !fell_back,
            "scope": {
                "default_namespace": scope.default_namespace,
                "read_namespaces": scope.read_namespaces,
                "write_namespaces": scope.write_namespaces,
                "allow_global": scope.allow_global,
                "allow_read_all": scope.allow_read_all,
                "allowed_operations": ops,
                "existence_disclosure": disclosure,
            },
            "limits": {
                "max_content_bytes": limits.max_content_bytes,
                "max_query_bytes": limits.max_query_bytes,
                "max_tags": limits.max_tags,
                "max_topic_key_bytes": limits.max_topic_key_bytes,
                "default_retention_days": limits.default_retention_days,
                "idempotency_retention_days": limits.idempotency_retention_days,
            },
        }),
        format!(
            "data_dir: {}\nconfig: {} ({})\nscope: default={}, read={:?}, write={:?}, global={}, read_all={}, ops={}, disclosure={}\nlimits: content={}B query={}B tags={} topic_key={}B retention={}d idempotency={}d",
            data_dir.display(),
            config_path.display(),
            if fell_back {
                "defaults (file absent)"
            } else {
                "file"
            },
            scope.default_namespace,
            scope.read_namespaces,
            scope.write_namespaces,
            scope.allow_global,
            scope.allow_read_all,
            ops.join(","),
            disclosure,
            limits.max_content_bytes,
            limits.max_query_bytes,
            limits.max_tags,
            limits.max_topic_key_bytes,
            limits.default_retention_days,
            limits.idempotency_retention_days,
        ),
    ))
}
