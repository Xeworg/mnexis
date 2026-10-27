//! Headless application state for `mnexis tui`.
//!
//! The model is deliberately terminal-free: integration tests drive it
//! through [`App::apply`] against a real profile while `ui.rs` only reads
//! it. No SQL and no domain rules live here — every read and write goes
//! through [`MemoryService`], exactly as the CLI commands do. Write forms
//! live in [`crate::tui::form`]; this module owns which form (or dialog) is
//! open and what happens to the read surface after a write lands. The
//! agent integration is not a rule of this module either: it goes through
//! the application service [`crate::application::agents`].

use crate::application::agents::{
    AgentEnvironment, AgentInstaller, AssetAction, AssetFilter, EnvironmentStatus, FileAction,
    InstallReport, status,
};
use crate::application::{ListFilter, MemoryService};
use crate::cli::commands::MAX_LIMIT;
use crate::cli::output::snippet;
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink};
use crate::retrieval::SearchHit;
use crate::storage::memory_repo::SqliteMemoryRepository;
use crate::tui::form::{Form, SubmitRejected, WriteOutcome};

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
/// newer-memory picker, a y/N confirmation dialog, the initial menu, or
/// the agent-installation view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    Form,
    /// Choosing the newer memory for an open supersede (`s`).
    Pick,
    /// A y/N dialog is open; nothing is written until it is answered.
    Confirm,
    /// The initial menu: where `mnexis tui` opens (S3).
    Menu,
    /// The agent-installation view: the state of the integration for
    /// every supported environment, and the install/uninstall actions
    /// (S1).
    Agents,
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
    /// Move the menu highlight down (`j` / `Down`).
    MenuNext,
    /// Move the menu highlight up (`k` / `Up`).
    MenuPrev,
    /// Activate the highlighted menu option (`Enter`).
    MenuSelect,
    /// Move the agents-view selection down (`j` / `Down`).
    AgentNext,
    /// Move the agents-view selection up (`k` / `Up`).
    AgentPrev,
    /// Install the integration for the selected environment (`i`).
    AgentInstall,
    /// Remove the integration from the selected environment (`u`).
    AgentUninstall,
    /// Re-read the installation state of every environment (`r`).
    AgentRefresh,
    /// Install the newer release, when the menu says there is one (`u`).
    Update,
}

/// The options of the initial menu (S3, S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuOption {
    /// Open the three-pane memory view.
    Memories,
    /// Open the agent-installation view.
    Agents,
    /// Leave the TUI.
    Quit,
}

impl MenuOption {
    /// Every option, in presentation order.
    pub const ALL: [MenuOption; 3] = [MenuOption::Memories, MenuOption::Agents, MenuOption::Quit];

    /// The option's title, as shown in the menu.
    pub fn label(self) -> &'static str {
        match self {
            MenuOption::Memories => "Memories",
            MenuOption::Agents => "Install for agents",
            MenuOption::Quit => "Quit",
        }
    }

    /// The one-line description shown under the title.
    pub fn description(self) -> &'static str {
        match self {
            MenuOption::Memories => "Browse and edit your memories",
            MenuOption::Agents => "Install the mnexis skill and extension",
            MenuOption::Quit => "Close the TUI",
        }
    }
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
    /// The highlighted option of the initial menu (`Mode::Menu`).
    pub menu_index: usize,
    /// The update work of the TUI, when it can check for a new release.
    pub update: Option<std::sync::Arc<crate::tui::update::UpdateController>>,
    /// The installer of the agent integration (S1, S10). The TUI never
    /// writes agent files itself: every operation goes through this
    /// application service.
    pub agents: AgentInstaller,
    /// The installation state of every supported environment (S1).
    pub agent_status: Vec<EnvironmentStatus>,
    /// The highlighted environment of the agents view.
    pub agent_index: usize,
    quit: bool,
}

