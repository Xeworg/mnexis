//! The session-state block on the door (call-budget ascension, S6).
//!
//! The block is one additive field of the `mnexis context` answer — and only
//! that answer: `mnexis status` does not carry it. It is project state: the
//! **live** `project_state` and `decision` records of the namespace, newest
//! first, with the whole content that fits the block's own budget; the live
//! records that do not fit ride an index (identity and title, no content) and
//! the most recent session summary rides beside them — the path `mnexis recall
//! --previous` takes. It is additive and outside the token budget of the
//! door, so the budgeted items keep their selection, their order and their
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

use mnexis_core::application::SessionState;
use mnexis_core::application::budget::token_count_v1;
use mnexis_core::application::service::session_state::SESSION_STATE_TOKEN_BUDGET;

/// The retired bench switch: it must not change any answer any more.
const RETIRED_SWITCH: &str = "MNEXIS_BENCH_SESSION_STATE";

/// Where the project is, long enough that a priced preview would have cut it:
/// the block carries the whole content.
const STATE_CONTENT: &str = "la puerta esta abierta y el bench necesita podman; las capturas \
corrieron con la version release del binario; el bloque viaja en la puerta porque la medicion lo \
puso ahi y el workspace conserva su forma preciada";

/// The decision the trap phase keeps: a chosen value, with its reason.
const DECISION_CONTENT: &str = "desplegamos a staging-eu; produccion no se toca en esta fase";

