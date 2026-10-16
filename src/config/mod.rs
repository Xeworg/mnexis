//! Process configuration (PRD §3, e2-core §6).
//!
//! The profile settings live in `mnexis.toml` inside the data directory.
//! This is a **closed flat schema** (no sections, no nested tables): every
//! key is documented here and produced by [`ProfileSettings::to_toml`].
//! The parser accepts exactly the subset of TOML the writer emits —
//! strings, booleans, integers, and arrays of strings — so the profile
//! file is parsed without an external TOML dependency (the pinned
//! offline registry has none).

pub mod settings;

pub use settings::{ProfileSettings, TomlValue};
