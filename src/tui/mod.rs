//! Interactive terminal UI behind `mnexis tui` (S2 read surface).
//!
//! The read surface is a pure UI over [`MemoryService`]: `app.rs` holds
//! the terminal-free model, `ui.rs` renders it, and this module owns the
//! crossterm lifecycle (alternate screen + raw mode), including
//! panic-safe terminal restoration.

pub mod app;
mod ui;

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
pub fn run(profile: &Profile) -> Result<(), MnexisError> {
    let mut terminal = setup_terminal()?;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        default_hook(info);
    }));

    let mut app = App::new(profile.service_ref());
    let loop_result = event_loop(&mut terminal, &mut app, profile);

    let _ = std::panic::take_hook(); // drop our hook, restore the default
    loop_result.and(restore_terminal())
}

fn event_loop(
    terminal: &mut TuiTerminal,
    app: &mut App,
    profile: &Profile,
) -> Result<(), MnexisError> {
    loop {
        terminal
            .draw(|frame| ui::draw(frame, app))
            .map_err(term_err)?;
        match event::read().map_err(term_err)? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.apply(map_key(key, app.mode), profile.service_ref());
            }
            _ => {}
        }
        if app.should_quit() {
            return Ok(());
        }
    }
}

fn map_key(key: KeyEvent, mode: Mode) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }
    match key.code {
        KeyCode::Char('q') if mode == Mode::Normal => Action::Quit,
        KeyCode::Char(c) if mode == Mode::Search => Action::Input(c),
        KeyCode::Backspace if mode == Mode::Search => Action::Backspace,
        KeyCode::Char('/') if mode == Mode::Normal => Action::StartSearch,
        KeyCode::Char('j') | KeyCode::Down => Action::NextRow,
        KeyCode::Char('k') | KeyCode::Up => Action::PrevRow,
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
}
