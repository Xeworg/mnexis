//! TUI integration tests: the read surface (T2), the write forms (T3), and
//! the supersede/delete/restore flows (T4).
//!
//! The headless `App` is driven against real temp profiles through the
//! public `MemoryService` surface; nothing here renders a terminal.

mod cli_wiring;
mod read_surface;
mod write_flows;
mod write_forms;
