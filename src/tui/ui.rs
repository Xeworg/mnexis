//! Terminal rendering for `mnexis tui`.
//!
//! Rendering is a pure read of [`App`]; all state transitions live in
//! `app.rs`, so tests exercise the model without a terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::application::agents::EnvironmentStatus;
use crate::domain::memory::MemoryLink;
use crate::tui::app::{App, MemoryRow, MenuOption, Mode, Pane, PendingConfirm};
use crate::tui::form::{Field, Form};

/// Rows the menu popup needs: a blank line, two lines per option, a blank
/// line between the options, the key hints, and the block borders.
const MENU_HEIGHT: u16 = 13;

pub fn draw(frame: &mut Frame, app: &App) {
    match app.mode {
        Mode::Menu => draw_menu(frame, app),
        Mode::Agents => draw_agents(frame, app),
        _ => draw_panes(frame, app),
    }
}

/// The three-pane memory surface.
fn draw_panes(frame: &mut Frame, app: &App) {
    let [main, status, footer] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let [namespaces, memories, detail] = Layout::horizontal([
        Constraint::Percentage(22),
        Constraint::Percentage(33),
        Constraint::Percentage(45),
    ])
    .areas(main);

    render_namespaces(frame, app, namespaces);
    render_memories(frame, app, memories);
    render_detail(frame, app, detail);

    let status_text = match app.mode {
        Mode::Confirm => app
            .confirm
            .as_ref()
            .map(|pending| pending.prompt.clone())
            .unwrap_or_default(),
        Mode::Form => match app.form.as_ref() {
            Some(form) => form.error.clone().unwrap_or_else(|| {
                let label = form.focused().map(|f| f.label).unwrap_or_default();
                format!("editing `{label}`")
            }),
            None => app.status.clone().unwrap_or_default(),
        },
        Mode::Search => format!("Search: {}▏", app.query),
        _ => app.status.as_deref().unwrap_or_default().to_string(),
    };
    frame.render_widget(Paragraph::new(status_text), status);
    frame.render_widget(Paragraph::new(footer_text(app.mode)), footer);

    if let (Some(form), Mode::Form) = (app.form.as_ref(), app.mode) {
        render_form(frame, form);
    }
    if let (Some(pending), Mode::Confirm) = (app.confirm.as_ref(), app.mode) {
        render_confirm(frame, pending);
    }
    if app.mode == Mode::Pick {
        render_pick(frame, app);
    }
}

/// The initial menu: the options with their description, the active one in
/// reverse video, and the key hints.
fn draw_menu(frame: &mut Frame, app: &App) {
    let [main, status, footer] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_menu(frame, app.menu_index, app.update_notice(), main);
    frame.render_widget(
        Paragraph::new(app.status.as_deref().unwrap_or_default()),
        status,
    );
    frame.render_widget(Paragraph::new(footer_text(Mode::Menu)), footer);
}

/// The agent-installation view: one block per environment with the state
/// of every asset, plus the status line and the key hints.
fn draw_agents(frame: &mut Frame, app: &App) {
    let [main, status, footer] = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    render_agents(frame, &app.agent_status, app.agent_index, main);
    frame.render_widget(
        Paragraph::new(app.status.as_deref().unwrap_or_default()),
        status,
    );
    frame.render_widget(Paragraph::new(footer_text(Mode::Agents)), footer);
}

/// The menu popup: the options, the highlighted one, and the key hints.
fn render_menu(
    frame: &mut Frame,
    selected: usize,
    notice: crate::tui::update::UpdateNotice,
    area: Rect,
) {
    let area = popup_area(area, 70, MENU_HEIGHT);
    frame.render_widget(Clear, area);
    let inner = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = vec![Line::from("")];
    for (index, option) in MenuOption::ALL.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        lines.extend(menu_option_lines(*option, index == selected, inner));
    }
    lines.push(Line::from(""));
    if let Some(line) = update_notice_line(notice) {
        lines.push(line);
        lines.push(Line::from(""));
    }
    lines.push(Line::from(Span::styled(
        "↑/↓ or j/k move · Enter choose · Esc/q quit",
        Style::new().fg(Color::DarkGray),
    )));
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("mnexis".into(), true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The line the menu shows about a newer release, when there is
/// something to say.
fn update_notice_line(notice: crate::tui::update::UpdateNotice) -> Option<Line<'static>> {
    use crate::tui::update::UpdateNotice;
    let (text, color) = match notice {
        UpdateNotice::Checking => return None,
        UpdateNotice::Available { version } => (
            format!("mnexis {version} is published · press u to install it"),
            Color::Yellow,
        ),
        UpdateNotice::Installing => ("Installing the newer release…".to_string(), Color::Yellow),
        UpdateNotice::Installed { version } => (
            format!("mnexis {version} is installed · restart to use it"),
            Color::Green,
        ),
        UpdateNotice::Failed { reason } => (format!("The update failed: {reason}"), Color::Red),
        UpdateNotice::None => return None,
    };
    Some(Line::from(Span::styled(text, Style::new().fg(color))))
}

