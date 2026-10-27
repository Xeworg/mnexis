//! call-budget S4 (C2, C3) — the session-state block, behind the bench switch.
//!
//! The block is one additive payload field carried by one surface at a time:
//! `mnexis context` with `MNEXIS_BENCH_SESSION_STATE=door`, `mnexis status`
//! with `MNEXIS_BENCH_SESSION_STATE=workspace`. With the switch unset the
//! shipped answers are unchanged — no field at all.
//!
//! The block is project state with its whole content: the `project_state` and
//! `decision` records of the namespace, newest first, plus the most recent
//! session summary — the path `mnexis recall --previous` takes. It is additive
//! and outside the token budget, so the budgeted items keep their selection
//! and their order.
//!
//! Every test drives the real binary against a fresh profile under a tempdir
//! inside `target/`, like the other integration suites. The switch is set on
//! the **child** process only: the test process environment is never mutated,
//! and `env_remove` keeps the suite hermetic.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::tempdir_in;

/// The bench switch the arms select the surface with (S4).
const SWITCH: &str = "MNEXIS_BENCH_SESSION_STATE";

/// Where the project is, long enough that the priced preview the other
/// surfaces return would have cut it: the block carries the whole content.
const STATE_CONTENT: &str = "la puerta esta abierta y el bench necesita docker; las capturas \
corrieron con la version release del binario; queda pendiente medir el coste del prefijo antes de \
decidir si el bloque viaja en la puerta o en el workspace";

/// The decision the trap phase keeps: a chosen value, with its reason.
const DECISION_CONTENT: &str = "desplegamos a staging-eu; produccion no se toca en esta fase";

struct Profile {
    data_dir: PathBuf,
    #[allow(dead_code)]
    guard: tempfile::TempDir,
}

impl Profile {
    fn new(namespaces: &[&str]) -> Self {
        let guard = tempdir_in("target").expect("tempdir under target/");
        let profile = Self {
            data_dir: guard.path().to_path_buf(),
            guard,
        };
        profile.run(&["init"]).unwrap_status("init bootstrap");
        std::fs::write(profile.data_dir.join("mnexis.toml"), config(namespaces))
            .expect("write mnexis.toml");
        profile
    }

    /// Run the binary with the bench switch unset for the child.
    fn run(&self, args: &[&str]) -> Output {
        self.run_as(None, None, args)
    }

    /// Run with the bench switch set for the child process only.
    fn run_with_switch(&self, switch: Option<&str>, args: &[&str]) -> Output {
        self.run_as(None, switch, args)
    }

    fn run_with_switch_in_dir(&self, dir: &Path, switch: Option<&str>, args: &[&str]) -> Output {
        self.run_as(Some(dir), switch, args)
    }

    fn run_as(&self, dir: Option<&Path>, switch: Option<&str>, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_mnexis"));
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        cmd.arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--json")
            .args(args)
            .stdin(Stdio::null());
        // Hermetic: the test process may carry the variable; the child sees
        // only what this test sets.
        cmd.env_remove(SWITCH);
        if let Some(value) = switch {
            cmd.env(SWITCH, value);
        }
        cmd.output().expect("spawn mnexis binary")
    }

    /// Save one record and return its uuid.
    fn save(&self, namespace: &str, memory_type: &str, title: &str, content: &str) -> String {
        let out = self.run(&[
            "save",
            "--namespace",
            namespace,
            "--type",
            memory_type,
            "--title",
            title,
            content,
        ]);
        let out = out.unwrap_status("save");
        stdout_json(&out)["uuid"]
            .as_str()
            .expect("uuid in save output")
            .to_string()
    }

    fn start_session(&self, id: &str, namespace: &str) {
        self.run(&["session", "--id", id, "--namespace", namespace])
            .unwrap_status("session");
    }

    fn end_session(&self, id: &str, summary: &str) {
        self.run(&["end", "--id", id, "--summary", summary])
            .unwrap_status("end");
    }
}

/// A working directory for the binary: a bare tempdir under `target/` carries
/// no `.mnexis-namespace` marker, so the workspace resolves to a key with no
/// namespace until a test binds it.
struct Workspace {
    dir: tempfile::TempDir,
}

