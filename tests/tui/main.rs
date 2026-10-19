//! T2 integration tests for the `mnexis tui` read surface.
//!
//! The headless `App` is driven against real temp profiles through the
//! public `MemoryService` surface; nothing here renders a terminal.

mod cli_wiring;
mod read_surface;
