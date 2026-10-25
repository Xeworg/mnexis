//! project-bindings T2/T3 — `mnexis config allow-namespace` (S3) — and
//! read-granularity T1 — `mnexis config allow-read-namespace` (S1).
//!
//! Authorizing a namespace is a permission change, so every test pins the
//! observable effect: the profile toml allowlists, the exit code, and the
//! real consequence (a write into the newly allowed namespace). The
//! read-only variant must grow only the read allowlist, so its tests pin
//! the write allowlist staying exactly as it was.

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

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let text = stdout_of(out);
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!(
            "stdout is not JSON ({e}): {text:?}\nstderr: {:?}",
            stderr_of(out)
        )
    })
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

#[test]
fn allow_read_namespace_adds_the_namespace_to_the_read_allowlist_only() {
    let p = Profile::new();
    let out = p.run_silent(&["--json", "config", "allow-read-namespace", "mnexis"]);
    assert!(
        out.status.success(),
        "allow-read-namespace failed ({}): {}",
        out.status,
        stderr_of(&out)
    );

    let json = stdout_json(&out);
    assert_eq!(json["namespace"], "mnexis", "envelope: {json}");
    assert_eq!(json["changed"], true, "envelope: {json}");
    assert_eq!(
        json["read_only"], true,
        "the contract must say the write list was not touched: {json}"
    );
    assert_eq!(
        json["read_namespaces"],
        serde_json::json!(["default", "mnexis"]),
        "the read allowlist must gain the namespace: {json}"
    );
    assert_eq!(
        json["write_namespaces"],
        serde_json::json!(["default"]),
        "the write allowlist must not gain the namespace: {json}"
    );
    assert!(
        json["config_path"].is_string(),
        "envelope must name the profile: {json}"
    );

    let toml = p.toml_text();
    assert!(
        toml.contains("read_namespaces = [\"default\", \"mnexis\"]"),
        "read allowlist must gain the namespace: {toml}"
    );
    assert!(
        toml.contains("write_namespaces = [\"default\"]"),
        "write allowlist must stay untouched: {toml}"
    );
}

#[test]
fn allow_read_namespace_grants_reading_but_not_writing() {
    // A closed scope: no `allow_read_all`, so the allowlist decides, and
    // `forbid` so a refusal is observable as a permission error instead of
    // the default `not_found` disclosure.
    let p = Profile::new().with_config(
        "default_namespace = \"default\"\nread_namespaces = [\"default\"]\nwrite_namespaces = [\"default\"]\nallow_global = false\nallow_read_all = false\nexistence_disclosure = \"forbid\"\nallowed_operations = [\"all\"]\n",
    );
    let before = p.run_silent(&["list", "--namespace", "mnexis"]);
    assert_eq!(
        before.status.code(),
        Some(4),
        "reading must be refused before the permission: {}",
        stderr_of(&before)
    );

    let out = p.run_silent(&["config", "allow-read-namespace", "mnexis"]);
    assert!(
        out.status.success(),
        "allow-read-namespace failed: {}",
        stderr_of(&out)
    );

    let read = p.run_silent(&["list", "--namespace", "mnexis"]);
    assert!(
        read.status.success(),
        "reading must be allowed now: {}",
        stderr_of(&read)
    );
    let write = p.run_silent(&["save", "--namespace", "mnexis", "first project memory"]);
    assert_eq!(
        write.status.code(),
        Some(4),
        "writing must stay refused: {}",
        stderr_of(&write)
    );
}

#[test]
fn allow_read_namespace_is_idempotent() {
    let p = Profile::new();
    for _ in 0..2 {
        let out = p.run_silent(&["config", "allow-read-namespace", "mnexis"]);
        assert!(
            out.status.success(),
            "repeat allow-read-namespace failed: {}",
            stderr_of(&out)
        );
    }
    let again = p.run_silent(&["--json", "config", "allow-read-namespace", "mnexis"]);
    assert!(
        again.status.success(),
        "repeat allow-read-namespace failed: {}",
        stderr_of(&again)
    );
    let json = stdout_json(&again);
    assert_eq!(
        json["changed"], false,
        "a repeated request changes nothing: {json}"
    );
    assert_eq!(
        json["read_namespaces"],
        serde_json::json!(["default", "mnexis"]),
        "the entry must not be duplicated: {json}"
    );
    let toml = p.toml_text();
    assert_eq!(
        toml.matches("\"mnexis\"").count(),
        1,
        "the namespace must appear exactly once in the read allowlist: {toml}"
    );
}