impl Workspace {
    fn bare() -> Self {
        Self {
            dir: tempdir_in("target").expect("tempdir under target/"),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// The profile's allowlists are exactly `namespaces`; the first one is where a
/// save without a namespace lands.
fn config(namespaces: &[&str]) -> String {
    let list = namespaces
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "default_namespace = \"{first}\"\nread_namespaces = [{list}]\nwrite_namespaces = [{list}]\n\
         allow_global = false\nallow_read_all = false\n\
         existence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
        first = namespaces[0],
    )
}

trait UnwrapStatus {
    fn unwrap_status(self, what: &str) -> Output;
}

impl UnwrapStatus for Output {
    fn unwrap_status(self, what: &str) -> Output {
        assert!(
            self.status.success(),
            "{what} failed ({}): {}",
            self.status,
            String::from_utf8_lossy(&self.stderr)
        );
        self
    }
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let text = String::from_utf8(out.stdout.clone()).expect("utf-8 stdout");
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {text:?}\nstderr: {:?}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// A deterministic context answer: the same task and budget every time, so a
/// run with the block and a run without it compare directly.
fn context_json(
    profile: &Profile,
    switch: Option<&str>,
    namespace: &str,
    task: &str,
) -> serde_json::Value {
    let out = profile.run_with_switch(
        switch,
        &[
            "context",
            "--namespace",
            namespace,
            "--token-budget",
            "1024",
            "--budget-mode",
            "exact",
            task,
        ],
    );
    let out = out.unwrap_status("context");
    stdout_json(&out)
}

/// The status payload for a working directory.
fn status_json(profile: &Profile, dir: &Path, switch: Option<&str>) -> serde_json::Value {
    let out = profile.run_with_switch_in_dir(dir, switch, &["status"]);
    let out = out.unwrap_status("status");
    stdout_json(&out)
}

/// The seeded project: a searchable fact, one `project_state`, one `decision`,
/// and a finished session with a summary.
fn seed(profile: &Profile, namespace: &str) {
    profile.save(
        namespace,
        "fact",
        "la palabra de busqueda",
        "alpha es la palabra de busqueda de este proyecto",
    );
    profile.save(namespace, "project_state", "donde estamos", STATE_CONTENT);
    profile.save(namespace, "decision", "el destino", DECISION_CONTENT);
    profile.start_session("S1", namespace);
    profile.end_session("S1", "cerramos la fase de captura; sigue el bench");
}

fn uuids(records: &[serde_json::Value]) -> Vec<String> {
    records
        .iter()
        .map(|record| {
            record["uuid"]
                .as_str()
                .expect("every record carries its uuid")
                .to_string()
        })
        .collect()
}

// --- rule 1: the shipped answers are unchanged with the switch absent ---

#[test]
fn with_the_switch_off_neither_answer_carries_the_block() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let ws = Workspace::bare();

    let context = context_json(&p, None, "proyecto", "alpha");
    let status = status_json(&p, ws.path(), None);

    assert!(
        context.get("session_state").is_none(),
        "the shipped door answer carries no block: {context}"
    );
    assert!(
        status.get("session_state").is_none(),
        "the shipped status payload carries no block: {status}"
    );
}

// --- rule 2: door mode puts the block in the context answer, not in status ---

#[test]
fn door_mode_carries_the_full_state_records_in_context_and_leaves_status_alone() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let ws = Workspace::bare();

    let shipped = context_json(&p, None, "proyecto", "alpha");
    let door = context_json(&p, Some("door"), "proyecto", "alpha");

    let block = door
        .get("session_state")
        .unwrap_or_else(|| panic!("door mode carries the block: {door}"));
    let records = block["records"].as_array().expect("records is a list");

    // Newest first, and only the kinds that are project state.
    let kinds: Vec<&str> = records
        .iter()
        .map(|record| record["memory_type"].as_str().expect("a type"))
        .collect();
    assert_eq!(
        kinds,
        vec!["decision", "project_state"],
        "the state records only, newest first: {block}"
    );

    // The whole content, not the priced preview the other surfaces return.
    let state = records
        .iter()
        .find(|record| record["memory_type"] == "project_state")
        .expect("the project_state record");
    assert!(
        STATE_CONTENT.chars().count() > 160,
        "the fixture content is longer than a preview would keep"
    );
    assert_eq!(
        state["content"].as_str(),
        Some(STATE_CONTENT),
        "the block carries the content in full: {state}"
    );
    assert_eq!(state["title"].as_str(), Some("donde estamos"));
    assert!(
        state["uuid"].is_string(),
        "the uuid travels with it: {state}"
    );
    assert!(state["created_at"].is_string(), "and its time: {state}");

    let decision = records
        .iter()
        .find(|record| record["memory_type"] == "decision")
        .expect("the decision record");
    assert_eq!(
        decision["content"].as_str(),
        Some(DECISION_CONTENT),
        "the chosen value is here, with the reason: {decision}"
    );

    // A state record is in the block once, never twice.
    let listed = uuids(records);
    let mut unique = listed.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(
        listed.len(),
        unique.len(),
        "no record is duplicated: {block}"
    );

    // The most recent finished session's summary, with its content.
    assert_eq!(block["session"]["id"].as_str(), Some("S1"), "{block}");
    assert_eq!(
        block["session"]["summary"].as_str(),
        Some("cerramos la fase de captura; sigue el bench"),
        "{block}"
    );

    // The block is additive and outside the budget: the budgeted hits keep
    // their selection, their order and their count.
    assert_eq!(
        door["items"], shipped["items"],
        "the block must not change the selection or the order of the items"
    );
    assert_eq!(
        door["tokens_used"], shipped["tokens_used"],
        "the block is outside the token budget"
    );

    // The other surface stays shipped in door mode.
    let status = status_json(&p, ws.path(), Some("door"));
    assert!(
        status.get("session_state").is_none(),
        "door mode does not touch status: {status}"
    );
}

// --- rule 3: workspace mode puts the block in status, not in the door ---

#[test]
fn workspace_mode_carries_the_block_in_status_and_leaves_the_door_alone() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let ws = Workspace::bare();

