//! project-bindings T4/T5 — the project binding registry (S2, S4).
//!
//! The registry lives next to the profile (`projects.toml`) and maps a
//! workspace identity (key) to the namespace an agent writes into. The
//! stability guarantee is the point of these tests: a key must never
//! silently change its namespace, because that would split the memories.

use std::path::Path;
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
        let out = profile.run_silent(&["init"]);
        assert!(
            out.status.success(),
            "init failed: {:?}",
            String::from_utf8_lossy(&out.stderr)
        );
        profile
    }

    fn run_silent(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }

    fn with_config(self, toml: &str) -> Self {
        self.set_config(toml);
        self
    }

    fn set_config(&self, toml: &str) {
        std::fs::write(self.data_dir.join("mnexis.toml"), toml).expect("write mnexis.toml");
    }

    /// Run with an explicit working directory, so a test chooses the
    /// workspace the binary resolves.
    fn run_in_dir(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .current_dir(dir)
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }

    fn registry_text(&self) -> String {
        std::fs::read_to_string(self.data_dir.join("projects.toml")).unwrap_or_default()
    }

    fn registry_path(&self) -> std::path::PathBuf {
        self.data_dir.join("projects.toml")
    }

    /// Run the command inside a workspace whose identity is fixed: a git
    /// repository with one known remote. A test that depends on the
    /// remote of the checkout it runs in is not a test.
    fn run_in_workspace(&self, remote: &str, args: &[&str]) -> Output {
        let dir = tempdir_in("target").expect("tempdir under target/");
        let root = dir.path();
        git_workspace(root, remote);
        self.run_in_dir(root, args)
    }
}

fn git_workspace(root: &Path, remote: &str) {
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
    git(&["remote", "add", "origin", remote]);
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let text = stdout_of(out);
    serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("stdout is not JSON ({e}): {text:?}"))
}

/// A profile whose reads are exactly `read` and whose writes are exactly
/// `write`. Reads stay closed across namespaces, so a binding is the only
/// thing that can open one.
fn config(read: &[&str], write: &[&str]) -> String {
    let list = |names: &[&str]| {
        names
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!(
        "default_namespace = \"default\"\nread_namespaces = [{}]\nwrite_namespaces = [{}]\n\
         allow_global = false\nallow_read_all = false\n\
         existence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
        list(read),
        list(write),
    )
}

/// Save one memory through a working directory and return its uuid.
fn save_in(profile: &Profile, dir: &Path, namespace: &str, content: &str) -> String {
    let out = profile.run_in_dir(dir, &["--json", "save", "--namespace", namespace, content]);
    assert!(
        out.status.success(),
        "save failed ({}): {}",
        out.status,
        stderr_of(&out)
    );
    stdout_json(&out)["uuid"]
        .as_str()
        .expect("uuid in save output")
        .to_string()
}

/// A directory that names its namespace with the marker file only.
fn marked_workspace(namespace: &str) -> tempfile::TempDir {
    let dir = tempdir_in("target").expect("tempdir under target/");
    std::fs::write(
        dir.path().join(".mnexis-namespace"),
        format!("{namespace}\n"),
    )
    .expect("write .mnexis-namespace");
    dir
}

/// The key a human may write: the remote, not yet normalized.
const KEY_RAW: &str = "git:github.com/Xeworg/mnexis";
/// The key the engine records and every side compares against.
const KEY_CANONICAL: &str = "git:github.com-xeworg-mnexis";

#[test]
fn bind_records_the_binding_in_the_profile_registry() {
    let p = Profile::new();
    let out = p.run_silent(&[
        "project",
        "bind",
        "mnexis",
        "--key",
        KEY_RAW,
        "--path",
        "/home/xeworg/Proyectos/mnexis",
    ]);
    assert!(
        out.status.success(),
        "bind failed ({}): {}",
        out.status,
        stderr_of(&out)
    );
    let registry = p.registry_text();
    assert!(
        registry.contains(KEY_CANONICAL),
        "the registry must record the canonical workspace key: {registry}"
    );
    assert!(
        registry.contains("namespace = \"mnexis\""),
        "the registry must record the namespace: {registry}"
    );
    assert!(
        registry.contains("/home/xeworg/Proyectos/mnexis"),
        "the registry must record the workspace path: {registry}"
    );
}

#[test]
fn list_shows_the_recorded_bindings() {
    let p = Profile::new();
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);
    let out = p.run_silent(&["project", "list"]);
    assert!(out.status.success(), "list failed: {}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(
        text.contains(KEY_CANONICAL),
        "list must show the canonical key: {text}"
    );
    assert!(
        text.contains("mnexis"),
        "list must show the namespace: {text}"
    );
}

