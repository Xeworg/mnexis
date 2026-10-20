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
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

const KEY: &str = "git:github.com/Xeworg/mnexis";

#[test]
fn bind_records_the_binding_in_the_profile_registry() {
    let p = Profile::new();
    let out = p.run_silent(&[
        "project",
        "bind",
        "mnexis",
        "--key",
        KEY,
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
        registry.contains(KEY),
        "the registry must record the workspace key: {registry}"
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
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY]);
    let out = p.run_silent(&["project", "list"]);
    assert!(out.status.success(), "list failed: {}", stderr_of(&out));
    let text = stdout_of(&out);
    assert!(text.contains(KEY), "list must show the key: {text}");
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
        let out = p.run_silent(&["project", "bind", "mnexis", "--key", KEY]);
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
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY]);
    let out = p.run_silent(&["project", "bind", "other", "--key", KEY]);
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
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY]);
    let out = p.run_silent(&["project", "bind", "other", "--key", KEY, "--force"]);
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
    p.run_silent(&["project", "bind", "mnexis", "--key", KEY]);
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

    let empty_ns = p.run_silent(&["project", "bind", "", "--key", KEY]);
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
