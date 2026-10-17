//! Stdio transport glue for the MCP server (T20).
//!
//! Re-exports the stdio transport from rmcp so `mcp::server` can use
//! `stdio()` without importing from rmcp directly.

pub use rmcp::transport::stdio;
