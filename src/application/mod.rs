//! E2 application layer: use-case orchestration. Stateless on top of the
//! `MemoryRepository` port. The service is shared by the CLI and the MCP
//! server; all business rules, validation, and authorization live here.

pub mod budget;
pub mod service;

pub use budget::{Budget, BudgetMode};
pub use service::{
    ListFilter, MemoryService, SaveRequest, ServiceConfig, SupersedeRequest, UpdateRequest,
};
