//! T3 headless tests for the `mnexis tui` write forms (save `n`, update `e`).
//!
//! Every assertion goes through the model (`App`/`Form`) and the public
//! `MemoryService` surface: nothing here reads SQL or a terminal.

use mnexis_core::application::{ListFilter, SaveRequest, UpdateRequest};
use mnexis_core::cli::commands;
use mnexis_core::cli::profile::{Profile, now_rfc3339, uuid_v4};
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::tui::app::{Action, App, Mode};
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
    save_tagged(profile, ns, content, Vec::new())
}

fn save_tagged(profile: &mut Profile, ns: &str, content: &str, tags: Vec<String>) -> String {
    let uuid = uuid_v4().unwrap();
    profile
        .service_mut()
        .save(SaveRequest {
            uuid: uuid.clone(),
            title: String::new(),
            topic_key: String::new(),
            namespace: ns.to_string(),
            memory_type: Some("fact".to_string()),
            content: content.to_string(),
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
            valid_from: None,
            valid_until: None,
            tags,
            idempotency_key: None,
            keywords: Vec::new(),
            replace_keywords: false,
            state: MemoryState::Active,
            now: now_rfc3339(),
        })
        .expect("save memory");
    uuid
}

fn list(profile: &Profile, ns: &str) -> Vec<mnexis_core::domain::memory::Memory> {
    profile
        .service_ref()
        .list(ListFilter {
            memory_types: Vec::new(),

            namespace: Some(ns.to_string()),
            state: None,
            value: None,
            limit: 100,
            offset: 0,
        })
        .expect("list memories")
}

fn type_text(app: &mut App, profile: &mut Profile, text: &str) {
    for c in text.chars() {
        app.apply(Action::Input(c), profile.service_mut());
    }
}

fn backspace(app: &mut App, profile: &mut Profile, times: usize) {
    for _ in 0..times {
        app.apply(Action::Backspace, profile.service_mut());
    }
}

fn next_fields(app: &mut App, profile: &mut Profile, times: usize) {
    for _ in 0..times {
        app.apply(Action::NextField, profile.service_mut());
    }
}

fn form_error(app: &App) -> String {
    app.form
        .as_ref()
        .and_then(|f| f.error.clone())
        .unwrap_or_default()
}

#[test]
fn save_form_prefills_defaults_and_writes_through_the_service() {
    let (_guard, mut p) = profile(SCOPE);
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSave, p.service_mut());
    assert_eq!(app.mode, Mode::Form, "n opens the save form");
    let form = app.form.as_ref().expect("save form");
    assert_eq!(
        form.value("namespace"),
        "ns-a",
        "current selection prefilled"
    );
    assert_eq!(form.value("type"), "fact");
    assert_eq!(form.value("basis"), "user_stated");
    assert_eq!(form.value("state"), "active");

    // Enter inside the multi-line content field inserts a newline...
    type_text(&mut app, &mut p, "graph engine note");
    app.apply(Action::Confirm, p.service_mut());
    type_text(&mut app, &mut p, "second line");
    assert_eq!(
        app.form.as_ref().unwrap().value("content"),
        "graph engine note\nsecond line"
    );
    // ...and Tab (NextField) moves to the next field.
    app.apply(Action::NextField, p.service_mut());
    assert_eq!(
        app.form.as_ref().unwrap().focused().unwrap().label,
        "namespace"
    );
    // Tags: namespace -> type -> basis -> topic_key -> idempotency_key ->
    // tags.
    next_fields(&mut app, &mut p, 5);
    assert_eq!(app.form.as_ref().unwrap().focused().unwrap().label, "tags");
    type_text(&mut app, &mut p, "alpha, beta");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Normal, "the form closes on success");
    assert!(app.form.is_none());
    let status = app.status.clone().unwrap_or_default();
    assert!(status.starts_with("saved "), "status: {status:?}");

    let stored = list(&p, "ns-a");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].content, "graph engine note\nsecond line");
    assert_eq!(stored[0].memory_type, "fact");
    assert_eq!(stored[0].assertion_basis, "user_stated");
    assert_eq!(stored[0].state, MemoryState::Active);
    assert_eq!(app.memories.len(), 1, "the list refreshed");
    assert_eq!(app.memories[0].uuid, stored[0].uuid);
    assert_eq!(
        app.detail.as_ref().map(|m| m.uuid.clone()),
        Some(stored[0].uuid.clone()),
        "the new memory is selected"
    );
}

