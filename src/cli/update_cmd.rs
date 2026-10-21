//! `mnexis upgrade` — check whether a newer release exists, and install it.
//!
//! The command is named `upgrade` because `mnexis update` already updates
//! a memory in place. It does not need the profile: what it updates is the
//! binary, not the memories. The version check is cached for a day in the
//! profile directory, so opening the TUI does not spend the network on
//! every start.

use std::path::PathBuf;

use serde_json::json;

use crate::application::update::UpdateService;
use crate::cli::output::CommandOutcome;
use crate::domain::error::MnexisError;

/// `mnexis upgrade [--check] [--version <v>] [--force]`.
pub fn update(
    data_dir: PathBuf,
    check_only: bool,
    version: Option<String>,
    force: bool,
) -> Result<CommandOutcome, MnexisError> {
    let service = UpdateService::new(&data_dir);

    if check_only {
        let status = service.check(force)?;
        let text = match &status.latest_version {
            Some(latest) if status.update_available => format!(
                "mnexis {} is running; {} is published.\nRun `mnexis upgrade` to install it.",
                status.current_version, latest
            ),
            Some(_) => format!(
                "mnexis {} is running; it is the latest published version.",
                status.current_version
            ),
            None => format!(
                "mnexis {} is running; the latest release could not be resolved.",
                status.current_version
            ),
        };
        return Ok(CommandOutcome::new(
            json!({
                "current_version": status.current_version,
                "latest_version": status.latest_version,
                "update_available": status.update_available,
                "checked_at": status.checked_at,
            }),
            text,
        ));
    }

    let outcome = service.install(version.as_deref())?;
    let text = format!(
        "Updated mnexis {} -> {}.\n{}",
        outcome.from_version, outcome.to_version, outcome.note
    );
    Ok(CommandOutcome::new(
        json!({
            "updated": outcome.updated,
            "from_version": outcome.from_version,
            "to_version": outcome.to_version,
            "binary": outcome.binary.display().to_string(),
            "note": outcome.note,
        }),
        text,
    ))
}
