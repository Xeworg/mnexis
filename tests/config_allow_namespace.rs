//! project-bindings T2/T3 — `mnexis config allow-namespace` (S3).
//!
//! Authorizing a namespace is a permission change, so every test pins the
//! observable effect: the profile toml allowlists, the exit code, and the
//! real consequence (a write into the newly allowed namespace).

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

    /// A data directory with no profile at all (no init).
    fn uninitialised() -> Self {
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

    fn run_silent(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }

    fn toml_text(&self) -> String {
        std::fs::read_to_string(self.data_dir.join("mnexis.toml")).expect("read mnexis.toml")
    }
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn allow_namespace_adds_the_namespace_to_read_and_write() {
    let p = Profile::new();
    let out = p.run_silent(&["config", "allow-namespace", "mnexis"]);
    assert!(
        out.status.success(),
        "allow-namespace failed ({}): {}",
        out.status,
        stderr_of(&out)
    );
    let toml = p.toml_text();
    assert!(
        toml.contains("read_namespaces = [\"default\", \"mnexis\"]"),
        "read allowlist must gain the namespace: {toml}"
    );
    assert!(
        toml.contains("write_namespaces = [\"default\", \"mnexis\"]"),
        "write allowlist must gain the namespace: {toml}"
    );

    // The observable effect: writing into the new namespace now works.
    let save = p.run_silent(&["save", "--namespace", "mnexis", "first project memory"]);
    assert!(
        save.status.success(),
        "save into the allowed namespace failed: {}",
        stderr_of(&save)
    );
}

#[test]
fn allow_namespace_is_idempotent() {
    let p = Profile::new();
    for _ in 0..3 {
        let out = p.run_silent(&["config", "allow-namespace", "mnexis"]);
        assert!(
            out.status.success(),
            "repeat allow-namespace failed: {}",
            stderr_of(&out)
        );
    }
    let toml = p.toml_text();
    assert_eq!(
        toml.matches("\"mnexis\"").count(),
        2,
        "the namespace must appear exactly once per allowlist: {toml}"
    );
}

#[test]
fn allow_namespace_rejects_an_empty_namespace() {
    let p = Profile::new();
    let out = p.run_silent(&["config", "allow-namespace", ""]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an empty namespace is INVALID_ARGUMENT: {}",
        stderr_of(&out)
    );
    assert!(
        stderr_of(&out).contains("namespace must not be empty"),
        "stderr must name the problem: {}",
        stderr_of(&out)
    );
    let toml = p.toml_text();
    assert!(
        toml.contains("read_namespaces = [\"default\"]"),
        "a rejected request must not change the allowlist: {toml}"
    );
}

#[test]
fn allow_namespace_preserves_the_other_settings() {
    let p = Profile::new().with_config(
        "default_namespace = \"default\"\nread_namespaces = [\"default\"]\nwrite_namespaces = [\"default\"]\nallow_global = true\nexistence_disclosure = \"forbid\"\nallowed_operations = [\"all\"]\nmax_tags = 7\n",
    );
    let out = p.run_silent(&["config", "allow-namespace", "mnexis"]);
    assert!(
        out.status.success(),
        "allow-namespace failed: {}",
        stderr_of(&out)
    );
    let toml = p.toml_text();
    assert!(
        toml.contains("max_tags = 7"),
        "explicit limits must survive the rewrite: {toml}"
    );
    assert!(
        toml.contains("existence_disclosure = \"forbid\""),
        "the disclosure mode must survive the rewrite: {toml}"
    );
}

#[test]
fn allow_namespace_requires_an_initialised_profile() {
    let p = Profile::uninitialised();
    let out = p.run_silent(&["config", "allow-namespace", "mnexis"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "a missing profile is NOT_FOUND: {}",
        stderr_of(&out)
    );
    assert!(
        stderr_of(&out).contains("run `mnexis init` first"),
        "stderr must point at init: {}",
        stderr_of(&out)
    );
    assert!(
        !p.data_dir.join("mnexis.toml").exists(),
        "a refused request must not create a profile file"
    );
}
