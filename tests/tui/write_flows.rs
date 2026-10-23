//! T4 headless tests for the supersede (`s`), delete (`d`), and restore (`r`)
//! flows, the deleted view (`x`), and the detail chain the flows refresh.
//!
//! Every assertion goes through the model (`App`) and the public
//! `MemoryService` surface: nothing here reads SQL or a terminal.

use mnexis_core::application::{SaveRequest, UpdateRequest};
use mnexis_core::cli::commands;
use mnexis_core::cli::profile::{Profile, now_rfc3339, uuid_v4};
use mnexis_core::domain::memory::MemoryState;
use mnexis_core::tui::app::{Action, App, ConfirmKind, Mode};
use mnexis_core::tui::form::FormKind;
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
            memory_type: "fact".to_string(),
            content: content.to_string(),
            actor: "user".to_string(),
            channel: "cli".to_string(),
            assertion_basis: "user_stated".to_string(),
            valid_from: None,
            valid_until: None,
            tags,
            idempotency_key: None,
            state: MemoryState::Active,
            now: now_rfc3339(),
        })
        .expect("save memory");
    uuid
}

fn type_text(app: &mut App, profile: &mut Profile, text: &str) {
    for c in text.chars() {
        app.apply(Action::Input(c), profile.service_mut());
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

fn status(app: &App) -> String {
    app.status.clone().unwrap_or_default()
}

#[test]
fn supersede_picks_a_newer_memory_and_links_both_through_the_service() {
    let (_guard, mut p) = profile(SCOPE);
    let older = save(&mut p, "ns-a", "first wording");
    let newer = save(&mut p, "ns-a", "second wording");
    let mut app = App::new(p.service_ref());
    assert_eq!(app.memories.len(), 2);

    // `s` starts the picker from the selected memory as the older side.
    app.apply(Action::StartSupersede, p.service_mut());
    assert_eq!(app.mode, Mode::Pick);
    assert_eq!(app.supersede_older.as_deref(), Some(older.as_str()));
    assert!(status(&app).contains("pick the newer memory"));

    // j/k moves through the list and Enter opens the supersede form.
    app.apply(Action::NextRow, p.service_mut());
    assert_eq!(app.memories[app.mem_index].uuid, newer);
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(app.mode, Mode::Form);
    let form = app.form.as_ref().expect("supersede form");
    assert_eq!(form.kind, FormKind::Supersede);
    assert_eq!(form.value("older_uuid"), older);
    assert_eq!(form.value("newer_uuid"), newer);
    assert_eq!(
        form.value("expected_old_revision"),
        "1",
        "prefilled from the load"
    );
    assert_eq!(
        form.value("expected_new_revision"),
        "1",
        "prefilled from the load"
    );

    // The reason is required: submitting blank leaves the database alone.
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(app.mode, Mode::Form);
    assert_eq!(form_error(&app), "reason is required");
    assert!(p.service_ref().links(&older).unwrap().is_empty());

    next_fields(&mut app, &mut p, 2); // older_uuid -> newer_uuid -> reason
    assert_eq!(
        app.form.as_ref().unwrap().focused().unwrap().label,
        "reason"
    );
    type_text(&mut app, &mut p, "new wording wins");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Normal, "the form closes on success");
    assert!(app.form.is_none());
    assert!(
        status(&app).contains("superseded"),
        "status: {:?}",
        status(&app)
    );

    // The write landed through the service: older superseded plus one link.
    let stored = p.service_ref().get(&older).unwrap().unwrap();
    assert_eq!(stored.state, MemoryState::Superseded);
    let chain = p.service_ref().links(&older).unwrap();
    assert_eq!(chain.len(), 1, "one supersession link: {chain:?}");
    assert_eq!(chain[0].older_uuid, older);
    assert_eq!(chain[0].newer_uuid, newer);
    // The detail pane refreshed with the real chain, not a placeholder.
    assert_eq!(app.chain, chain);
    assert_eq!(app.detail.as_ref().map(|m| m.uuid.clone()), Some(older));
}

#[test]
fn supersede_conflict_names_the_revision_on_disk_and_reload_recovers() {
    let (_guard, mut p) = profile(SCOPE);
    let older = save(&mut p, "ns-a", "first");
    save(&mut p, "ns-a", "second");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSupersede, p.service_mut());
    app.apply(Action::NextRow, p.service_mut());
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(
        app.form.as_ref().unwrap().value("expected_old_revision"),
        "1"
    );

    // Another writer moves the older memory to revision 2 after the form loaded.
    p.service_mut()
        .update(UpdateRequest {
            uuid: older.clone(),
            expected_revision: 1,
            content: Some("elsewhere".to_string()),
            state: None,
            tags: None,
            assertion_basis: None,
            reason: "other writer".to_string(),
            now: now_rfc3339(),
        })
        .unwrap();

    next_fields(&mut app, &mut p, 2);
    type_text(&mut app, &mut p, "stale expectation");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Form, "a conflict never closes the form");
    let error = form_error(&app);
    assert!(error.contains("revision conflict"), "error: {error:?}");
    assert!(
        error.contains("current is 2"),
        "the conflict reports the row actually on disk: {error:?}"
    );
    assert!(
        error.contains("expected older revision 1"),
        "the conflict names what the form expected: {error:?}"
    );
    assert!(app.form.as_ref().unwrap().reload_offered);
    assert!(
        p.service_ref().links(&older).unwrap().is_empty(),
        "nothing written"
    );

    // Ctrl-R refreshes the expectation and keeps the typed reason.
    app.apply(Action::ReloadForm, p.service_mut());
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.value("expected_old_revision"), "2");
    assert_eq!(form.value("expected_new_revision"), "1");
    assert_eq!(form.value("reason"), "stale expectation");
    assert!(form.error.is_none());

    app.apply(Action::Submit, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(p.service_ref().links(&older).unwrap().len(), 1);
    assert_eq!(
        p.service_ref().get(&older).unwrap().unwrap().state,
        MemoryState::Superseded
    );
}

