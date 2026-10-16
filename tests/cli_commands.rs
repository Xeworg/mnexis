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
    assert!(text.contains("0.1.0"), "stdout: {text}");
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
    assert_eq!(items[0]["content"], "listable content");
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

// --- errors and scope (S10) ---

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
