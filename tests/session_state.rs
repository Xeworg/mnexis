//! The session-state block on the door (call-budget ascension, S6).
//!
//! The block is one additive field of the `mnexis context` answer — and only
//! that answer: `mnexis status` does not carry it. It is project state with its
//! whole content: the `project_state` and `decision` records of the namespace,
//! newest first, plus the most recent session summary — the path
//! `mnexis recall --previous` takes. It is additive and outside the token
//! budget, so the budgeted items keep their selection, their order and their
//! count.
//!
//! The experiment selected it with a bench switch (`MNEXIS_BENCH_SESSION_STATE`)
//! and measured it against the shipped surfaces; the block won on the door and
//! the switch retired with it. These tests pin the shipped contract: the block
//! is there with no variable set, and setting the old variable changes nothing.
//!
//! Every test drives the real binary against a fresh profile under a tempdir
//! inside `target/`, like the other integration suites.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::tempdir_in;

/// The retired bench switch: it must not change any answer any more.
const RETIRED_SWITCH: &str = "MNEXIS_BENCH_SESSION_STATE";

/// Where the project is, long enough that a priced preview would have cut it:
/// the block carries the whole content.
const STATE_CONTENT: &str = "la puerta esta abierta y el bench necesita podman; las capturas \
corrieron con la version release del binario; el bloque viaja en la puerta porque la medicion lo \
puso ahi y el workspace conserva su forma preciada";

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

    fn run(&self, args: &[&str]) -> Output {
        self.run_as(None, None, args)
    }

    fn run_in_dir(&self, dir: &Path, args: &[&str]) -> Output {
        self.run_as(Some(dir), None, args)
    }

    /// Run with the retired switch set for the child process only.
    fn run_with_retired_switch(&self, value: &str, args: &[&str]) -> Output {
        self.run_as(None, Some(value), args)
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
        cmd.env_remove(RETIRED_SWITCH);
        if let Some(value) = switch {
            cmd.env(RETIRED_SWITCH, value);
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

/// A deterministic context answer: the same task and budget every time.
fn context_json(profile: &Profile, namespace: &str, task: &str) -> serde_json::Value {
    let out = profile.run(&[
        "context",
        "--namespace",
        namespace,
        "--token-budget",
        "1024",
        "--budget-mode",
        "exact",
        task,
    ]);
    let out = out.unwrap_status("context");
    stdout_json(&out)
}

/// The status payload for a working directory.
fn status_json(profile: &Profile, dir: &Path) -> serde_json::Value {
    let out = profile.run_in_dir(dir, &["status"]);
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

// --- rule 1: the door carries the block with nothing set ---

#[test]
fn the_door_carries_the_full_state_records_with_nothing_set() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");

    let door = context_json(&p, "proyecto", "alpha");
    let block = door
        .get("session_state")
        .unwrap_or_else(|| panic!("the shipped door carries the block: {door}"));
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
}

// --- rule 2: the block is outside the budget ---

#[test]
fn the_block_never_moves_the_budgeted_items() {
    let p = Profile::new(&["proyecto"]);
    seed(&p, "proyecto");

    let answer = context_json(&p, "proyecto", "alpha");
    let block = answer
        .get("session_state")
        .expect("the door carries the block");
    assert!(
        !block["records"].as_array().expect("records").is_empty(),
        "the fixture must exercise a non-empty block: {block}"
    );

    // The items are the searchable fact only: the state records ride the
    // block, and the budgeted envelope never counts them as items.
    let items = answer["items"].as_array().expect("items is a list");
    let titles: Vec<&str> = items
        .iter()
        .map(|item| item["title"].as_str().unwrap_or(""))
        .collect();
    assert!(
        !titles.contains(&"donde estamos") && !titles.contains(&"el destino"),
        "the state records ride the block, not the budgeted items: {titles:?}"
    );
}

// --- rule 3: the status payload does not carry the block ---

#[test]
fn status_does_not_carry_the_block() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let ws = Workspace::bare();

    let status = status_json(&p, ws.path());
    assert!(
        status.get("session_state").is_none(),
        "the workspace surface keeps its priced shape (#56): {status}"
    );
}

// --- rule 4: the block is the searched namespace, never another one ---

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

    let door = context_json(&p, "proyecto", "alpha");
    let block = door
        .get("session_state")
        .unwrap_or_else(|| panic!("the door carries the block: {door}"));
    let records = block["records"].as_array().expect("records");
    let listed = uuids(records);
    assert!(
        !listed.contains(&other_state) && !listed.contains(&other_decision),
        "the door must not carry another namespace's records: {block}"
    );
    assert_eq!(
        records.len(),
        2,
        "only the two records of the searched namespace: {block}"
    );
    assert_eq!(
        block["session"]["id"].as_str(),
        Some("S1"),
        "the summary is the searched namespace's own session: {block}"
    );
    assert_eq!(
        door["searched_namespace"].as_str(),
        Some("proyecto"),
        "the envelope still declares where it looked: {door}"
    );
}

// --- rule 5: an empty store still answers, with an empty block ---

#[test]
fn an_empty_store_still_answers_with_an_empty_block() {
    let p = Profile::new(&["proyecto"]);

    let context = context_json(&p, "proyecto", "alpha");
    let block = context
        .get("session_state")
        .unwrap_or_else(|| panic!("the door carries the block even when empty: {context}"));
    assert_eq!(
        block["records"],
        serde_json::json!([]),
        "no state record is the empty list, never null: {block}"
    );
    assert_eq!(
        block["session"],
        serde_json::Value::Null,
        "no session is null, and the command still answered: {block}"
    );
}

// --- rule 6: the retired switch changes nothing any more ---

#[test]
fn the_retired_switch_changes_no_answer() {
    let p = Profile::new(&["proyecto", "otro"]);
    seed(&p, "proyecto");
    let ws = Workspace::bare();

    let shipped = context_json(&p, "proyecto", "alpha");
    let shipped_status = status_json(&p, ws.path());

    for value in ["door", "workspace", "off", "turbo", ""] {
        let with_value = {
            let out = p.run_with_retired_switch(
                value,
                &[
                    "context",
                    "--namespace",
                    "proyecto",
                    "--token-budget",
                    "1024",
                    "--budget-mode",
                    "exact",
                    "alpha",
                ],
            );
            stdout_json(&out.unwrap_status("context"))
        };
        assert_eq!(
            with_value, shipped,
            "`{value}` must not change the door any more: the switch retired with the experiment"
        );

        let status_with_value = {
            let out = p.run_with_retired_switch(value, &["status"]);
            let mut cmd_status = out;
            if !cmd_status.status.success() {
                // `status` needs the workspace cwd; rerun it there.
                cmd_status = p.run_in_dir(ws.path(), &["status"]);
            }
            stdout_json(&cmd_status)
        };
        assert_eq!(
            status_with_value, shipped_status,
            "`{value}` must not change the status payload any more"
        );
    }
}
