//! TUI integration tests: the read surface (T2) and the write forms (T3).
//!
//! The headless `App` is driven against real temp profiles through the
//! public `MemoryService` surface; nothing here renders a terminal.

mod cli_wiring;
mod read_surface;
mod write_forms;