#[test]
fn list_on_a_profile_without_a_registry_is_empty_and_successful() {
    let p = Profile::new();
    let out = p.run_silent(&["project", "list"]);
    assert!(
        out.status.success(),
        "list on an empty registry must succeed: {}",
        stderr_of(&out)
    );
    assert!(
        stdout_of(&out).contains("No project bindings"),
        "an empty registry must say so: {}",
        stdout_of(&out)
    );
}

#[test]
fn the_json_of_the_list_is_a_pinned_contract() {
    use mnexis_core::cli::project_cmd::{ProjectBindingView, ProjectList};

    let p = Profile::new();
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);

    let out = p.run_silent(&["--json", "project", "list"]);
    assert!(out.status.success(), "list failed: {}", stderr_of(&out));
    let stdout = stdout_of(&out);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("the JSON must parse: {e}: {stdout}"));

    // The contract: the top-level fields, by name.
    let bindings = json["bindings"].as_array().expect("bindings array");
    assert_eq!(bindings.len(), 1, "one binding: {json}");
    let binding = &bindings[0];
    for field in ["key", "namespace", "path"] {
        assert!(
            binding.get(field).is_some(),
            "every binding carries {field}: {binding}"
        );
    }
    assert_eq!(
        binding["key"], KEY_CANONICAL,
        "the canonical key: {binding}"
    );
    assert_eq!(binding["namespace"], "mnexis", "{binding}");
    assert!(
        binding["path"].is_null(),
        "a binding without a path: {binding}"
    );
    assert!(
        json["current_key"].is_string(),
        "current_key is always a string: {json}"
    );

    // The contract is a type, not a hand-written envelope: the wire form
    // of the type has exactly these field names.
    let typed = serde_json::to_value(ProjectList {
        bindings: vec![ProjectBindingView {
            key: KEY_CANONICAL.into(),
            namespace: "mnexis".into(),
            path: None,
        }],
        current_key: String::new(),
    })
    .unwrap();
    assert_eq!(typed["bindings"][0]["key"], KEY_CANONICAL, "{typed}");
    assert_eq!(typed["bindings"][0]["namespace"], "mnexis", "{typed}");
    assert!(typed["bindings"][0]["path"].is_null(), "{typed}");
    assert!(typed["current_key"].is_string(), "{typed}");
}

#[test]
fn the_json_of_an_empty_registry_is_still_the_contract() {
    let p = Profile::new();
    let out = p.run_silent(&["--json", "project", "list"]);
    assert!(out.status.success(), "list failed: {}", stderr_of(&out));
    let stdout = stdout_of(&out);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("the JSON must parse: {e}: {stdout}"));
    assert_eq!(json["bindings"].as_array().map(Vec::len), Some(0), "{json}");
    assert!(json["current_key"].is_string(), "{json}");
}

#[test]
fn bind_is_idempotent_for_the_same_key_and_namespace() {
    let p = Profile::new();
    for _ in 0..3 {
        let out = p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);
        assert!(
            out.status.success(),
            "repeat bind failed: {}",
            stderr_of(&out)
        );
    }
    let registry = p.registry_text();
    assert_eq!(
        registry.matches("[[project]]").count(),
        1,
        "the same binding must not be duplicated: {registry}"
    );
}

#[test]
fn bind_refuses_to_rebind_a_key_to_another_namespace() {
    let p = Profile::new();
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);
    let out = p.run_silent(&["project", "bind", "other", "--key", KEY_RAW]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "rebinding a key is INVALID_ARGUMENT: {}",
        stderr_of(&out)
    );
    assert!(
        stderr_of(&out).contains("already bound"),
        "stderr must explain the conflict: {}",
        stderr_of(&out)
    );
    let registry = p.registry_text();
    assert!(
        registry.contains("namespace = \"mnexis\""),
        "the refused rebind must leave the binding intact: {registry}"
    );
    assert!(
        !registry.contains("namespace = \"other\""),
        "the refused rebind must not be written: {registry}"
    );
}