#[test]
fn supersede_conflict_on_the_newer_side_is_surfaced_and_reload_recovers() {
    let (_guard, mut p) = profile(SCOPE);
    save(&mut p, "ns-a", "first");
    let newer = save(&mut p, "ns-a", "second");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSupersede, p.service_mut());
    app.apply(Action::NextRow, p.service_mut());
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(
        app.form.as_ref().unwrap().value("expected_new_revision"),
        "1"
    );

    // Another writer moves the newer memory to revision 2 after the pick.
    p.service_mut()
        .update(UpdateRequest {
            uuid: newer.clone(),
            expected_revision: 1,
            content: Some("elsewhere".to_string()),
            state: None,
            tags: None,
            assertion_basis: None,
            reason: "other writer".to_string(),
            now: now_rfc3339(),
        })
        .unwrap();

    next_fields(&mut app, &mut p, 2);
    type_text(&mut app, &mut p, "stale picker");
    app.apply(Action::Submit, p.service_mut());

    assert_eq!(app.mode, Mode::Form, "a conflict never closes the form");
    let error = form_error(&app);
    assert!(error.contains("revision conflict"), "error: {error:?}");
    assert!(error.contains("current is 2"), "error: {error:?}");
    assert!(
        error.contains("expected newer revision 1"),
        "the conflict names the side that drifted: {error:?}"
    );
    assert!(app.form.as_ref().unwrap().reload_offered);
    assert!(
        p.service_ref().links(&newer).unwrap().is_empty(),
        "nothing written"
    );

    // Ctrl-R refreshes both sides and the retry succeeds.
    app.apply(Action::ReloadForm, p.service_mut());
    assert_eq!(
        app.form.as_ref().unwrap().value("expected_new_revision"),
        "2"
    );
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(p.service_ref().links(&newer).unwrap().len(), 1);
}

#[test]
fn supersede_cancel_paths_write_nothing() {
    let (_guard, mut p) = profile(SCOPE);
    let older = save(&mut p, "ns-a", "first");
    save(&mut p, "ns-a", "second");
    let mut app = App::new(p.service_ref());

    // Esc in the picker cancels before any form opens.
    app.apply(Action::StartSupersede, p.service_mut());
    assert_eq!(app.mode, Mode::Pick);
    app.apply(Action::Escape, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.supersede_older.is_none());
    assert_eq!(status(&app), "cancelled, nothing written");
    assert!(p.service_ref().links(&older).unwrap().is_empty());

    // Esc in the form cancels after the pick.
    app.apply(Action::StartSupersede, p.service_mut());
    app.apply(Action::NextRow, p.service_mut());
    app.apply(Action::Confirm, p.service_mut());
    assert_eq!(app.mode, Mode::Form);
    app.apply(Action::Escape, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.form.is_none());
    assert!(
        p.service_ref().links(&older).unwrap().is_empty(),
        "nothing written"
    );
    assert_eq!(p.service_ref().get(&older).unwrap().unwrap().revision, 1);
}

#[test]
fn supersede_refuses_a_self_link_without_writing() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "only one");
    let mut app = App::new(p.service_ref());
    app.apply(Action::StartSupersede, p.service_mut());
    app.apply(Action::Confirm, p.service_mut()); // pick the same memory
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.value("older_uuid"), uuid);
    assert_eq!(form.value("newer_uuid"), uuid);

    next_fields(&mut app, &mut p, 2);
    type_text(&mut app, &mut p, "self");
    app.apply(Action::Submit, p.service_mut());
    assert_eq!(app.mode, Mode::Form, "the form stays open");
    assert_eq!(form_error(&app), "older_uuid and newer_uuid must differ");
    assert!(p.service_ref().links(&uuid).unwrap().is_empty());
    assert_eq!(
        p.service_ref().get(&uuid).unwrap().unwrap().state,
        MemoryState::Active
    );
}