impl App {
    /// Build the model from the profile's read scope and load the first
    /// namespace. A scope with no read namespaces is reported, not fatal.
    ///
    /// The model starts in [`Mode::Normal`]: the menu is the entry point
    /// of the TUI, not of the model, so [`App::new_at_menu`] is what
    /// `mnexis tui` uses.
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
            menu_index: 0,
            update: None,
            agents: AgentInstaller::from_env(),
            agent_status: Vec::new(),
            agent_index: 0,
            quit: false,
        };
        if app.namespaces.is_empty() {
            app.status = Some("profile scope has no read namespaces".to_string());
        } else {
            app.reload_memories(service);
        }
        app
    }

    /// Build the model on the initial menu: this is what `mnexis tui`
    /// opens with (S3). The memory surface is loaded exactly as in
    /// [`App::new`], so choosing "Memories" shows the same view as
    /// before.
    pub fn new_at_menu(service: &AppService) -> Self {
        let mut app = Self::new(service);
        app.mode = Mode::Menu;
        app
    }

    /// What the menu shows about a newer release.
    pub fn update_notice(&self) -> crate::tui::update::UpdateNotice {
        self.update
            .as_ref()
            .map(|controller| controller.notice())
            .unwrap_or(crate::tui::update::UpdateNotice::None)
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
                    memory_types: Vec::new(),
                    namespace: Some(ns),
                    state: None,
                    limit: MAX_LIMIT,
                    offset: 0,
                })
                .map(|list| list.iter().map(MemoryRow::from).collect::<Vec<_>>())
                .map_err(|e| format!("list failed: {e}"))
        } else {
            service
                .search(
                    &self.query,
                    Some(&ns),
                    MAX_LIMIT,
                    crate::domain::query::MatchMode::Any,
                )
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
    /// picker, confirmation, menu, agents) own their keys.
    pub fn apply(&mut self, action: Action, service: &mut AppService) {
        match self.mode {
            Mode::Form => return self.apply_form(action, service),
            Mode::Confirm => return self.apply_confirm(action, service),
            Mode::Pick => return self.apply_pick(action, service),
            Mode::Menu => return self.apply_menu(action, service),
            Mode::Agents => return self.apply_agents(action, service),
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
            // The update is handled by the menu; in the other modes the
            // key does nothing.
            Action::Update => {}
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
            | Action::ConfirmNo
            | Action::MenuNext
            | Action::MenuPrev
            | Action::MenuSelect
            | Action::AgentNext
            | Action::AgentPrev
            | Action::AgentInstall
            | Action::AgentUninstall
            | Action::AgentRefresh => {}
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

    /// While the menu is open it owns the keys: `j`/`Down` and `k`/`Up`
    /// move the highlight, Enter activates the option, `q`/Esc leave the
    /// TUI. Every other key is inert, so nothing happens behind the menu.
    fn apply_menu(&mut self, action: Action, service: &mut AppService) {
        match action {
            Action::Quit | Action::Escape => self.quit = true,
            Action::MenuNext => {
                self.menu_index = shift(self.menu_index, 1, MenuOption::ALL.len());
            }
            Action::MenuPrev => {
                self.menu_index = shift(self.menu_index, -1, MenuOption::ALL.len());
            }
            Action::MenuSelect => self.select_menu_option(service),
            Action::Update => {
                // Only when the menu announced a release: the key does
                // nothing when there is nothing to install.
                if let (Some(controller), crate::tui::update::UpdateNotice::Available { .. }) =
                    (&self.update, self.update_notice())
                {
                    controller.start_install();
                }
            }
            _ => {}
        }
    }

    /// Enter on the menu: open the chosen view, or leave the TUI.
    fn select_menu_option(&mut self, service: &mut AppService) {
        let Some(option) = MenuOption::ALL.get(self.menu_index).copied() else {
            return;
        };
        match option {
            MenuOption::Memories => {
                // The three-pane view is exactly the one the TUI showed
                // before the menu existed.
                self.mode = Mode::Normal;
                self.pane = Pane::Namespaces;
                self.status = None;
                self.reload_memories(service);
            }
            MenuOption::Agents => {
                self.mode = Mode::Agents;
                self.agent_index = 0;
                self.status = None;
                self.refresh_agent_status();
            }
            MenuOption::Quit => self.quit = true,
        }
    }

    /// The agents view owns its keys: `j`/`Down` and `k`/`Up` move the
    /// environment, `i` installs, `u` uninstalls, `r` re-reads the state,
    /// and Esc goes back to the menu.
    fn apply_agents(&mut self, action: Action, _service: &mut AppService) {
        match action {
            Action::Quit => self.quit = true,
            Action::Escape => {
                self.mode = Mode::Menu;
                self.status = None;
            }
            Action::AgentNext => {
                self.agent_index = shift(self.agent_index, 1, self.agent_status.len());
            }
            Action::AgentPrev => {
                self.agent_index = shift(self.agent_index, -1, self.agent_status.len());
            }
            Action::AgentInstall => self.install_agents(),
            Action::AgentUninstall => self.uninstall_agents(),
            Action::AgentRefresh => {
                self.refresh_agent_status();
                self.status = Some("agent status refreshed".to_string());
            }
            _ => {}
        }
    }

    /// `i`: install the integration of the selected environment. The
    /// application service owns every rule (S10); the TUI only reports
    /// what the service did.
    fn install_agents(&mut self) {
        let Some(environment) = self.selected_environment() else {
            self.status = Some("no agent environment selected".to_string());
            return;
        };
        let result = self.agents.install(environment, AssetFilter::All);
        match result {
            Ok(report) => {
                self.status = Some(report_status("install", &report));
                self.refresh_agent_status();
            }
            Err(error) => self.status = Some(format!("install failed: {error}")),
        }
    }

    /// `u`: remove the integration from the selected environment.
    fn uninstall_agents(&mut self) {
        let Some(environment) = self.selected_environment() else {
            self.status = Some("no agent environment selected".to_string());
            return;
        };
        let result = self.agents.uninstall(environment, AssetFilter::All);
        match result {
            Ok(report) => {
                self.status = Some(report_status("uninstall", &report));
                self.refresh_agent_status();
            }
            Err(error) => self.status = Some(format!("uninstall failed: {error}")),
        }
    }

    /// Re-read the installation state of every supported environment.
    pub fn refresh_agent_status(&mut self) {
        self.agent_status = status(&self.agents).environments;
    }

    /// The environment highlighted in the agents view.
    pub fn selected_environment(&self) -> Option<AgentEnvironment> {
        self.agent_status
            .get(self.agent_index)
            .map(|entry| entry.environment)
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
            self.status =
                Some("supersede needs an active memory (x leaves the deleted view)".to_string());
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
            Err(SubmitRejected) => {
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
        if let Some(pos) = self
            .memories
            .iter()
            .position(|row| row.uuid == outcome.uuid)
        {
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

/// One status line describing what an install or an uninstall did: the
/// assets it touched and the files it wrote or removed.
fn report_status(operation: &str, report: &InstallReport) -> String {
    if report.assets.is_empty() {
        return format!(
            "nothing to {operation} for {}",
            report.environment.display_name()
        );
    }
    let assets = report
        .assets
        .iter()
        .map(|asset| {
            format!(
                "{} {} '{}' in {}",
                asset_action_verb(asset.action),
                asset.kind.label(),
                asset.name,
                asset.directory.display()
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let files = report
        .files
        .iter()
        .map(|file| format!("{} {}", file_action_verb(file.action), file.path.display()))
        .collect::<Vec<_>>()
        .join(", ");
    if files.is_empty() {
        assets
    } else {
        format!("{assets} — {files}")
    }
}

/// The past-tense verb of an asset action.
fn asset_action_verb(action: AssetAction) -> &'static str {
    match action {
        AssetAction::Installed => "installed",
        AssetAction::Updated => "updated",
        AssetAction::Removed => "removed",
    }
}

/// The past-tense verb of a file action.
fn file_action_verb(action: FileAction) -> &'static str {
    match action {
        FileAction::Written => "wrote",
        FileAction::Removed => "removed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ServiceConfig;
    use crate::domain::memory::MemoryState;
    use crate::domain::scope::{AccessScope, Operation};
    use crate::retrieval::SearchHit;
    use std::fs;
    use std::path::Path;

    fn memory(content: &str) -> Memory {
        Memory {
            id: Some(1),
            uuid: "u-1".to_string(),
            title: String::new(),
            keywords: Vec::new(),
            topic_key: String::new(),
            namespace: "ns-a".to_string(),
            memory_type: "fact".to_string(),
            content: content.to_string(),
            state: MemoryState::Active,
            revision: 3,
            deleted: false,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            deleted_at: None,
            purge_after: None,
            valid_from: None,
            valid_until: None,
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
        }
    }

    #[test]
    fn pane_navigation_cycles_through_all_three_panes() {
        assert_eq!(Pane::Namespaces.next(), Pane::Memories);
        assert_eq!(Pane::Memories.next(), Pane::Detail);
        assert_eq!(Pane::Detail.next(), Pane::Namespaces, "next wraps");
        assert_eq!(Pane::Namespaces.prev(), Pane::Detail, "prev wraps");
        assert_eq!(Pane::Detail.prev(), Pane::Memories);
        assert_eq!(Pane::Memories.prev(), Pane::Namespaces);
    }

    #[test]
    fn shift_clamps_to_the_list_bounds_and_survives_empty_lists() {
        assert_eq!(shift(0, -1, 4), 0, "stays on the first row");
        assert_eq!(shift(3, 1, 4), 3, "stays on the last row");
        assert_eq!(shift(1, -5, 4), 0, "a big jump clamps below");
        assert_eq!(shift(1, 99, 4), 3, "a big jump clamps above");
        assert_eq!(shift(0, 1, 0), 0, "an empty list never moves");
        assert_eq!(shift(0, -1, 1), 0, "a single row list never moves");
    }

    #[test]
    fn memory_rows_carry_a_one_line_snippet_preview() {
        let row = MemoryRow::from(&memory("short content"));
        assert_eq!(row.uuid, "u-1");
        assert_eq!(row.memory_type, "fact");
        assert_eq!(row.state, "active");
        assert_eq!(row.revision, 3);
        assert_eq!(row.preview.as_deref(), Some("short content"));

        // Long content is clipped to the CLI snippet width and marked with
        // an ellipsis; newlines collapse so the row stays one line.
        let long = MemoryRow::from(&memory(&format!("{}\nsecond line", "x".repeat(200))));
        let preview = long.preview.expect("preview present");
        assert_eq!(preview.chars().count(), 61, "60 chars + ellipsis");
        assert!(preview.ends_with('\u{2026}'));
        assert!(!preview.contains('\n'), "preview is one line");
    }

    #[test]
    fn search_hits_render_without_a_preview() {
        let hit = SearchHit {
            uuid: "u-2".to_string(),
            rank: -1.5,
            namespace: "ns-a".to_string(),
            memory_type: "fact".to_string(),
            state: "active".to_string(),
            revision: 2,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            valid_from: None,
            valid_until: None,
        };
        let row = MemoryRow::from(&hit);
        assert_eq!(row.uuid, "u-2");
        assert_eq!(row.state, "active");
        assert_eq!(row.revision, 2);
        assert_eq!(row.preview, None, "FTS5 hits carry no content");
    }

    // --- the initial menu and the agents view (T4) ---

    /// A service backed by an in-memory database: the menu and the agents
    /// view never write a memory, but `App` needs a scope to load.
    fn test_service() -> AppService {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory database");
        crate::storage::sqlite::run_migrations(&conn).expect("migrations");
        let repo = SqliteMemoryRepository::new(conn);
        let scope = AccessScope::new(
            vec!["ns-a".to_string()],
            vec!["ns-a".to_string()],
            "ns-a".to_string(),
            false,
            vec![
                Operation::List,
                Operation::Get,
                Operation::Search,
                Operation::History,
            ],
        );
        MemoryService::new(repo, scope, ServiceConfig::default())
    }

    /// An app on the menu whose installer points at a temporary home, so
    /// the real `$HOME` is never read nor written.
    fn menu_app(home: &Path) -> (AppService, App) {
        let service = test_service();
        let mut app = App::new_at_menu(&service);
        app.agents = AgentInstaller::new(home);
        (service, app)
    }

    /// Open the agents view from the menu ("Install for agents").
    fn enter_agents(app: &mut App, service: &mut AppService) {
        app.apply(Action::MenuNext, service);
        app.apply(Action::MenuSelect, service);
        assert_eq!(app.mode, Mode::Agents, "the agents view is open");
    }

    #[test]
    fn the_tui_opens_on_the_menu() {
        let home = tempfile::tempdir().expect("temp home");
        let (_service, app) = menu_app(home.path());
        assert_eq!(app.mode, Mode::Menu, "the TUI opens on the menu");
        assert_eq!(app.menu_index, 0, "the first option is the active one");
    }

    #[test]
    fn the_menu_highlight_moves_and_stays_in_bounds() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());

        app.apply(Action::MenuNext, &mut service);
        assert_eq!(app.menu_index, 1);
        app.apply(Action::MenuNext, &mut service);
        assert_eq!(app.menu_index, 2);
        app.apply(Action::MenuNext, &mut service);
        assert_eq!(app.menu_index, 2, "the highlight stops at the last option");
        app.apply(Action::MenuPrev, &mut service);
        assert_eq!(app.menu_index, 1);
        app.apply(Action::MenuPrev, &mut service);
        assert_eq!(app.menu_index, 0);
        app.apply(Action::MenuPrev, &mut service);
        assert_eq!(app.menu_index, 0, "the highlight stops at the first option");
    }

    #[test]
    fn enter_on_the_menu_opens_the_chosen_view() {
        let home = tempfile::tempdir().expect("temp home");

        // "Memories": the three-pane view, exactly as before the menu.
        let (mut service, mut app) = menu_app(home.path());
        app.apply(Action::MenuSelect, &mut service);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.pane, Pane::Namespaces);
        assert_eq!(app.namespaces, vec!["ns-a"], "the scope is loaded");

        // "Install for agents": the agents view.
        let (mut service, mut app) = menu_app(home.path());
        app.apply(Action::MenuNext, &mut service);
        app.apply(Action::MenuSelect, &mut service);
        assert_eq!(app.mode, Mode::Agents);

        // "Quit": the TUI ends.
        let (mut service, mut app) = menu_app(home.path());
        app.apply(Action::MenuNext, &mut service);
        app.apply(Action::MenuNext, &mut service);
        app.apply(Action::MenuSelect, &mut service);
        assert!(app.should_quit(), "the third option leaves the TUI");
    }

    #[test]
    fn the_menu_leaves_the_tui_on_quit_and_on_escape() {
        let home = tempfile::tempdir().expect("temp home");

        let (mut service, mut app) = menu_app(home.path());
        app.apply(Action::Quit, &mut service);
        assert!(app.should_quit(), "q leaves the TUI");

        let (mut service, mut app) = menu_app(home.path());
        app.apply(Action::Escape, &mut service);
        assert!(app.should_quit(), "Esc leaves the TUI");
    }

    #[test]
    fn the_agents_view_lists_every_environment_with_its_assets() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);

        assert_eq!(app.agent_status.len(), 2, "pi and opencode");
        assert_eq!(app.agent_status[0].environment, AgentEnvironment::Pi);
        assert_eq!(app.agent_status[1].environment, AgentEnvironment::Opencode);
        assert_eq!(
            app.agent_status[0].assets.len(),
            2,
            "pi gets the skill and the extension"
        );
        assert_eq!(
            app.agent_status[1].assets.len(),
            1,
            "opencode gets the skill only"
        );
        assert!(
            app.agent_status
                .iter()
                .all(|environment| environment.assets.iter().all(|asset| !asset.installed)),
            "nothing is installed in a fresh home"
        );
    }

    #[test]
    fn the_agents_view_selection_moves_between_the_environments() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);

        assert_eq!(app.selected_environment(), Some(AgentEnvironment::Pi));
        app.apply(Action::AgentNext, &mut service);
        assert_eq!(app.selected_environment(), Some(AgentEnvironment::Opencode));
        app.apply(Action::AgentNext, &mut service);
        assert_eq!(
            app.selected_environment(),
            Some(AgentEnvironment::Opencode),
            "the selection stops at the last environment"
        );
        app.apply(Action::AgentPrev, &mut service);
        assert_eq!(app.selected_environment(), Some(AgentEnvironment::Pi));
    }

    #[test]
    fn installing_from_the_agents_view_writes_the_assets_and_reports_them() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);

        app.apply(Action::AgentInstall, &mut service);
        let message = app.status.clone().unwrap_or_default();
        assert!(
            message.contains("installed"),
            "the status names the action: {message}"
        );
        assert!(
            message.contains("skill"),
            "the status names the asset: {message}"
        );
        assert!(
            message.contains("SKILL.md"),
            "the status names the file: {message}"
        );

        // The state is re-read after the operation.
        assert!(
            app.agent_status[0]
                .assets
                .iter()
                .all(|asset| asset.installed),
            "pi is installed"
        );
        assert!(
            !app.agent_status[1].assets[0].installed,
            "opencode is untouched"
        );
        // The files live under the temporary home, never under `$HOME`.
        assert!(
            home.path()
                .join(".pi")
                .join("agent")
                .join("skills")
                .join("mnexis-memory")
                .join("SKILL.md")
                .is_file(),
            "the skill is written"
        );
        assert!(
            home.path()
                .join(".pi")
                .join("agent")
                .join("extensions")
                .join("mnexis-memory")
                .join("index.js")
                .is_file(),
            "the extension is written"
        );
    }

    #[test]
    fn uninstalling_from_the_agents_view_removes_the_assets_and_reports_them() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);
        app.apply(Action::AgentInstall, &mut service);

        app.apply(Action::AgentUninstall, &mut service);
        let message = app.status.clone().unwrap_or_default();
        assert!(
            message.contains("removed"),
            "the status names the action: {message}"
        );
        assert!(
            !app.agent_status[0]
                .assets
                .iter()
                .any(|asset| asset.installed),
            "pi is not installed any more"
        );
        assert!(
            !home
                .path()
                .join(".pi")
                .join("agent")
                .join("skills")
                .join("mnexis-memory")
                .join("SKILL.md")
                .exists(),
            "the skill file is gone"
        );
    }

    #[test]
    fn the_agents_view_refreshes_the_installation_state() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);
        app.apply(Action::AgentInstall, &mut service);
        assert!(
            app.agent_status[0]
                .assets
                .iter()
                .all(|asset| asset.installed)
        );

        // Remove the files behind the TUI's back: the refresh must see it.
        fs::remove_dir_all(home.path().join(".pi").join("agent")).ok();
        app.apply(Action::AgentRefresh, &mut service);
        assert!(
            !app.agent_status[0]
                .assets
                .iter()
                .any(|asset| asset.installed),
            "the state was re-read"
        );
        assert!(
            app.status.clone().unwrap_or_default().contains("refreshed"),
            "the status says the state was refreshed"
        );
    }

    #[test]
    fn a_failing_install_is_reported_without_breaking_the_tui() {
        let home = tempfile::tempdir().expect("temp home");
        // A file where the skills directory must be: the install fails.
        let skills = home.path().join(".pi").join("agent").join("skills");
        fs::create_dir_all(skills.parent().expect("parent")).expect("create parent");
        fs::write(&skills, "not a directory").expect("write blocker");

        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);
        app.apply(Action::AgentInstall, &mut service);

        let message = app.status.clone().unwrap_or_default();
        assert!(
            message.contains("install failed"),
            "the error is a status message: {message}"
        );
        assert_eq!(app.mode, Mode::Agents, "the view stays open");
        assert!(!app.should_quit(), "the TUI keeps running");
    }

    #[test]
    fn escaping_the_agents_view_goes_back_to_the_menu() {
        let home = tempfile::tempdir().expect("temp home");
        let (mut service, mut app) = menu_app(home.path());
        enter_agents(&mut app, &mut service);

        app.apply(Action::Escape, &mut service);
        assert_eq!(app.mode, Mode::Menu, "Esc goes back to the menu");
        assert!(!app.should_quit(), "the TUI is still running");
    }
}
