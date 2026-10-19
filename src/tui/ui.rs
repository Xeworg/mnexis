//! Terminal rendering for the `mnexis tui` read surface.
//!
//! Rendering is a pure read of [`App`]; all state transitions live in
//! `app.rs`, so tests exercise the model without a terminal.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph, Wrap};

use crate::domain::memory::MemoryState;
use crate::tui::app::{App, MemoryRow, Mode, Pane};

pub fn draw(frame: &mut Frame, app: &App) {
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

    let status_text = if app.mode == Mode::Search {
        format!("Search: {}▏", app.query)
    } else {
        app.status.as_deref().unwrap_or_default().to_string()
    };
    frame.render_widget(Paragraph::new(status_text), status);
    frame.render_widget(Paragraph::new(footer_text()), footer);
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
    let title = if app.mode == Mode::Search {
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
            lines.push(Line::from(""));
            for content_line in m.content.lines() {
                lines.push(Line::from(content_line.to_string()));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "History (newest first)",
                Style::new().add_modifier(Modifier::BOLD),
            )));
            if app.history.is_empty() {
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
            if m.state == MemoryState::Superseded {
                lines.push(Line::from(
                    "  newer-memory link: not exposed by the service read surface",
                ));
            }
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

fn pane_block(title: String, focused: bool) -> Block<'static> {
    let border = if focused {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::DarkGray)
    };
    Block::bordered().title(title).border_style(border)
}

fn footer_text() -> &'static str {
    "Tab/Shift-Tab panes  Up/k Down/j rows  / search  Enter select  Esc clear  q quit"
}