#[test]
fn bind_force_rebinds_a_key_to_another_namespace() {
    let p = Profile::new();
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);
    let out = p.run_silent(&["project", "bind", "other", "--key", KEY_RAW, "--force"]);
    assert!(
        out.status.success(),
        "forced rebind failed: {}",
        stderr_of(&out)
    );
    let registry = p.registry_text();
    assert!(
        registry.contains("namespace = \"other\""),
        "the forced rebind must be written: {registry}"
    );
    assert_eq!(
        registry.matches("[[project]]").count(),
        1,
        "the forced rebind must replace, not append: {registry}"
    );
}

#[test]
fn unbind_removes_the_binding() {
    let p = Profile::new();
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);
    let out = p.run_silent(&["project", "unbind", "mnexis"]);
    assert!(out.status.success(), "unbind failed: {}", stderr_of(&out));
    let list = p.run_silent(&["project", "list"]);
    assert!(
        stdout_of(&list).contains("No project bindings"),
        "the binding must be gone: {}",
        stdout_of(&list)
    );
}

#[test]
fn unbind_on_an_empty_registry_is_a_successful_no_op() {
    let p = Profile::new();
    let out = p.run_silent(&["project", "unbind", "mnexis"]);
    assert!(
        out.status.success(),
        "unbind with nothing bound must succeed: {}",
        stderr_of(&out)
    );
}

#[test]
fn bind_rejects_an_empty_namespace_and_an_empty_key() {
    let p = Profile::new();

    let empty_ns = p.run_silent(&["project", "bind", "", "--key", KEY_RAW]);
    assert_eq!(
        empty_ns.status.code(),
        Some(2),
        "an empty namespace is INVALID_ARGUMENT: {}",
        stderr_of(&empty_ns)
    );
    assert!(
        stderr_of(&empty_ns).contains("namespace must not be empty"),
        "stderr must name the problem: {}",
        stderr_of(&empty_ns)
    );

    let empty_key = p.run_silent(&["project", "bind", "mnexis", "--key", ""]);
    assert_eq!(
        empty_key.status.code(),
        Some(2),
        "an empty key is INVALID_ARGUMENT: {}",
        stderr_of(&empty_key)
    );
    assert!(
        stderr_of(&empty_key).contains("key must not be empty"),
        "stderr must name the problem: {}",
        stderr_of(&empty_key)
    );
}

// ── one canonical workspace key ─────────────────────────────────────────────
//
// A human may write the remote; the agent always computes the normalized
// key. Both have to name the same workspace, or the binding the human
// recorded is invisible to the agent.

#[test]
fn the_raw_and_the_normalized_key_are_the_same_binding() {
    let p = Profile::new();
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY_RAW]);
    let again = p.run_silent(&["project", "bind", "mnexis", "--key", KEY_CANONICAL]);
    assert!(
        again.status.success(),
        "the second bind failed: {}",
        stderr_of(&again)
    );
    let registry = p.registry_text();
    assert_eq!(
        registry.matches(KEY_CANONICAL).count(),
        1,
        "both keys name one workspace, so one binding is recorded: {registry}"
    );
}

#[test]
fn a_binding_recorded_before_the_rule_still_resolves() {
    let p = Profile::new();
    // A registry written by an older engine: the raw key, by hand.
    std::fs::write(
        p.registry_path(),
        "[[project]]\nkey = \"git:github.com/Xeworg/pagos\"\nnamespace = \"pagos\"\n",
    )
    .expect("write the legacy registry");
    let out = p.run_in_workspace("https://github.com/Xeworg/pagos.git", &["status", "--json"]);
    assert!(out.status.success(), "status failed: {}", stderr_of(&out));
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("status must answer json");
    assert_eq!(
        json["workspace"]["namespace"], "pagos",
        "the legacy binding must resolve to its namespace: {json}"
    );
    assert_eq!(
        json["workspace"]["key"], "git:github.com-xeworg-pagos",
        "the key is reported in its canonical form: {json}"
    );
}