/// The two lines of one menu option: the title (in reverse video when it
/// is the active option) and its description.
fn menu_option_lines(option: MenuOption, selected: bool, width: usize) -> Vec<Line<'static>> {
    let marker = if selected { "▸ " } else { "  " };
    let mut title = format!("{marker}{}", option.label());
    title.push_str(&" ".repeat(width.saturating_sub(title.chars().count())));
    let title_style = if selected {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new()
    };
    let description_style = if selected {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::DarkGray)
    };
    vec![
        Line::from(Span::styled(title, title_style)),
        Line::from(Span::styled(
            format!("  {}", option.description()),
            description_style,
        )),
    ]
}

/// The agent view: one row per environment, with the state of every asset
/// and where the asset lives.
fn render_agents(
    frame: &mut Frame,
    environments: &[EnvironmentStatus],
    selected: usize,
    area: Rect,
) {
    let inner = area.width.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();
    if environments.is_empty() {
        lines.push(Line::from("no agent environment available"));
    }
    for (index, environment) in environments.iter().enumerate() {
        if index > 0 {
            lines.push(Line::from(""));
        }
        let states = environment
            .assets
            .iter()
            .map(|asset| {
                format!(
                    "{}: {}",
                    asset.kind.label(),
                    if asset.installed {
                        "installed"
                    } else {
                        "not installed"
                    }
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut header = format!("{} — {states}", environment.environment.display_name());
        header.push_str(&" ".repeat(inner.saturating_sub(header.chars().count())));
        let header_style = if index == selected {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        lines.push(Line::from(Span::styled(header, header_style)));
        for asset in &environment.assets {
            lines.push(Line::from(format!(
                "    {} → {}",
                asset.kind.label(),
                asset.directory.display()
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Agents".into(), true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_namespaces(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .namespaces
        .iter()
        .map(|ns| ListItem::new(ns.clone()))
        .collect();
    render_list(
        frame,
        items,
        "Namespaces".into(),
        app.ns_index,
        app.namespaces.len(),
        app.pane == Pane::Namespaces,
        area,
    );
}

fn render_memories(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.deleted_view {
        "Memories (deleted)".into()
    } else if app.mode == Mode::Search {
        "Memories (search)".into()
    } else {
        "Memories".into()
    };
    let items: Vec<ListItem> = app
        .memories
        .iter()
        .map(|row| ListItem::new(row_label(row)))
        .collect();
    render_list(
        frame,
        items,
        title,
        app.mem_index,
        app.memories.len(),
        app.pane == Pane::Memories,
        area,
    );
}

fn render_list(
    frame: &mut Frame,
    items: Vec<ListItem>,
    title: String,
    selected: usize,
    len: usize,
    focused: bool,
    area: Rect,
) {
    let list = List::new(items)
        .block(pane_block(title, focused))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if len > 0 {
        state.select(Some(selected.min(len - 1)));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();
    match &app.detail {
        None => lines.push(Line::from("no memory selected")),
        Some(m) => {
            lines.push(Line::from(format!("uuid:       {}", m.uuid)));
            lines.push(Line::from(format!("namespace:  {}", m.namespace)));
            lines.push(Line::from(format!(
                "type/state: {}/{}",
                m.memory_type,
                m.state.as_str()
            )));
            lines.push(Line::from(format!("revision:   {}", m.revision)));
            lines.push(Line::from(format!("updated_at: {}", m.updated_at)));
            lines.push(Line::from(format!(
                "tags:       {}",
                if app.tags.is_empty() {
                    "-".to_string()
                } else {
                    app.tags.join(", ")
                }
            )));
            if m.deleted {
                lines.push(Line::from(Span::styled(
                    "SOFT-DELETED — press r to restore",
                    Style::new().fg(Color::Red),
                )));
            }
            lines.push(Line::from(""));
            for content_line in m.content.lines() {
                lines.push(Line::from(content_line.to_string()));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "History (newest first)",
                Style::new().add_modifier(Modifier::BOLD),
            )));
            if app.deleted_view {
                lines.push(Line::from("  (soft-deleted: history stays hidden)"));
            } else if app.history.is_empty() {
                lines.push(Line::from("  no prior revisions"));
            }
            for h in &app.history {
                lines.push(Line::from(format!(
                    "  rev {} {} - {}",
                    h.revision,
                    h.state.as_str(),
                    h.change_reason
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Supersession",
                Style::new().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(format!("  state: {}", m.state.as_str())));
            lines.extend(chain_lines(&app.chain, &m.uuid));
        }
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Detail".into(), app.pane == Pane::Detail))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn row_label(row: &MemoryRow) -> String {
    let short: String = row.uuid.chars().take(8).collect();
    let preview = row.preview.as_deref().unwrap_or("");
    format!(
        "{short} {}/{} rev{}  {preview}",
        row.memory_type, row.state, row.revision
    )
}

/// The persisted supersession chain for one memory, one line per link that
/// touches it as either side. Since migration 003 each link carries the
/// reason the supersede was requested with (empty for pre-migration rows).
fn chain_lines(chain: &[MemoryLink], uuid: &str) -> Vec<Line<'static>> {
    if chain.is_empty() {
        return vec![Line::from("  no supersession links")];
    }
    chain
        .iter()
        .map(|link| {
            let reason = if link.reason.trim().is_empty() {
                String::new()
            } else {
                format!(" — {}", link.reason)
            };
            if link.newer_uuid == uuid {
                Line::from(format!(
                    "  supersedes {} on {}{}",
                    short_uuid(&link.older_uuid),
                    link.created_at,
                    reason
                ))
            } else {
                Line::from(format!(
                    "  superseded by {} on {}{}",
                    short_uuid(&link.newer_uuid),
                    link.created_at,
                    reason
                ))
            }
        })
        .collect()
}

fn short_uuid(uuid: &str) -> String {
    uuid.chars().take(8).collect()
}

fn pane_block(title: String, focused: bool) -> Block<'static> {
    let border = if focused {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::DarkGray)
    };
    Block::bordered().title(title).border_style(border)
}

fn footer_text(mode: Mode) -> &'static str {
    match mode {
        Mode::Form => {
            "Tab/Shift-Tab field  Enter next/submit  Esc cancel  Ctrl-S submit  Ctrl-R reload  Ctrl-C quit"
        }
        Mode::Pick => "select the newer memory: j/k or arrows, Enter chooses it, Esc cancels",
        Mode::Confirm => "y confirm  n/Esc cancel (nothing is written until y)",
        Mode::Menu => "↑/↓ or j/k move  Enter choose  Esc/q quit",
        Mode::Agents => "i install  u uninstall  r refresh  j/k environment  Esc menu  q quit",
        _ => {
            "n save  e update  s supersede  d delete  r restore  x deleted view  \
             Tab panes  j/k rows  / search  Enter select  Esc clear  q quit"
        }
    }
}

/// The modal y/N dialog: the prompt verbatim, then the two answers. Nothing
/// is written until the user picks `y`.
fn render_confirm(frame: &mut Frame, pending: &PendingConfirm) {
    let area = popup_area(frame.area(), 70, 5);
    frame.render_widget(Clear, area);
    let lines = vec![
        Line::from(pending.prompt.clone()),
        Line::from(""),
        Line::from(Span::styled(
            "[y] yes    [n]/Esc cancel",
            Style::new().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Confirm".into(), true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// The newer-memory picker banner: the older side stays visible while the
/// user moves through the list.
fn render_pick(frame: &mut Frame, app: &App) {
    let older = app
        .supersede_older
        .as_deref()
        .map(short_uuid)
        .unwrap_or_default();
    let area = popup_area(frame.area(), 70, 4);
    frame.render_widget(Clear, area);
    let lines = vec![
        Line::from(format!("supersede {older}: pick the newer memory")),
        Line::from(Span::styled(
            "j/k or arrows move, Enter chooses it, Esc cancels",
            Style::new().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block("Supersede".into(), true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

/// Center a popup over the frame: `percent_x` of the width, `height` rows.
fn popup_area(area: Rect, percent_x: u16, height: u16) -> Rect {
    let width = area.width.saturating_mul(percent_x.min(100)) / 100;
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

/// The modal save/update form: one line per field with the focused field
/// highlighted, then the error (or the focused field's hint).
fn render_form(frame: &mut Frame, form: &Form) {
    let area = popup_area(frame.area(), 80, (form.fields.len() + 3) as u16);
    frame.render_widget(Clear, area);
    let mut lines: Vec<Line> = form
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| field_line(field, index == form.index))
        .collect();
    lines.push(match (&form.error, form.reload_offered) {
        (Some(error), _) => Line::from(Span::styled(error.clone(), Style::new().fg(Color::Red))),
        (None, true) => Line::from(Span::styled(
            "Ctrl-R reloads the memory",
            Style::new().fg(Color::DarkGray),
        )),
        (None, false) => Line::from(Span::styled(
            form.focused().map(|f| f.hint).unwrap_or_default(),
            Style::new().fg(Color::DarkGray),
        )),
    });
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(pane_block(form.title(), true))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn field_line(field: &Field, focused: bool) -> Line<'static> {
    // One row per field: content newlines render as `⏎`.
    let shown = field.value.replace('\n', " ⏎ ");
    let marker = if focused { "> " } else { "  " };
    let caret = if focused { "▏" } else { "" };
    let style = if focused {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new()
    };
    Line::from(Span::styled(
        format!("{marker}{}: {shown}{caret}", field.label),
        style,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::agents::{AgentEnvironment, AssetKind, AssetStatus};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;

    fn render(form: &Form, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| render_form(frame, form)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    #[test]
    fn form_renders_in_a_normal_and_in_a_tiny_terminal() {
        let mut form = Form::save("ns-a");
        form.insert('x');
        let normal = render(&form, 80, 24);
        assert!(normal.contains("content"), "fields render: {normal}");
        assert!(normal.contains("Save memory"), "the title renders");

        // A tiny terminal must clamp instead of panicking.
        form.error = Some("revision conflict: expected revision 1".to_string());
        let tiny = render(&form, 8, 3);
        assert!(!tiny.is_empty());
    }

    #[test]
    fn chain_lines_name_the_other_side_of_each_link() {
        let mut link = MemoryLink {
            id: Some(1),
            older_uuid: "aaaaaaaa-1111".to_string(),
            newer_uuid: "bbbbbbbb-2222".to_string(),
            link_type: "supersedes".to_string(),
            reason: String::new(),
            created_at: "2026-01-02T00:00:00Z".to_string(),
        };
        let older = chain_lines(std::slice::from_ref(&link), "aaaaaaaa-1111");
        assert_eq!(
            older[0].to_string(),
            "  superseded by bbbbbbbb on 2026-01-02T00:00:00Z"
        );
        let newer = chain_lines(std::slice::from_ref(&link), "bbbbbbbb-2222");
        assert_eq!(
            newer[0].to_string(),
            "  supersedes aaaaaaaa on 2026-01-02T00:00:00Z"
        );
        // A persisted reason (migration 003) renders on the line; blank
        // reasons (pre-migration rows) render without the dash.
        link.reason = "replaced by newer fact".to_string();
        let with_reason = chain_lines(std::slice::from_ref(&link), "bbbbbbbb-2222");
        assert_eq!(
            with_reason[0].to_string(),
            "  supersedes aaaaaaaa on 2026-01-02T00:00:00Z — replaced by newer fact"
        );
        let empty = chain_lines(&[], "cccccccc");
        assert_eq!(empty[0].to_string(), "  no supersession links");
    }

    #[test]
    fn footer_help_names_the_keys_of_each_mode() {
        // The footer is the in-app help: every key the task table in
        // docs/tui.md documents must appear in its mode's footer.
        let normal = footer_text(Mode::Normal);
        for key in [
            "n save",
            "e update",
            "s supersede",
            "d delete",
            "r restore",
            "x deleted",
            "/ search",
            "q quit",
        ] {
            assert!(
                normal.contains(key),
                "normal footer must mention {key}: {normal}"
            );
        }
        let form = footer_text(Mode::Form);
        for key in [
            "Tab/Shift-Tab",
            "Ctrl-S submit",
            "Ctrl-R reload",
            "Esc cancel",
        ] {
            assert!(form.contains(key), "form footer must mention {key}: {form}");
        }
        assert!(footer_text(Mode::Pick).contains("Enter chooses it"));
        assert!(footer_text(Mode::Confirm).contains("nothing is written until y"));
        assert!(
            footer_text(Mode::Search).contains("q quit"),
            "search keeps quit visible after Enter"
        );
        assert!(
            footer_text(Mode::Menu).contains("Enter choose"),
            "the menu footer names the keys: {}",
            footer_text(Mode::Menu)
        );
        assert!(
            footer_text(Mode::Agents).contains("i install")
                && footer_text(Mode::Agents).contains("u uninstall")
                && footer_text(Mode::Agents).contains("r refresh"),
            "the agents footer names the keys: {}",
            footer_text(Mode::Agents)
        );
    }

    #[test]
    fn the_menu_announces_a_newer_release() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_menu(
                    frame,
                    0,
                    crate::tui::update::UpdateNotice::Available {
                        version: "0.2.0".to_string(),
                    },
                    area,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            text.contains("0.2.0"),
            "the menu names the published version: {text}"
        );
        assert!(
            text.contains("press u"),
            "the menu says how to install it: {text}"
        );
    }

    #[test]
    fn the_menu_says_nothing_about_the_update_when_there_is_none() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_menu(frame, 0, crate::tui::update::UpdateNotice::None, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            !text.contains("press u"),
            "the menu stays quiet when there is no update: {text}"
        );
    }

    #[test]
    fn the_menu_renders_every_option_with_its_description() {
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_menu(frame, 1, crate::tui::update::UpdateNotice::None, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(text.contains("mnexis"), "the title renders: {text}");
        assert!(
            text.contains("Memories"),
            "the first option renders: {text}"
        );
        assert!(
            text.contains("Install for agents"),
            "the second option renders: {text}"
        );
        assert!(text.contains("Quit"), "the third option renders: {text}");
        assert!(
            text.contains("Browse and edit your memories"),
            "the descriptions render: {text}"
        );
        assert!(
            text.contains("Esc/q quit"),
            "the key hints render without clipping: {text}"
        );

        // The active option is the one in reverse video.
        let highlighted: String = buffer
            .content()
            .iter()
            .filter(|cell| cell.modifier.contains(Modifier::REVERSED))
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            highlighted.contains("Install for agents"),
            "the active option is highlighted: {highlighted}"
        );
        assert!(
            !highlighted.contains("Memories"),
            "only the active option is highlighted: {highlighted}"
        );
    }

    #[test]
    fn the_agents_view_renders_every_environment_and_its_assets() {
        let statuses = agent_statuses();
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                render_agents(frame, &statuses, 0, area);
            })
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            text.contains("pi — skill: installed, extension: not installed"),
            "the pi row renders its asset states: {text}"
        );
        assert!(
            text.contains("opencode — skill: not installed"),
            "the opencode row renders its asset states: {text}"
        );
        assert!(
            text.contains("/home/tester/.pi/agent/skills/mnexis-memory"),
            "the destination renders: {text}"
        );
        assert!(
            text.contains("/home/tester/.config/opencode/skills/mnexis-memory"),
            "the opencode destination renders: {text}"
        );
    }

    /// Two environments with the assets mnexis installs in them.
    fn agent_statuses() -> Vec<EnvironmentStatus> {
        vec![
            EnvironmentStatus {
                environment: AgentEnvironment::Pi,
                assets: vec![
                    AssetStatus {
                        kind: AssetKind::Skill,
                        name: "mnexis-memory",
                        directory: PathBuf::from("/home/tester/.pi/agent/skills/mnexis-memory"),
                        installed: true,
                    },
                    AssetStatus {
                        kind: AssetKind::Extension,
                        name: "mnexis-memory",
                        directory: PathBuf::from("/home/tester/.pi/agent/extensions/mnexis-memory"),
                        installed: false,
                    },
                ],
            },
            EnvironmentStatus {
                environment: AgentEnvironment::Opencode,
                assets: vec![AssetStatus {
                    kind: AssetKind::Skill,
                    name: "mnexis-memory",
                    directory: PathBuf::from("/home/tester/.config/opencode/skills/mnexis-memory"),
                    installed: false,
                }],
            },
        ]
    }

    #[test]
    fn confirm_dialog_renders_its_prompt_and_cancel_hint() {
        use crate::tui::app::ConfirmKind;
        let pending = PendingConfirm {
            prompt: "soft-delete uuid-1 at revision 2? [y/N]".to_string(),
            action: ConfirmKind::Delete {
                uuid: "uuid-1".to_string(),
                revision: 2,
            },
        };
        let mut terminal = Terminal::new(TestBackend::new(80, 12)).unwrap();
        terminal
            .draw(|frame| render_confirm(frame, &pending))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
        assert!(
            text.contains("soft-delete uuid-1"),
            "prompt renders: {text}"
        );
        assert!(text.contains("[y] yes"), "answers render: {text}");

        // A tiny terminal must clamp instead of panicking.
        let mut tiny = Terminal::new(TestBackend::new(6, 2)).unwrap();
        tiny.draw(|frame| render_confirm(frame, &pending)).unwrap();
    }
}
