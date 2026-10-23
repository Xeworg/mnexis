//! E2 retrieval layer: FTS5 ranked search over the production `memories`
//! canonical table.
//!
//! The retrieval module is the only owner of the SQL that returns ranked
//! rows; the `MemoryRepository` storage port delegates to it, and the
//! application service exposes it through `MemoryService::search` and
//! `MemoryService::reindex`.
//!
//! See `odd/tasks/e2-core.md` §9 for the design:
//! - Plain `fts5(content, title, topic_key, keywords)` table maintained by
//!   triggers (no external-content). The title and the topic key are what makes
//!   the identity of a fact findable (identity-visibility S2), and the keywords
//!   are the vocabulary the agent supplies for it (agent-keywords S3).
//! - Default filters: `state = 'active'`, `deleted = 0`.
//! - bm25 ordering; lower (more negative) ranks are more relevant. The weights
//!   are the FTS5 defaults: `search` and `explain` share the same
//!   `bm25(memories_fts)` expression and must not drift apart.

pub mod explain;
pub mod fts5;
pub mod prompts;
pub mod reindex;

pub use fts5::{DeletedFilter, NamespaceFilter, SearchHit, StateFilter, ValidWindow, search};
pub use prompts::{PromptHit, prompt_from_hit, search_prompts};