#[test]
fn a_legacy_raw_binding_is_updated_in_place_not_duplicated() {
    let p = Profile::new();
    // The default state of an existing user's registry: one binding, a raw
    // key. Binding the same workspace again must not leave two entries
    // behind, and the stale one must not shadow the canonical one.
    std::fs::write(
        p.registry_path(),
        "[[project]]\nkey = \"git:github.com/Xeworg/pagos\"\nnamespace = \"pagos\"\n",
    )
    .expect("write the legacy registry");
    let out = p.run_silent(&[
        "project",
        "bind",
        "web",
        "--key",
        "https://github.com/Xeworg/pagos.git",
        "--force",
    ]);
    assert!(out.status.success(), "bind failed: {}", stderr_of(&out));
    let registry = p.registry_text();
    assert_eq!(
        registry.matches("[[project]]").count(),
        1,
        "one workspace, one binding, no shadow copy: {registry}"
    );
    assert!(
        !registry.contains("git:github.com/Xeworg/pagos"),
        "the stale raw key must be gone: {registry}"
    );
    assert!(
        registry.contains("git:github.com-xeworg-pagos"),
        "the canonical key is recorded: {registry}"
    );
}

// ── the binding implies reading (read-granularity S2) ──────────────────────
//
// A binding is a human permission recorded in the local registry. The
// `.mnexis-namespace` marker is a file in the repository, so it is not.
// Both facts have to hold at every read surface, and neither may widen the
// write allowlist.

const REMOTE: &str = "https://github.com/Xeworg/mnexis.git";
const BOUND_NAMESPACE: &str = "mnexis";

#[test]
fn a_binding_opens_the_bound_namespace_for_reads() {
    let p = Profile::new().with_config(&config(&["default"], &["default", BOUND_NAMESPACE]));
    let seed = tempdir_in("target").expect("tempdir under target/");
    let uuid = save_in(&p, seed.path(), BOUND_NAMESPACE, "memoria del proyecto");

    // Without the binding the profile allowlist is the only source of
    // permission, so the namespace is invisible.
    let before = p.run_in_workspace(REMOTE, &["--json", "list", "--namespace", BOUND_NAMESPACE]);
    assert_eq!(
        before.status.code(),
        Some(3),
        "an unbound read must stay NOT_FOUND: {}",
        stderr_of(&before)
    );

    p.run_silent(&["project", "bind", BOUND_NAMESPACE, "--key", KEY_RAW]);

    let listed = p.run_in_workspace(REMOTE, &["--json", "list", "--namespace", BOUND_NAMESPACE]);
    assert!(
        listed.status.success(),
        "the bound namespace must be readable: {}",
        stderr_of(&listed)
    );
    assert_eq!(stdout_json(&listed)["count"], 1, "the memory is visible");

    // A read without an explicit namespace uses the scope's read allowlist,
    // so the binding has to reach that too, not only the explicit check.
    let searched = p.run_in_workspace(REMOTE, &["--json", "search", "memoria"]);
    assert!(
        searched.status.success(),
        "search failed: {}",
        stderr_of(&searched)
    );
    assert_eq!(
        stdout_json(&searched)["count"],
        1,
        "the binding must open the namespace for scoped reads too"
    );

    let shown = p.run_in_workspace(REMOTE, &["--json", "show", &uuid]);
    assert!(shown.status.success(), "show failed: {}", stderr_of(&shown));
    assert_eq!(stdout_json(&shown)["namespace"], BOUND_NAMESPACE);
}

#[test]
fn a_binding_does_not_open_the_bound_namespace_for_writes() {
    let p = Profile::new().with_config(&config(&["default"], &["default", BOUND_NAMESPACE]));
    let seed = tempdir_in("target").expect("tempdir under target/");
    save_in(&p, seed.path(), BOUND_NAMESPACE, "solo lectura");
    // The profile no longer authorizes the namespace for writes: the
    // binding must not put it back.
    p.set_config(&config(&["default"], &["default"]));
    p.run_silent(&["project", "bind", BOUND_NAMESPACE, "--key", KEY_RAW]);

    // Reading is open...
    let listed = p.run_in_workspace(REMOTE, &["--json", "list", "--namespace", BOUND_NAMESPACE]);
    assert!(
        listed.status.success(),
        "the binding grants reading: {}",
        stderr_of(&listed)
    );
    assert_eq!(stdout_json(&listed)["count"], 1);

    // ...but writing still needs the namespace in `write_namespaces`.
    let refused = p.run_in_workspace(
        REMOTE,
        &[
            "--json",
            "save",
            "--namespace",
            BOUND_NAMESPACE,
            "no debe entrar",
        ],
    );
    assert_eq!(
        refused.status.code(),
        Some(3),
        "a write outside write_namespaces must stay NOT_FOUND: {}",
        stderr_of(&refused)
    );

    let after = p.run_in_workspace(REMOTE, &["--json", "list", "--namespace", BOUND_NAMESPACE]);
    assert_eq!(
        stdout_json(&after)["count"],
        1,
        "the refused write must store nothing"
    );
}

