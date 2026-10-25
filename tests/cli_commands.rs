//! T19 — CLI integration tests (PRD §E2.6, e2-core §7).
//!
//! Every test drives the real `mnexis` binary via `CARGO_BIN_EXE_mnexis`
//! against a fresh profile under a tempdir inside `target/` (the repo-local
//! TMPDIR/quota mitigation). Exit codes follow e2-core §8: 0 success,
//! 1 application error, 2 INVALID_ARGUMENT, 3 NOT_FOUND, 4 FORBIDDEN,
//! 5 REVISION_CONFLICT/TOPIC_CONFLICT, 6 INDEX_UNAVAILABLE,
//! 7 BUDGET_TOO_SMALL, 8 CURSOR_EXPIRED, 9 UNSUPPORTED_VERSION, 10 BUSY.

use std::process::{Command, Output, Stdio};
use tempfile::tempdir_in;

struct Profile {
    data_dir: std::path::PathBuf,
    #[allow(dead_code)]
    guard: tempfile::TempDir,
}

impl Profile {
    fn new() -> Self {
        let guard = tempdir_in("target").expect("tempdir under target/");
        let profile = Self {
            data_dir: guard.path().to_path_buf(),
            guard,
        };
        // Every data command requires an initialised profile (the design
        // keeps `init` explicit), so bootstrap it like a real user would.
        profile
            .run_silent(&["init"])
            .unwrap_status("init bootstrap");
        profile
    }

    fn restricted(self) -> Self {
        self.with_config(
            "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\"]\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
        )
    }

    /// A profile directory without `mnexis.toml`: the commands that edit
    /// an existing profile must refuse it instead of creating one.
    fn without_init() -> Self {
        let guard = tempdir_in("target").expect("tempdir under target/");
        Self {
            data_dir: guard.path().to_path_buf(),
            guard,
        }
    }

    fn with_config(self, toml: &str) -> Self {
        std::fs::write(self.data_dir.join("mnexis.toml"), toml).expect("write mnexis.toml");
        self
    }

    fn namespaces(names: &[&str]) -> String {
        let list = names
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "default_namespace = \"{first}\"\nread_namespaces = [{list}]\nwrite_namespaces = [{list}]\nallow_global = true\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
            first = names[0]
        )
    }

    /// Run with an explicit working directory, without `--json`: the human
    /// rendering of a command that resolves a workspace.
    fn run_text_in_dir(&self, dir: &std::path::Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .current_dir(dir)
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }

    /// Run with stdin not inherited (default: null) so confirmation
    /// prompts never block the test runner.
    fn run_silent(&self, args: &[&str]) -> Output {
        self.run_with(Stdio::null(), args)
    }

    fn run_with(&self, stdin: Stdio, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--json")
            .args(args)
            .stdin(stdin)
            .output()
            .expect("spawn mnexis binary")
    }

    /// Run without `--json`: the human rendering is what a person reads, and
    /// it has to carry the identity too.
    fn run_text(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }

    /// Run with an explicit working directory. The workspace guard reads the
    /// `.mnexis-namespace` marker (and git) from there, so a test that
    /// exercises it has to choose the directory instead of inheriting the
    /// one cargo runs the test binary in.
    fn run_in_dir(&self, dir: &std::path::Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .current_dir(dir)
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let text = String::from_utf8(out.stdout.clone()).expect("utf-8 stdout");
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!("stdout is not JSON ({e}): {text:?}\nstderr: {:?}", {
            String::from_utf8_lossy(&out.stderr)
        })
    })
}

/// Save a memory and return its uuid (from the JSON envelope).
fn save(profile: &Profile, content: &str, extra: &[&str]) -> String {
    let mut args = vec!["save", "--type", "fact"];
    args.extend_from_slice(extra);
    args.push(content);
    let out = profile.run_silent(&args);
    assert!(
        out.status.success(),
        "save failed: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout_json(&out)["uuid"]
        .as_str()
        .expect("uuid in save output")
        .to_string()
}

/// A profile whose scope allows the given namespaces (plus `default`).
fn namespaced_profile(names: &[&str]) -> Profile {
    let mut all: Vec<&str> = names.to_vec();
    all.push("default");
    Profile::new().with_config(&Profile::namespaces(&all))
}

/// A content with a carriage return: on its own it does not end the line,
/// but a terminal reads it as "back to the start of the line" and the fact
/// that follows is painted over. Every renderer that prints one fact per
/// line folds it.
#[test]
fn a_carriage_return_in_a_content_stays_on_one_line() {
    let p = namespaced_profile(&["proyecto-a"]);
    save(
        &p,
        "primera linea\rsegunda linea",
        &["--namespace", "proyecto-a"],
    );

    let out = p
        .run_text(&["list", "--namespace", "proyecto-a"])
        .unwrap_status("list");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains('\r'),
        "no carriage return reaches the terminal: {text:?}"
    );
    // The fact is one physical line: if the break had survived, the second
    // half would sit on a line of its own.
    let first = text.lines().next().expect("a fact line");
    assert!(
        first.contains("primera linea segunda linea"),
        "the break became a space and the fact stayed on one line: {text:?}"
    );
}

/// The timeline's header names the fact: a title with a line break must not
/// split it into two.
#[test]
fn a_title_with_a_line_break_keeps_the_timeline_header_on_one_line() {
    let p = namespaced_profile(&["proyecto-a"]);
    let uuid = save(
        &p,
        "el contenido",
        &["--namespace", "proyecto-a", "--title", "titulo\ncon salto"],
    );

    let out = p
        .run_text(&["timeline", &uuid])
        .unwrap_status("timeline");
    let text = String::from_utf8_lossy(&out.stdout);
    let header = text.lines().next().expect("a header line");
    assert!(
        header.contains("titulo con salto"),
        "the break became a space in the header: {text:?}"
    );
    assert!(
        header.contains(&uuid),
        "the header is the fact's identity: {text:?}"
    );
}

// --- version / init ---
#[test]
fn version_prints_name_and_semver() {
    let out = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("version")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("mnexis"), "stdout: {text}");
    // The version is the crate's own, not a literal: a release bump must
    // not have to touch this test to keep it true.
    assert!(text.contains(env!("CARGO_PKG_VERSION")), "stdout: {text}");
}

#[test]
fn init_creates_profile_files() {
    let p = Profile::new();
    let out = p.run_silent(&["init"]);
    assert!(
        out.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(p.data_dir.join("mnexis.db").exists());
    assert!(p.data_dir.join("mnexis.toml").exists());
}

#[test]
fn init_is_idempotent_and_keeps_data() {
    let p = Profile::new();
    p.run_silent(&["init"]).unwrap_status("first init");
    let uuid = save(&p, "survives re-init", &[]);
    let out = p.run_silent(&["init"]);
    assert!(out.status.success());
    let show = p.run_silent(&["show", &uuid]);
    assert!(show.status.success());
}

trait UnwrapStatus {
    fn unwrap_status(self, ctx: &str) -> Output;
}

impl UnwrapStatus for Output {
    fn unwrap_status(self, ctx: &str) -> Output {
        assert!(
            self.status.success(),
            "{ctx} failed ({}): {:?}",
            self.status,
            String::from_utf8_lossy(&self.stderr)
        );
        self
    }
}

// --- save / show / list ---

#[test]
fn save_show_round_trip() {
    let p = namespaced_profile(&["proyecto-a"]);
    let uuid = save(
        &p,
        "SQLite is the canonical store",
        &["--namespace", "proyecto-a"],
    );
    let out = p.run_silent(&["show", &uuid]).unwrap_status("show");
    let json = stdout_json(&out);
    assert_eq!(json["content"], "SQLite is the canonical store");
    assert_eq!(json["namespace"], "proyecto-a");
    assert_eq!(json["memory_type"], "fact");
    assert_eq!(json["state"], "active");
    assert_eq!(json["revision"], 1);
}

#[test]
fn save_generates_uuid_v4() {
    let p = Profile::new();
    let uuid = save(&p, "uuid shape", &[]);
    let parts: Vec<&str> = uuid.split('-').collect();
    assert_eq!(parts.len(), 5, "uuid: {uuid}");
    assert_eq!(parts[0].len(), 8);
    assert_eq!(parts[1].len(), 4);
    assert_eq!(parts[2].len(), 4);
    assert_eq!(parts[3].len(), 4);
    assert_eq!(parts[4].len(), 12);
    assert!(uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    // RFC 4122 version nibble: 4 in the first hex digit of the third group.
    assert!(parts[2].starts_with('4'), "uuid: {uuid}");
}

#[test]
fn list_shows_saved_memory_and_count() {
    let p = namespaced_profile(&["proyecto-a"]);
    save(&p, "listable content", &["--namespace", "proyecto-a"]);
    let out = p
        .run_silent(&["list", "--namespace", "proyecto-a"])
        .unwrap_status("list");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 1);
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1);
    // A list is a list: the identity of the memory and a preview of it.
    assert_eq!(items[0]["preview"], "listable content");
    assert!(items[0]["uuid"].is_string(), "the item names the memory");
    assert_eq!(items[0]["namespace"], "proyecto-a");
    assert!(
        items[0].get("content").is_none(),
        "the whole content is not in the list: {items:?}"
    );
}

#[test]
fn list_full_asks_for_the_whole_content() {
    let p = namespaced_profile(&["proyecto-a"]);
    save(&p, "listable content", &["--namespace", "proyecto-a"]);
    let out = p
        .run_silent(&["list", "--namespace", "proyecto-a", "--full"])
        .unwrap_status("list --full");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items[0]["content"], "listable content");
}

#[test]
fn a_long_memory_is_previewed_and_the_cut_is_marked() {
    let p = Profile::new();
    let long = "una frase bastante larga que se repite. ".repeat(40);
    save(&p, &long, &[]);
    let out = p.run_silent(&["list"]).unwrap_status("list");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items array");
    let preview = items[0]["preview"].as_str().expect("preview");
    assert!(
        preview.ends_with('\u{2026}'),
        "the cut is marked: {preview}"
    );
    assert!(
        preview.chars().count() < long.chars().count(),
        "the preview is shorter than the content"
    );
}

#[test]
fn list_state_filter_excludes_other_states() {
    let p = Profile::new();
    save(&p, "active one", &[]);
    let out = p
        .run_silent(&["list", "--state", "archived"])
        .unwrap_status("list");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 0);
}

// --- update / history ---

#[test]
fn update_bumps_revision_and_writes_history() {
    let p = Profile::new();
    let uuid = save(&p, "first revision", &[]);
    let out = p
        .run_silent(&[
            "update",
            &uuid,
            "--content",
            "second revision",
            "--expected-revision",
            "1",
            "--reason",
            "correct content",
        ])
        .unwrap_status("update");
    let json = stdout_json(&out);
    assert_eq!(json["revision"], 2);

    let show = p.run_silent(&["show", &uuid]).unwrap_status("show");
    assert_eq!(stdout_json(&show)["content"], "second revision");

    let hist = p.run_silent(&["history", &uuid]).unwrap_status("history");
    let hjson = stdout_json(&hist);
    // Service contract (pinned by tests/application_service.rs): history
    // holds one entry per update revision; the initial save is revision 1
    // and is not duplicated into the log.
    assert_eq!(hjson["count"], 1, "history: {hjson}");
    let items = hjson["items"].as_array().expect("items");
    assert_eq!(items[0]["revision"], 2);
    assert_eq!(items[0]["change_reason"], "correct content");
}

#[test]
fn update_wrong_expected_revision_exits_5() {
    let p = Profile::new();
    let uuid = save(&p, "original", &[]);
    let out = p.run_silent(&[
        "update",
        &uuid,
        "--content",
        "nope",
        "--expected-revision",
        "99",
        "--reason",
        "stale",
    ]);
    assert_eq!(out.status.code(), Some(5));
}

#[test]
fn show_specific_revision_uses_history() {
    // History holds one entry per UPDATE revision (the T17 contract); the
    // initial save is the canonical row, not a history row. Two updates
    // make revision 2 a historical revision worth showing.
    let p = Profile::new();
    let uuid = save(&p, "v1 content", &[]);
    p.run_silent(&[
        "update",
        &uuid,
        "--content",
        "v2 content",
        "--expected-revision",
        "1",
        "--reason",
        "r",
    ])
    .unwrap_status("update to v2");
    p.run_silent(&[
        "update",
        &uuid,
        "--content",
        "v3 content",
        "--expected-revision",
        "2",
        "--reason",
        "r",
    ])
    .unwrap_status("update to v3");
    let out = p
        .run_silent(&["show", &uuid, "--revision", "2"])
        .unwrap_status("show revision 2");
    assert_eq!(stdout_json(&out)["content"], "v2 content");
    let current = p.run_silent(&["show", &uuid]).unwrap_status("show current");
    assert_eq!(stdout_json(&current)["content"], "v3 content");
}

