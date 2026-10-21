//! Interactive terminal UI behind `mnexis tui` (S2).
//!
//! The UI is a pure layer over [`MemoryService`]: `app.rs` holds the
//! terminal-free model, `form.rs` the write forms, `ui.rs` renders them, and
//! this module owns the crossterm lifecycle (alternate screen + raw mode),
//! including panic-safe terminal restoration.

pub mod app;
pub mod form;
mod ui;
pub mod update;

use std::io::{self, Stdout};

use crossterm::cursor::Show;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::cli::profile::Profile;
use crate::domain::error::MnexisError;
use app::{Action, App, Mode};

type TuiTerminal = Terminal<CrosstermBackend<Stdout>>;

/// Enter the TUI until `q`. The profile must already be open (the caller
/// resolves it exactly like every other command, so `mnexis tui` without
/// an initialised profile fails with the same `NOT_FOUND` error).
pub fn run(profile: &mut Profile) -> Result<(), MnexisError> {
    let mut terminal = setup_terminal()?;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));

    let mut app = App::new_at_menu(profile.service_ref());
    // The version check is a network call: it runs in a thread and the
    // menu shows what came back.
    let controller = std::sync::Arc::new(update::UpdateController::new(profile.data_dir.clone()));
    controller.start_check();
    app.update = Some(std::sync::Arc::clone(&controller));
    let loop_result = event_loop(&mut terminal, &mut app, profile);

    let _ = std::panic::take_hook(); // drop our hook, restore the default
    loop_result.and(restore_terminal())
}

fn event_loop(
    terminal: &mut TuiTerminal,
    app: &mut App,
    profile: &mut Profile,
) -> Result<(), MnexisError> {
    loop {
        terminal
            .draw(|frame| ui::draw(frame, app))
            .map_err(term_err)?;
        // The screen is redrawn even when nothing happens, so the update
        // notice appears as soon as the check answers.
        if event::poll(std::time::Duration::from_millis(200)).map_err(term_err)? {
            match event::read().map_err(term_err)? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.apply(map_key(key, app.mode), profile.service_mut());
                }
                _ => {}
            }
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

fn map_key(key: KeyEvent, mode: Mode) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => return Action::Quit,
            KeyCode::Char('s') if mode == Mode::Form => return Action::Submit,
            KeyCode::Char('r') if mode == Mode::Form => return Action::ReloadForm,
            _ => {}
        }
    }
    // The y/N dialog owns its answers before any text-key mapping: `y` and
    // `n` must never reach a pane or a field while it is open. Enter is the
    // safe default answer (`n`), matching the `[y/N]` prompt.
    if mode == Mode::Confirm {
        return match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Action::ConfirmYes,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Enter | KeyCode::Esc => {
                Action::ConfirmNo
            }
            _ => Action::Noop,
        };
    }
    // The menu owns its keys: nothing else reaches a pane until an option
    // is chosen. `q` and Esc leave the TUI.
    if mode == Mode::Menu {
        return match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('j') | KeyCode::Down => Action::MenuNext,
            KeyCode::Char('k') | KeyCode::Up => Action::MenuPrev,
            KeyCode::Enter => Action::MenuSelect,
            // `u` installs the newer release the menu announces.
            KeyCode::Char('u') => Action::Update,
            _ => Action::Noop,
        };
    }
    // The agents view owns its keys too: the environment selection, the
    // install/uninstall actions, the refresh, and the way back to the menu.
    if mode == Mode::Agents {
        return match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('j') | KeyCode::Down => Action::AgentNext,
            KeyCode::Char('k') | KeyCode::Up => Action::AgentPrev,
            KeyCode::Char('i') => Action::AgentInstall,
            KeyCode::Char('u') => Action::AgentUninstall,
            KeyCode::Char('r') => Action::AgentRefresh,
            KeyCode::Esc => Action::Escape,
            _ => Action::Noop,
        };
    }
    match key.code {
        KeyCode::Char(c) if matches!(mode, Mode::Search | Mode::Form) => Action::Input(c),
        KeyCode::Char('q') if mode == Mode::Normal => Action::Quit,
        KeyCode::Backspace if matches!(mode, Mode::Search | Mode::Form) => Action::Backspace,
        KeyCode::Char('/') if mode == Mode::Normal => Action::StartSearch,
        KeyCode::Char('n') if mode == Mode::Normal => Action::StartSave,
        KeyCode::Char('e') if mode == Mode::Normal => Action::StartUpdate,
        KeyCode::Char('s') if mode == Mode::Normal => Action::StartSupersede,
        KeyCode::Char('d') if mode == Mode::Normal => Action::StartDelete,
        KeyCode::Char('r') if mode == Mode::Normal => Action::StartRestore,
        KeyCode::Char('x') if mode == Mode::Normal => Action::ToggleDeleted,
        KeyCode::Down if mode == Mode::Form => Action::NextField,
        KeyCode::Up if mode == Mode::Form => Action::PrevField,
        KeyCode::Char('j') | KeyCode::Down => Action::NextRow,
        KeyCode::Char('k') | KeyCode::Up => Action::PrevRow,
        KeyCode::Tab if mode == Mode::Form => Action::NextField,
        KeyCode::BackTab if mode == Mode::Form => Action::PrevField,
        KeyCode::Tab | KeyCode::BackTab if mode == Mode::Pick => Action::Noop,
        KeyCode::Tab => Action::NextPane,
        KeyCode::BackTab => Action::PrevPane,
        KeyCode::Enter => Action::Confirm,
        KeyCode::Esc => Action::Escape,
        _ => Action::Noop,
    }
}

