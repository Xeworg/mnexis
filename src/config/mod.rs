//! Process configuration (PRD §3, e2-core §6).
//!
//! The profile settings live in `mnexis.toml` inside the data directory.
//! This is a **closed flat schema** (no sections, no nested tables): every
//! key is documented here and produced by [`ProfileSettings::to_toml`].
//! The parser accepts exactly the subset of TOML the writer emits —
//! strings, booleans, integers, and arrays of strings — so the profile
//! file is parsed without an external TOML dependency (the pinned
//! offline registry has none).

pub mod projects;
pub mod settings;

pub use projects::{ProjectBinding, ProjectRegistry};
pub use settings::{ProfileSettings, TomlValue};

use std::path::Path;

use crate::domain::error::MnexisError;

/// Write a file atomically: a sibling temporary file plus a rename, so a
/// crash mid-write cannot leave a half-written profile file behind. The
/// rename is atomic within the same directory on the same filesystem.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), MnexisError> {
    let tmp = path.with_extension(format!(
        "{}tmp",
        path.extension()
            .map(|e| format!("{}.", e.to_string_lossy()))
            .unwrap_or_default()
    ));
    std::fs::write(&tmp, contents).map_err(|e| {
        MnexisError::invalid_argument(path.display().to_string(), format!("cannot write: {e}"))
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        // Best effort: do not leave the temporary file behind.
        let _ = std::fs::remove_file(&tmp);
        MnexisError::invalid_argument(path.display().to_string(), format!("cannot replace: {e}"))
    })
}