    // Bind the workspace to the project's namespace, so the status payload
    // resolves it the way the bench's first call does.
    let key = status_json(&p, ws.path(), None)["workspace"]["key"]
        .as_str()
        .expect("workspace key")
        .to_string();
    p.run(&["project", "bind", "proyecto", "--key", &key])
        .unwrap_status("project bind");

    let status = status_json(&p, ws.path(), Some("workspace"));
    let block = status
        .get("session_state")
        .unwrap_or_else(|| panic!("workspace mode carries the block: {status}"));
    let records = block["records"].as_array().expect("records is a list");
    let kinds: Vec<&str> = records
        .iter()
        .map(|record| record["memory_type"].as_str().expect("a type"))
        .collect();
    assert_eq!(kinds, vec!["decision", "project_state"], "{block}");
    assert_eq!(
        records
            .iter()
            .find(|record| record["memory_type"] == "project_state")
            .expect("the project_state record")["content"]
            .as_str(),
        Some(STATE_CONTENT),
        "the whole content, like the door"
    );
    assert_eq!(
        block["session"]["summary"].as_str(),
        Some("cerramos la fase de captura; sigue el bench"),
        "{block}"
    );

    // The shipped status fields are untouched.
    assert_eq!(status["workspace"]["namespace"].as_str(), Some("proyecto"));
    assert!(status["recent"].is_array(), "{status}");
    assert_eq!(status["memory_count"], 3, "{status}");

    // The door stays shipped in workspace mode.
    let context = context_json(&p, Some("workspace"), "proyecto", "alpha");
    assert!(
        context.get("session_state").is_none(),
        "workspace mode does not touch the door: {context}"
    );
}

// --- rule 4: the block is the project's namespace, never another one ---

#[test]
fn the_block_never_leaks_another_namespaces_records() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let other_state = p.save(
        "otro",
        "project_state",
        "lo de otro",
        "el estado de otro proyecto",
    );
    let other_decision = p.save(
        "otro",
        "decision",
        "decision de otro",
        "otro decide otra cosa",
    );
    p.start_session("OTRA", "otro");
    p.end_session("OTRA", "lo que hizo el otro proyecto");
    let ws = Workspace::bare();
    let key = status_json(&p, ws.path(), None)["workspace"]["key"]
        .as_str()
        .expect("workspace key")
        .to_string();
    p.run(&["project", "bind", "proyecto", "--key", &key])
        .unwrap_status("project bind");

    let door = context_json(&p, Some("door"), "proyecto", "alpha");
    let workspace = status_json(&p, ws.path(), Some("workspace"));

    for (surface, payload) in [("context", &door), ("status", &workspace)] {
        let block = payload
            .get("session_state")
            .unwrap_or_else(|| panic!("{surface} carries the block: {payload}"));
        let records = block["records"].as_array().expect("records");
        let listed = uuids(records);
        assert!(
            !listed.contains(&other_state) && !listed.contains(&other_decision),
            "{surface} must not carry another namespace's records: {block}"
        );
        assert_eq!(
            records.len(),
            2,
            "{surface}: only the two records of this project: {block}"
        );
        assert_eq!(
            block["session"]["id"].as_str(),
            Some("S1"),
            "{surface}: the summary is the project's own session, not the other one: {block}"
        );
    }
}

// --- rule 5: an empty store still answers, with an empty block ---

#[test]
fn an_empty_store_still_answers_with_an_empty_block() {
    let p = Profile::new(&["proyecto"]);
    let ws = Workspace::bare();

    let context = context_json(&p, Some("door"), "proyecto", "alpha");
    let status = status_json(&p, ws.path(), Some("workspace"));

    for (surface, payload) in [("context", &context), ("status", &status)] {
        let block = payload
            .get("session_state")
            .unwrap_or_else(|| panic!("{surface} carries the block even when empty: {payload}"));
        assert_eq!(
            block["records"],
            serde_json::json!([]),
            "{surface}: no state record is the empty list, never null: {block}"
        );
        assert_eq!(
            block["session"],
            serde_json::Value::Null,
            "{surface}: no session is null, and the command still answered: {block}"
        );
    }
}

// --- rule 6: an unknown switch value behaves exactly like off ---

#[test]
fn an_unknown_switch_value_behaves_exactly_like_off() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let ws = Workspace::bare();

    for value in ["turbo", "off", ""] {
        let shipped = context_json(&p, None, "proyecto", "alpha");
        let unknown = context_json(&p, Some(value), "proyecto", "alpha");
        assert_eq!(
            unknown, shipped,
            "`{value}` must read as unset, not as a mode"
        );
        assert!(unknown.get("session_state").is_none(), "{unknown}");

        let shipped_status = status_json(&p, ws.path(), None);
        let unknown_status = status_json(&p, ws.path(), Some(value));
        assert_eq!(
            unknown_status, shipped_status,
            "`{value}` must leave the status payload identical"
        );
        assert!(
            unknown_status.get("session_state").is_none(),
            "{unknown_status}"
        );
    }
}