// --- search / explain / delete / restore ---

#[test]
fn search_finds_then_delete_removes_then_restore_reindexes() {
    let p = Profile::new();
    let uuid = save(&p, "petgraph replaced by custom engine", &[]);

    let found = p
        .run_silent(&["search", "petgraph", "--limit", "10"])
        .unwrap_status("search");
    let fjson = stdout_json(&found);
    assert_eq!(fjson["count"], 1, "search: {fjson}");

    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete");

    let after_delete = p
        .run_silent(&["search", "petgraph", "--limit", "10"])
        .unwrap_status("search after delete");
    assert_eq!(stdout_json(&after_delete)["count"], 0);

    p.run_silent(&["restore", &uuid]).unwrap_status("restore");

    let after_restore = p
        .run_silent(&["search", "petgraph", "--limit", "10"])
        .unwrap_status("search after restore");
    assert_eq!(stdout_json(&after_restore)["count"], 1);
}

#[test]
fn search_explain_includes_matched_terms() {
    let p = Profile::new();
    save(&p, "bellman ford shortest paths", &[]);
    let out = p
        .run_silent(&["search", "--explain", "bellman"])
        .unwrap_status("search --explain");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items");
    assert!(!items.is_empty());
    let terms = items[0]["matched_terms"].as_array().expect("matched_terms");
    assert!(!terms.is_empty(), "json: {json}");
}

#[test]
fn a_natural_language_query_finds_the_memory() {
    let p = Profile::new();
    save(&p, "El servicio escucha en el puerto 8081", &[]);

    // The query an agent actually writes: several words, none of them the
    // whole phrase that is stored.
    let out = p
        .run_silent(&["search", "puerto base de datos", "--limit", "10"])
        .unwrap_status("search");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 1, "search: {json}");
}

#[test]
fn a_hyphenated_query_finds_the_memory() {
    let p = Profile::new();
    save(&p, "El deploy por defecto apunta a staging-eu", &[]);

    let out = p
        .run_silent(&["search", "staging-eu", "--limit", "10"])
        .unwrap_status("search");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 1, "search: {json}");
}

#[test]
fn the_match_mode_is_callers_choice() {
    let p = Profile::new();
    save(&p, "El servicio escucha en el puerto 8081", &[]);

    let all = p
        .run_silent(&["search", "puerto 8081", "--match", "all", "--limit", "10"])
        .unwrap_status("search --match all");
    assert_eq!(stdout_json(&all)["count"], 1, "both words are there");

    let phrase = p
        .run_silent(&[
            "search",
            "8081 puerto",
            "--match",
            "phrase",
            "--limit",
            "10",
        ])
        .unwrap_status("search --match phrase");
    assert_eq!(
        stdout_json(&phrase)["count"],
        0,
        "the words are out of order"
    );
}

