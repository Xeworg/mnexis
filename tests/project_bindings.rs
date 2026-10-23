//! project-bindings T4/T5 — the project binding registry (S2, S4).
//!
//! The registry lives next to the profile (`projects.toml`) and maps a
//! workspace identity (key) to the namespace an agent writes into. The
//! stability guarantee is the point of these tests: a key must never
//! silently change its namespace, because that would split the memories.

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
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(root)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "."]);
        git(&["remote", "add", "origin", remote]);
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .current_dir(root)
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
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
