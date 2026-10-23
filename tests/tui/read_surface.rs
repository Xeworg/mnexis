//! Headless `App` tests: namespaces, listing, FTS5 search, history, errors.

use mnexis_core::application::{SaveRequest, UpdateRequest};
use mnexis_core::cli::commands;
use mnexis_core::cli::profile::{Profile, now_rfc3339, uuid_v4};
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::tui::app::{Action, App, Mode, Pane};
use tempfile::tempdir_in;

const SCOPE: &str = "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\", \"ns-b\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n";

fn profile(scope: &str) -> (tempfile::TempDir, Profile) {
    let guard = tempdir_in("target").expect("tempdir under target/");
    let dir = guard.path().to_path_buf();
    commands::init(dir.clone()).expect("init profile");
    std::fs::write(dir.join("mnexis.toml"), scope).expect("write mnexis.toml");
    (guard, Profile::open(dir).expect("open profile"))
}

fn save(profile: &mut Profile, ns: &str, content: &str) -> String {
    let uuid = uuid_v4().unwrap();
    profile
        .service_mut()
        .save(SaveRequest {
            uuid: uuid.clone(),
            new_topic: false,
            topic_key: String::new(),
            namespace: ns.to_string(),
            memory_type: "fact".to_string(),
            content: content.to_string(),
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
            valid_from: None,
            valid_until: None,
            tags: Vec::new(),
            idempotency_key: None,
            state: MemoryState::Active,
            now: now_rfc3339(),
        })
        .expect("save memory");
    uuid
}

#[test]
fn loads_scope_namespaces_and_lists_selected_namespace_memories() {
    let (_guard, mut p) = profile(SCOPE);
    let alpha = save(&mut p, "ns-a", "alpha memory");
    save(&mut p, "ns-b", "beta one");
    save(&mut p, "ns-b", "beta two");

    let mut app = App::new(p.service_ref());
    assert_eq!(app.namespaces, vec!["ns-a", "ns-b"]);
    assert_eq!(app.selected_namespace(), Some("ns-a"));
    assert_eq!(app.memories.len(), 1);
    assert_eq!(app.memories[0].uuid, alpha);
    assert_eq!(
        app.detail.as_ref().map(|m| m.uuid.as_str()),
        Some(&alpha[..])
    );

    app.apply(Action::NextRow, p.service_mut());
    assert_eq!(app.selected_namespace(), Some("ns-b"));
    assert_eq!(app.memories.len(), 2);
    app.apply(Action::NextRow, p.service_mut());
    assert_eq!(app.selected_namespace(), Some("ns-b"), "clamped at the end");
}

#[test]
fn tab_and_shift_tab_cycle_panes_and_enter_descends() {
    let (_guard, mut p) = profile(SCOPE);
    save(&mut p, "ns-a", "alpha");
    let mut app = App::new(p.service_ref());

    assert_eq!(app.pane, Pane::Namespaces);
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(app.pane, Pane::Memories, "Enter descends from namespaces");
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(app.pane, Pane::Detail, "Enter descends from memories");
    app.apply(Action::PrevPane, p.service_mut());
    assert_eq!(app.pane, Pane::Memories);
    app.apply(Action::PrevPane, p.service_mut());
    app.apply(Action::PrevPane, p.service_mut());
    assert_eq!(app.pane, Pane::Detail, "Shift-Tab wraps backwards");
    app.apply(Action::NextPane, p.service_mut());
    assert_eq!(app.pane, Pane::Namespaces, "Tab wraps forwards");
    app.apply(Action::Quit, p.service_mut());
    assert!(app.should_quit());
}

#[test]
fn search_uses_service_fts5_and_escape_restores_the_list() {
    let (_guard, mut p) = profile(SCOPE);
    let target = save(&mut p, "ns-a", "petgraph replaced by custom engine");
    save(&mut p, "ns-a", "unrelated content");

    let mut app = App::new(p.service_ref());
    assert_eq!(app.memories.len(), 2);
    app.apply(Action::StartSearch, p.service_mut());
    assert_eq!(app.mode, Mode::Search);
    for c in "petgraph".chars() {
        app.apply(Action::Input(c), p.service_mut());
    }
    assert_eq!(app.query, "petgraph");
    assert_eq!(app.memories.len(), 1);
    assert_eq!(app.memories[0].uuid, target);

    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(app.mode, Mode::Normal, "Enter leaves the search input");
    app.apply(Action::Escape, p.service_mut());
    assert_eq!(app.query, "");
    assert_eq!(app.memories.len(), 2, "Esc clears the search filter");
}

#[test]
fn enter_on_a_memory_shows_detail_with_revision_history() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "first wording");
    let current = p.service_ref().get(&uuid).unwrap().unwrap();
    p.service_mut()
        .update(UpdateRequest {
            uuid: uuid.clone(),
            expected_revision: current.revision,
            content: Some("second wording".to_string()),
            state: None,
            tags: None,
            assertion_basis: None,
            reason: "reword".to_string(),
            now: now_rfc3339(),
        })
        .unwrap();

    let mut app = App::new(p.service_ref());
    app.apply(Action::NextPane, p.service_mut());
    app.apply(Action::Confirm, p.service_mut());
    let detail = app.detail.as_ref().expect("detail loaded");
    assert_eq!(detail.revision, 2);
    assert_eq!(detail.content, "second wording");
    assert!(!app.history.is_empty(), "history must be visible");
    assert_eq!(app.history[0].revision, 2, "newest revision first");
}

#[test]
fn service_errors_surface_in_status_and_bad_input_never_panics() {
    let (_guard, mut p) = profile(SCOPE);
    save(&mut p, "ns-a", "alpha");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSearch, p.service_mut());
    for c in "(".chars() {
        app.apply(Action::Input(c), p.service_mut());
    }
    let status = app.status.clone().unwrap_or_default();
    assert!(status.contains("search failed"), "status: {status:?}");
    assert!(app.memories.is_empty());
    app.apply(Action::Escape, p.service_mut());

    // A scope with no read namespaces must stay harmless under every key.
    let (_empty_guard, mut empty) = profile(
        "default_namespace = \"ns-a\"\nread_namespaces = []\nwrite_namespaces = []\nallow_global = false\nallowed_operations = [\"all\"]\n",
    );
    let mut app = App::new(empty.service_ref());
    assert!(app.namespaces.is_empty());
    for action in [
        Action::NextRow,
        Action::PrevRow,
        Action::NextPane,
        Action::Confirm,
        Action::Escape,
        Action::Backspace,
    ] {
        app.apply(action, empty.service_mut());
    }
    assert!(app.memories.is_empty());
    assert!(app.detail.is_none());
}