#[test]
fn allow_read_namespace_rejects_an_empty_namespace() {
    let p = Profile::new();
    let out = p.run_silent(&["config", "allow-read-namespace", ""]);
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
fn allow_read_namespace_rejects_a_line_break_in_the_namespace() {
    let p = Profile::new();
    let out = p.run_silent(&["config", "allow-read-namespace", "bad\nname"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "a line break is INVALID_ARGUMENT: {}",
        stderr_of(&out)
    );
    assert!(
        stderr_of(&out).contains("namespace must not contain line breaks"),
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
fn allow_read_namespace_requires_an_initialised_profile() {
    let p = Profile::uninitialised();
    let out = p.run_silent(&["config", "allow-read-namespace", "mnexis"]);
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

#[test]
fn allow_read_namespace_leaves_an_existing_write_allowlist_untouched() {
    let p = Profile::new().with_config(
        "default_namespace = \"default\"\nread_namespaces = [\"default\"]\nwrite_namespaces = [\"default\", \"other\"]\nallow_global = false\nallow_read_all = false\nexistence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\nmax_tags = 7\n",
    );
    let out = p.run_silent(&["--json", "config", "allow-read-namespace", "mnexis"]);
    assert!(
        out.status.success(),
        "allow-read-namespace failed: {}",
        stderr_of(&out)
    );
    let json = stdout_json(&out);
    assert_eq!(
        json["write_namespaces"],
        serde_json::json!(["default", "other"]),
        "the write allowlist must stay exactly as it was: {json}"
    );
    let toml = p.toml_text();
    assert!(
        toml.contains("write_namespaces = [\"default\", \"other\"]"),
        "the write allowlist must survive the rewrite: {toml}"
    );
    assert!(
        toml.contains("max_tags = 7"),
        "explicit limits must survive the rewrite: {toml}"
    );
}

#[test]
fn allow_read_namespace_text_says_reading_only() {
    let p = Profile::new();
    let first = p.run_silent(&["config", "allow-read-namespace", "mnexis"]);
    assert!(first.status.success(), "failed: {}", stderr_of(&first));
    assert!(
        stdout_of(&first).contains("reading only"),
        "the answer must say the permission is read-only: {:?}",
        stdout_of(&first)
    );
    let again = p.run_silent(&["config", "allow-read-namespace", "mnexis"]);
    assert!(again.status.success(), "failed: {}", stderr_of(&again));
    assert!(
        stdout_of(&again).contains("reading only"),
        "the repeated answer must say the permission is read-only: {:?}",
        stdout_of(&again)
    );
}

// --- the reserved `global` namespace has its own permission (S3) ------------

#[test]
fn allow_read_namespace_refuses_the_reserved_global_namespace() {
    let p = Profile::new();
    let out = p.run_silent(&["config", "allow-read-namespace", "global"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "the reserved namespace must not enter the allowlists: {:?}",
        stderr_of(&out)
    );
    let stderr = stderr_of(&out);
    assert!(
        stderr.contains("allow-global"),
        "the refusal names the command that governs it: {stderr}"
    );
    let toml = p.toml_text();
    assert!(
        !toml.contains("\"global\""),
        "the allowlists must stay free of it: {toml}"
    );
}

#[test]
fn allow_namespace_refuses_the_reserved_global_namespace() {
    let p = Profile::new();
    let out = p.run_silent(&["config", "allow-namespace", "global"]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "the reserved namespace must not enter the allowlists: {:?}",
        stderr_of(&out)
    );
    let toml = p.toml_text();
    assert!(
        !toml.contains("\"global\""),
        "the allowlists must stay free of it: {toml}"
    );
}

#[test]
fn the_reserved_global_namespace_stays_out_of_a_scoped_read() {
    let open = "default_namespace = \"default\"\n\
         read_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\", \"global\"]\n\
         allow_global = true\n\
         allow_read_all = false\n\
         existence_disclosure = \"not_found\"\n\
         allowed_operations = [\"all\"]\n";
    let closed = open.replace("allow_global = true", "allow_global = false");

    let p = Profile::new().with_config(open);
    // A memory in the reserved namespace, written while it was permitted.
    let seeded = p.run_silent(&["save", "--namespace", "global", "memoria reservada"]);
    assert!(
        seeded.status.success(),
        "the seed must land: {:?}",
        stderr_of(&seeded)
    );

    // Now the reserved namespace is closed again.
    let p = p.with_config(&closed);
    let grant = p.run_silent(&["config", "allow-read-namespace", "global"]);
    assert_eq!(
        grant.status.code(),
        Some(2),
        "the reserved namespace cannot be granted: {:?}",
        stderr_of(&grant)
    );

    // A read without a namespace must not carry the reserved namespace in.
    let listed = p.run_silent(&["--json", "list"]);
    assert!(
        listed.status.success(),
        "the list must succeed: {:?}",
        stderr_of(&listed)
    );
    assert_eq!(
        stdout_json(&listed)["count"],
        0,
        "the reserved namespace stays out of the scope: {:?}",
        stdout_of(&listed)
    );

    // And an explicit read of it is refused, naming the command that
    // governs it.
    let direct = p.run_silent(&["list", "--namespace", "global"]);
    assert_eq!(
        direct.status.code(),
        Some(3),
        "an explicit global read must stay NOT_FOUND: {:?}",
        stderr_of(&direct)
    );
    assert!(
        stderr_of(&direct).contains("allow-global"),
        "the refusal names the command that opens it: {:?}",
        stderr_of(&direct)
    );
}
