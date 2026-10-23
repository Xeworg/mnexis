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

// --- save carries what the namespace already holds (near-duplicate S3, S4) ---

/// Save a titled memory and return its JSON envelope.
fn save_titled(profile: &Profile, title: &str, content: &str) -> serde_json::Value {
    let out = profile.run_silent(&[
        "save",
        "--type",
        "fact",
        "--title",
        title,
        "--namespace",
        "proyecto-a",
        content,
    ]);
    stdout_json(&out.unwrap_status("save"))
}

/// Run a command in text mode: the human rendering, not the envelope.
fn run_text(profile: &Profile, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("--data-dir")
        .arg(&profile.data_dir)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn mnexis binary")
}

/// A namespace with enough titles for a word to be evidence (a word used by
/// more than half of them is not), one of which a new title looks like.
fn namespace_with_history(profile: &Profile) -> serde_json::Value {
    let existing = save_titled(profile, "puerto del servicio", "el servicio usa 8081");
    save_titled(profile, "timeout de conexion", "treinta segundos");
    save_titled(profile, "reintentos de envio", "tres reintentos");
    existing
}

#[test]
fn a_save_carries_the_records_its_title_looks_like() {
    let p = namespaced_profile(&["proyecto-a"]);
    let existing = namespace_with_history(&p);

    let json = save_titled(&p, "puerto de pagos", "el puerto de pagos es 9090");

    // The save that creates the record is the moment to say what the
    // namespace already holds.
    assert_eq!(
        json["revision"], 1,
        "the envelope carries the revision the extension reads: {json}"
    );
    let candidates = json["duplicates"]
        .as_array()
        .unwrap_or_else(|| panic!("the envelope carries the shortlist: {json}"));
    assert_eq!(candidates.len(), 1, "one record looks like it: {json}");
    let candidate = &candidates[0];
    assert_eq!(candidate["uuid"], existing["uuid"]);
    assert_eq!(candidate["title"], "puerto del servicio");
    assert_eq!(candidate["topic_key"], "puerto-del-servicio");
    assert!(
        candidate["score"].as_f64().unwrap_or(0.0) > 0.0,
        "the proposal carries its score: {json}"
    );
    let shared = candidate["shared"]
        .as_array()
        .expect("the proposal carries the words it shares");
    assert!(
        shared.contains(&serde_json::json!("puerto")),
        "the evidence names the shared word: {json}"
    );

    // The agent adjudicates with the relation that already exists, and the
    // candidate is the older record.
    let link = candidate["link"].as_str().expect("the link command");
    assert_eq!(
        link,
        format!(
            "mnexis supersede --older {} --newer {} --reason \"same fact, different title\"",
            existing["uuid"].as_str().expect("uuid"),
            json["uuid"].as_str().expect("uuid")
        ),
    );
}

#[test]
fn a_title_with_nothing_in_common_carries_no_proposal() {
    let p = namespaced_profile(&["proyecto-a"]);
    namespace_with_history(&p);

    let json = save_titled(&p, "facturacion mensual", "la factura sale el dia uno");

    let candidates = json["duplicates"]
        .as_array()
        .unwrap_or_else(|| panic!("the envelope carries the shortlist: {json}"));
    assert!(candidates.is_empty(), "no proposal: {json}");
}

#[test]
fn a_save_that_updates_the_key_carries_no_proposal() {
    let p = namespaced_profile(&["proyecto-a"]);
    namespace_with_history(&p);

    let first = save_titled(&p, "puerto de pagos", "el puerto de pagos es 9090");
    assert_eq!(first["revision"], 1, "the first save creates: {first}");

    // The key was already recorded: the fact is one record, so there is
    // nothing to propose.
    let second = save_titled(&p, "puerto de pagos", "el puerto de pagos es 9091");
    assert_eq!(second["uuid"], first["uuid"], "one fact, one record");
    assert_eq!(second["revision"], 2, "the update bumps the revision");
    let candidates = second["duplicates"]
        .as_array()
        .unwrap_or_else(|| panic!("the envelope carries the shortlist: {second}"));
    assert!(candidates.is_empty(), "no proposal on an update: {second}");
}