#[test]
fn delete_and_restore_flow_confirms_each_write_and_refreshes_the_list() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "to delete");
    let mut app = App::new(p.service_ref());

    // `d` opens the dialog naming the row and its revision; Esc cancels.
    app.apply(Action::StartDelete, p.service_mut());
    assert_eq!(app.mode, Mode::Confirm);
    let pending = app.confirm.as_ref().expect("dialog");
    assert!(pending.prompt.contains("soft-delete"), "{}", pending.prompt);
    assert!(pending.prompt.contains("revision 1"), "{}", pending.prompt);
    assert_eq!(
        pending.action,
        ConfirmKind::Delete {
            uuid: uuid.clone(),
            revision: 1
        }
    );
    app.apply(Action::Escape, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.confirm.is_none());
    assert_eq!(status(&app), "cancelled, nothing written");
    assert!(
        p.service_ref().get(&uuid).unwrap().is_some(),
        "Esc wrote nothing"
    );

    // `n` also cancels; `y` soft-deletes and refreshes the list.
    app.apply(Action::StartDelete, p.service_mut());
    app.apply(Action::ConfirmNo, p.service_mut());
    assert!(
        p.service_ref().get(&uuid).unwrap().is_some(),
        "n wrote nothing"
    );
    app.apply(Action::StartDelete, p.service_mut());
    app.apply(Action::ConfirmYes, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(p.service_ref().get(&uuid).unwrap().is_none());
    let deleted = p
        .service_ref()
        .get_including_deleted(&uuid)
        .unwrap()
        .unwrap();
    assert!(deleted.deleted);
    assert_eq!(deleted.revision, 2, "soft_delete bumped the revision");
    assert!(app.memories.is_empty(), "the live list refreshed");
    assert!(
        status(&app).contains("soft-deleted"),
        "status: {:?}",
        status(&app)
    );

    // `x` shows the deleted row, and the detail pane renders it.
    app.apply(Action::ToggleDeleted, p.service_mut());
    assert!(app.deleted_view);
    assert_eq!(app.memories.len(), 1);
    assert_eq!(app.memories[0].uuid, uuid);
    assert!(app.detail.as_ref().unwrap().deleted);
    assert!(
        app.history.is_empty(),
        "history stays hidden for deleted rows"
    );

    // `r` cancels on Esc, then restores on `y` and leaves the deleted view.
    app.apply(Action::StartRestore, p.service_mut());
    assert_eq!(app.mode, Mode::Confirm);
    assert_eq!(
        app.confirm.as_ref().unwrap().action,
        ConfirmKind::Restore { uuid: uuid.clone() }
    );
    app.apply(Action::Escape, p.service_mut());
    assert!(
        p.service_ref().get(&uuid).unwrap().is_none(),
        "Esc wrote nothing"
    );

    app.apply(Action::StartRestore, p.service_mut());
    app.apply(Action::ConfirmYes, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(!app.deleted_view, "restore returns to the live view");
    let restored = p.service_ref().get(&uuid).unwrap().expect("live again");
    assert!(!restored.deleted);
    assert_eq!(restored.revision, 3, "restore bumped the revision");
    assert_eq!(app.memories.len(), 1, "the live list refreshed");
    assert_eq!(app.memories[0].uuid, uuid);
    assert_eq!(app.detail.as_ref().map(|m| m.uuid.clone()), Some(uuid));
    assert!(
        status(&app).contains("restored"),
        "status: {:?}",
        status(&app)
    );
}

#[test]
fn write_flows_refuse_out_of_context_and_stray_keys_never_write() {
    let (_guard, mut p) = profile(SCOPE);
    let uuid = save(&mut p, "ns-a", "kept");
    let mut app = App::new(p.service_ref());

    // `r` outside the deleted view explains the way in and writes nothing.
    app.apply(Action::StartRestore, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(app.confirm.is_none());
    assert!(
        status(&app).contains("press x"),
        "status: {:?}",
        status(&app)
    );

    // Neither `d` nor `s` is allowed from inside the deleted view.
    app.apply(Action::ToggleDeleted, p.service_mut());
    app.apply(Action::StartDelete, p.service_mut());
    assert!(app.confirm.is_none());
    assert!(status(&app).contains("already in the deleted view"));
    app.apply(Action::StartSupersede, p.service_mut());
    assert_eq!(app.mode, Mode::Normal);
    assert!(status(&app).contains("supersede needs an active memory"));

    // Inside a dialog no key other than y/n answers it.
    app.apply(Action::ToggleDeleted, p.service_mut());
    assert!(!app.deleted_view);
    app.apply(Action::StartDelete, p.service_mut());
    for action in [
        Action::NextRow,
        Action::ToggleDeleted,
        Action::Input('x'),
        Action::Submit,
        Action::Noop,
    ] {
        app.apply(action, p.service_mut());
    }
    assert_eq!(
        app.mode,
        Mode::Confirm,
        "a stray key never answers the dialog"
    );
    assert!(
        p.service_ref().get(&uuid).unwrap().is_some(),
        "nothing written"
    );
    app.apply(Action::ConfirmNo, p.service_mut());
    assert!(p.service_ref().get(&uuid).unwrap().is_some());
}