#[test]
fn save_form_enter_on_the_last_field_submits() {
    let (_guard, mut p) = profile(SCOPE);
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSave, p.service_mut());
    type_text(&mut app, &mut p, "submitted with enter");
    for _ in 0..7 {
        app.apply(Action::NextField, p.service_mut());
    }
    assert_eq!(app.form.as_ref().unwrap().focused().unwrap().label, "state");
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(app.mode, Mode::Normal, "Enter on the last field submits");
    assert_eq!(list(&p, "ns-a").len(), 1);
}

#[test]
fn save_form_validation_blocks_blank_content_and_bad_state() {
    let (_guard, mut p) = profile(SCOPE);
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSave, p.service_mut());
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(form_error(&app), "content is required");
    assert!(
        app.status
            .clone()
            .unwrap_or_default()
            .contains("content is required")
    );
    assert_eq!(app.mode, Mode::Form, "the form stays open");
    assert!(list(&p, "ns-a").is_empty(), "nothing was written");

    // Fill the content and break the state instead.
    type_text(&mut app, &mut p, "keep me");
    next_fields(&mut app, &mut p, 7);
    backspace(&mut app, &mut p, "active".len());
    type_text(&mut app, &mut p, "bogus");
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(form_error(&app), "state must be one of active, proposed");
    assert!(list(&p, "ns-a").is_empty(), "nothing was written");
    assert_eq!(
        app.form.as_ref().unwrap().value("content"),
        "keep me",
        "entered values survive a rejection"
    );
}

#[test]
fn save_form_surfaces_service_validation_errors() {
    let (_guard, mut p) = profile(SCOPE);
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSave, p.service_mut());
    type_text(&mut app, &mut p, "content with a bad topic key");
    // content -> namespace -> type -> basis -> topic_key
    next_fields(&mut app, &mut p, 4);
    assert_eq!(
        app.form.as_ref().unwrap().focused().unwrap().label,
        "topic_key"
    );
    type_text(&mut app, &mut p, "Bad Key");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Form, "service errors keep the form open");
    let error = form_error(&app);
    assert!(error.contains("invalid argument"), "error: {error:?}");
    assert!(error.contains("topic_key"), "error: {error:?}");
    assert!(list(&p, "ns-a").is_empty(), "nothing was written");
}

#[test]
fn update_form_prefills_and_writes_a_new_revision() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "first wording");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartUpdate, p.service_mut());
    let form = app.form.as_ref().expect("update form");
    assert_eq!(form.value("content"), "first wording");
    assert_eq!(form.value("state"), "active");
    assert_eq!(form.value("basis"), "user_stated");
    assert_eq!(
        form.value("expected_revision"),
        "1",
        "prefilled from the load"
    );

    backspace(&mut app, &mut p, "first wording".len());
    type_text(&mut app, &mut p, "second wording");
    // content -> state -> tags -> basis -> reason
    next_fields(&mut app, &mut p, 4);
    assert_eq!(
        app.form.as_ref().unwrap().focused().unwrap().label,
        "reason"
    );
    type_text(&mut app, &mut p, "reword");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Normal);
    let stored = p.service_ref().get(&uuid).unwrap().unwrap();
    assert_eq!(stored.revision, 2);
    assert_eq!(stored.content, "second wording");
    let status = app.status.clone().unwrap_or_default();
    assert!(status.contains("to revision 2"), "status: {status:?}");
    assert_eq!(
        app.history.first().map(|h| h.revision),
        Some(1),
        "the detail history shows the revision the update replaced"
    );
}

#[test]
fn update_form_rejects_blank_reason_and_bad_revision() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "kept");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartUpdate, p.service_mut());
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(form_error(&app), "reason is required");
    assert_eq!(p.service_ref().get(&uuid).unwrap().unwrap().revision, 1);

    // content -> state -> tags -> basis -> reason
    next_fields(&mut app, &mut p, 4);
    type_text(&mut app, &mut p, "because");
    next_fields(&mut app, &mut p, 1);
    backspace(&mut app, &mut p, 1);
    type_text(&mut app, &mut p, "x");
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(form_error(&app), "expected_revision must be a whole number");
    assert_eq!(p.service_ref().get(&uuid).unwrap().unwrap().revision, 1);
}

