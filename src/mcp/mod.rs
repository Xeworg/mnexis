//! MCP server module (T20; e2-core §3, §8).
//!
//! Implements the Model Context Protocol over stdio. Wraps
//! [`MemoryService`] to expose the 17 memory_* tools to MCP clients.
//!
//! Module layout (e2-core §3):
//! ```text
//! mcp/
//!   mod.rs         — module root
//!   server.rs      — McpServer + serve_stdio
//!   tools.rs       — tool argument types, error mapping, McpError
//!   transport.rs   — stdio transport re-export
//! ```

pub mod server;
pub mod tools;
pub mod transport;

pub use server::serve_stdio;
