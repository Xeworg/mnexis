//! E2 application layer: use-case orchestration. Stateless on top of the
//! `MemoryRepository` port. The service is shared by the CLI and the MCP
//! server; all business rules, validation, and authorization live here.

pub mod agents;
pub mod budget;
pub mod context;
pub mod io;
pub mod opencode_config;
pub mod preview;
pub mod service;
pub mod update;
pub mod workspace;
pub mod workspace_status;

/// Convenient alias for the concrete service used by both CLI and MCP.
pub type ApplicationService = MemoryService<crate::storage::memory_repo::SqliteMemoryRepository>;

pub use agents::{
    AgentEnvironment, AgentInstaller, AssetAction, AssetKind, AssetStatus, AssetTarget,
    EnvironmentStatus, FileAction, InstallReport, InstalledAsset, InstalledFile, StatusReport,
};
pub use budget::{Budget, BudgetMode, SUPPORTED_TOKENIZER_V1, token_count_v1};
pub use context::{ContextItem, ContextRequest, ContextResponse, SearchFilter};
pub use io::ConflictPolicy;
pub use opencode_config::{ConfigAction, ConfigOutcome, OpencodeConfig};
pub use preview::{MemoryPreview, previews};
pub use service::{
    ImportSummary, ListFilter, MemoryService, SaveRequest, ServiceConfig, SupersedeRequest,
    UpdateRequest,
};
pub use workspace_status::{
    RecentMemory, StatusScope, StatusWorkspace, WorkspaceStatus, workspace_status,
};
