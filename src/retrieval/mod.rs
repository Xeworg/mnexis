//! E2 retrieval layer: FTS5 ranked search over the production `memories`
//! canonical table.
//!
//! The retrieval module is the only owner of the SQL that returns ranked
//! rows; the `MemoryRepository` storage port delegates to it, and the
//! application service exposes it through `MemoryService::search` and
//! `MemoryService::reindex`.
//!
//! See `odd/tasks/e2-core.md` §9 for the design:
//! - Plain `fts5(content)` table maintained by triggers (no external-content).
//! - Default filters: `state = 'active'`, `deleted = 0`.
//! - bm25 ordering; lower (more negative) ranks are more relevant.

pub mod explain;
pub mod fts5;
pub mod reindex;

pub use fts5::{DeletedFilter, NamespaceFilter, SearchHit, StateFilter, ValidWindow, search};