fn setup_terminal() -> Result<TuiTerminal, MnexisError> {
    enable_raw_mode().map_err(term_err)?;
    let mut stdout = io::stdout();
    if let Err(e) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(term_err(e));
    }
    match Terminal::new(CrosstermBackend::new(stdout)) {
        Ok(terminal) => Ok(terminal),
        Err(e) => {
            let _ = restore_terminal();
            Err(term_err(e))
        }
    }
}

fn restore_terminal() -> Result<(), MnexisError> {
    let mut first: Option<MnexisError> = None;
    if let Err(e) = disable_raw_mode() {
        first = Some(term_err(e));
    }
    if let Err(e) = execute!(io::stdout(), LeaveAlternateScreen, Show) {
        first.get_or_insert(term_err(e));
    }
    match first {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn term_err(e: io::Error) -> MnexisError {
    MnexisError::busy(format!("tui terminal: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn keymap_covers_navigation_panes_search_and_quit() {
        assert_eq!(
            map_key(key(KeyCode::Char('j')), Mode::Normal),
            Action::NextRow
        );
        assert_eq!(map_key(key(KeyCode::Down), Mode::Normal), Action::NextRow);
        assert_eq!(
            map_key(key(KeyCode::Char('k')), Mode::Normal),
            Action::PrevRow
        );
        assert_eq!(map_key(key(KeyCode::Up), Mode::Normal), Action::PrevRow);
        assert_eq!(map_key(key(KeyCode::Tab), Mode::Normal), Action::NextPane);
        assert_eq!(
            map_key(key(KeyCode::BackTab), Mode::Normal),
            Action::PrevPane
        );
        assert_eq!(
            map_key(key(KeyCode::Char('/')), Mode::Normal),
            Action::StartSearch
        );
        assert_eq!(map_key(key(KeyCode::Enter), Mode::Normal), Action::Confirm);
        assert_eq!(map_key(key(KeyCode::Esc), Mode::Normal), Action::Escape);
        assert_eq!(map_key(key(KeyCode::Char('q')), Mode::Normal), Action::Quit);
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Mode::Normal
            ),
            Action::Quit
        );
        // In search mode `q` is text, not quit, and chars feed the query.
        assert_eq!(
            map_key(key(KeyCode::Char('q')), Mode::Search),
            Action::Input('q')
        );
        assert_eq!(
            map_key(key(KeyCode::Backspace), Mode::Search),
            Action::Backspace
        );
    }

    #[test]
    fn keymap_covers_write_forms() {
        assert_eq!(
            map_key(key(KeyCode::Char('n')), Mode::Normal),
            Action::StartSave
        );
        assert_eq!(
            map_key(key(KeyCode::Char('e')), Mode::Normal),
            Action::StartUpdate
        );
        assert_eq!(
            map_key(key(KeyCode::Char('s')), Mode::Normal),
            Action::StartSupersede
        );
        assert_eq!(
            map_key(key(KeyCode::Char('d')), Mode::Normal),
            Action::StartDelete
        );
        assert_eq!(
            map_key(key(KeyCode::Char('r')), Mode::Normal),
            Action::StartRestore
        );
        assert_eq!(
            map_key(key(KeyCode::Char('x')), Mode::Normal),
            Action::ToggleDeleted
        );
        // Form mode owns the text keys: `n`, `e`, and `q` are input.
        for c in ['n', 'e', 'q'] {
            assert_eq!(map_key(key(KeyCode::Char(c)), Mode::Form), Action::Input(c));
        }
        assert_eq!(map_key(key(KeyCode::Tab), Mode::Form), Action::NextField);
        assert_eq!(
            map_key(key(KeyCode::BackTab), Mode::Form),
            Action::PrevField
        );
        assert_eq!(map_key(key(KeyCode::Down), Mode::Form), Action::NextField);
        assert_eq!(map_key(key(KeyCode::Up), Mode::Form), Action::PrevField);
        assert_eq!(
            map_key(key(KeyCode::Backspace), Mode::Form),
            Action::Backspace
        );
        assert_eq!(map_key(key(KeyCode::Enter), Mode::Form), Action::Confirm);
        assert_eq!(map_key(key(KeyCode::Esc), Mode::Form), Action::Escape);
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL),
                Mode::Form
            ),
            Action::Submit
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL),
                Mode::Form
            ),
            Action::ReloadForm
        );
    }

    #[test]
    fn keymap_covers_the_picker_and_the_confirmation_dialog() {
        // Picker: the list keeps j/k/Enter/Esc, pane keys are inert.
        assert_eq!(
            map_key(key(KeyCode::Char('j')), Mode::Pick),
            Action::NextRow
        );
        assert_eq!(map_key(key(KeyCode::Down), Mode::Pick), Action::NextRow);
        assert_eq!(
            map_key(key(KeyCode::Char('k')), Mode::Pick),
            Action::PrevRow
        );
        assert_eq!(map_key(key(KeyCode::Up), Mode::Pick), Action::PrevRow);
        assert_eq!(map_key(key(KeyCode::Enter), Mode::Pick), Action::Confirm);
        assert_eq!(map_key(key(KeyCode::Esc), Mode::Pick), Action::Escape);
        assert_eq!(map_key(key(KeyCode::Tab), Mode::Pick), Action::Noop);
        assert_eq!(map_key(key(KeyCode::Char('y')), Mode::Pick), Action::Noop);

        // Confirmation: `y` answers yes, everything that is not an answer
        // is inert (never a pane action or a text edit).
        assert_eq!(
            map_key(key(KeyCode::Char('y')), Mode::Confirm),
            Action::ConfirmYes
        );
        assert_eq!(
            map_key(key(KeyCode::Char('Y')), Mode::Confirm),
            Action::ConfirmYes
        );
        for code in [
            KeyCode::Char('n'),
            KeyCode::Char('N'),
            KeyCode::Enter,
            KeyCode::Esc,
        ] {
            assert_eq!(map_key(key(code), Mode::Confirm), Action::ConfirmNo);
        }
        for code in [KeyCode::Tab, KeyCode::Char('j'), KeyCode::Char('q')] {
            assert_eq!(map_key(key(code), Mode::Confirm), Action::Noop);
        }
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Mode::Confirm
            ),
            Action::Quit
        );
    }

    #[test]
    fn keymap_covers_the_initial_menu() {
        // The menu owns its keys: the arrows and j/k move the highlight,
        // Enter activates the option, q/Esc leave the TUI.
        assert_eq!(map_key(key(KeyCode::Down), Mode::Menu), Action::MenuNext);
        assert_eq!(
            map_key(key(KeyCode::Char('j')), Mode::Menu),
            Action::MenuNext
        );
        assert_eq!(map_key(key(KeyCode::Up), Mode::Menu), Action::MenuPrev);
        assert_eq!(
            map_key(key(KeyCode::Char('k')), Mode::Menu),
            Action::MenuPrev
        );
        assert_eq!(map_key(key(KeyCode::Enter), Mode::Menu), Action::MenuSelect);
        assert_eq!(map_key(key(KeyCode::Char('u')), Mode::Menu), Action::Update);
        assert_eq!(map_key(key(KeyCode::Char('q')), Mode::Menu), Action::Quit);
        assert_eq!(map_key(key(KeyCode::Esc), Mode::Menu), Action::Quit);
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Mode::Menu
            ),
            Action::Quit
        );
        // No other key reaches a pane while the menu is open.
        for code in [
            KeyCode::Char('n'),
            KeyCode::Char('e'),
            KeyCode::Char('s'),
            KeyCode::Char('d'),
            KeyCode::Char('r'),
            KeyCode::Char('x'),
            KeyCode::Char('/'),
            KeyCode::Tab,
        ] {
            assert_eq!(
                map_key(key(code), Mode::Menu),
                Action::Noop,
                "the menu ignores {code:?}"
            );
        }
    }

    #[test]
    fn keymap_covers_the_agents_view() {
        assert_eq!(map_key(key(KeyCode::Down), Mode::Agents), Action::AgentNext);
        assert_eq!(
            map_key(key(KeyCode::Char('j')), Mode::Agents),
            Action::AgentNext
        );
        assert_eq!(map_key(key(KeyCode::Up), Mode::Agents), Action::AgentPrev);
        assert_eq!(
            map_key(key(KeyCode::Char('k')), Mode::Agents),
            Action::AgentPrev
        );
        assert_eq!(
            map_key(key(KeyCode::Char('i')), Mode::Agents),
            Action::AgentInstall
        );
        assert_eq!(
            map_key(key(KeyCode::Char('u')), Mode::Agents),
            Action::AgentUninstall
        );
        assert_eq!(
            map_key(key(KeyCode::Char('r')), Mode::Agents),
            Action::AgentRefresh
        );
        assert_eq!(map_key(key(KeyCode::Esc), Mode::Agents), Action::Escape);
        assert_eq!(map_key(key(KeyCode::Char('q')), Mode::Agents), Action::Quit);
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
                Mode::Agents
            ),
            Action::Quit
        );
        // The memory keys are inert in the agents view.
        for code in [
            KeyCode::Char('n'),
            KeyCode::Char('e'),
            KeyCode::Char('s'),
            KeyCode::Char('d'),
            KeyCode::Char('x'),
            KeyCode::Char('/'),
            KeyCode::Enter,
            KeyCode::Tab,
        ] {
            assert_eq!(
                map_key(key(code), Mode::Agents),
                Action::Noop,
                "the agents view ignores {code:?}"
            );
        }
    }
}