#[test]
fn an_unknown_match_mode_is_an_invalid_argument() {
    let p = Profile::new();
    save(&p, "contenido", &[]);
    let out = p.run_silent(&["search", "contenido", "--match", "nope"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn an_empty_query_is_an_invalid_argument() {
    let p = Profile::new();
    save(&p, "contenido", &[]);
    let out = p.run_silent(&["search", "   "]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn delete_without_force_prompts_and_stdin_no_aborts() {
    let p = Profile::new();
    let uuid = save(&p, "prompted deletion", &[]);
    // stdin is null (EOF): the confirmation must abort without deleting.
    let out = p.run_silent(&["delete", &uuid]);
    assert_eq!(out.status.code(), Some(2));
    let found = p.run_silent(&["show", &uuid]).unwrap_status("show");
    assert_eq!(stdout_json(&found)["content"], "prompted deletion");
}

// --- supersede ---

#[test]
fn supersede_marks_older_superseded() {
    let p = Profile::new();
    let older = save(&p, "old decision", &[]);
    let newer = save(&p, "new decision", &[]);
    p.run_silent(&[
        "supersede",
        "--older",
        &older,
        "--newer",
        &newer,
        "--reason",
        "r",
    ])
    .unwrap_status("supersede");
    let show = p.run_silent(&["show", &older]).unwrap_status("show older");
    assert_eq!(stdout_json(&show)["state"], "superseded");
    let show_new = p.run_silent(&["show", &newer]).unwrap_status("show newer");
    assert_eq!(stdout_json(&show_new)["state"], "active");
}

// --- purge ---

#[test]
fn purge_dry_run_is_default_and_keeps_records() {
    let p = Profile::new();
    let uuid = save(&p, "purge candidate", &[]);
    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete");

    // No --apply: dry-run. The record must still exist (restorable).
    let out = p
        .run_silent(&["purge", "--all"])
        .unwrap_status("dry-run purge");
    let json = stdout_json(&out);
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["purged"], 0);

    p.run_silent(&["restore", &uuid])
        .unwrap_status("restore after dry-run");
}

#[test]
fn purge_apply_removes_and_restore_then_not_found() {
    let p = Profile::new();
    let uuid = save(&p, "gone after purge", &[]);
    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete");
    let out = p
        .run_silent(&["purge", "--all", "--apply"])
        .unwrap_status("purge apply");
    assert_eq!(stdout_json(&out)["purged"], 1);

    let restore = p.run_silent(&["restore", &uuid]);
    assert_eq!(restore.status.code(), Some(3));
}

// --- reindex / config ---

#[test]
fn reindex_reports_reindexed_count() {
    let p = Profile::new();
    save(&p, "reindexable", &[]);
    let out = p.run_silent(&["reindex"]).unwrap_status("reindex");
    let json = stdout_json(&out);
    assert!(json["reindexed"].as_i64().expect("reindexed count") >= 1);
    let found = p
        .run_silent(&["search", "reindexable"])
        .unwrap_status("search after reindex");
    assert_eq!(stdout_json(&found)["count"], 1);
}

#[test]
fn config_show_prints_effective_settings() {
    let p = Profile::new();
    p.run_silent(&["init"]).unwrap_status("init");
    let out = p
        .run_silent(&["config", "show"])
        .unwrap_status("config show");
    let json = stdout_json(&out);
    assert!(json["scope"]["default_namespace"].is_string());
    assert!(json["data_dir"].is_string());
}

// --- config permissions (project-isolation S3, S4, S10) ---

#[test]
fn init_keeps_the_reserved_namespace_closed_and_the_reads_scoped() {
    let p = Profile::new();
    let out = p
        .run_silent(&["config", "show"])
        .unwrap_status("config show");
    let json = stdout_json(&out);
    assert_eq!(json["scope"]["allow_global"], false);
    assert_eq!(json["scope"]["allow_read_all"], false);
    assert_eq!(
        json["scope"]["read_namespaces"],
        serde_json::json!(["default"]),
        "a new profile reads its own namespace: {json}"
    );
}

#[test]
fn the_allow_read_all_switch_opens_the_reads_again() {
    let p = Profile::new();
    p.run_silent(&["config", "allow-read-all"])
        .unwrap_status("allow-read-all");
    let json = stdout_json(
        &p.run_silent(&["config", "show"])
            .unwrap_status("config show"),
    );
    assert_eq!(json["scope"]["allow_read_all"], true);
    // And the way back: the switch is revocable.
    p.run_silent(&["config", "allow-read-all", "--revoke"])
        .unwrap_status("revoke the open reads");
    let json = stdout_json(
        &p.run_silent(&["config", "show"])
            .unwrap_status("config show"),
    );
    assert_eq!(json["scope"]["allow_read_all"], false);
}

#[test]
fn allow_global_opens_and_revokes_the_reserved_namespace() {
    let p = Profile::new();
    p.run_silent(&["config", "allow-global"])
        .unwrap_status("allow-global");
    let json = stdout_json(
        &p.run_silent(&["config", "show"])
            .unwrap_status("config show"),
    );
    assert_eq!(json["scope"]["allow_global"], true);
    p.run_silent(&["config", "allow-global", "--revoke"])
        .unwrap_status("allow-global --revoke");
    let json = stdout_json(
        &p.run_silent(&["config", "show"])
            .unwrap_status("config show"),
    );
    assert_eq!(json["scope"]["allow_global"], false);
}

#[test]
fn allow_read_all_opens_and_revokes_cross_project_reads() {
    let p = Profile::new();
    p.run_silent(&["config", "allow-read-all", "--revoke"])
        .unwrap_status("allow-read-all --revoke");
    let json = stdout_json(
        &p.run_silent(&["config", "show"])
            .unwrap_status("config show"),
    );
    assert_eq!(json["scope"]["allow_read_all"], false);
    p.run_silent(&["config", "allow-read-all"])
        .unwrap_status("allow-read-all");
    let json = stdout_json(
        &p.run_silent(&["config", "show"])
            .unwrap_status("config show"),
    );
    assert_eq!(json["scope"]["allow_read_all"], true);
}

#[test]
fn config_permission_commands_refuse_a_missing_profile() {
    let p = Profile::without_init();
    for args in [
        vec!["config", "allow-global"],
        vec!["config", "allow-read-all"],
    ] {
        let out = p.run_silent(&args);
        assert_eq!(out.status.code(), Some(3), "args: {args:?}");
    }
}

#[test]
fn cross_project_reads_are_open_with_allow_read_all() {
    let p = Profile::new().with_config(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_read_all = true\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    );
    let uuid = save(&p, "compartido entre proyectos", &["--namespace", "ns-b"]);
    let out = p
        .run_silent(&["show", &uuid])
        .unwrap_status("show a memory of another project");
    assert!(stdout_json(&out)["uuid"].is_string());
}

#[test]
fn cross_project_reads_are_closed_without_allow_read_all() {
    let p = Profile::new().with_config(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    );
    let uuid = save(&p, "compartido entre proyectos", &["--namespace", "ns-b"]);
    let out = p.run_silent(&["show", &uuid]);
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn a_scoped_profile_searches_only_its_read_namespaces() {
    let p = Profile::new().with_config(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_read_all = false\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    );
    save(&p, "memoria de otro proyecto", &["--namespace", "ns-b"]);
    let out = p.run_silent(&["search", "memoria"]).unwrap_status("search");
    assert_eq!(
        stdout_json(&out)["count"],
        0,
        "a scoped profile must not search outside its read namespaces"
    );
    let listed = p.run_silent(&["list"]).unwrap_status("list");
    assert_eq!(
        stdout_json(&listed)["count"],
        0,
        "a scoped profile must not list outside its read namespaces"
    );
}

#[test]
fn an_open_profile_searches_across_projects() {
    let p = Profile::new().with_config(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_read_all = true\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    );
    save(&p, "memoria de otro proyecto", &["--namespace", "ns-b"]);
    let out = p.run_silent(&["search", "memoria"]).unwrap_status("search");
    assert_eq!(stdout_json(&out)["count"], 1);
    let listed = p.run_silent(&["list"]).unwrap_status("list");
    assert_eq!(stdout_json(&listed)["count"], 1);
}

// --- the scope a read was bounded by (read-scope-disclosure S1-S3) ---
//
// A read without an explicit namespace is filtered by the scope, so it
// returns fewer rows than exist. Without a declaration that silence reads as
// "there are no memories" when the truth is "you may not read them", and the
// agent saves the fact again. Every read answer says which mode bounded it,
// and the mode names no namespace and counts nothing.

/// A profile that reads `ns-a` while `ns-b` may hold data it cannot read.
fn read_scope_profile(allow_read_all: bool, allow_global: bool) -> Profile {
    Profile::new().with_config(&format!(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_read_all = {allow_read_all}\nallow_global = {allow_global}\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n"
    ))
}

#[test]
fn every_read_answer_declares_the_scope_that_bounded_it() {
    // The three modes and the two permissions that decide them.
    for (allow_read_all, allow_global, expected) in [
        (false, false, "restricted"),
        (true, false, "except_global"),
        (true, true, "all"),
    ] {
        let p = read_scope_profile(allow_read_all, allow_global);
        save(&p, "memoria legible", &["--namespace", "ns-a"]);

        let search = p.run_silent(&["search", "memoria"]).unwrap_status("search");
        let json = stdout_json(&search);
        assert_eq!(json["read_scope"], expected, "search ({expected}): {json}");
        // Additive: the envelope keeps every field it had.
        assert_eq!(json["query"], "memoria", "{json}");
        assert_eq!(json["namespace"], serde_json::Value::Null, "{json}");
        assert_eq!(json["count"], 1, "{json}");
        assert!(json["items"].is_array(), "{json}");
        assert!(json["relations"].is_array(), "{json}");

        let explained = p
            .run_silent(&["search", "--explain", "memoria"])
            .unwrap_status("search --explain");
        let json = stdout_json(&explained);
        assert_eq!(
            json["read_scope"], expected,
            "search --explain ({expected}): {json}"
        );
        assert_eq!(json["query"], "memoria", "{json}");
        assert_eq!(json["count"], 1, "{json}");
        assert!(json["items"].is_array(), "{json}");

        let listed = p.run_silent(&["list"]).unwrap_status("list");
        let json = stdout_json(&listed);
        assert_eq!(json["read_scope"], expected, "list ({expected}): {json}");
        assert_eq!(json["count"], 1, "{json}");
        assert!(json["items"].is_array(), "{json}");
        assert!(json["relations"].is_array(), "{json}");

        let context = p
            .run_silent(&["context", "--token-budget", "1024", "memoria"])
            .unwrap_status("context");
        let json = stdout_json(&context);
        assert_eq!(json["read_scope"], expected, "context ({expected}): {json}");
        assert!(json["items"].is_array(), "{json}");
        assert!(json["tokenizer_id"].is_string(), "{json}");
    }
}

#[test]
fn the_human_text_of_a_read_says_which_scope_bounded_it() {
    let p = read_scope_profile(false, false);
    save(&p, "memoria legible", &["--namespace", "ns-a"]);

    let search = p
        .run_text(&["search", "memoria"])
        .unwrap_status("search text");
    let text = String::from_utf8_lossy(&search.stdout);
    assert!(
        text.contains("read scope: restricted"),
        "the text of a search names the mode: {text}"
    );
    assert!(
        text.contains("this profile may not read every namespace"),
        "and what the mode means: {text}"
    );

    // A zero-row answer says it too: silence is the hazard this closes.
    let empty = p
        .run_text(&["search", "zxcvbnmqwerty"])
        .unwrap_status("empty search text");
    let text = String::from_utf8_lossy(&empty.stdout);
    assert!(
        text.contains("matched 0 memories"),
        "the empty answer keeps its wording: {text}"
    );
    assert!(
        text.contains("read scope: restricted"),
        "and says the answer was bounded: {text}"
    );

    let listed = p.run_text(&["list"]).unwrap_status("list text");
    let text = String::from_utf8_lossy(&listed.stdout);
    assert!(
        text.contains("read scope: restricted"),
        "the text of a list names the mode: {text}"
    );

    let context = p
        .run_text(&["context", "--token-budget", "1024", "memoria"])
        .unwrap_status("context text");
    let text = String::from_utf8_lossy(&context.stdout);
    assert!(
        text.contains("read scope: restricted"),
        "the text of a context names the mode: {text}"
    );
}

#[test]
fn the_read_scope_names_no_namespace_and_counts_nothing() {
    let p = read_scope_profile(false, false);
    // `ns-b` holds a memory this profile may not read: the declaration must
    // not announce it, not even by counting it.
    save(&p, "memoria escondida", &["--namespace", "ns-b"]);

    let search = p.run_silent(&["search", "memoria"]).unwrap_status("search");
    let json = stdout_json(&search);
    assert_eq!(
        json["count"], 0,
        "the scoped read returns fewer rows: {json}"
    );
    let scope = json["read_scope"].as_str().expect("read_scope is a word");
    assert_eq!(scope, "restricted", "{json}");
    assert!(
        !scope.contains("ns-a") && !scope.contains("ns-b"),
        "the mode names no namespace: {scope}"
    );
    assert!(
        !scope.chars().any(|c| c.is_ascii_digit()),
        "the mode carries no count: {scope}"
    );

    let listed = p.run_silent(&["list"]).unwrap_status("list");
    let json = stdout_json(&listed);
    assert_eq!(json["count"], 0, "{json}");
    assert_eq!(json["read_scope"], "restricted", "{json}");
}

// --- errors and scope (S10) ---

#[test]
fn invalid_topic_key_exits_2_naming_the_character_and_the_allowed_set() {
    let p = Profile::new();
    let out = p.run_silent(&["save", "--topic-key", "snake design", "content"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("INVALID_ARGUMENT"), "stderr: {stderr}");
    assert!(
        stderr.contains("' '"),
        "stderr must name the offending character: {stderr}"
    );
    assert!(
        stderr.contains("allowed"),
        "stderr must name the allowed set: {stderr}"
    );
    // The rejected save stores nothing.
    let listed = p.run_silent(&["list"]).unwrap_status("list");
    assert_eq!(stdout_json(&listed)["count"], 0);
}

#[test]
fn invalid_state_exits_2_with_invalid_argument_code() {
    let p = Profile::new();
    let out = p.run_silent(&["save", "--state", "bogus", "content"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("INVALID_ARGUMENT"), "stderr: {stderr}");
}

#[test]
fn show_unknown_uuid_exits_3() {
    let p = Profile::new();
    let out = p.run_silent(&["show", "11111111-1111-4111-8111-111111111111"]);
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn restricted_scope_blocks_foreign_namespace_without_leak() {
    let p = Profile::new().restricted();
    let out = p.run_silent(&["save", "--namespace", "ns-b", "secret"]);
    // Default existence_disclosure = not_found: deny without revealing.
    // The error names the denied namespace, never the content.
    assert_eq!(out.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("namespace:ns-b"),
        "must be the scope denial, not a missing profile: {stderr}"
    );
    assert!(
        !stderr.contains("secret"),
        "stderr leaked content: {stderr}"
    );
}

#[test]
fn restricted_scope_allows_own_namespace() {
    let p = Profile::new().restricted();
    let uuid = save(&p, "own namespace ok", &["--namespace", "ns-a"]);
    let out = p.run_silent(&["show", &uuid]).unwrap_status("show");
    assert_eq!(stdout_json(&out)["namespace"], "ns-a");
}

// --- mcp serve rename (regression for the CLI contract change) ---

#[test]
fn mcp_serve_is_nested_and_old_spelling_rejected() {
    let help = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("--help")
        .output()
        .expect("spawn");
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("mcp"), "help: {text}");

    let nested = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("mcp")
        .arg("serve")
        .arg("--help")
        .output()
        .expect("spawn");
    assert!(nested.status.success());

    let old = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("mcp-serve")
        .output()
        .expect("spawn");
    assert!(!old.status.success(), "old spelling must be rejected");
}

// --- save --help documents the topic_key charset (S3) ---

#[test]
fn save_help_documents_the_topic_key_charset() {
    let help = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .args(["save", "--help"])
        .output()
        .expect("spawn");
    assert!(help.status.success(), "save --help must succeed");
    let text = String::from_utf8_lossy(&help.stdout);
    // clap wraps the help, so compare against whitespace-normalised text.
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        normalized.contains("lowercase letters, digits, '.', '-' and '/'"),
        "save --help must document the topic_key charset: {text}"
    );
}

// --- T21: doctor diagnostics ---

#[test]
fn doctor_on_healthy_profile_exits_0() {
    let p = Profile::new();
    // A profile with at least one memory so FTS has content to verify.
    save(&p, "doctor healthy check", &[]);

    let out = p
        .run_silent(&["doctor"])
        .unwrap_status("doctor on healthy profile");
    let json = stdout_json(&out);

    // status must be healthy
    assert_eq!(
        json["status"], "healthy",
        "status should be healthy on a clean profile: {json}"
    );
    // checks array must carry the stable envelope per check
    let checks = json["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty(), "checks array must not be empty: {json}");
    for check in checks {
        assert!(
            check["check_id"].is_string(),
            "each check must have a check_id: {check}"
        );
        assert!(
            check["ok"].is_boolean(),
            "each check must have an ok field: {check}"
        );
        assert!(
            check["severity"].is_string(),
            "each check must have a severity: {check}"
        );
    }
}

#[test]
fn doctor_detects_missing_fts_and_exits_6() {
    let p = Profile::new();
    save(&p, "searchable memory", &[]);

    // Break FTS: open the DB and drop memories_fts.
    // A checkpoint is required so the DROP is visible to the second connection
    // that `doctor` opens (WAL pages are not immediately applied to the main db).
    let db_path = p.data_dir.join("mnexis.db");
    {
        let conn = mnexis_core::open_sqlite(&db_path).expect("open for break");
        conn.execute("DROP TABLE memories_fts", [])
            .expect("drop memories_fts");
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .expect("checkpoint after drop");
    }

    let out = p.run_silent(&["doctor"]);
    // Missing FTS is INDEX_UNAVAILABLE → exit 6
    assert_eq!(
        out.status.code(),
        Some(6),
        "doctor must exit 6 when FTS is missing: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = stdout_json(&out);
    assert_eq!(
        json["status"], "failed",
        "status must be failed after FTS drop: {json}"
    );
    // In JSON mode the check evidence is in stdout.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("INDEX_UNAVAILABLE"),
        "stdout must mention INDEX_UNAVAILABLE: {stdout}"
    );
}

#[test]
fn doctor_repair_rebuilds_fts_and_search_works() {
    let p = Profile::new();
    let uuid = save(&p, "repaired memory content", &[]);

    // Break FTS.
    let db_path = p.data_dir.join("mnexis.db");
    {
        let conn = mnexis_core::open_sqlite(&db_path).expect("open for break");
        conn.execute("DROP TABLE memories_fts", [])
            .expect("drop memories_fts");
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .expect("checkpoint after drop");
    }

    // Repair: re-creates FTS, triggers, and re-indexes.
    let repair_out = p.run_silent(&["doctor", "repair", "--apply"]);
    assert_eq!(
        repair_out.status.code(),
        Some(0),
        "doctor repair --apply should exit 0: {:?}",
        String::from_utf8_lossy(&repair_out.stderr)
    );
    let repair_json = stdout_json(&repair_out);
    assert_eq!(
        repair_json["status"], "healthy",
        "status must be healthy after repair: {repair_json}"
    );
    // The repair block must be present.
    assert!(
        repair_json["repair"].is_object(),
        "repair block must be present after --apply: {repair_json}"
    );
    assert_eq!(repair_json["repair"]["mode"], "apply", "{repair_json}");

    // Search must find the saved memory after repair (proves reindex).
    let after = p
        .run_silent(&["search", "repaired"])
        .unwrap_status("search after repair");
    let hits = stdout_json(&after);
    assert_eq!(hits["count"], 1, "search must find repaired memory: {hits}");
    // Verify it's the correct uuid.
    assert_eq!(
        hits["items"][0]["uuid"].as_str().unwrap(),
        uuid,
        "found uuid must match the saved memory"
    );
}

#[test]
fn doctor_flags_newer_schema_as_unsupported_version() {
    let p = Profile::new();

    // Advance schema_migrations.version beyond what this binary knows (2).
    let db_path = p.data_dir.join("mnexis.db");
    {
        let conn = mnexis_core::open_sqlite(&db_path).expect("open for schema bump");
        conn.execute(
            "INSERT OR REPLACE INTO schema_migrations (version) VALUES (99)",
            [],
        )
        .expect("bump schema version to 99");
    }

    let out = p.run_silent(&["doctor"]);
    assert_eq!(
        out.status.code(),
        Some(9),
        "doctor must exit 9 on newer schema: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("UNSUPPORTED_VERSION"),
        "stderr must mention UNSUPPORTED_VERSION: {stderr}"
    );
}

#[test]
fn doctor_check_filter_runs_one_check_and_unknown_code_is_invalid() {
    let p = Profile::new();
    save(&p, "filter target memory", &[]);

    let out = p
        .run_silent(&["doctor", "--check", "fts_shape"])
        .unwrap_status("doctor --check on a healthy profile");
    let json = stdout_json(&out);
    let checks = json["checks"].as_array().expect("checks array");
    assert_eq!(checks.len(), 1, "exactly one check ran: {json}");
    assert_eq!(checks[0]["check_id"], "fts_shape");
    assert_eq!(json["status"], "healthy");

    // An unknown CODE is an invalid argument (exit 2), not a crash, and
    // the error names the valid codes.
    let bad = p.run_silent(&["doctor", "--check", "nonsense"]);
    assert_eq!(
        bad.status.code(),
        Some(2),
        "unknown check code must exit 2 (INVALID_ARGUMENT): {:?}",
        String::from_utf8_lossy(&bad.stderr)
    );
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("fts_shape"),
        "the error names the valid codes: {stderr}"
    );
}

// --- T21b: doctor repair (plan / dry-run / apply) ---

#[test]
fn doctor_repair_plan_reports_and_writes_nothing() {
    let p = Profile::new();
    save(&p, "plan target memory", &[]);
    let db_path = p.data_dir.join("mnexis.db");
    {
        let conn = mnexis_core::open_sqlite(&db_path).expect("open for break");
        conn.execute("DROP TABLE memories_fts", [])
            .expect("drop memories_fts");
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .expect("checkpoint after drop");
    }

    let out = p.run_silent(&["doctor", "repair", "--plan"]);
    assert_eq!(
        out.status.code(),
        Some(6),
        "plan reports the current (broken) state: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = stdout_json(&out);
    assert_eq!(
        json["repair"]["mode"], "plan",
        "the mode is explicit: {json}"
    );
    let actions = json["repair"]["actions"].as_array().expect("actions array");
    assert!(
        actions.iter().any(|a| a["check_id"] == "fts_shape"),
        "the plan names the broken checks: {json}"
    );

    // A plan writes nothing: the store is exactly as broken as before.
    let still = p.run_silent(&["doctor"]);
    assert_eq!(
        still.status.code(),
        Some(6),
        "--plan must not change the store"
    );
}

#[test]
fn doctor_repair_dry_run_measures_and_leaves_the_store_broken() {
    let p = Profile::new();
    save(&p, "dry-run target memory", &[]);
    let db_path = p.data_dir.join("mnexis.db");
    {
        let conn = mnexis_core::open_sqlite(&db_path).expect("open for break");
        conn.execute("DROP TABLE memories_fts", [])
            .expect("drop memories_fts");
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .expect("checkpoint after drop");
    }

    let out = p.run_silent(&["doctor", "repair", "--dry-run"]);
    assert_eq!(
        out.status.code(),
        Some(6),
        "the dry-run leaves the store broken: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = stdout_json(&out);
    assert_eq!(json["repair"]["mode"], "dry_run", "{json}");
    assert_eq!(
        json["repair"]["reindexed"], 1,
        "the work ran inside the savepoint and was measured: {json}"
    );

    let still = p.run_silent(&["doctor"]);
    assert_eq!(
        still.status.code(),
        Some(6),
        "a dry-run must not heal the store"
    );
}

#[test]
fn the_old_repair_flag_is_gone() {
    let p = Profile::new();
    save(&p, "flag gone memory", &[]);

    let out = p.run_silent(&["doctor", "--repair"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "the old flag must be rejected (clean break, L2): {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// --- T22: operational commands (board C4) ---

#[test]
fn stats_counts_every_namespace_and_the_totals() {
    let p = Profile::new();
    // default namespace: one live pair (older superseded by newer) and one
    // soft-deleted memory.
    let older = save(&p, "the port is 8081", &[]);
    let newer = save(&p, "the port is 8082", &[]);
    let dead = save(&p, "obsolete note", &[]);
    p.run_silent(&[
        "supersede",
        "--older",
        &older,
        "--newer",
        &newer,
        "--reason",
        "port changed",
    ])
    .unwrap_status("supersede for stats");
    p.run_silent(&["delete", "--force", &dead])
        .unwrap_status("delete for stats");
    // a second namespace with a memory, a prompt and a session of its own.
    p.run_silent(&["config", "allow-namespace", "other"])
        .unwrap_status("allow the other namespace");
    save(
        &p,
        "memory of the other namespace",
        &["--namespace", "other"],
    );
    p.run_silent(&["session", "--id", "sess-1", "--namespace", "other"])
        .unwrap_status("session for stats");
    p.run_silent(&["prompt", "what the user said", "--namespace", "other"])
        .unwrap_status("prompt for stats");

    let out = p.run_silent(&["stats"]).unwrap_status("stats");
    let json = stdout_json(&out);

    // The stable contract: snake_case field names, typed shape.
    assert!(json["schema_version"].is_u64(), "schema_version: {json}");
    let namespaces = json["namespaces"].as_array().expect("namespaces array");
    let by_ns = |name: &str| {
        namespaces
            .iter()
            .find(|n| n["namespace"] == name)
            .unwrap_or_else(|| panic!("namespace {name} missing: {json}"))
    };
    let default = by_ns("default");
    assert_eq!(default["active"], 1, "default ns: {default}");
    assert_eq!(default["superseded"], 1, "default ns: {default}");
    assert_eq!(default["soft_deleted"], 1, "default ns: {default}");
    let other = by_ns("other");
    assert_eq!(other["active"], 1, "other ns: {other}");
    assert_eq!(other["superseded"], 0);
    assert_eq!(other["soft_deleted"], 0);

    let totals = &json["totals"];
    assert_eq!(totals["memories"], 4, "totals: {totals}");
    assert_eq!(totals["active"], 2);
    assert_eq!(totals["superseded"], 1);
    assert_eq!(totals["soft_deleted"], 1);
    assert_eq!(totals["supersede_links"], 1);
    assert_eq!(totals["sessions"], 1);
    assert_eq!(totals["prompts"], 1);
}

#[test]
fn stats_on_an_empty_store_reports_zeroes() {
    let p = Profile::new();
    let out = p
        .run_silent(&["stats"])
        .unwrap_status("stats on empty store");
    let json = stdout_json(&out);
    assert_eq!(
        json["namespaces"].as_array().map(Vec::len),
        Some(0),
        "{json}"
    );
    for field in [
        "memories",
        "active",
        "superseded",
        "soft_deleted",
        "supersede_links",
        "sessions",
        "prompts",
    ] {
        assert_eq!(json["totals"][field], 0, "totals.{field}: {json}");
    }
}

#[test]
fn timeline_shows_the_life_of_a_memory() {
    let p = Profile::new();
    let older = save(&p, "the port is 8081", &[]);
    let newer = save(&p, "the port is 8082", &[]);
    // An update is the one event the store records in the history table.
    p.run_silent(&[
        "update",
        &older,
        "--content",
        "the port is 8083",
        "--expected-revision",
        "1",
        "--reason",
        "port moved",
    ])
    .unwrap_status("update for timeline");
    p.run_silent(&[
        "supersede",
        "--older",
        &older,
        "--newer",
        &newer,
        "--reason",
        "port changed",
    ])
    .unwrap_status("supersede for timeline");

    let out = p
        .run_silent(&["timeline", &older])
        .unwrap_status("timeline of the superseded side");
    let json = stdout_json(&out);
    assert_eq!(json["memory"]["uuid"], older, "{json}");
    assert_eq!(json["memory"]["state"], "superseded", "{json}");
    assert!(json["memory"]["title"].is_string(), "{json}");
    assert!(json["memory"]["created_at"].is_string(), "{json}");
    // The recorded revisions, oldest first.
    let events = json["events"].as_array().expect("events array");
    assert!(
        !events.is_empty(),
        "the update the store recorded is an event: {json}"
    );
    assert!(events[0]["revision"].is_u64(), "{json}");
    assert!(events[0]["at"].is_string(), "{json}");
    let revisions: Vec<i64> = events
        .iter()
        .map(|e| e["revision"].as_i64().unwrap_or(-1))
        .collect();
    let mut sorted = revisions.clone();
    sorted.sort_unstable();
    assert_eq!(revisions, sorted, "the events run oldest first: {json}");

    let superseded_by = json["superseded_by"].as_array().expect("superseded_by");
    assert_eq!(superseded_by.len(), 1, "{json}");
    assert_eq!(superseded_by[0]["uuid"], newer, "{json}");
    assert_eq!(superseded_by[0]["reason"], "port changed", "{json}");
    assert!(superseded_by[0]["at"].is_string(), "{json}");
    assert_eq!(
        json["supersedes"].as_array().map(Vec::len),
        Some(0),
        "{json}"
    );

    // The other side of the link.
    let out2 = p
        .run_silent(&["timeline", &newer])
        .unwrap_status("timeline of the superseding side");
    let j2 = stdout_json(&out2);
    assert_eq!(j2["supersedes"][0]["uuid"], older, "{j2}");
    assert_eq!(
        j2["superseded_by"].as_array().map(Vec::len),
        Some(0),
        "{j2}"
    );
}

#[test]
fn timeline_of_an_unknown_uuid_is_not_found() {
    let p = Profile::new();
    let out = p.run_silent(&["timeline", "no-such-uuid"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "an unknown uuid is NOT_FOUND, not an empty timeline: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn timeline_shows_a_soft_deleted_memory() {
    let p = Profile::new();
    let uuid = save(&p, "doomed memory", &[]);
    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete for timeline");

    let out = p
        .run_silent(&["timeline", &uuid])
        .unwrap_status("timeline of a soft-deleted memory");
    let json = stdout_json(&out);
    assert_eq!(json["memory"]["deleted"], true, "{json}");
    assert!(
        json["memory"]["deleted_at"].is_string(),
        "the deletion is part of the life: {json}"
    );
}

#[test]
fn purge_dry_run_reports_eligible_count() {
    let p = Profile::new();
    // Save one memory and soft-delete it.
    let uuid = save(&p, "purge eligible memory", &[]);
    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete for purge");

    // Dry-run must report eligible count (1) alongside dry_run=true and purged=0.
    let out = p
        .run_silent(&["purge", "--all"])
        .unwrap_status("purge dry-run");
    let json = stdout_json(&out);
    assert_eq!(json["dry_run"], true, "dry_run must be true: {json}");
    assert_eq!(json["purged"], 0, "purged must be 0 in dry-run: {json}");
    assert_eq!(
        json["eligible"], 1,
        "eligible must be 1 (one soft-deleted record): {json}"
    );
}

// --- export / import (T22, PRD §13, e2-core §5/§7) ---

/// Helper: write text to a path and return the path.
fn write_tmp(label: &str, body: &str) -> std::path::PathBuf {
    let dir = tempfile::tempdir_in("target").expect("tmpdir");
    let path = dir.path().join(label);
    std::fs::write(&path, body).expect("write tmp file");
    // Leak the guard so the path outlives the call (it's inside target/).
    std::mem::forget(dir);
    path
}

#[test]
fn export_writes_one_jsonl_line_per_memory() {
    let p = namespaced_profile(&["ns-a"]);
    save(&p, "first content", &[]);
    save(&p, "second content", &[]);
    let path = write_tmp("export.jsonl", "");
    let out = p
        .run_silent(&["export", "--output", path.to_str().expect("utf8")])
        .unwrap_status("export");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 2);
    let body = std::fs::read_to_string(&path).expect("read export");
    let lines: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), 2);
    for line in &lines {
        let parsed: serde_json::Value = serde_json::from_str(line).expect("line JSON");
        assert!(parsed["uuid"].is_string());
        assert!(parsed["namespace"].is_string());
        assert!(parsed["content"].is_string());
        assert!(parsed["revision"].is_i64());
    }
}

#[test]
fn export_excludes_deleted_by_default() {
    let p = namespaced_profile(&["ns-a"]);
    let uuid = save(&p, "to be deleted", &[]);
    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete");
    let path = write_tmp("export.jsonl", "");
    let out = p
        .run_silent(&["export", "--output", path.to_str().expect("utf8")])
        .unwrap_status("export");
    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.is_empty(), "no live memories to export: {body:?}");
    assert_eq!(stdout_json(&out)["count"], 0);
}

#[test]
fn export_include_deleted_carries_the_record() {
    let p = namespaced_profile(&["ns-a"]);
    let uuid = save(&p, "kept for the record", &[]);
    p.run_silent(&["delete", "--force", &uuid])
        .unwrap_status("delete");
    let path = write_tmp("export.jsonl", "");
    let out = p
        .run_silent(&[
            "export",
            "--include-deleted",
            "--output",
            path.to_str().expect("utf8"),
        ])
        .unwrap_status("export");
    assert_eq!(stdout_json(&out)["count"], 1);
    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.contains(&uuid));
}

#[test]
fn import_round_trip_preserves_ids_and_content() {
    let a = namespaced_profile(&["ns-a"]);
    save(&a, "alpha", &[]);
    let b_uuid = save(&a, "bravo", &["--topic-key", "topic.bravo"]);

    let export_path = write_tmp("export.jsonl", "");
    a.run_silent(&["export", "--output", export_path.to_str().expect("utf8")])
        .unwrap_status("export");

    let b = namespaced_profile(&["ns-a"]);
    let out = b
        .run_silent(&[
            "import",
            "--policy",
            "skip",
            "--input",
            export_path.to_str().expect("utf8"),
        ])
        .unwrap_status("import");
    let json = stdout_json(&out);
    assert_eq!(json["imported"], 2, "json: {json}");
    let show = b.run_silent(&["show", &b_uuid]).unwrap_status("show");
    assert_eq!(stdout_json(&show)["content"], "bravo");
}

#[test]
fn import_dry_run_reports_writes_nothing() {
    let a = namespaced_profile(&["ns-a"]);
    save(&a, "one", &[]);
    save(&a, "two", &[]);

    let export_path = write_tmp("export.jsonl", "");
    a.run_silent(&["export", "--output", export_path.to_str().expect("utf8")])
        .unwrap_status("export");

    let b = namespaced_profile(&["ns-a"]);
    let out = b
        .run_silent(&[
            "import",
            "--dry-run",
            "--input",
            export_path.to_str().expect("utf8"),
        ])
        .unwrap_status("import dry-run");
    let json = stdout_json(&out);
    assert_eq!(json["dry_run"], true);
    assert_eq!(json["would_import"], 2);
    assert_eq!(json["imported"], 0);

    let list = b.run_silent(&["list"]).unwrap_status("list");
    assert_eq!(stdout_json(&list)["count"], 0);
}

#[test]
fn import_reject_rejects_identical_uuid_with_different_payload() {
    let a = namespaced_profile(&["ns-a"]);
    let uuid = save(&a, "original", &[]);
    // Pre-populate profile B with the same uuid (identical content), so the
    // tampered import is a real uuid clash on B's side.
    let clean = format!(
        "{{\"uuid\":\"{uuid}\",\"topic_key\":\"\",\"namespace\":\"ns-a\",\"memory_type\":\"fact\",\
         \"content\":\"original\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
         \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
         \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
         \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n"
    );
    let path_clean = write_tmp("clean.jsonl", &clean);
    let tampered = format!(
        "{{\"uuid\":\"{uuid}\",\"topic_key\":\"\",\"namespace\":\"ns-a\",\"memory_type\":\"fact\",\
         \"content\":\"tampered\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
         \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
         \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
         \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n"
    );
    let path = write_tmp("tampered.jsonl", &tampered);

    let b = namespaced_profile(&["ns-a"]);
    b.run_silent(&["import", "--input", path_clean.to_str().expect("utf8")])
        .unwrap_status("import baseline");
    let out = b.run_silent(&[
        "import",
        "--policy",
        "reject",
        "--input",
        path.to_str().expect("utf8"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(5),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("CONFLICT") || stderr.contains("conflict"),
        "stderr: {stderr}"
    );
    let show = b.run_silent(&["show", &uuid]).unwrap_status("show missing");
    assert_eq!(stdout_json(&show)["content"], "original");
}

#[test]
fn import_skip_leaves_unchanged_records_untouched() {
    let a = namespaced_profile(&["ns-a"]);
    let uuid = save(&a, "original", &[]);
    let clean = format!(
        "{{\"uuid\":\"{uuid}\",\"topic_key\":\"\",\"namespace\":\"ns-a\",\"memory_type\":\"fact\",\
         \"content\":\"original\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
         \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
         \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
         \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n"
    );
    let path_clean = write_tmp("clean.jsonl", &clean);
    let tampered = format!(
        "{{\"uuid\":\"{uuid}\",\"topic_key\":\"\",\"namespace\":\"ns-a\",\"memory_type\":\"fact\",\
         \"content\":\"tampered\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
         \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
         \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
         \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n"
    );
    let path = write_tmp("tampered.jsonl", &tampered);

    let b = namespaced_profile(&["ns-a"]);
    b.run_silent(&["import", "--input", path_clean.to_str().expect("utf8")])
        .unwrap_status("import baseline");
    let out = b
        .run_silent(&[
            "import",
            "--policy",
            "skip",
            "--input",
            path.to_str().expect("utf8"),
        ])
        .unwrap_status("import skip");
    let json = stdout_json(&out);
    assert_eq!(json["imported"], 0);
    assert!(json["skipped"].as_i64().expect("skipped count") >= 1);
    let show = b.run_silent(&["show", &uuid]).unwrap_status("show");
    assert_eq!(stdout_json(&show)["content"], "original");
}

#[test]
fn import_remap_creates_a_new_uuid_for_the_conflicting_record() {
    let a = namespaced_profile(&["ns-a"]);
    let uuid = save(&a, "original", &[]);
    let clean = format!(
        "{{\"uuid\":\"{uuid}\",\"topic_key\":\"\",\"namespace\":\"ns-a\",\"memory_type\":\"fact\",\
         \"content\":\"original\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
         \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
         \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
         \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n"
    );
    let path_clean = write_tmp("clean.jsonl", &clean);
    let tampered = format!(
        "{{\"uuid\":\"{uuid}\",\"topic_key\":\"\",\"namespace\":\"ns-a\",\"memory_type\":\"fact\",\
         \"content\":\"remap-target\",\"state\":\"active\",\"revision\":1,\"deleted\":false,\
         \"created_at\":\"2026-01-01T00:00:00Z\",\"updated_at\":\"2026-01-01T00:00:00Z\",\
         \"deleted_at\":null,\"purge_after\":null,\"valid_from\":null,\"valid_until\":null,\
         \"actor\":\"user\",\"channel\":\"cli\",\"assertion_basis\":\"user_stated\"}}\n"
    );
    let path = write_tmp("tampered.jsonl", &tampered);

    let b = namespaced_profile(&["ns-a"]);
    b.run_silent(&["import", "--input", path_clean.to_str().expect("utf8")])
        .unwrap_status("import baseline");
    let out = b
        .run_silent(&[
            "import",
            "--policy",
            "remap",
            "--input",
            path.to_str().expect("utf8"),
        ])
        .unwrap_status("import remap");
    let json = stdout_json(&out);
    assert_eq!(json["imported"], 1, "remap must produce a new uuid: {json}");
    assert!(json["remapped"].as_i64().expect("remapped count") >= 1);
    let show = b
        .run_silent(&["show", &uuid])
        .unwrap_status("show original");
    assert_eq!(stdout_json(&show)["content"], "original");
    let list = b
        .run_silent(&["list", "--namespace", "ns-a"])
        .unwrap_status("list");
    assert_eq!(stdout_json(&list)["count"], 2);
}

#[test]
fn import_rejects_topic_key_already_occupied_by_another_record() {
    let a = namespaced_profile(&["ns-a"]);
    let _uuid_a = save(&a, "occupier", &["--topic-key", "shared"]);
    let export_path = write_tmp("export.jsonl", "");
    a.run_silent(&["export", "--output", export_path.to_str().expect("utf8")])
        .unwrap_status("export");

    let b = namespaced_profile(&["ns-a"]);
    let _uuid_b = save(&b, "pre-existing", &["--topic-key", "shared"]);

    let out = b.run_silent(&[
        "import",
        "--policy",
        "remap",
        "--input",
        export_path.to_str().expect("utf8"),
    ]);
    assert_eq!(
        out.status.code(),
        Some(5),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn import_validation_rejects_oversized_input() {
    let b = namespaced_profile(&["ns-a"]);
    let big = std::iter::repeat("x")
        .take(51 * 1024 * 1024)
        .collect::<String>();
    let path = write_tmp("big.jsonl", &big);
    let out = b.run_silent(&["import", "--input", path.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("INVALID_ARGUMENT"), "stderr: {stderr}");
}

#[test]
fn import_validation_rejects_oversized_content() {
    let b = namespaced_profile(&["ns-a"]);
    let oversize = "x".repeat(16 * 1024 + 1);
    let line = serde_json::json!({
        "uuid": "00000000-0000-4000-8000-000000000001",
        "topic_key": "",
        "namespace": "ns-a",
        "memory_type": "fact",
        "content": oversize,
        "state": "active",
        "revision": 1,
        "deleted": false,
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "deleted_at": null,
        "purge_after": null,
        "valid_from": null,
        "valid_until": null,
        "actor": "user",
        "channel": "cli",
        "assertion_basis": "user_stated"
    })
    .to_string();
    let path = write_tmp("oversize.jsonl", &(line + "\n"));
    let out = b.run_silent(&["import", "--input", path.to_str().expect("utf8")]);
    assert_eq!(out.status.code(), Some(2));
}

// --- context (E3) ---

#[test]
fn context_exact_mode_returns_items_and_pins_a_07_invariant() {
    let p = namespaced_profile(&["default"]);
    save(&p, "alpha alpha alpha", &[]);
    save(&p, "beta beta", &[]);
    let out = p
        .run_silent(&[
            "context",
            "--namespace",
            "default",
            "--token-budget",
            "1024",
            "--tokenizer-id",
            "supported-tokenizer-v1",
            "--budget-mode",
            "exact",
            "alpha",
        ])
        .unwrap_status("context exact");
    let json = stdout_json(&out);
    assert_eq!(json["budget_enforcement"], "exact");
    assert_eq!(json["tokenizer_id"], "supported-tokenizer-v1");
    assert_eq!(json["retrieval_mode"], "textual");
    assert!(json["items"].as_array().expect("items").len() >= 1);
    let tokens_used = json["tokens_used"].as_i64().expect("tokens_used");
    assert!(tokens_used > 0, "tokens_used must be > 0");
    assert!(tokens_used <= 1024, "A-07: tokens_used <= token_budget");
    let items = json["items"].as_array().unwrap();
    for item in items {
        assert!(item["uuid"].is_string());
        assert!(item["content"].is_string());
        assert!(item["revision"].is_i64());
        assert!(item["state"].is_string());
    }
}

#[test]
fn context_estimated_mode_keeps_estimated_label_and_warning() {
    let p = namespaced_profile(&["default"]);
    save(&p, "alpha", &[]);
    let out = p
        .run_silent(&[
            "context",
            "--namespace",
            "default",
            "--token-budget",
            "1024",
            "--budget-mode",
            "estimated",
            "alpha",
        ])
        .unwrap_status("context estimated");
    let json = stdout_json(&out);
    assert_eq!(json["budget_enforcement"], "estimated");
    assert!(
        json["warnings"]
            .as_array()
            .map(|a| a.iter().any(|v| v == "lexical_estimation"))
            .unwrap_or(false),
        "estimated mode must include the lexical_estimation warning: {json}"
    );
}

#[test]
fn context_unsupported_tokenizer_in_exact_mode_exits_7() {
    let p = namespaced_profile(&["default"]);
    save(&p, "alpha", &[]);
    let out = p.run_silent(&[
        "context",
        "--namespace",
        "default",
        "--token-budget",
        "1024",
        "--tokenizer-id",
        "unsupported-tokenizer-vX",
        "--budget-mode",
        "exact",
        "alpha",
    ]);
    assert_eq!(out.status.code(), Some(7));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("BUDGET_TOO_SMALL"), "stderr: {stderr}");
}

#[test]
fn context_with_no_candidates_returns_empty_with_reason() {
    let p = namespaced_profile(&["default"]);
    // FTS5 bareword tokens only; dashes / colons would be parsed as
    // column refs and surface as a transport-level error, not an
    // empty result. Use a token that won't match any saved memory.
    let out = p
        .run_silent(&[
            "context",
            "--namespace",
            "default",
            "--token-budget",
            "1024",
            "--budget-mode",
            "estimated",
            "zxcvbnmqwerty",
        ])
        .unwrap_status("context empty");
    let json = stdout_json(&out);
    assert_eq!(json["items"].as_array().expect("items").len(), 0);
    assert!(json["empty_reason"].is_string());
}

// --- context: the namespace it searched (#57) ---

/// The remote of a bound workspace: a fixed identity, so the key the engine
/// computes never depends on the checkout the test runs in.
const BOUND_REMOTE: &str = "https://github.com/Xeworg/contexto.git";
/// The key a human may write; the registry records its canonical form.
const BOUND_KEY_RAW: &str = "git:github.com/Xeworg/contexto";
/// The namespace the bound workspace writes into.
const BOUND_NAMESPACE: &str = "mnexis";

/// A profile that reads two projects, with the profile's own default
/// first: the shape a real multi-project profile has.
fn two_project_profile() -> Profile {
    Profile::new().with_config(&Profile::namespaces(&["default", BOUND_NAMESPACE]))
}

#[test]
fn context_in_a_bound_workspace_answers_from_the_workspaces_namespace() {
    let p = two_project_profile();
    let workspace = Workspace::bound(&p, BOUND_NAMESPACE);
    save_in(
        &p,
        workspace.path(),
        BOUND_NAMESPACE,
        "el puerto del servicio es 8081",
    );

    let out = p
        .run_in_dir(
            workspace.path(),
            &["context", "--token-budget", "1024", "puerto"],
        )
        .unwrap_status("context");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items array");
    assert_eq!(
        items.len(),
        1,
        "the answer comes from the workspace's namespace: {json}"
    );
    assert_eq!(items[0]["namespace"], BOUND_NAMESPACE, "{json}");
    assert_eq!(
        json["searched_namespace"], BOUND_NAMESPACE,
        "the envelope declares the namespace it searched: {json}"
    );
}

#[test]
fn context_in_an_unbound_directory_keeps_the_scopes_first_namespace_and_declares_it() {
    let p = two_project_profile();
    let workspace = Workspace::bare();
    save_in(
        &p,
        workspace.path(),
        "default",
        "el puerto del servicio es 8081",
    );

    let out = p
        .run_in_dir(
            workspace.path(),
            &["context", "--token-budget", "1024", "puerto"],
        )
        .unwrap_status("context");
    let json = stdout_json(&out);
    assert_eq!(
        json["items"].as_array().expect("items array").len(),
        1,
        "the behaviour before the workspace was consulted is unchanged: {json}"
    );
    assert_eq!(
        json["searched_namespace"], "default",
        "and the envelope says where it looked: {json}"
    );
}

#[test]
fn an_explicit_namespace_wins_over_the_workspaces_namespace() {
    let p = two_project_profile();
    let workspace = Workspace::bound(&p, BOUND_NAMESPACE);
    // The other project's record, saved from a directory that is not the
    // bound workspace: a write there is not the workspace's to refuse.
    let elsewhere = Workspace::bare();
    save_in(
        &p,
        elsewhere.path(),
        "default",
        "el puerto del servicio es 8081",
    );

    let out = p
        .run_in_dir(
            workspace.path(),
            &[
                "context",
                "--namespace",
                "default",
                "--token-budget",
                "1024",
                "puerto",
            ],
        )
        .unwrap_status("context");
    let json = stdout_json(&out);
    assert_eq!(
        json["items"].as_array().expect("items array").len(),
        1,
        "the caller asked for a namespace and got it: {json}"
    );
    assert_eq!(
        json["searched_namespace"], "default",
        "and the envelope names it: {json}"
    );
}

#[test]
fn the_text_of_a_context_names_the_namespace_it_searched() {
    let p = two_project_profile();
    let workspace = Workspace::bound(&p, BOUND_NAMESPACE);
    save_in(
        &p,
        workspace.path(),
        BOUND_NAMESPACE,
        "el puerto del servicio es 8081",
    );

    let out = p
        .run_text_in_dir(
            workspace.path(),
            &["context", "--token-budget", "1024", "puerto"],
        )
        .unwrap_status("context");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains(BOUND_NAMESPACE),
        "the human text says where it looked: {text}"
    );
}

#[test]
fn a_bulk_import_cannot_write_into_another_workspace() {
    let source = guard_profile();
    save(
        &source,
        "from another project",
        &["--namespace", OTHER_NAMESPACE],
    );
    let export_path = write_tmp("foreign.jsonl", "");
    source
        .run_silent(&["export", "--output", export_path.to_str().expect("utf8")])
        .unwrap_status("export");

    let p = guard_profile();
    let workspace = Workspace::marked(MARKER_NAMESPACE);
    let out = p.run_in_dir(
        workspace.path(),
        &["import", "--input", export_path.to_str().expect("utf8")],
    );
    assert_eq!(out.status.code(), Some(4));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(MARKER_NAMESPACE),
        "the message names the allowed namespace: {stderr}"
    );
    assert_eq!(
        count_in(&p, OTHER_NAMESPACE),
        0,
        "the foreign record was not written"
    );
}

#[test]
fn a_bulk_import_into_the_own_workspace_is_accepted() {
    let source = guard_profile();
    save(
        &source,
        "from this project",
        &["--namespace", MARKER_NAMESPACE],
    );
    let export_path = write_tmp("own.jsonl", "");
    source
        .run_silent(&["export", "--output", export_path.to_str().expect("utf8")])
        .unwrap_status("export");

    let p = guard_profile();
    let workspace = Workspace::marked(MARKER_NAMESPACE);
    p.run_in_dir(
        workspace.path(),
        &["import", "--input", export_path.to_str().expect("utf8")],
    )
    .unwrap_status("import");
    assert_eq!(count_in(&p, MARKER_NAMESPACE), 1);
}

// --- workspace write isolation (project-isolation S5, S9) ---

/// A working directory the binary runs in, so a test can place a
/// `.mnexis-namespace` marker (or leave the directory bare) and drive the
/// workspace guard end to end.
struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    /// The repository tree, `target/`, and `$HOME` carry no marker, so a bare
    /// directory resolves to no namespace. If a marker ever appears above
    /// this tempdir the "no workspace" tests fail loudly instead of passing
    /// for the wrong reason.
    fn bare() -> Self {
        Self {
            dir: tempdir_in("target").expect("tempdir under target/"),
        }
    }

    /// The same directory, plus a marker naming `namespace`. The content is
    /// deliberately padded: the marker reader trims it.
    fn marked(namespace: &str) -> Self {
        let workspace = Self::bare();
        std::fs::write(
            workspace.dir.path().join(".mnexis-namespace"),
            format!("  {namespace}\n"),
        )
        .expect("write .mnexis-namespace");
        workspace
    }

    /// The same directory, as a git repository with one known remote: the
    /// identity the engine computes does not depend on the checkout the
    /// test runs in. The namespace is bound in the profile's registry, so
    /// the binding is a permission and not a file in the repository.
    fn bound(profile: &Profile, namespace: &str) -> Self {
        let workspace = Self::bare();
        let root = workspace.dir.path();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(root)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "."]);
        git(&["remote", "add", "origin", BOUND_REMOTE]);
        let out = profile.run_silent(&["project", "bind", namespace, "--key", BOUND_KEY_RAW]);
        assert!(
            out.status.success(),
            "bind failed ({:?}): {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        );
        workspace
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

const MARKER_NAMESPACE: &str = "proyecto-marcado";
const OTHER_NAMESPACE: &str = "otro-proyecto";

fn guard_profile() -> Profile {
    namespaced_profile(&[MARKER_NAMESPACE, OTHER_NAMESPACE])
}

/// Save through an explicit working directory and return the uuid.
fn save_in(profile: &Profile, dir: &std::path::Path, namespace: &str, content: &str) -> String {
    let out = profile.run_in_dir(dir, &["save", "--namespace", namespace, content]);
    assert!(
        out.status.success(),
        "save failed ({:?}): {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    stdout_json(&out)["uuid"]
        .as_str()
        .expect("uuid in save output")
        .to_string()
}

fn count_in(profile: &Profile, namespace: &str) -> u64 {
    let out = profile
        .run_silent(&["list", "--namespace", namespace])
        .unwrap_status("list");
    stdout_json(&out)["count"].as_u64().expect("count")
}

/// The refusal must name the allowed namespace and how to change it.
fn assert_workspace_refusal(out: &Output, allowed: &str) {
    assert_eq!(
        out.status.code(),
        Some(4),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("FORBIDDEN"), "stderr: {stderr}");
    assert!(
        stderr.contains(&format!(
            "this workspace writes only into namespace '{allowed}'"
        )),
        "stderr must name the allowed namespace: {stderr}"
    );
    assert!(
        stderr.contains("project bind"),
        "stderr must say how to change it: {stderr}"
    );
}

#[test]
fn a_marker_workspace_refuses_a_save_into_another_namespace() {
    let p = guard_profile();
    let ws = Workspace::marked(MARKER_NAMESPACE);
    let out = p.run_in_dir(ws.path(), &["save", "--namespace", OTHER_NAMESPACE, "x"]);
    assert_workspace_refusal(&out, MARKER_NAMESPACE);
    // A refused write stores nothing, in either namespace.
    assert_eq!(count_in(&p, OTHER_NAMESPACE), 0);
    assert_eq!(count_in(&p, MARKER_NAMESPACE), 0);
}

#[test]
fn a_marker_workspace_accepts_a_save_into_its_own_namespace() {
    let p = guard_profile();
    let ws = Workspace::marked(MARKER_NAMESPACE);
    save_in(&p, ws.path(), MARKER_NAMESPACE, "memoria del proyecto");
    assert_eq!(count_in(&p, MARKER_NAMESPACE), 1);
    assert_eq!(count_in(&p, OTHER_NAMESPACE), 0);
}

#[test]
fn without_a_workspace_the_profile_allowlist_still_decides() {
    let p = guard_profile();
    let ws = Workspace::bare();
    // Allowed by the profile: the old behaviour, no workspace rule involved.
    save_in(&p, ws.path(), OTHER_NAMESPACE, "permitido por la allowlist");
    assert_eq!(count_in(&p, OTHER_NAMESPACE), 1);
    // Forbidden by the profile stays the scope denial (NOT_FOUND, exit 3),
    // not the workspace refusal (exit 4).
    let out = p.run_in_dir(ws.path(), &["save", "--namespace", "fuera", "x"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("FORBIDDEN"), "stderr: {stderr}");
}

#[test]
fn update_and_delete_are_refused_for_memories_of_another_workspace() {
    let p = guard_profile();
    let bare = Workspace::bare();
    let uuid = save_in(&p, bare.path(), OTHER_NAMESPACE, "otra memoria");
    let ws = Workspace::marked(MARKER_NAMESPACE);

    let update = p.run_in_dir(
        ws.path(),
        &[
            "update",
            &uuid,
            "--content",
            "intento ajeno",
            "--expected-revision",
            "1",
            "--reason",
            "cross-workspace",
        ],
    );
    assert_workspace_refusal(&update, MARKER_NAMESPACE);

    let delete = p.run_in_dir(ws.path(), &["delete", "--force", &uuid]);
    assert_workspace_refusal(&delete, MARKER_NAMESPACE);

    // Neither refusal changed anything: same content, still active.
    let shown = p.run_silent(&["show", &uuid]).unwrap_status("show");
    let json = stdout_json(&shown);
    assert_eq!(json["content"], "otra memoria");
    assert_eq!(json["state"], "active");
    assert_eq!(json["revision"], 1);
}

#[test]
fn supersede_and_restore_are_refused_for_memories_of_another_workspace() {
    let p = guard_profile();
    let bare = Workspace::bare();
    let older = save_in(&p, bare.path(), OTHER_NAMESPACE, "vieja decision");
    let newer = save_in(&p, bare.path(), OTHER_NAMESPACE, "nueva decision");
    let deleted = save_in(&p, bare.path(), OTHER_NAMESPACE, "borrada y restaurable");
    p.run_in_dir(bare.path(), &["delete", "--force", &deleted])
        .unwrap_status("seed delete");

    let ws = Workspace::marked(MARKER_NAMESPACE);
    let supersede = p.run_in_dir(
        ws.path(),
        &[
            "supersede",
            "--older",
            &older,
            "--newer",
            &newer,
            "--reason",
            "cross-workspace",
        ],
    );
    assert_workspace_refusal(&supersede, MARKER_NAMESPACE);

    let restore = p.run_in_dir(ws.path(), &["restore", &deleted]);
    assert_workspace_refusal(&restore, MARKER_NAMESPACE);

    // The refused supersede left both memories active.
    let shown = p.run_silent(&["show", &older]).unwrap_status("show older");
    assert_eq!(stdout_json(&shown)["state"], "active");
    // The refused restore left the memory deleted: only the two active
    // memories still list.
    assert_eq!(count_in(&p, OTHER_NAMESPACE), 2);
}

#[test]
fn every_write_still_works_inside_the_workspace_namespace() {
    let p = guard_profile();
    let ws = Workspace::marked(MARKER_NAMESPACE);
    let older = save_in(&p, ws.path(), MARKER_NAMESPACE, "decision vieja");
    let newer = save_in(&p, ws.path(), MARKER_NAMESPACE, "decision nueva");
    let doomed = save_in(&p, ws.path(), MARKER_NAMESPACE, "para borrar");

    p.run_in_dir(
        ws.path(),
        &[
            "update",
            &newer,
            "--content",
            "decision nueva y corregida",
            "--expected-revision",
            "1",
            "--reason",
            "fix",
        ],
    )
    .unwrap_status("guarded update");
    p.run_in_dir(
        ws.path(),
        &[
            "supersede",
            "--older",
            &older,
            "--newer",
            &newer,
            "--reason",
            "reemplazo",
        ],
    )
    .unwrap_status("guarded supersede");
    p.run_in_dir(ws.path(), &["delete", "--force", &doomed])
        .unwrap_status("guarded delete");
    p.run_in_dir(ws.path(), &["restore", &doomed])
        .unwrap_status("guarded restore");
}

// --- sessions and prompts ---

#[test]
fn a_session_starts_ends_and_is_listed() {
    let p = Profile::new();
    let started = p
        .run_silent(&["session", "--id", "01JX", "--directory", "/work/project"])
        .unwrap_status("session");
    let json = stdout_json(&started);
    assert_eq!(json["id"], "01JX");
    assert_eq!(json["ended_at"], serde_json::Value::Null);

    let ended = p
        .run_silent(&["end", "--id", "01JX", "--summary", "what we did"])
        .unwrap_status("end");
    assert_eq!(stdout_json(&ended)["summary"], "what we did");

    let sessions = p.run_silent(&["sessions"]).unwrap_status("sessions");
    let json = stdout_json(&sessions);
    assert_eq!(json["count"], 1);
    assert_eq!(json["items"][0]["id"], "01JX");
}

#[test]
fn a_prompt_is_stored_and_searched() {
    let p = Profile::new();
    p.run_silent(&["session", "--id", "01JX"])
        .unwrap_status("session");
    let stored = p
        .run_silent(&[
            "prompt",
            "el puerto del servicio es 8081",
            "--session",
            "01JX",
        ])
        .unwrap_status("prompt");
    assert_eq!(stdout_json(&stored)["session_id"], "01JX");

    let found = p
        .run_silent(&["prompts", "--search", "puerto 8081"])
        .unwrap_status("prompts --search");
    let json = stdout_json(&found);
    assert_eq!(json["count"], 1, "{json}");
    assert_eq!(
        json["items"][0]["content"],
        "el puerto del servicio es 8081"
    );

    let listed = p.run_silent(&["prompts"]).unwrap_status("prompts");
    assert_eq!(stdout_json(&listed)["count"], 1);
}

#[test]
fn a_hyphenated_prompt_search_finds_the_prompt() {
    let p = Profile::new();
    p.run_silent(&["prompt", "el deploy por defecto es staging-eu"])
        .unwrap_status("prompt");
    let found = p
        .run_silent(&["prompts", "--search", "staging-eu"])
        .unwrap_status("prompts --search");
    assert_eq!(stdout_json(&found)["count"], 1);
}

// --- session recall (session-recall S1) ---
//
// The recall answers what a session was, in one answer: the session, its
// prompts in order, and the memories of its namespace created inside its
// window. The window is a window, not a link: the engine does not know which
// session wrote which memory, so the answer says exactly that and never makes
// a claim it cannot check.

/// The one sentence the answer uses for what a window can and cannot claim.
const WINDOW_LABEL: &str = "the memories of the namespace in the session's window";

#[test]
fn recall_carries_the_session_its_prompts_and_the_memories_of_its_window() {
    let p = namespaced_profile(&["ns-a"]);
    p.run_silent(&["session", "--id", "01JX", "--directory", "/work/project"])
        .unwrap_status("session");
    for said in ["el puerto es 8081", "y el deploy va a staging-eu"] {
        p.run_silent(&["prompt", said, "--session", "01JX"])
            .unwrap_status("prompt");
    }
    // A prompt of another session is not this session's prompt.
    p.run_silent(&["session", "--id", "OTRA"])
        .unwrap_status("other session");
    p.run_silent(&["prompt", "otra conversacion", "--session", "OTRA"])
        .unwrap_status("prompt of another session");

    let inside = save(&p, "el puerto del servicio es 8081", &[]);
    // A memory of another namespace, written in the same window, is not
    // read through this session's window.
    let elsewhere = save(
        &p,
        "una memoria de otro namespace",
        &["--namespace", "default"],
    );

    let out = p
        .run_silent(&["recall", "--session", "01JX"])
        .unwrap_status("recall --session");
    let json = stdout_json(&out);

    // The session itself.
    assert_eq!(json["session"]["id"], "01JX", "{json}");
    assert_eq!(json["session"]["namespace"], "ns-a", "{json}");
    assert_eq!(json["session"]["directory"], "/work/project", "{json}");
    assert!(json["session"]["started_at"].is_string(), "{json}");
    assert_eq!(json["session"]["ended_at"], serde_json::Value::Null);
    assert_eq!(json["session"]["summary"], serde_json::Value::Null);

    // Its prompts, in the order they were said.
    let prompts: Vec<&str> = json["prompts"]
        .as_array()
        .expect("prompts array")
        .iter()
        .map(|prompt| prompt["content"].as_str().expect("prompt content"))
        .collect();
    assert_eq!(
        prompts,
        vec!["el puerto es 8081", "y el deploy va a staging-eu"],
        "{json}"
    );

    // The window says what it is, and it holds the memories of the
    // session's namespace created inside it.
    assert_eq!(json["window"]["label"], WINDOW_LABEL, "{json}");
    assert_eq!(json["window"]["namespace"], "ns-a", "{json}");
    assert!(json["window"]["from"].is_string(), "{json}");
    assert_eq!(
        json["window"]["from"], json["session"]["started_at"],
        "the window opens when the session started: {json}"
    );
    assert_eq!(json["window"]["open"], true, "{json}");
    let uuids: Vec<&str> = json["memories_in_window"]
        .as_array()
        .expect("memories_in_window array")
        .iter()
        .map(|memory| memory["uuid"].as_str().expect("memory uuid"))
        .collect();
    assert_eq!(uuids, vec![inside.as_str()], "{json}");
    assert!(
        !uuids.contains(&elsewhere.as_str()),
        "another namespace is not read through this window: {json}"
    );

    // The human rendering says the same thing.
    let text = p
        .run_text(&["recall", "--session", "01JX"])
        .unwrap_status("recall text");
    let text = String::from_utf8_lossy(&text.stdout).to_string();
    assert!(text.contains("01JX"), "{text}");
    assert!(text.contains("y el deploy va a staging-eu"), "{text}");
    assert!(
        text.contains(WINDOW_LABEL),
        "the text says what the memories are: {text}"
    );
    assert!(
        text.contains(&inside),
        "the memory is in the answer: {text}"
    );
    assert!(!text.contains(&elsewhere), "{text}");
}

#[test]
fn recall_is_priced_preview_by_default_full_on_request() {
    let p = namespaced_profile(&["ns-a"]);
    p.run_silent(&["session", "--id", "01JX"])
        .unwrap_status("session");
    let inside = save(&p, "el puerto del servicio es 8081", &[]);

    // Default: the identity of every memory and a preview of its content,
    // the shape `list` got in #24/#26 (extension-recall S1). A recall that
    // answers with 100 full memories is not the cheap door the extension
    // would open.
    let out = p
        .run_silent(&["recall", "--session", "01JX"])
        .unwrap_status("recall default");
    let json = stdout_json(&out);
    let memories = json["memories_in_window"]
        .as_array()
        .expect("memories_in_window array");
    assert_eq!(memories.len(), 1, "{json}");
    assert_eq!(memories[0]["uuid"], inside.as_str(), "{json}");
    assert!(
        memories[0]["preview"].is_string(),
        "the default answer carries a preview: {json}"
    );
    assert!(
        memories[0].get("content").is_none(),
        "the default answer carries no full content: --full asks for it: {json}"
    );

    // `--full` asks for the whole records.
    let out = p
        .run_silent(&["recall", "--session", "01JX", "--full"])
        .unwrap_status("recall --full");
    let json = stdout_json(&out);
    let memories = json["memories_in_window"]
        .as_array()
        .expect("memories_in_window array");
    assert_eq!(memories.len(), 1, "{json}");
    assert_eq!(
        memories[0]["content"], "el puerto del servicio es 8081",
        "the full record is one flag away: {json}"
    );
}

#[test]
fn recall_previous_is_the_most_recent_finished_session() {
    let p = Profile::new();
    for (id, summary) in [("PRIMERA", "lo primero"), ("SEGUNDA", "lo segundo")] {
        p.run_silent(&["session", "--id", id])
            .unwrap_status("session");
        p.run_silent(&["end", "--id", id, "--summary", summary])
            .unwrap_status("end");
    }
    // A session that has not finished is not "the session before this one".
    p.run_silent(&["session", "--id", "ABIERTA"])
        .unwrap_status("open session");

    let out = p
        .run_silent(&["recall", "--previous"])
        .unwrap_status("recall --previous");
    let json = stdout_json(&out);
    assert_eq!(json["session"]["id"], "SEGUNDA", "{json}");
    assert_eq!(json["session"]["summary"], "lo segundo", "{json}");
    assert!(json["session"]["ended_at"].is_string(), "{json}");
    assert_eq!(
        json["window"]["to"], json["session"]["ended_at"],
        "a finished session closes its window where it ended: {json}"
    );
    assert_eq!(json["window"]["open"], false, "{json}");
}

#[test]
fn recall_previous_without_a_finished_session_says_there_is_none() {
    let p = Profile::new();
    // A session that is still open is not a finished one.
    p.run_silent(&["session", "--id", "ABIERTA"])
        .unwrap_status("open session");

    let out = p.run_silent(&["recall", "--previous"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("NOT_FOUND"), "stderr: {stderr}");
    assert!(
        stderr.contains("no finished session"),
        "the answer says there is none: {stderr}"
    );
    // NOT_FOUND is an error, not an empty answer.
    assert!(
        out.stdout.is_empty(),
        "stdout: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn recall_of_an_unknown_session_is_not_found() {
    let p = Profile::new();
    let out = p.run_silent(&["recall", "--session", "NO-EXISTE"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("NOT_FOUND"), "stderr: {stderr}");
    assert!(out.stdout.is_empty());
}

/// A profile that reads one namespace and writes two, so a session can live
/// where the read scope does not reach.
fn write_without_read_profile() -> Profile {
    Profile::new().with_config(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"otro\", \"proyecto-marcado\"]\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    )
}

#[test]
fn recall_of_a_session_outside_the_read_scope_is_not_found() {
    let p = write_without_read_profile();
    p.run_silent(&["session", "--id", "AJENA", "--namespace", "otro"])
        .unwrap_status("session outside the read scope");
    p.run_silent(&["end", "--id", "AJENA"]).unwrap_status("end");

    let out = p.run_silent(&["recall", "--session", "AJENA"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("NOT_FOUND"), "stderr: {stderr}");
    assert!(
        stderr.contains("allow-read-namespace"),
        "the answer names the route to the missing read: {stderr}"
    );
    // Nothing of the session reaches a caller that cannot read it.
    assert!(
        out.stdout.is_empty(),
        "stdout: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn recall_previous_never_reaches_a_namespace_the_profile_cannot_read() {
    let p = write_without_read_profile();
    let workspace = Workspace::marked(MARKER_NAMESPACE);
    // The session exists, in the namespace this workspace writes into.
    p.run_in_dir(
        workspace.path(),
        &["session", "--id", "MARC", "--namespace", MARKER_NAMESPACE],
    )
    .unwrap_status("session in the workspace namespace");
    p.run_in_dir(workspace.path(), &["end", "--id", "MARC"])
        .unwrap_status("end");

    let out = p.run_in_dir(workspace.path(), &["recall", "--previous"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("NOT_FOUND"), "stderr: {stderr}");
    assert!(
        stderr.contains("allow-read-namespace"),
        "the answer names the route to the missing read: {stderr}"
    );
    assert!(
        !stderr.contains("no finished session"),
        "a session exists there; the read is what is missing: {stderr}"
    );
    assert!(out.stdout.is_empty());
}

// --- identity visibility (identity-visibility S1) ---
//
// The title is the identity of a fact. Every read surface carries it, so a
// search result says what it *is* and not only what it is not. A memory
// saved without a title reports the empty string, exactly like its topic
// key — never `null` and never a placeholder.

#[test]
fn a_search_hit_carries_the_title_of_the_memory_it_refers_to() {
    let p = Profile::new();
    save(
        &p,
        "El servicio escucha en el puerto 8081",
        &["--title", "puerto del servicio"],
    );
    let out = p
        .run_silent(&["search", "puerto", "--limit", "10"])
        .unwrap_status("search");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 1, "search: {json}");
    let item = &json["items"][0];
    assert_eq!(
        item["title"], "puerto del servicio",
        "a hit says what it is: {json}"
    );
    // Additive: every key the hit already carried keeps its meaning.
    for key in [
        "uuid",
        "rank",
        "namespace",
        "memory_type",
        "state",
        "revision",
        "created_at",
        "valid_from",
        "valid_until",
    ] {
        assert!(
            item.get(key).is_some(),
            "the hit still carries `{key}`: {json}"
        );
    }
}

#[test]
fn the_compact_list_carries_the_title_and_still_carries_the_preview() {
    let p = namespaced_profile(&["proyecto-a"]);
    save(
        &p,
        "listable content",
        &["--namespace", "proyecto-a", "--title", "contenido listable"],
    );
    let out = p
        .run_silent(&["list", "--namespace", "proyecto-a"])
        .unwrap_status("list");
    let json = stdout_json(&out);
    let item = &json["items"][0];
    assert_eq!(
        item["title"], "contenido listable",
        "the identity travels with the preview: {json}"
    );
    assert_eq!(
        item["preview"], "listable content",
        "the compact list still shows the preview: {json}"
    );
    assert!(
        item.get("content").is_none(),
        "and it stays compact: {json}"
    );
}

#[test]
fn a_context_item_carries_the_title() {
    let p = namespaced_profile(&["default"]);
    save(
        &p,
        "el puerto del servicio es 8081",
        &["--title", "puerto del servicio"],
    );
    let out = p
        .run_silent(&[
            "context",
            "--namespace",
            "default",
            "--token-budget",
            "1024",
            "--budget-mode",
            "exact",
            "puerto",
        ])
        .unwrap_status("context");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "context: {json}");
    assert_eq!(
        items[0]["title"], "puerto del servicio",
        "the context says what its items are: {json}"
    );
    assert_eq!(
        items[0]["content"], "el puerto del servicio es 8081",
        "additive: the content stays: {json}"
    );
}

#[test]
fn a_memory_saved_without_a_title_reports_the_empty_string() {
    let p = Profile::new();
    save(&p, "sin titulo pero con contenido", &[]);

    let listed = p.run_silent(&["list"]).unwrap_status("list");
    let json = stdout_json(&listed);
    let item = &json["items"][0];
    assert_eq!(
        item["title"], "",
        "no title is the empty string, like the topic key: {json}"
    );
    assert_eq!(
        item["preview"], "sin titulo pero con contenido",
        "the preview stays: {json}"
    );

    let found = p
        .run_silent(&["search", "contenido", "--limit", "10"])
        .unwrap_status("search");
    let json = stdout_json(&found);
    assert_eq!(json["count"], 1, "search: {json}");
    assert_eq!(
        json["items"][0]["title"], "",
        "no title is the empty string, like the topic key: {json}"
    );
}

#[test]
fn the_explain_hits_carry_the_title_and_keep_the_matched_terms() {
    // `--explain` is the same command with more detail: an item shape that
    // changes with a flag is a trap, so the title travels here too.
    let p = Profile::new();
    save(
        &p,
        "bellman ford shortest paths",
        &["--title", "algoritmo de rutas"],
    );
    let out = p
        .run_silent(&["search", "--explain", "bellman"])
        .unwrap_status("search --explain");
    let json = stdout_json(&out);
    let item = &json["items"][0];
    assert_eq!(item["title"], "algoritmo de rutas", "explain: {json}");
    assert!(
        item["matched_terms"]
            .as_array()
            .map(|terms| !terms.is_empty())
            .unwrap_or(false),
        "the explanation still explains: {json}"
    );
}

#[test]
fn the_full_list_carries_the_title_and_the_whole_content() {
    let p = namespaced_profile(&["proyecto-a"]);
    save(
        &p,
        "listable content",
        &["--namespace", "proyecto-a", "--title", "contenido listable"],
    );
    let out = p
        .run_silent(&["list", "--namespace", "proyecto-a", "--full"])
        .unwrap_status("list --full");
    let json = stdout_json(&out);
    let item = &json["items"][0];
    assert_eq!(
        item["content"], "listable content",
        "--full still means the whole content: {json}"
    );
    assert_eq!(item["title"], "contenido listable", "--full: {json}");
}

#[test]
fn the_human_line_of_a_search_and_a_list_names_the_title() {
    let p = Profile::new();
    save(
        &p,
        "el puerto del servicio es 8081",
        &["--title", "puerto del servicio"],
    );

    let search = p.run_text(&["search", "puerto"]);
    assert!(
        search.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&search.stderr)
    );
    let text = String::from_utf8_lossy(&search.stdout);
    assert!(
        text.contains("\"puerto del servicio\""),
        "the hit line names the title: {text}"
    );
    assert!(text.contains("rank="), "and keeps the shape it had: {text}");

    let list = p.run_text(&["list"]);
    assert!(
        list.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&list.stderr)
    );
    let text = String::from_utf8_lossy(&list.stdout);
    assert!(
        text.contains("\"puerto del servicio\""),
        "the list line names the title: {text}"
    );
}

// --- the words of a fact (agent-keywords S1, S4) ---
//
// A memory carries the words its fact is called by in another language or
// another team's jargon, so a search in one vocabulary finds a record
// written in another. The agent supplies them on the save, they accumulate
// across sessions, and every read answer carries them back — a memory with
// no words reports the empty list, never `null`.

#[test]
fn a_save_carries_the_words_and_the_answer_carries_them_back() {
    let p = Profile::new();
    let out = p
        .run_silent(&[
            "save",
            "--type",
            "fact",
            "--title",
            "puerto del servicio",
            "--keywords",
            "puerto, port, listen port",
            "El servicio escucha en el puerto 8081",
        ])
        .unwrap_status("save --keywords");
    let json = stdout_json(&out);
    assert_eq!(
        json["keywords"],
        serde_json::json!(["puerto", "port", "listen port"]),
        "the save's answer carries the words the record kept: {json}"
    );
    let uuid = json["uuid"]
        .as_str()
        .expect("uuid in save output")
        .to_string();

    let shown = p.run_silent(&["show", &uuid]).unwrap_status("show");
    assert_eq!(
        stdout_json(&shown)["keywords"],
        serde_json::json!(["puerto", "port", "listen port"]),
        "the record itself keeps them"
    );
}

#[test]
fn the_human_answer_of_a_save_shows_the_words() {
    let p = Profile::new();
    let out = p.run_text(&[
        "save",
        "--title",
        "puerto del servicio",
        "--keywords",
        "puerto, port",
        "El servicio escucha en el puerto 8081",
    ]);
    assert!(
        out.status.success(),
        "stderr: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("puerto, port"),
        "the human answer shows the accumulated words: {text}"
    );
}

#[test]
fn a_second_save_accumulates_the_words_of_the_first() {
    let p = Profile::new();
    let first = save(
        &p,
        "El servicio escucha en el puerto 8081",
        &[
            "--title",
            "puerto del servicio",
            "--keywords",
            "puerto, port",
        ],
    );
    let out = p
        .run_silent(&[
            "save",
            "--type",
            "fact",
            "--title",
            "puerto del servicio",
            "--keywords",
            "listen port",
            "El servicio escucha en el puerto 9090",
        ])
        .unwrap_status("save (second)");
    let json = stdout_json(&out);
    assert_eq!(
        json["uuid"], first,
        "the same title lands on the same record: {json}"
    );
    assert_eq!(
        json["keywords"],
        serde_json::json!(["listen port", "puerto", "port"]),
        "the words accumulate, the requested ones first: {json}"
    );
}

#[test]
fn replace_keywords_replaces_the_accumulated_words() {
    let p = Profile::new();
    save(
        &p,
        "El servicio escucha en el puerto 8081",
        &[
            "--title",
            "puerto del servicio",
            "--keywords",
            "puerto, port",
        ],
    );
    let out = p
        .run_silent(&[
            "save",
            "--type",
            "fact",
            "--title",
            "puerto del servicio",
            "--keywords",
            "listen port",
            "--replace-keywords",
            "El servicio escucha en el puerto 9090",
        ])
        .unwrap_status("save --replace-keywords");
    let json = stdout_json(&out);
    assert_eq!(
        json["keywords"],
        serde_json::json!(["listen port"]),
        "a wrong word is replaced, not accumulated: {json}"
    );
}

#[test]
fn a_save_without_words_answers_with_the_empty_list() {
    let p = Profile::new();
    let out = p
        .run_silent(&["save", "--type", "fact", "sin palabras"])
        .unwrap_status("save");
    let json = stdout_json(&out);
    assert_eq!(
        json["keywords"],
        serde_json::json!([]),
        "no words is the empty list, never null: {json}"
    );
}

#[test]
fn a_search_hit_carries_the_words_of_the_memory() {
    let p = Profile::new();
    save(
        &p,
        "listen on port 8081",
        &["--title", "listen port", "--keywords", "puerto, port"],
    );
    let out = p
        .run_silent(&["search", "listen", "--limit", "10"])
        .unwrap_status("search");
    let json = stdout_json(&out);
    assert_eq!(json["count"], 1, "search: {json}");
    assert_eq!(
        json["items"][0]["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the hit carries the words: {json}"
    );

    save(&p, "sin palabras pero encontrable", &[]);
    let out = p
        .run_silent(&["search", "encontrable", "--limit", "10"])
        .unwrap_status("search (no words)");
    let json = stdout_json(&out);
    assert_eq!(
        json["items"][0]["keywords"],
        serde_json::json!([]),
        "a memory with no words is the empty list, never null: {json}"
    );
}

#[test]
fn the_explain_hits_carry_the_words_and_keep_the_matched_terms() {
    let p = Profile::new();
    save(
        &p,
        "listen on port 8081",
        &["--title", "listen port", "--keywords", "puerto, port"],
    );
    let out = p
        .run_silent(&["search", "--explain", "listen"])
        .unwrap_status("search --explain");
    let json = stdout_json(&out);
    let item = &json["items"][0];
    assert_eq!(
        item["keywords"],
        serde_json::json!(["puerto", "port"]),
        "explain: {json}"
    );
    assert!(
        item["matched_terms"]
            .as_array()
            .map(|terms| !terms.is_empty())
            .unwrap_or(false),
        "the explanation still explains: {json}"
    );
}

#[test]
fn the_compact_list_carries_the_words_and_still_carries_the_preview() {
    let p = namespaced_profile(&["proyecto-a"]);
    save(
        &p,
        "listable content",
        &[
            "--namespace",
            "proyecto-a",
            "--title",
            "contenido listable",
            "--keywords",
            "puerto, port",
        ],
    );
    save(&p, "sin palabras", &["--namespace", "proyecto-a"]);
    let out = p
        .run_silent(&["list", "--namespace", "proyecto-a"])
        .unwrap_status("list");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items array");

    let worded = items
        .iter()
        .find(|item| item["preview"] == "listable content")
        .unwrap_or_else(|| panic!("the worded memory is listed: {json}"));
    assert_eq!(
        worded["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the words travel with the preview: {json}"
    );
    assert_eq!(
        worded["preview"], "listable content",
        "the compact list still shows the preview: {json}"
    );

    let wordless = items
        .iter()
        .find(|item| item["preview"] == "sin palabras")
        .unwrap_or_else(|| panic!("the wordless memory is listed: {json}"));
    assert_eq!(
        wordless["keywords"],
        serde_json::json!([]),
        "no words is the empty list, never null: {json}"
    );
}

#[test]
fn a_context_item_carries_the_words() {
    let p = namespaced_profile(&["default"]);
    save(
        &p,
        "listen on port 8081",
        &["--title", "listen port", "--keywords", "puerto, port"],
    );
    let out = p
        .run_silent(&[
            "context",
            "--namespace",
            "default",
            "--token-budget",
            "1024",
            "--budget-mode",
            "exact",
            "listen",
        ])
        .unwrap_status("context");
    let json = stdout_json(&out);
    let items = json["items"].as_array().expect("items array");
    assert_eq!(items.len(), 1, "context: {json}");
    assert_eq!(
        items[0]["keywords"],
        serde_json::json!(["puerto", "port"]),
        "the context carries the words of its items: {json}"
    );
    assert_eq!(
        items[0]["content"], "listen on port 8081",
        "additive: the content stays: {json}"
    );
}

// --- the explanation answers what the search answers (read-scope-disclosure) --

#[test]
fn an_explanation_of_a_bounded_search_answers_the_same_as_the_search() {
    let p = Profile::new().with_config(
        "default_namespace = \"default\"\n\
         read_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\", \"secreto\"]\n\
         allow_global = false\n\
         allow_read_all = false\n\
         existence_disclosure = \"not_found\"\n\
         allowed_operations = [\"all\"]\n",
    );
    // A memory the profile may write but not read.
    let seeded = p.run_silent(&["save", "--namespace", "secreto", "el codigo es zafiro7"]);
    assert!(
        seeded.status.success(),
        "the seed must land: {:?}",
        String::from_utf8_lossy(&seeded.stderr)
    );

    let plain = p
        .run_silent(&["search", "zafiro7"])
        .unwrap_status("plain search");
    let explained = p
        .run_silent(&["search", "zafiro7", "--explain"])
        .unwrap_status("explained search");

    let plain_json = stdout_json(&plain);
    let explained_json = stdout_json(&explained);
    assert_eq!(
        plain_json["count"], 0,
        "the plain search sees nothing: {plain_json}"
    );
    assert_eq!(
        explained_json["count"], 0,
        "the explanation must answer the same as the search: {explained_json}"
    );
    assert_eq!(
        explained_json["read_scope"], "restricted",
        "and it declares the bound: {explained_json}"
    );
}

// --- stats does not reveal what the profile cannot read ---

#[test]
fn stats_does_not_reveal_a_namespace_the_profile_cannot_read() {
    let p = Profile::new().with_config(
        "default_namespace = \"default\"\n\
         read_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\", \"secreto\"]\n\
         allow_global = false\n\
         allow_read_all = false\n\
         existence_disclosure = \"not_found\"\n\
         allowed_operations = [\"all\"]\n",
    );
    let seeded = p.run_silent(&["save", "--namespace", "secreto", "un dato de otro proyecto"]);
    assert!(
        seeded.status.success(),
        "the seed must land: {:?}",
        String::from_utf8_lossy(&seeded.stderr)
    );

    let out = p
        .run_silent(&["stats"])
        .unwrap_status("stats with a closed scope");
    let json = stdout_json(&out);
    assert_eq!(
        json["namespaces"].as_array().map(Vec::len),
        Some(0),
        "a namespace the profile cannot read must not be counted: {json}"
    );
    assert_eq!(json["totals"]["memories"], 0, "{json}");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains("secreto"),
        "and it must not be named in the text: {text}"
    );
}

#[test]
fn stats_with_open_reads_counts_every_namespace() {
    let p = Profile::new();
    p.run_silent(&["config", "allow-namespace", "otro"])
        .unwrap_status("allow the other namespace");
    let seeded = p.run_silent(&["save", "--namespace", "otro", "un dato de otro proyecto"]);
    assert!(
        seeded.status.success(),
        "the seed must land: {:?}",
        String::from_utf8_lossy(&seeded.stderr)
    );

    let out = p
        .run_silent(&["stats"])
        .unwrap_status("stats with open reads");
    let json = stdout_json(&out);
    let namespaces = json["namespaces"].as_array().expect("namespaces array");
    assert!(
        namespaces.iter().any(|n| n["namespace"] == "otro"),
        "with the reads open every namespace is counted: {json}"
    );
}
