//! `mnexis config show` — the effective configuration and scope
//! (e2-core §7 "Admin commands"). Prints what the process would run
//! with; never any memory content.

use std::path::PathBuf;

use serde_json::json;

use crate::cli::output::CommandOutcome;
use crate::config::ProfileSettings;
use crate::domain::error::MnexisError;

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
            "data_dir: {}\nconfig: {} ({})\nscope: default={}, read={:?}, write={:?}, global={}, ops={}, disclosure={}\nlimits: content={}B query={}B tags={} topic_key={}B retention={}d idempotency={}d",
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
