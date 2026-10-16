//! E2 pure domain layer: business types and rules.
//!
//! No infrastructure dependencies: no `rmcp`, no `rusqlite`, no `tokio`.
//! All I/O is described by the `MemoryRepository` port; the `storage/`
//! adapter (T16) implements it.

pub mod error;
pub mod idempotency;
pub mod memory;
pub mod repository;
pub mod scope;

pub use error::MnexisError;
pub use idempotency::{IdempotencyKey, IdempotencyRecord};
pub use memory::{Memory, MemoryHistory, MemoryLink, MemoryState, MemoryTag};
pub use repository::MemoryRepository;
pub use scope::{AccessScope, ExistenceDisclosure, Operation};
