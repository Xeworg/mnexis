//! Headless application state for `mnexis tui`.
//!
//! The model is deliberately terminal-free: integration tests drive it
//! through [`App::apply`] against a real profile while `ui.rs` only reads
//! it. No SQL and no domain rules live here — every read goes through
//! [`MemoryService`], exactly as the CLI commands do.

use crate::application::{ListFilter, MemoryService};
use crate::cli::commands::MAX_LIMIT;
use crate::cli::output::snippet;
use crate::domain::memory::{Memory, MemoryHistory};
use crate::retrieval::SearchHit;
use crate::storage::memory_repo::SqliteMemoryRepository;

/// The concrete service the profile hands to the TUI.
pub type AppService = MemoryService<SqliteMemoryRepository>;

/// Which of the three panes has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Namespaces,
    Memories,
    Detail,
}

impl Pane {
    fn next(self) -> Self {
        match self {
            Pane::Namespaces => Pane::Memories,
            Pane::Memories => Pane::Detail,
            Pane::Detail => Pane::Namespaces,
        }
    }

    fn prev(self) -> Self {
        match self {
            Pane::Namespaces => Pane::Detail,
            Pane::Memories => Pane::Namespaces,
            Pane::Detail => Pane::Memories,
        }
    }
}

/// Normal navigation or the `/` search input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
}

/// One user intent, decoupled from the crossterm key map in `mod.rs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    NextRow,
    PrevRow,
    NextPane,
    PrevPane,
    StartSearch,
    Input(char),
    Backspace,
    Confirm,
    Escape,
    Quit,
    Noop,
}

/// One row of the memories pane (list row or FTS5 hit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryRow {
    pub uuid: String,
    pub memory_type: String,
    pub state: String,
    pub revision: i32,
    /// `Some` for list rows; FTS5 hits do not carry content.
    pub preview: Option<String>,
}

impl From<&Memory> for MemoryRow {
    fn from(m: &Memory) -> Self {
        Self {
            uuid: m.uuid.clone(),
            memory_type: m.memory_type.clone(),
            state: m.state.as_str().to_string(),
            revision: m.revision,
            preview: Some(snippet(&m.content)),
        }
    }
}

impl From<&SearchHit> for MemoryRow {
    fn from(h: &SearchHit) -> Self {
        Self {
            uuid: h.uuid.clone(),
            memory_type: h.memory_type.clone(),
            state: h.state.clone(),
            revision: h.revision,
            preview: None,
        }
    }
}

/// The read surface state. All fields are public so tests can assert on
/// the model without a terminal.
pub struct App {
    pub namespaces: Vec<String>,
    pub ns_index: usize,
    pub memories: Vec<MemoryRow>,
    pub mem_index: usize,
    pub query: String,
    pub mode: Mode,
    pub pane: Pane,
    pub detail: Option<Memory>,
    pub history: Vec<MemoryHistory>,
    pub status: Option<String>,
    quit: bool,
}

impl App {
    /// Build the model from the profile's read scope and load the first
    /// namespace. A scope with no read namespaces is reported, not fatal.
    pub fn new(service: &AppService) -> Self {
        let mut app = Self {
            namespaces: service.scope().read_namespaces.clone(),
            ns_index: 0,
            memories: Vec::new(),
            mem_index: 0,
            query: String::new(),
            mode: Mode::Normal,
            pane: Pane::Namespaces,
            detail: None,
            history: Vec::new(),
            status: None,
            quit: false,
        };
        if app.namespaces.is_empty() {
            app.status = Some("profile scope has no read namespaces".to_string());
        } else {
            app.reload_memories(service);
        }
        app
    }