#[test]
fn update_form_surfaces_revision_conflict_with_the_revision_involved() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "first");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartUpdate, p.service_mut());

    // Another writer moves the memory to revision 2 after the form loaded.
    p.service_mut()
        .update(UpdateRequest {
            memory_type: None,

            uuid: uuid.clone(),
            expected_revision: 1,
            content: Some("elsewhere".to_string()),
            state: None,
            tags: None,
            assertion_basis: None,
            reason: "other writer".to_string(),
            now: now_rfc3339(),
        })
        .unwrap();
    next_fields(&mut app, &mut p, 4);
    type_text(&mut app, &mut p, "reword");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Form, "a conflict never closes the form");
    let error = form_error(&app);
    assert!(error.contains("revision conflict"), "error: {error:?}");
    assert!(
        error.contains("current is 2"),
        "the conflict reports the revision actually stored: {error:?}"
    );
    assert!(
        error.contains("loaded revision is 2"),
        "the conflict names the revision on disk: {error:?}"
    );
    assert!(
        error.contains("expected revision 1"),
        "the conflict names what the form expected: {error:?}"
    );
    assert!(
        app.form.as_ref().unwrap().reload_offered,
        "the conflict offers a reload"
    );
    assert!(
        app.status
            .clone()
            .unwrap_or_default()
            .contains("revision conflict")
    );

    // The offer: Ctrl-R re-reads the memory and refreshes the expectation.
    app.apply(Action::ReloadForm, p.service_mut());
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.value("expected_revision"), "2");
    assert_eq!(form.value("content"), "first", "user edits are kept");
    assert!(form.error.is_none(), "the reload clears the error");
    assert!(!form.reload_offered);
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    let stored = p.service_ref().get(&uuid).unwrap().unwrap();
    assert_eq!(stored.revision, 3);
    assert_eq!(stored.content, "first");
}

#[test]
fn update_form_prefills_tags_and_blank_still_keeps_the_current_set() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save_tagged(
        &mut p,
        "ns-a",
        "tagged memory",
        vec!["alpha".to_string(), "beta".to_string()],
    );
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartUpdate, p.service_mut());
    assert_eq!(
        app.form.as_ref().unwrap().value("tags"),
        "alpha, beta",
        "the update form prefills the tags read through the service"
    );

    // Blanking the field means "keep current", exactly like `update` without
    // `--tags`: the service drops an empty tag list.
    next_fields(&mut app, &mut p, 2); // content -> state -> tags
    assert_eq!(app.form.as_ref().unwrap().focused().unwrap().label, "tags");
    backspace(&mut app, &mut p, "alpha, beta".len());
    assert_eq!(app.form.as_ref().unwrap().value("tags"), "");
    next_fields(&mut app, &mut p, 2); // tags -> basis -> reason
    type_text(&mut app, &mut p, "keep the tags");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Normal);
    let tags: Vec<String> = p
        .service_ref()
        .tags(&uuid)
        .unwrap()
        .into_iter()
        .map(|t| t.tag)
        .collect();
    assert_eq!(tags, vec!["alpha", "beta"]);

    // A non-blank tag list replaces the set.
    app.apply(Action::StartUpdate, p.service_mut());
    assert_eq!(app.form.as_ref().unwrap().value("tags"), "alpha, beta");
    next_fields(&mut app, &mut p, 2);
    backspace(&mut app, &mut p, "alpha, beta".len());
    type_text(&mut app, &mut p, "gamma");
    next_fields(&mut app, &mut p, 2);
    type_text(&mut app, &mut p, "retag");
    app.apply(Action::Submit, p.service_mut());
    let tags: Vec<String> = p
        .service_ref()
        .tags(&uuid)
        .unwrap()
        .into_iter()
        .map(|t| t.tag)
        .collect();
    assert_eq!(tags, vec!["gamma"]);
}

#[test]
fn escape_cancels_each_form_without_writing() {
    let (_guard, mut p) = profile(SCOPE);
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSave, p.service_mut());
    type_text(&mut app, &mut p, "never written");
    app.apply(Action::Escape, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.form.is_none());
    assert!(list(&p, "ns-a").is_empty(), "Esc wrote nothing");

    let uuid = save(&mut p, "ns-a", "kept");
    app.reload_memories(p.service_mut());
    app.apply(Action::StartUpdate, p.service_mut());
    backspace(&mut app, &mut p, "kept".len());
    type_text(&mut app, &mut p, "changed");
    app.apply(Action::Escape, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.form.is_none());
    let stored = p.service_ref().get(&uuid).unwrap().unwrap();
    assert_eq!(stored.revision, 1, "Esc wrote nothing");
    assert_eq!(stored.content, "kept");
}
