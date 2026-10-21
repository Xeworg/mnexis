//! The `mnexis agent` command family: install, uninstall and inspect the
//! agent integration (skill and pi extension) from the binary itself.
//!
//! The assets are embedded in the executable, so an installed `mnexis` can
//! set up pi or opencode without a source checkout.

use serde_json::json;

use crate::application::agents::{
    AgentEnvironment, AgentInstaller, AssetAction, AssetFilter, FileAction, InstallReport, status,
};
use crate::cli::output::CommandOutcome;
use crate::domain::error::MnexisError;

/// Resolve the environment requested on the command line.
fn resolve_environment(requested: Option<&String>) -> Result<AgentEnvironment, MnexisError> {
    match requested {
        None => Ok(AgentEnvironment::Pi),
        Some(value) => AgentEnvironment::parse(value).ok_or_else(|| {
            MnexisError::invalid_argument(
                "env",
                format!(
                    "unknown environment '{}' (supported: {})",
                    value,
                    AgentEnvironment::supported_ids()
                ),
            )
        }),
    }
}

/// Resolve the asset filter requested on the command line.
fn resolve_filter(requested: Option<&String>) -> Result<AssetFilter, MnexisError> {
    match requested {
        None => Ok(AssetFilter::All),
        Some(value) => AssetFilter::parse(value).ok_or_else(|| {
            MnexisError::invalid_argument(
                "only",
                format!(
                    "unknown asset selection '{}' (supported: {})",
                    value,
                    AssetFilter::supported_ids()
                ),
            )
        }),
    }
}

/// An environment that cannot host a kind of asset is an error, not a
/// silent no-op: asking for an extension in opencode must say so.
fn ensure_applicable(
    environment: AgentEnvironment,
    filter: AssetFilter,
) -> Result<(), MnexisError> {
    if filter == AssetFilter::Extension && environment == AgentEnvironment::Opencode {
        return Err(MnexisError::invalid_argument(
            "only",
            "opencode has no extension directory in the mnexis registry; \
             install the skill instead (--only skill)",
        ));
    }
    Ok(())
}

/// Install the integration for an environment.
pub fn install(env: Option<&String>, only: Option<&String>) -> Result<CommandOutcome, MnexisError> {
    let environment = resolve_environment(env)?;
    let filter = resolve_filter(only)?;
    ensure_applicable(environment, filter)?;
    let installer = AgentInstaller::from_env();
    let report = installer.install(environment, filter).map_err(io_error)?;
    Ok(CommandOutcome::new(
        json!({
            "environment": environment.id(),
            "assets": report.assets.iter().map(asset_json).collect::<Vec<_>>(),
            "files": report.files.iter().map(file_json).collect::<Vec<_>>(),
        }),
        render_report(&report, "install"),
    ))
}

/// Remove the integration for an environment.
pub fn uninstall(
    env: Option<&String>,
    only: Option<&String>,
) -> Result<CommandOutcome, MnexisError> {
    let environment = resolve_environment(env)?;
    let filter = resolve_filter(only)?;
    ensure_applicable(environment, filter)?;
    let installer = AgentInstaller::from_env();
    let report = installer.uninstall(environment, filter).map_err(io_error)?;
    Ok(CommandOutcome::new(
        json!({
            "environment": environment.id(),
            "assets": report.assets.iter().map(asset_json).collect::<Vec<_>>(),
            "files": report.files.iter().map(file_json).collect::<Vec<_>>(),
        }),
        render_report(&report, "uninstall"),
    ))
}

/// Show the installation state of every supported environment.
pub fn show() -> Result<CommandOutcome, MnexisError> {
    let installer = AgentInstaller::from_env();
    let report = status(&installer);
    let mut text = String::new();
    for environment in &report.environments {
        text.push_str(&format!("{}\n", environment.environment.display_name()));
        for asset in &environment.assets {
            text.push_str(&format!(
                "  {:<10} {}\n",
                format!("{}:", asset.kind.label()),
                if asset.installed {
                    format!("installed  {}", asset.directory.display())
                } else {
                    format!("not installed  ({})", asset.directory.display())
                }
            ));
        }
    }
    Ok(CommandOutcome::new(
        json!({
            "environments": report.environments.iter().map(|environment| json!({
                "environment": environment.environment.id(),
                "assets": environment.assets.iter().map(|asset| json!({
                    "kind": asset.kind.label(),
                    "name": asset.name,
                    "directory": asset.directory.display().to_string(),
                    "installed": asset.installed,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
        text,
    ))
}

fn asset_json(asset: &crate::application::agents::InstalledAsset) -> serde_json::Value {
    let action = match asset.action {
        AssetAction::Installed => "installed",
        AssetAction::Updated => "updated",
        AssetAction::Removed => "removed",
    };
    json!({
        "kind": asset.kind.label(),
        "name": asset.name,
        "directory": asset.directory.display().to_string(),
        "action": action,
    })
}

fn file_json(file: &crate::application::agents::InstalledFile) -> serde_json::Value {
    let action = match file.action {
        FileAction::Written => "written",
        FileAction::Removed => "removed",
    };
    json!({
        "path": file.path.display().to_string(),
        "action": action,
    })
}

/// Human-readable summary of what an operation did.
fn render_report(report: &InstallReport, action: &str) -> String {
    if report.assets.is_empty() {
        let verb = if action == "uninstall" {
            "Nothing to remove"
        } else {
            "Nothing to install"
        };
        return format!(
            "{} for {}: no matching assets are present.\n",
            verb,
            report.environment.display_name()
        );
    }
    let mut text = String::new();
    for asset in &report.assets {
        let verb = match asset.action {
            AssetAction::Installed => "Installed",
            AssetAction::Updated => "Updated",
            AssetAction::Removed => "Removed",
        };
        text.push_str(&format!(
            "{} {} '{}' in {}\n",
            verb,
            asset.kind.label(),
            asset.name,
            asset.directory.display()
        ));
    }
    for file in &report.files {
        let verb = match file.action {
            FileAction::Written => "wrote",
            FileAction::Removed => "removed",
        };
        text.push_str(&format!("  {} {}\n", verb, file.path.display()));
    }
    text
}

/// Map an IO error into the domain error type.
fn io_error(error: std::io::Error) -> MnexisError {
    MnexisError::invalid_argument("io", format!("could not write the agent assets: {error}"))
}
