//! Headless application state for `mnexis tui`.
//!
//! The model is deliberately terminal-free: integration tests drive it
//! through [`App::apply`] against a real profile while `ui.rs` only reads
//! it. No SQL and no domain rules live here — every read and write goes
//! through [`MemoryService`], exactly as the CLI commands do. Write forms
//! live in [`crate::tui::form`]; this module owns which form (or dialog) is
//! open and what happens to the read surface after a write lands.

use crate::application::{ListFilter, MemoryService};
use crate::cli::commands::MAX_LIMIT;
use crate::cli::output::snippet;
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink};
use crate::retrieval::SearchHit;
use crate::storage::memory_repo::SqliteMemoryRepository;
use crate::tui::form::{Form, WriteOutcome};

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

/// Normal navigation, the `/` search input, an open write form, the
/// newer-memory picker, or a y/N confirmation dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Form,
    /// Choosing the newer memory for an open supersede (`s`).
    Pick,
    /// A y/N dialog is open; nothing is written until it is answered.
    Confirm,
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
    /// Open the save form (`n`).
    StartSave,
    /// Open the update form for the selected memory (`e`).
    StartUpdate,
    /// Move the focused form field down (`Tab` / `Down`).
    NextField,
    /// Move the focused form field up (`Shift-Tab` / `Up`).
    PrevField,
    /// Submit the open form from any field (`Ctrl-S`).
    Submit,
    /// Re-read the update target after a revision conflict (`Ctrl-R`).
    ReloadForm,
    /// Open the supersede picker for the selected memory (`s`).
    StartSupersede,
    /// Open the soft-delete confirmation (`d`).
    StartDelete,
    /// Open the restore confirmation (`r`).
    StartRestore,
    /// Toggle between the live list and the soft-deleted view (`x`).
    ToggleDeleted,
    /// Answer a confirmation dialog with `y`.
    ConfirmYes,
    /// Answer a confirmation dialog with `n` (or Esc).
    ConfirmNo,
}

/// The action a confirmation dialog will perform when answered `y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    /// Soft-delete `uuid`, which was at `revision` when the dialog opened.
    Delete { uuid: String, revision: i32 },
    /// Restore the soft-deleted `uuid`.
    Restore { uuid: String },
}

