//! Diagnostics module: doctor and repair commands (T21).
//!
//! - [`doctor::run_checks`] runs a suite of SQLite/FTS5 health checks and
//!   returns a structured [`DoctorReport`].
//! - [`doctor::repair_and_rerun`] drops and recreates `memories_fts`, the full
//!   trigger set, rebuilds the index from canonical data, then re-runs every
//!   check to report the post-repair state.

pub mod doctor;
pub mod repair;