#[test]
fn the_marker_does_not_open_the_marked_namespace_for_reads() {
    let p = Profile::new().with_config(&config(&["default"], &["default", BOUND_NAMESPACE]));
    let seed = tempdir_in("target").expect("tempdir under target/");
    save_in(&p, seed.path(), BOUND_NAMESPACE, "memoria marcada");
    let ws = marked_workspace(BOUND_NAMESPACE);

    // The marker still resolves the workspace for writes...
    let status = p.run_in_dir(ws.path(), &["--json", "status"]);
    assert!(
        status.status.success(),
        "status failed: {}",
        stderr_of(&status)
    );
    let json = stdout_json(&status);
    assert_eq!(
        json["workspace"]["namespace"], BOUND_NAMESPACE,
        "the marker still names the workspace namespace: {json}"
    );
    // ...but it is a file, not a permission: the read scope is unchanged.
    assert_eq!(
        json["scope"]["read_namespaces"],
        serde_json::json!(["default"]),
        "the marker must not widen the read scope: {json}"
    );

    let listed = p.run_in_dir(
        ws.path(),
        &["--json", "list", "--namespace", BOUND_NAMESPACE],
    );
    assert_eq!(
        listed.status.code(),
        Some(3),
        "a marked workspace must not read its namespace: {}",
        stderr_of(&listed)
    );

    let searched = p.run_in_dir(ws.path(), &["--json", "search", "memoria"]);
    assert!(
        searched.status.success(),
        "search failed: {}",
        stderr_of(&searched)
    );
    assert_eq!(
        stdout_json(&searched)["count"],
        0,
        "the marker must not make the namespace searchable"
    );
}

#[test]
fn a_binding_never_opens_the_reserved_global_namespace() {
    let p = Profile::new();
    // Seed a global memory while the reserved namespace is allowed...
    p.set_config(
        "default_namespace = \"default\"\nread_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\", \"global\"]\nallow_global = true\n\
         allow_read_all = false\nexistence_disclosure = \"not_found\"\n\
         allowed_operations = [\"all\"]\n",
    );
    let seed = tempdir_in("target").expect("tempdir under target/");
    save_in(&p, seed.path(), "global", "memoria global");
    // ...then close it again and bind the workspace to it. The binding must
    // not reopen it through `read_namespaces`: `global` keeps its own flag.
    p.set_config(&config(&["default"], &["default", BOUND_NAMESPACE]));
    p.run_silent(&["project", "bind", "global", "--key", KEY_RAW]);

    let listed = p.run_in_workspace(REMOTE, &["--json", "list"]);
    assert!(
        listed.status.success(),
        "list failed: {}",
        stderr_of(&listed)
    );
    assert_eq!(
        stdout_json(&listed)["count"],
        0,
        "the reserved namespace stays closed without allow_global"
    );
    let direct = p.run_in_workspace(REMOTE, &["--json", "list", "--namespace", "global"]);
    assert_eq!(
        direct.status.code(),
        Some(3),
        "a global read must stay NOT_FOUND without allow_global: {}",
        stderr_of(&direct)
    );
}

// ── the topic-key suggestion is a read (topic-key-read-scope S1–S6) ────────
//
// `suggest-topic-key` asks which key a title yields and who holds it. It
// writes nothing, so the write isolation of the workspace must not refuse it;
// the read scope decides, and a read without an explicit namespace asks in the
// workspace's own namespace — the one a save lands in — the same rule the door
// (`mnexis context`) follows. The answer names the namespace it asked about.

/// Save one titled memory through a working directory and return its uuid.
/// The title is what the topic key derives from, so a suggestion can find the
/// record that already holds it.
fn save_titled_in(profile: &Profile, dir: &Path, namespace: &str, title: &str) -> String {
    let out = profile.run_in_dir(
        dir,
        &[
            "--json",
            "save",
            "--namespace",
            namespace,
            "--title",
            title,
            "el puerto del servicio es 8081",
        ],
    );
    assert!(
        out.status.success(),
        "save failed ({:?}): {}",
        out.status.code(),
        stderr_of(&out)
    );
    stdout_json(&out)["uuid"]
        .as_str()
        .expect("uuid in save output")
        .to_string()
}