/// A record past half the block's budget: the shipped tokenizer counts words,
/// so ten words a repeat over 120 repeats costs about 1 200 tokens — one of
/// these fits the 2 048-token budget, two of them cannot.
fn big_state_content(tag: &str) -> String {
    format!(
        "{tag}: {}",
        "uno dos tres cuatro cinco seis siete ocho nueve diez ".repeat(120)
    )
}

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
        self.save_with_state(namespace, memory_type, title, content, "active")
    }

    /// Save one record in the lifecycle state the block's policy is about.
    fn save_with_state(
        &self,
        namespace: &str,
        memory_type: &str,
        title: &str,
        content: &str,
        state: &str,
    ) -> String {
        let out = self.run(&[
            "save",
            "--namespace",
            namespace,
            "--type",
            memory_type,
            "--state",
            state,
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

/// Every record the block carries, resident and indexed, in the order it
/// declares them.
fn block_uuids(block: &serde_json::Value) -> Vec<String> {
    let mut listed = Vec::new();
    for part in ["records", "index"] {
        if let Some(entries) = block[part].as_array() {
            listed.extend(uuids(entries));
        }
    }
    listed
}

/// The block as the service declares it: the wire JSON read back into the
/// shipped type, so a field the contract names and the answer drops is a
/// failure here, never a silent default.
fn declared_block(door: &serde_json::Value) -> SessionState {
    let block = door
        .get("session_state")
        .unwrap_or_else(|| panic!("the door carries the block: {door}"));
    serde_json::from_value(block.clone())
        .unwrap_or_else(|e| panic!("the block is the shape S5 declares ({e}): {block}"))
}

/// What one part of the block costs, with the shipped estimator: the block's
/// counts are a measurement, so the test measures the same parts.
fn cost_of<T: serde::Serialize>(part: &T) -> i64 {
    token_count_v1(&serde_json::to_string(part).expect("serialise the part"))
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
    assert_eq!(
        block["index"],
        serde_json::json!([]),
        "no indexed record is the empty list, never null: {block}"
    );
    assert_eq!(
        block["budget"].as_i64(),
        Some(SESSION_STATE_TOKEN_BUDGET),
        "an empty block still declares the budget it would use: {block}"
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

// --- rule 7: the live only, silently (S8.1) ---

#[test]
fn the_block_carries_only_the_live_states() {
    // `active` and `disputed` are the live states. `superseded`, `archived`
    // and `proposed` are the history: neither resident nor indexed, and not
    // even counted (S2: the block says nothing about what it left out).
    let p = Profile::new(&["proyecto"]);
    let live = p.save_with_state(
        "proyecto",
        "project_state",
        "donde estamos",
        STATE_CONTENT,
        "active",
    );
    let disputed = p.save_with_state(
        "proyecto",
        "decision",
        "lo que se discute",
        DECISION_CONTENT,
        "disputed",
    );
    let history = [
        p.save_with_state(
            "proyecto",
            "project_state",
            "lo superado",
            "esto quedo superado",
            "superseded",
        ),
        p.save_with_state(
            "proyecto",
            "decision",
            "lo archivado",
            "esto se archivo",
            "archived",
        ),
        p.save_with_state(
            "proyecto",
            "decision",
            "lo propuesto",
            "esto se propone",
            "proposed",
        ),
    ];

    let door = context_json(&p, "proyecto", "alpha");
    let block = &door["session_state"];
    let listed = block_uuids(block);

    let mut expected = vec![live, disputed];
    expected.sort();
    let mut shipped = listed.clone();
    shipped.sort();
    assert_eq!(
        shipped, expected,
        "the live records only, and no history in either part: {block}"
    );
    for uuid in &history {
        assert!(
            !listed.contains(uuid),
            "a record that is not live is not carried at all ({uuid}): {block}"
        );
    }
    for record in block["records"]
        .as_array()
        .expect("records is a list")
        .iter()
        .chain(block["index"].as_array().unwrap_or(&Vec::new()))
    {
        let state = record["state"]
            .as_str()
            .unwrap_or_else(|| panic!("every carried record declares its state: {record}"));
        assert!(
            state == "active" || state == "disputed",
            "the block carries live records only, and {state} is not live: {record}"
        );
    }
}

#[test]
fn the_live_state_filter_is_in_the_sql_not_after_the_limit() {
    // The cap is 25 records. One live record, then 25 dead ones that are
    // newer: a filter applied after the query would let the dead ones fill
    // the cap and hide the live record. The filter has to be in the SQL, so
    // the 25 newest *live* rows are the candidates.
    let p = Profile::new(&["proyecto"]);
    let live = p.save_with_state(
        "proyecto",
        "project_state",
        "el unico vivo",
        "este sigue siendo el estado",
        "active",
    );
    for i in 0..25 {
        p.save_with_state(
            "proyecto",
            "project_state",
            &format!("historia {i}"),
            &format!("esta historia ya no cuenta {i}"),
            "archived",
        );
    }

    let door = context_json(&p, "proyecto", "alpha");
    let block = &door["session_state"];
    let listed = block_uuids(block);
    assert_eq!(
        listed,
        vec![live],
        "the dead rows are filtered before the cap, so the live one is a candidate: {block}"
    );
}

// --- rule 8: the budget of the resident part (S8.2) ---

#[test]
fn the_resident_part_stays_within_the_budget_and_the_rest_is_indexed() {
    let p = Profile::new(&["proyecto"]);
    let mut saved: Vec<String> = Vec::new();
    for i in 0..5 {
        let title = format!("corte {i}");
        saved.push(p.save_with_state(
            "proyecto",
            "project_state",
            &title,
            &big_state_content(&title),
            "active",
        ));
    }
    // Newest first: the reverse of the save order.
    let newest_first: Vec<String> = saved.iter().rev().cloned().collect();

    let door = context_json(&p, "proyecto", "alpha");
    let block = &door["session_state"];
    let records = uuids(block["records"].as_array().expect("records is a list"));
    let index = block["index"]
        .as_array()
        .unwrap_or_else(|| panic!("the block declares what did not fit: {block}"));
    let indexed = uuids(index);

    let budget = block["budget"]
        .as_i64()
        .unwrap_or_else(|| panic!("the block declares its budget: {block}"));
    let resident_tokens = block["resident_tokens"]
        .as_i64()
        .unwrap_or_else(|| panic!("the block declares what the resident part costs: {block}"));
    assert_eq!(budget, 2048, "the budget of the resident part: {block}");
    assert!(
        resident_tokens <= budget,
        "the resident part is never above its budget: {resident_tokens} > {budget}"
    );

    // The fixture is past the budget: the newest record fits, something older
    // does not.
    assert!(
        !records.is_empty(),
        "at least the newest record is resident: {block}"
    );
    assert!(
        !indexed.is_empty(),
        "the fixture costs more than the budget, so the rest is indexed: {block}"
    );

    // Greedy and newest-first: the resident part is a prefix of the live
    // records, the index is everything after the first one that did not fit.
    assert_eq!(
        records,
        newest_first[..records.len()].to_vec(),
        "the resident part is the newest live records, in order: {block}"
    );
    assert_eq!(
        indexed,
        newest_first[records.len()..].to_vec(),
        "the index is every live record after the first that did not fit: {block}"
    );

    // Nothing is lost and nothing is carried twice.
    let mut carried = records.clone();
    carried.extend(indexed.clone());
    carried.sort();
    let mut expected = newest_first.clone();
    expected.sort();
    assert_eq!(carried, expected, "every live record, once: {block}");

    // The index is identity and title: an exact uuid the agent can pull with
    // `mnexis show`, and no content.
    for entry in index {
        assert!(
            entry.get("content").is_none(),
            "an indexed record carries no content: {entry}"
        );
        assert!(entry["memory_type"].is_string(), "{entry}");
        assert!(entry["title"].is_string(), "{entry}");
        assert!(entry["created_at"].is_string(), "{entry}");
        assert!(entry["state"].is_string(), "{entry}");
    }
}

#[test]
fn a_record_past_the_budget_takes_every_record_after_it_into_the_index() {
    // The fill stops at the first candidate that does not fit: the newest
    // record here is a single record past the whole budget, and the two
    // smaller live records behind it are indexed with it — even though they
    // would have fitted on their own. Deterministic and newest-first: the
    // order is the decision, so it never depends on which older record would
    // still fit.
    let p = Profile::new(&["proyecto"]);
    let older = p.save_with_state(
        "proyecto",
        "project_state",
        "el estado viejo",
        "el estado de antes sigue vivo",
        "active",
    );
    let middle = p.save_with_state(
        "proyecto",
        "decision",
        "la decision de antes",
        DECISION_CONTENT,
        "disputed",
    );
    // A single record past the 2 048-token budget, inside the store's 16 KiB
    // content ceiling.
    let huge = p.save_with_state(
        "proyecto",
        "project_state",
        "el corte enorme",
        &format!(
            "el corte enorme: {}",
            "uno dos tres cuatro cinco seis siete ocho nueve diez ".repeat(300)
        ),
        "active",
    );

    let door = context_json(&p, "proyecto", "alpha");
    let block = &door["session_state"];
    assert_eq!(
        block["records"].as_array().expect("records is a list"),
        &Vec::<serde_json::Value>::new(),
        "a record past the whole budget leaves the resident part empty: {block}"
    );
    assert_eq!(
        uuids(
            block["index"]
                .as_array()
                .expect("the block declares its index")
        ),
        vec![huge, middle, older],
        "the first one that does not fit, and every live record after it: {block}"
    );
    assert!(
        block["resident_tokens"].as_i64().expect("resident_tokens") <= 2048,
        "the budget still holds with an empty resident part: {block}"
    );
}

// --- rule 9: the block declares its budget and what it costs (S8.3) ---

#[test]
fn the_block_declares_its_budget_and_what_the_whole_block_costs() {
    let p = Profile::new(&["proyecto"]);
    for i in 0..3 {
        let title = format!("corte {i}");
        p.save_with_state(
            "proyecto",
            "project_state",
            &title,
            &big_state_content(&title),
            "active",
        );
    }
    p.start_session("S1", "proyecto");
    p.end_session("S1", "cerramos la fase de captura; sigue el bench");

    let door = context_json(&p, "proyecto", "alpha");
    let block = &door["session_state"];
    for key in ["budget", "resident_tokens", "tokens"] {
        assert!(
            block[key].is_i64(),
            "the envelope declares `{key}` as a number: {block}"
        );
    }
    // The block stayed additive on the wire: every key it had keeps its name,
    // and `records` keeps its order and its members' names.
    for key in ["records", "session"] {
        assert!(
            block.get(key).is_some(),
            "the additive change keeps `{key}`: {block}"
        );
    }
    for key in ["uuid", "memory_type", "title", "created_at"] {
        assert!(
            block["records"][0].get(key).is_some(),
            "`records` keeps the field `{key}`: {block}"
        );
    }

    let budget = block["budget"].as_i64().expect("budget");
    let resident_tokens = block["resident_tokens"].as_i64().expect("resident_tokens");
    let tokens = block["tokens"].as_i64().expect("tokens");
    assert_eq!(budget, SESSION_STATE_TOKEN_BUDGET);
    assert_eq!(SESSION_STATE_TOKEN_BUDGET, 2048);
    assert!(
        tokens >= resident_tokens,
        "the whole block costs at least its resident part: {tokens} < {resident_tokens}"
    );
    assert!(
        tokens > resident_tokens,
        "the index and the session ride the block too, so it costs more than \
         the resident part alone: {block}"
    );
}

#[test]
fn the_declared_counts_are_the_measurement_of_the_parts() {
    // The counts are written by the shipped estimator over the shipped shape,
    // so a caller can recompute them from the answer it was given.
    let p = Profile::new(&["proyecto"]);
    for i in 0..3 {
        let title = format!("corte {i}");
        p.save_with_state(
            "proyecto",
            "project_state",
            &title,
            &big_state_content(&title),
            "active",
        );
    }
    p.start_session("S1", "proyecto");
    p.end_session("S1", "cerramos la fase de captura; sigue el bench");

    let door = context_json(&p, "proyecto", "alpha");
    let block = declared_block(&door);
    assert_eq!(
        block.resident_tokens,
        cost_of(&block.records),
        "`resident_tokens` is what the resident part costs: {block:?}"
    );
    let mut counted = block.clone();
    counted.tokens = 0;
    assert_eq!(
        block.tokens,
        cost_of(&counted),
        "`tokens` is what the whole block costs: {block:?}"
    );
    assert_eq!(block.budget, SESSION_STATE_TOKEN_BUDGET);
    assert!(!block.index.is_empty(), "the fixture spills into the index");
    assert!(block.session.is_some(), "the session summary rides too");
}

// --- rule 10: the door's budgeted items do not move (S8.4) ---

#[test]
fn the_blocks_budget_never_moves_the_budgeted_items() {
    // The block's budget is its own: filling it to the brim, and spilling the
    // rest into the index, leaves the budgeted items, their order and
    // `tokens_used` exactly where they were.
    let p = Profile::new(&["proyecto"]);
    p.save(
        "proyecto",
        "fact",
        "la palabra de busqueda",
        "alpha es la palabra de busqueda de este proyecto",
    );
    let before = context_json(&p, "proyecto", "alpha");
    assert!(
        !before["items"].as_array().expect("items").is_empty(),
        "the fixture must answer with an item: {before}"
    );

    for i in 0..6 {
        let title = format!("corte {i}");
        p.save_with_state(
            "proyecto",
            "decision",
            &title,
            &big_state_content(&title),
            "active",
        );
    }
    let after = context_json(&p, "proyecto", "alpha");
    assert!(
        !after["session_state"]["index"]
            .as_array()
            .expect("the block declares what did not fit")
            .is_empty(),
        "the fixture must exercise a block past its budget: {after}"
    );

    assert_eq!(
        before["items"], after["items"],
        "the block changes nothing about the budgeted items, or their order"
    );
    assert_eq!(
        before["tokens_used"], after["tokens_used"],
        "the block is outside the door's budget, so `tokens_used` does not move"
    );
}