#[test]
fn a_save_never_proposes_itself() {
    let p = namespaced_profile(&["proyecto-a"]);
    namespace_with_history(&p);

    let json = save_titled(&p, "puerto de pagos", "el puerto de pagos es 9090");
    let saved_uuid = json["uuid"].as_str().expect("uuid");
    let candidates = json["duplicates"]
        .as_array()
        .unwrap_or_else(|| panic!("the envelope carries the shortlist: {json}"));
    assert!(
        !candidates.is_empty(),
        "the shortlist is there, so the absence below means something: {json}"
    );
    assert!(
        !candidates
            .iter()
            .any(|c| c["uuid"].as_str() == Some(saved_uuid)),
        "the record just saved is not proposed to itself: {json}"
    );
}

#[test]
fn the_text_rendering_puts_the_proposal_after_the_save_line() {
    let p = namespaced_profile(&["proyecto-a"]);
    let existing = namespace_with_history(&p);

    let out = run_text(
        &p,
        &[
            "save",
            "--type",
            "fact",
            "--title",
            "puerto de pagos",
            "--namespace",
            "proyecto-a",
            "el puerto de pagos es 9090",
        ],
    );
    assert!(
        out.status.success(),
        "save failed: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let save_line = text.find("saved ").expect("the save line: {text}");
    let proposal = text
        .find("these records of the namespace look like this title:")
        .unwrap_or_else(|| panic!("the proposal follows the save line: {text:?}"));
    assert!(
        save_line < proposal,
        "the proposal comes after the save line: {text:?}"
    );
    assert!(
        text.contains("  - puerto del servicio [puerto-del-servicio] score "),
        "the proposal carries the title, the key and the score: {text:?}"
    );
    assert!(
        text.contains("shared: puerto"),
        "the proposal carries the evidence: {text:?}"
    );
    assert!(
        text.contains(&format!(
            "    if it is the same fact: mnexis supersede --older {}",
            existing["uuid"].as_str().expect("uuid")
        )),
        "the proposal names the command: {text:?}"
    );
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
fn init_keeps_the_reserved_namespace_closed_and_reads_open() {
    let p = Profile::new();
    let out = p
        .run_silent(&["config", "show"])
        .unwrap_status("config show");
    let json = stdout_json(&out);
    assert_eq!(json["scope"]["allow_global"], false);
    assert_eq!(json["scope"]["allow_read_all"], true);
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
fn a_titled_save_into_a_write_only_namespace_still_saves() {
    let p = Profile::new().with_config(
        "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\", \"ns-b\"]\nallow_global = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    );
    // ns-b is writable and not readable: the proposal must never turn a save
    // the scope allows into a read of a namespace it hides.
    let out = p.run_silent(&[
        "save",
        "--namespace",
        "ns-b",
        "--title",
        "puerto del servicio",
        "el servicio usa 8081",
    ]);
    let json = stdout_json(&out.unwrap_status("save into a write-only namespace"));
    assert_eq!(json["revision"], 1, "the save happens: {json}");
    assert_eq!(
        json["duplicates"],
        serde_json::json!([]),
        "a namespace the scope cannot read proposes nothing: {json}"
    );
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

    // healthy must be true
    assert_eq!(
        json["healthy"], true,
        "healthy should be true on a clean profile: {json}"
    );
    // checks array must be present with at least the expected fields
    let checks = json["checks"].as_array().expect("checks array");
    assert!(!checks.is_empty(), "checks array must not be empty: {json}");
    for check in checks {
        assert!(
            check["name"].is_string(),
            "each check must have a name: {check}"
        );
        assert!(
            check["ok"].is_boolean(),
            "each check must have an ok field: {check}"
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
        json["healthy"], false,
        "healthy must be false after FTS drop: {json}"
    );
    // In JSON mode the check detail is in stdout.
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
    let repair_out = p.run_silent(&["doctor", "--repair"]);
    assert_eq!(
        repair_out.status.code(),
        Some(0),
        "doctor --repair should exit 0: {:?}",
        String::from_utf8_lossy(&repair_out.stderr)
    );
    let repair_json = stdout_json(&repair_out);
    assert_eq!(
        repair_json["healthy"], true,
        "healthy_after must be true after repair: {repair_json}"
    );
    // The repaired block must be present.
    assert!(
        repair_json["repaired"].is_object(),
        "repaired block must be present after --repair: {repair_json}"
    );

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

// --- duplicates: the agent can ask on demand (near-duplicate-titles S5) ---

/// The `mnexis duplicates` answer for a title, in JSON mode.
fn duplicates(profile: &Profile, title: &str, extra: &[&str]) -> serde_json::Value {
    let mut args = vec!["duplicates", "--title", title, "--namespace", "proyecto-a"];
    args.extend_from_slice(extra);
    stdout_json(&profile.run_silent(&args).unwrap_status("duplicates"))
}

#[test]
fn duplicates_names_the_record_the_title_looks_like() {
    let p = namespaced_profile(&["proyecto-a"]);
    let existing = namespace_with_history(&p);

    let json = duplicates(&p, "puerto de pagos", &[]);

    // The envelope names the question it answered.
    assert_eq!(json["title"], "puerto de pagos", "{json}");
    assert_eq!(json["namespace"], "proyecto-a", "{json}");
    assert_eq!(json["count"], 1, "{json}");
    let items = json["items"]
        .as_array()
        .unwrap_or_else(|| panic!("the envelope carries the items: {json}"));
    assert_eq!(items.len(), 1, "one record looks like it: {json}");

    // The candidate carries the same evidence the save proposal carries,
    // and it is the shortlist the agent adjudicates on its own.
    let item = &items[0];
    assert_eq!(item["uuid"], existing["uuid"]);
    assert_eq!(item["title"], "puerto del servicio");
    assert_eq!(item["topic_key"], "puerto-del-servicio");
    assert!(
        item["score"].as_f64().unwrap_or(0.0) > 0.0,
        "the candidate carries its score: {json}"
    );
    let shared = item["shared"]
        .as_array()
        .expect("the candidate carries the words it shares");
    assert!(
        shared.contains(&serde_json::json!("puerto")),
        "the evidence names the shared word: {json}"
    );
    // Asking before saving means there is nothing to link yet.
    assert!(
        item.get("link").is_none(),
        "the on-demand list carries no link command: {json}"
    );

    // The human rendering lists the same candidate, one per line, with the
    // title, the key, the score and the evidence.
    let text = run_text(
        &p,
        &[
            "duplicates",
            "--title",
            "puerto de pagos",
            "--namespace",
            "proyecto-a",
        ],
    )
    .unwrap_status("duplicates (text)");
    let text = String::from_utf8(text.stdout).expect("utf-8 stdout");
    assert!(
        text.contains("  - puerto del servicio [puerto-del-servicio] score "),
        "the line carries the title, the key and the score: {text:?}"
    );
    assert!(
        text.contains("shared: puerto"),
        "the line carries the evidence: {text:?}"
    );
}

#[test]
fn a_title_with_nothing_in_common_lists_nothing() {
    let p = namespaced_profile(&["proyecto-a"]);
    namespace_with_history(&p);

    let json = duplicates(&p, "facturacion mensual", &[]);

    assert_eq!(json["count"], 0, "nothing looks like that title: {json}");
    assert_eq!(json["items"], serde_json::json!([]), "{json}");
}

#[test]
fn duplicates_limit_bounds_the_shortlist() {
    let p = namespaced_profile(&["proyecto-a"]);
    namespace_with_history(&p);

    // `servicio` and `conexion` are each evidence of one record, so two
    // records of the namespace look like this title.
    let all = duplicates(&p, "servicio de conexion", &[]);
    assert_eq!(all["count"], 2, "two records look like it: {all}");

    let one = duplicates(&p, "servicio de conexion", &["--limit", "1"]);
    assert_eq!(one["count"], 1, "--limit 1 asks for one: {one}");
    assert_eq!(one["items"].as_array().map(Vec::len), Some(1), "{one}");

    // The limit is clamped, not refused: a huge one answers like the max.
    let huge = duplicates(&p, "servicio de conexion", &["--limit", "1000"]);
    assert_eq!(huge["count"], 2, "--limit 1000 is clamped: {huge}");
}

#[test]
fn the_duplicates_text_says_when_there_is_nothing() {
    let p = namespaced_profile(&["proyecto-a"]);
    namespace_with_history(&p);

    let out = run_text(
        &p,
        &[
            "duplicates",
            "--title",
            "facturacion mensual",
            "--namespace",
            "proyecto-a",
        ],
    )
    .unwrap_status("duplicates (text)");
    let text = String::from_utf8(out.stdout).expect("utf-8 stdout");
    assert!(
        text.contains("no record of this namespace looks like that title"),
        "the human rendering says there is nothing: {text:?}"
    );
}

#[test]
fn a_marked_workspace_refuses_duplicates_for_another_namespace() {
    let p = guard_profile();
    let ws = Workspace::marked(MARKER_NAMESPACE);
    let out = p.run_in_dir(
        ws.path(),
        &[
            "duplicates",
            "--title",
            "puerto de pagos",
            "--namespace",
            OTHER_NAMESPACE,
        ],
    );
    assert_workspace_refusal(&out, MARKER_NAMESPACE);
}