#[test]
fn the_suggestion_asks_in_the_namespace_the_workspace_writes_into() {
    let p = Profile::new().with_config(&config(&["default"], &["default", BOUND_NAMESPACE]));
    let seed = tempdir_in("target").expect("tempdir under target/");
    let uuid = save_titled_in(&p, seed.path(), BOUND_NAMESPACE, "puerto del servicio");
    p.run_silent(&["project", "bind", BOUND_NAMESPACE, "--key", KEY_RAW]);

    // No `--namespace`: the namespace the workspace writes into is the one a
    // save lands in, so it is the one whose records the suggestion reads. The
    // write guard used to refuse this read because the profile's default is
    // not the workspace's namespace.
    let out = p.run_in_workspace(
        REMOTE,
        &["--json", "suggest-topic-key", "puerto del servicio"],
    );
    assert!(
        out.status.success(),
        "the suggestion is a read and must answer ({:?}): {}",
        out.status.code(),
        stderr_of(&out)
    );
    assert_eq!(stderr_of(&out), "", "a read answers without noise");
    let json = stdout_json(&out);
    assert_eq!(
        json["namespace"], BOUND_NAMESPACE,
        "the answer names the namespace it asked about: {json}"
    );
    assert_eq!(json["topic_key"], "puerto-del-servicio", "{json}");
    assert_eq!(
        json["recorded_by"], uuid,
        "the record of the workspace's namespace holds the key: {json}"
    );

    // The text names the namespace it asked about too, so a reader can tell
    // which project was consulted.
    let text = p.run_in_workspace(REMOTE, &["suggest-topic-key", "puerto del servicio"]);
    assert!(text.status.success(), "{}", stderr_of(&text));
    assert_eq!(
        stdout_of(&text),
        format!(
            "the key `puerto-del-servicio` is already recorded by {uuid} in namespace \
             `{BOUND_NAMESPACE}`\n"
        )
    );

    // And so does the help of the command: the namespace it asks about is the
    // workspace's, not the profile's default one (S6).
    let help = p.run_silent(&["suggest-topic-key", "--help"]);
    assert!(
        help.status.success(),
        "{}: {}",
        help.status,
        stderr_of(&help)
    );
    let help_text = stdout_of(&help);
    assert!(
        help_text.contains("the namespace this workspace writes into"),
        "the help must say which namespace the suggestion asks about: {help_text}"
    );
    assert!(
        !help_text.contains("Namespace (default: the profile's default_namespace)"),
        "the help must not claim the profile's default namespace: {help_text}"
    );
}

#[test]
fn the_suggestion_is_not_a_write_and_a_write_still_leaves_the_workspace_nowhere() {
    // `otro` is readable but not writable: the read may ask about it, the
    // write may not land in it.
    let p =
        Profile::new().with_config(&config(&["default", "otro"], &["default", BOUND_NAMESPACE]));
    p.run_silent(&["project", "bind", BOUND_NAMESPACE, "--key", KEY_RAW]);

    // The refusal S1 replaces is gone: no FORBIDDEN and no instruction to
    // rebind the workspace to the namespace it is already bound to.
    let suggested = p.run_in_workspace(
        REMOTE,
        &["--json", "suggest-topic-key", "puerto del servicio"],
    );
    assert!(
        suggested.status.success(),
        "the suggestion must not be refused ({:?}): {}",
        suggested.status.code(),
        stderr_of(&suggested)
    );
    assert_eq!(stderr_of(&suggested), "");
    let json = stdout_json(&suggested);
    assert_eq!(json["namespace"], BOUND_NAMESPACE, "{json}");
    assert_eq!(
        json["recorded_by"],
        serde_json::Value::Null,
        "nothing records the key yet: {json}"
    );

    // A write into any other namespace is still refused, with the same code
    // and the same message, and stores nothing.
    let refused = p.run_in_workspace(REMOTE, &["save", "--namespace", "otro", "no debe entrar"]);
    assert_eq!(
        refused.status.code(),
        Some(4),
        "the write guard still applies to save: {}",
        stderr_of(&refused)
    );
    assert_eq!(stdout_of(&refused), "", "a refused write answers nothing");
    assert_eq!(
        stderr_of(&refused),
        format!(
            "error[FORBIDDEN]: forbidden: write: this workspace writes only into namespace \
             '{BOUND_NAMESPACE}'; run `mnexis project bind <namespace> --key {KEY_CANONICAL} \
             --force` to change it\n"
        )
    );
    let after = p.run_in_workspace(REMOTE, &["--json", "list", "--namespace", "otro"]);
    assert_eq!(
        stdout_json(&after)["count"],
        0,
        "the refused write must store nothing"
    );
}