    pub fn selected_namespace(&self) -> Option<&str> {
        self.namespaces.get(self.ns_index).map(String::as_str)
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Reload the memories pane for the selected namespace: an FTS5
    /// search when a query is set (the same service call as `mnexis
    /// search`), a plain list otherwise. Service errors land in `status`.
    pub fn reload_memories(&mut self, service: &AppService) {
        self.status = None;
        self.mem_index = 0;
        self.detail = None;
        self.history.clear();
        let Some(ns) = self.selected_namespace().map(str::to_string) else {
            self.memories.clear();
            return;
        };
        let loaded = if self.query.is_empty() {
            service
                .list(ListFilter {
                    namespace: Some(ns),
                    state: None,
                    limit: MAX_LIMIT,
                    offset: 0,
                })
                .map(|list| list.iter().map(MemoryRow::from).collect::<Vec<_>>())
                .map_err(|e| format!("list failed: {e}"))
        } else {
            service
                .search(&self.query, Some(&ns), MAX_LIMIT)
                .map(|hits| hits.iter().map(MemoryRow::from).collect::<Vec<_>>())
                .map_err(|e| format!("search failed: {e}"))
        };
        match loaded {
            Ok(rows) => self.memories = rows,
            Err(message) => {
                self.memories.clear();
                self.status = Some(message);
            }
        }
        self.refresh_detail(service);
    }

    /// Fetch the selected memory plus its revision history.
    pub fn refresh_detail(&mut self, service: &AppService) {
        self.detail = None;
        self.history.clear();
        let Some(uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            return;
        };
        match service.get(&uuid) {
            Ok(Some(mem)) => {
                match service.history(&uuid) {
                    Ok(history) => self.history = history,
                    Err(e) => self.status = Some(format!("history failed: {e}")),
                }
                self.detail = Some(mem);
            }
            Ok(None) => self.status = Some(format!("memory {uuid} not found")),
            Err(e) => self.status = Some(format!("detail failed: {e}")),
        }
    }

    /// Apply one intent. Read-only: every service call takes `&self`.
    pub fn apply(&mut self, action: Action, service: &AppService) {
        match action {
            Action::Quit => self.quit = true,
            Action::Noop => {}
            Action::NextRow => self.move_selection(1, service),
            Action::PrevRow => self.move_selection(-1, service),
            Action::NextPane => self.pane = self.pane.next(),
            Action::PrevPane => self.pane = self.pane.prev(),
            Action::StartSearch => {
                self.mode = Mode::Search;
                self.pane = Pane::Memories;
                self.status = None;
            }
            Action::Input(c) if self.mode == Mode::Search => {
                self.query.push(c);
                self.reload_memories(service);
            }
            Action::Backspace if self.mode == Mode::Search => {
                self.query.pop();
                self.reload_memories(service);
            }
            Action::Confirm => self.confirm(service),
            Action::Escape => self.escape(service),
            Action::Input(_) | Action::Backspace => {}
        }
    }

    fn move_selection(&mut self, delta: isize, service: &AppService) {
        match self.pane {
            Pane::Namespaces if !self.namespaces.is_empty() => {
                self.ns_index = shift(self.ns_index, delta, self.namespaces.len());
                self.reload_memories(service);
            }
            Pane::Memories if !self.memories.is_empty() => {
                self.mem_index = shift(self.mem_index, delta, self.memories.len());
                self.refresh_detail(service);
            }
            _ => {}
        }
    }

    fn confirm(&mut self, service: &AppService) {
        if self.mode == Mode::Search {
            self.mode = Mode::Normal;
            self.pane = Pane::Memories;
            return;
        }
        match self.pane {
            Pane::Namespaces => self.pane = Pane::Memories,
            Pane::Memories | Pane::Detail => {
                self.pane = Pane::Detail;
                self.refresh_detail(service);
            }
        }
    }

    fn escape(&mut self, service: &AppService) {
        if self.mode == Mode::Search || !self.query.is_empty() {
            self.mode = Mode::Normal;
            self.query.clear();
            self.pane = Pane::Memories;
            self.reload_memories(service);
            return;
        }
        self.pane = Pane::Memories;
        self.detail = None;
        self.history.clear();
        self.status = None;
    }
}

fn shift(index: usize, delta: isize, len: usize) -> usize {
    let last = len.saturating_sub(1) as isize;
    (index as isize + delta).clamp(0, last) as usize
}