/// An open y/N dialog. `prompt` is rendered verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConfirm {
    pub prompt: String,
    pub action: ConfirmKind,
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
    /// Supersession links touching the selected memory, from the service's
    /// links read (T4): older/newer uuids, link type, and timestamps.
    pub chain: Vec<MemoryLink>,
    /// Tags of the selected memory, from the service's tags read.
    pub tags: Vec<String>,
    /// Whether the memories pane is showing soft-deleted rows (`x`).
    pub deleted_view: bool,
    /// Older memory chosen for the open supersede picker (`Mode::Pick`).
    pub supersede_older: Option<String>,
    /// The open confirmation dialog, if any (`Mode::Confirm`).
    pub confirm: Option<PendingConfirm>,
    pub status: Option<String>,
    /// The open save/update form, if any. Only set while `mode` is `Form`.
    pub form: Option<Form>,
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
            chain: Vec::new(),
            tags: Vec::new(),
            deleted_view: false,
            supersede_older: None,
            confirm: None,
            status: None,
            form: None,
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

    /// Reload the memories pane for the selected namespace: the soft-deleted
    /// rows in the deleted view (`x`), an FTS5 search when a query is set
    /// (the same service call as `mnexis search`), a plain list otherwise.
    /// Service errors land in `status`.
    pub fn reload_memories(&mut self, service: &AppService) {
        self.status = None;
        self.mem_index = 0;
        self.detail = None;
        self.history.clear();
        self.chain.clear();
        self.tags.clear();
        let Some(ns) = self.selected_namespace().map(str::to_string) else {
            self.memories.clear();
            return;
        };
        let loaded = if self.deleted_view {
            service
                .list_deleted(Some(&ns), MAX_LIMIT, 0)
                .map(|list| list.iter().map(MemoryRow::from).collect::<Vec<_>>())
                .map_err(|e| format!("deleted list failed: {e}"))
        } else if self.query.is_empty() {
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

    /// Fetch the selected memory plus its revision history, supersession
    /// chain, and tags. The deleted view reads the row through
    /// `get_including_deleted` (plain `get` hides it) and skips history,
    /// which the service only exposes for live rows.
    pub fn refresh_detail(&mut self, service: &AppService) {
        self.detail = None;
        self.history.clear();
        self.chain.clear();
        self.tags.clear();
        let Some(uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            return;
        };
        let loaded = if self.deleted_view {
            service.get_including_deleted(&uuid)
        } else {
            service.get(&uuid)
        };
        match loaded {
            Ok(Some(mem)) => {
                if !self.deleted_view {
                    match service.history(&uuid) {
                        Ok(history) => self.history = history,
                        Err(e) => self.status = Some(format!("history failed: {e}")),
                    }
                }
                match service.links(&uuid) {
                    Ok(links) => self.chain = links,
                    Err(e) => self.status = Some(format!("links failed: {e}")),
                }
                match service.tags(&uuid) {
                    Ok(tags) => self.tags = tags.into_iter().map(|t| t.tag).collect(),
                    Err(e) => self.status = Some(format!("tags failed: {e}")),
                }
                self.detail = Some(mem);
            }
            Ok(None) => self.status = Some(format!("memory {uuid} not found")),
            Err(e) => self.status = Some(format!("detail failed: {e}")),
        }
    }

    /// Apply one intent. Write actions need `&mut` access to the service;
    /// every other action only reads through it. The modal modes (form,
    /// picker, confirmation) own their keys.
    pub fn apply(&mut self, action: Action, service: &mut AppService) {
        match self.mode {
            Mode::Form => return self.apply_form(action, service),
            Mode::Confirm => return self.apply_confirm(action, service),
            Mode::Pick => return self.apply_pick(action, service),
            _ => {}
        }
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
            Action::StartSave => self.start_save(),
            Action::StartUpdate => self.start_update(service),
            Action::StartSupersede => self.start_supersede(service),
            Action::StartDelete => self.start_delete(service),
            Action::StartRestore => self.start_restore(),
            Action::ToggleDeleted => self.toggle_deleted(service),
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
            Action::Input(_)
            | Action::Backspace
            | Action::NextField
            | Action::PrevField
            | Action::Submit
            | Action::ReloadForm
            | Action::ConfirmYes
            | Action::ConfirmNo => {}
        }
    }

    /// While a form is open it owns the keys: text edits go to the focused
    /// field and pane navigation is inert.
    fn apply_form(&mut self, action: Action, service: &mut AppService) {
        match action {
            Action::Quit => self.quit = true,
            Action::Escape => {
                self.form = None;
                self.mode = Mode::Normal;
                self.status = Some("cancelled, nothing written".to_string());
            }
            Action::Input(c) => {
                if let Some(form) = self.form.as_mut() {
                    form.insert(c);
                }
            }
            Action::Backspace => {
                if let Some(form) = self.form.as_mut() {
                    form.backspace();
                }
            }
            Action::NextField => {
                if let Some(form) = self.form.as_mut() {
                    form.next();
                }
            }
            Action::PrevField => {
                if let Some(form) = self.form.as_mut() {
                    form.prev();
                }
            }
            Action::Confirm => self.form_confirm(service),
            Action::Submit => self.form_submit(service),
            Action::ReloadForm => self.form_reload(service),
            _ => {}
        }
    }

    /// While picking the newer memory, the list owns j/k (rows), Enter
    /// (choose), and Esc (cancel). Every other key is inert so nothing can
    /// be written by accident.
    fn apply_pick(&mut self, action: Action, service: &mut AppService) {
        match action {
            Action::Quit => self.quit = true,
            Action::NextRow => self.move_selection(1, service),
            Action::PrevRow => self.move_selection(-1, service),
            Action::Confirm => self.pick_newer(service),
            Action::Escape => {
                self.supersede_older = None;
                self.mode = Mode::Normal;
                self.status = Some("cancelled, nothing written".to_string());
            }
            _ => {}
        }
    }

    /// While a confirmation dialog is open, only `y` writes; `n`/Enter/Esc
    /// cancel and leave the database exactly as it was.
    fn apply_confirm(&mut self, action: Action, service: &mut AppService) {
        match action {
            Action::Quit => self.quit = true,
            Action::ConfirmYes => self.confirm_yes(service),
            Action::ConfirmNo | Action::Escape => {
                self.confirm = None;
                self.mode = Mode::Normal;
                self.status = Some("cancelled, nothing written".to_string());
            }
            _ => {}
        }
    }

    fn confirm_yes(&mut self, service: &mut AppService) {
        let Some(pending) = self.confirm.take() else {
            self.mode = Mode::Normal;
            return;
        };
        self.mode = Mode::Normal;
        match pending.action {
            ConfirmKind::Delete { uuid, revision } => match service.soft_delete(&uuid, revision) {
                Ok(()) => {
                    self.reload_memories(service);
                    self.status = Some(format!(
                        "soft-deleted {uuid} (was revision {revision}) — x lists deleted rows, \
                         r restores"
                    ));
                }
                Err(e) => self.status = Some(format!("delete failed: {e}")),
            },
            ConfirmKind::Restore { uuid } => match service.restore(&uuid) {
                Ok(()) => {
                    // The restored row is live again: leave the deleted view
                    // and select it in the list.
                    self.deleted_view = false;
                    self.reload_memories(service);
                    if let Some(pos) = self.memories.iter().position(|row| row.uuid == uuid) {
                        self.mem_index = pos;
                        self.refresh_detail(service);
                    }
                    self.pane = Pane::Memories;
                    self.status = Some(format!("restored {uuid}"));
                }
                Err(e) => self.status = Some(format!("restore failed: {e}")),
            },
        }
    }

    /// `n`: open the save form prefilled with the current namespace.
    fn start_save(&mut self) {
        match self.selected_namespace().map(str::to_string) {
            Some(namespace) => {
                self.form = Some(Form::save(&namespace));
                self.mode = Mode::Form;
                self.status = None;
            }
            None => self.status = Some("no namespace to save into".to_string()),
        }
    }

    /// `e`: open the update form for the selected memory, loaded fresh so
    /// the prefilled `expected_revision` is the revision on disk and the
    /// prefilled tags are the ones currently attached.
    fn start_update(&mut self, service: &AppService) {
        let Some(uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            self.status = Some("select a memory to update".to_string());
            return;
        };
        match service.get(&uuid) {
            Ok(Some(memory)) => match service.tags(&uuid) {
                Ok(tags) => {
                    let tags: Vec<String> = tags.into_iter().map(|t| t.tag).collect();
                    self.form = Some(Form::update(&memory, &tags));
                    self.mode = Mode::Form;
                    self.status = None;
                }
                Err(e) => self.status = Some(format!("tags failed: {e}")),
            },
            Ok(None) => self.status = Some(format!("memory {uuid} not found")),
            Err(e) => self.status = Some(format!("detail failed: {e}")),
        }
    }

    /// `s`: start the supersede flow with the selected memory as the older
    /// side, then pick the newer memory in the list (`Mode::Pick`).
    fn start_supersede(&mut self, service: &AppService) {
        if self.deleted_view {
            self.status = Some(
                "supersede needs an active memory (x leaves the deleted view)".to_string(),
            );
            return;
        }
        let Some(uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            self.status = Some("select the older memory to supersede".to_string());
            return;
        };
        match service.get(&uuid) {
            Ok(Some(older)) => {
                self.supersede_older = Some(older.uuid);
                self.pane = Pane::Memories;
                self.mode = Mode::Pick;
                self.status = Some(
                    "pick the newer memory with j/k, Enter chooses it, Esc cancels".to_string(),
                );
            }
            Ok(None) => self.status = Some(format!("memory {uuid} not found")),
            Err(e) => self.status = Some(format!("detail failed: {e}")),
        }
    }

    /// Enter in the picker: load both rows so the form's expected revisions
    /// are the ones on disk now, then open the supersede form.
    fn pick_newer(&mut self, service: &mut AppService) {
        let Some(older_uuid) = self.supersede_older.clone() else {
            self.mode = Mode::Normal;
            return;
        };
        let Some(newer_uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            self.status = Some("pick a newer memory first".to_string());
            return;
        };
        let older = match service.get(&older_uuid) {
            Ok(Some(memory)) => memory,
            Ok(None) => {
                self.status = Some(format!("memory {older_uuid} not found"));
                return;
            }
            Err(e) => {
                self.status = Some(format!("detail failed: {e}"));
                return;
            }
        };
        let newer = match service.get(&newer_uuid) {
            Ok(Some(memory)) => memory,
            Ok(None) => {
                self.status = Some(format!("memory {newer_uuid} not found"));
                return;
            }
            Err(e) => {
                self.status = Some(format!("detail failed: {e}"));
                return;
            }
        };
        self.form = Some(Form::supersede(&older, &newer));
        self.supersede_older = None;
        self.mode = Mode::Form;
        self.status = None;
    }

    /// `d`: ask before soft-deleting the selected memory. The revision is
    /// read fresh here so the optimistic check uses the row on disk.
    fn start_delete(&mut self, service: &AppService) {
        if self.deleted_view {
            self.status = Some("already in the deleted view".to_string());
            return;
        }
        let Some(uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            self.status = Some("select a memory to delete".to_string());
            return;
        };
        match service.get(&uuid) {
            Ok(Some(memory)) => {
                self.confirm = Some(PendingConfirm {
                    prompt: format!(
                        "soft-delete {} (revision {})? [y/N]",
                        memory.uuid, memory.revision
                    ),
                    action: ConfirmKind::Delete {
                        uuid: memory.uuid,
                        revision: memory.revision,
                    },
                });
                self.mode = Mode::Confirm;
                self.status = None;
            }
            Ok(None) => self.status = Some(format!("memory {uuid} not found")),
            Err(e) => self.status = Some(format!("detail failed: {e}")),
        }
    }

    /// `r`: ask before restoring the memory selected in the deleted view.
    fn start_restore(&mut self) {
        if !self.deleted_view {
            self.status = Some("press x for the deleted view, then r to restore".to_string());
            return;
        }
        let Some(uuid) = self.memories.get(self.mem_index).map(|r| r.uuid.clone()) else {
            self.status = Some("select a deleted memory to restore".to_string());
            return;
        };
        self.confirm = Some(PendingConfirm {
            prompt: format!("restore {uuid}? [y/N]"),
            action: ConfirmKind::Restore { uuid },
        });
        self.mode = Mode::Confirm;
        self.status = None;
    }

    /// `x`: browse the soft-deleted memories of the selected namespace. This
    /// is the read that makes `restore` reachable for rows deleted outside
    /// this session.
    fn toggle_deleted(&mut self, service: &AppService) {
        self.deleted_view = !self.deleted_view;
        self.query.clear();
        self.reload_memories(service);
        self.status = Some(if self.deleted_view {
            "deleted view: r restores the selected memory, x goes back".to_string()
        } else {
            "live view".to_string()
        });
    }

    /// Enter: newline in a multi-line field, next field otherwise, and
    /// submit from the last field.
    fn form_confirm(&mut self, service: &mut AppService) {
        let Some(form) = self.form.as_ref() else {
            self.mode = Mode::Normal;
            return;
        };
        if form.enter_edits() {
            if let Some(form) = self.form.as_mut() {
                form.newline();
            }
            return;
        }
        if form.is_last() {
            self.form_submit(service);
            return;
        }
        if let Some(form) = self.form.as_mut() {
            form.next();
        }
    }

    fn form_submit(&mut self, service: &mut AppService) {
        let Some(mut form) = self.form.take() else {
            self.mode = Mode::Normal;
            return;
        };
        match form.submit(service) {
            Ok(outcome) => {
                self.mode = Mode::Normal;
                self.after_write(service, outcome);
            }
            Err(()) => {
                // Keep every entered value; the error also lands in the
                // status line so it is visible when the form is narrow.
                self.status = form.error.clone();
                self.form = Some(form);
            }
        }
    }

    fn form_reload(&mut self, service: &mut AppService) {
        // Only the update form targets a memory; Ctrl-R in the save form is
        // a no-op rather than an error.
        let has_target = self.form.as_ref().and_then(|f| f.target.as_ref()).is_some();
        if !has_target {
            return;
        }
        let result = match self.form.as_mut() {
            Some(form) => form.reload(service),
            None => return,
        };
        match result {
            Ok(memory) => {
                self.status = Some(format!(
                    "reloaded {} at revision {}",
                    memory.uuid, memory.revision
                ));
                self.refresh_detail(service);
            }
            Err(message) => {
                self.status = Some(message.clone());
                if let Some(form) = self.form.as_mut() {
                    form.error = Some(message);
                }
            }
        }
    }

    /// After a successful write: drop the search filter, focus the written
    /// namespace and row, and reload list + detail from the service.
    fn after_write(&mut self, service: &mut AppService, outcome: WriteOutcome) {
        self.query.clear();
        if let Some(pos) = self
            .namespaces
            .iter()
            .position(|ns| ns == &outcome.namespace)
        {
            self.ns_index = pos;
        }
        self.reload_memories(service);
        if let Some(pos) = self.memories.iter().position(|row| row.uuid == outcome.uuid) {
            self.mem_index = pos;
            self.refresh_detail(service);
        }
        self.pane = Pane::Memories;
        self.status = Some(outcome.message);
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