#[test]
fn a_workspace_that_names_no_namespace_keeps_the_profile_default() {
    // The behavior the suggestion had before this rule, for the profile that
    // names no namespace at all.
    let p = Profile::new().with_config(&config(&["default", "otro"], &["default", "otro"]));
    let seed = tempdir_in("target").expect("tempdir under target/");
    let other = save_titled_in(&p, seed.path(), "otro", "otra cosa");
    let bare = tempdir_in("target").expect("tempdir under target/");

    // No binding and no marker: the profile's default namespace answers.
    let asked = p.run_in_dir(bare.path(), &["--json", "suggest-topic-key", "otra cosa"]);
    assert!(asked.status.success(), "{}", stderr_of(&asked));
    assert_eq!(stderr_of(&asked), "");
    let json = stdout_json(&asked);
    assert_eq!(json["topic_key"], "otra-cosa", "{json}");
    assert_eq!(
        json["recorded_by"],
        serde_json::Value::Null,
        "the default namespace holds nothing for that title: {json}"
    );

    // An explicit `--namespace` is still the namespace asked about.
    let explicit = p.run_in_dir(
        bare.path(),
        &[
            "--json",
            "suggest-topic-key",
            "--namespace",
            "otro",
            "otra cosa",
        ],
    );
    assert!(explicit.status.success(), "{}", stderr_of(&explicit));
    let json = stdout_json(&explicit);
    assert_eq!(json["topic_key"], "otra-cosa", "{json}");
    assert_eq!(
        json["recorded_by"], other,
        "the explicit namespace is the one asked about: {json}"
    );

    // The read scope still decides: a namespace the profile may not read is
    // refused, exactly as every other read is. A read never widens itself.
    let denied = p.run_in_dir(
        bare.path(),
        &["suggest-topic-key", "--namespace", "ajeno", "otra cosa"],
    );
    assert_eq!(
        denied.status.code(),
        Some(3),
        "an unreadable namespace must stay NOT_FOUND: {}",
        stderr_of(&denied)
    );
    assert_eq!(stdout_of(&denied), "");
}

#[test]
fn the_suggestion_never_answers_about_a_namespace_the_profile_cannot_read() {
    // A profile whose `default_namespace` is NOT one of its readable
    // namespaces. The suggestion falls back to the scope's default when the
    // workspace names none, and that fallback is a read like any other: the
    // scope must refuse it, exactly as it refuses `list --namespace` for the
    // same namespace.
    let p = Profile::new().with_config(
        "default_namespace = \"secreto\"\nread_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\"]\nallow_global = false\nallow_read_all = false\n\
         existence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
    );
    let bare = tempdir_in("target").expect("tempdir under target/");

    // The same namespace, read through another command, for the reference.
    let listed = p.run_in_dir(bare.path(), &["list", "--namespace", "secreto"]);
    assert_eq!(
        listed.status.code(),
        Some(3),
        "the scope refuses that namespace for every read: {}",
        stderr_of(&listed)
    );

    // The suggestion must refuse it too: it must not answer about a
    // namespace the profile may not read, not even to say the key is free.
    let denied = p.run_in_dir(bare.path(), &["suggest-topic-key", "puerto del servidor"]);
    assert_eq!(
        denied.status.code(),
        Some(3),
        "the suggestion must not answer about an unreadable namespace: {}",
        stderr_of(&denied)
    );
    assert_eq!(stdout_of(&denied), "", "a refused read answers nothing");
    assert!(
        stderr_of(&denied).contains("not found: namespace:secreto"),
        "the refusal names the namespace and the route: {}",
        stderr_of(&denied)
    );
}
