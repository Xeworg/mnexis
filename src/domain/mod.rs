//! E2 pure domain layer: business types and rules.
//!
//! No infrastructure dependencies: no `rmcp`, no `rusqlite`, no `tokio`.
//! All I/O is described by the `MemoryRepository` port; the `storage/`
//! adapter (T16) implements it.

pub mod error;
pub mod idempotency;
pub mod memory;
pub mod memory_type;
pub mod query;
pub mod repository;
pub mod scope;
pub mod session;
pub mod title;
pub mod workspace;

pub use error::MnexisError;
pub use idempotency::{IdempotencyKey, IdempotencyRecord};
pub use memory::{Memory, MemoryHistory, MemoryLink, MemoryState, MemoryTag};
pub use memory_type::{
    DEFAULT_MEMORY_TYPE, MEMORY_TYPES, is_memory_type, resolve_memory_type, validate_memory_type,
};
pub use query::MatchMode;
pub use repository::MemoryRepository;
pub use scope::{AccessScope, ExistenceDisclosure, Operation};
pub use session::{Session, UserPrompt};
pub use title::{MAX_TITLE_BYTES, key_from_title};
pub use workspace::{canonical_workspace_key, normalize_remote_url};
