//! read-granularity — the advisory findings of the independent verification:
//! `config show` must report the scope the process runs with (the same one
//! `mnexis status` reports), and a broken `projects.toml` must not stop the
//! reads: it grants nothing, but it is a broken file, not a broken engine.

use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::tempdir_in;

fn run(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .current_dir(dir)
        .arg("--data-dir")
        .arg(dir.join("profile"))
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("spawn mnexis binary")
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

const CLOSED: &str = "default_namespace = \"default\"\n\
                      read_namespaces = [\"default\"]\n\
                      write_namespaces = [\"default\"]\n\
                      allow_global = false\n\
                      allow_read_all = false\n\
                      existence_disclosure = \"not_found\"\n\
                      allowed_operations = [\"all\"]\n";

fn profile_with(toml: &str) -> tempfile::TempDir {
    let root = tempdir_in("target").expect("tempdir under target/");
    let dir = root.path().join("profile");
    std::fs::create_dir_all(&dir).expect("create the profile dir");
    std::fs::write(dir.join("mnexis.toml"), toml).expect("write the profile");
    root
}

/// The scope `config show` reports must be the scope the process runs with:
/// a bound workspace reads its own namespace, and the two commands must not
/// disagree about it.
#[test]
fn config_show_reports_the_effective_read_scope() {
    let root = profile_with(CLOSED);
    let dir = root.path();
    let init = run(dir, &["init"]);
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));
    std::fs::write(dir.join("profile").join("mnexis.toml"), CLOSED).expect("write");

    // Bind the workspace this directory names to another namespace. The
    // key is the one the engine itself reports for this directory (a
    // directory under `target/` resolves to this repository's git key, so
    // the key is not a path here).
    let probe = run(dir, &["--json", "status"]);
    assert!(
        probe.status.success(),
        "status failed: {}",
        stderr_of(&probe)
    );
    let key = stdout_json(&probe)["workspace"]["key"]
        .as_str()
        .expect("workspace key in the status payload")
        .to_string();
    let bind = run(dir, &["project", "bind", "mnexis", "--key", &key]);
    assert!(bind.status.success(), "bind failed: {}", stderr_of(&bind));

    let shown = run(dir, &["--json", "config", "show"]);
    let status = run(dir, &["--json", "status"]);
    assert!(
        shown.status.success(),
        "config show failed: {}",
        stderr_of(&shown)
    );
    assert!(
        status.status.success(),
        "status failed: {}",
        stderr_of(&status)
    );
    assert_eq!(
        stdout_json(&shown)["scope"]["read_namespaces"],
        stdout_json(&status)["scope"]["read_namespaces"],
        "config show must report the scope the process runs with: {}",
        stdout_of(&shown)
    );
    assert!(
        stdout_json(&shown)["scope"]["read_namespaces"]
            .as_array()
            .is_some_and(|names| names.iter().any(|n| n == "mnexis")),
        "the bound namespace must be in the reported scope: {}",
        stdout_of(&shown)
    );
}

/// A broken `projects.toml` grants nothing and stops nothing: the reads
/// keep working, and the commands that work WITH the registry still fail
/// loudly.
#[test]
fn a_malformed_registry_does_not_break_the_read_commands() {
    let root = profile_with(CLOSED);
    let dir = root.path();
    let init = run(dir, &["init"]);
    assert!(init.status.success(), "init failed: {}", stderr_of(&init));
    std::fs::write(dir.join("profile").join("mnexis.toml"), CLOSED).expect("write");

    // The namespace is explicit on purpose: a namespace-less save inside a
    // repository that no binding names is refused (auto-namespace S5), and
    // this test is about the registry, not about the workspace.
    let saved = run(dir, &["--json", "save", "--namespace", "default", "una memoria"]);
    assert!(saved.status.success(), "save failed: {}", stderr_of(&saved));

    std::fs::write(
        dir.join("profile").join("projects.toml"),
        "[[project]]\nkey = \"k\"\nnamespace = \"n\"\nbogus = \"x\"\n",
    )
    .expect("write the broken registry");

    let listed = run(dir, &["--json", "list"]);
    assert!(
        listed.status.success(),
        "a broken registry must not stop a read: {}",
        stderr_of(&listed)
    );
    assert_eq!(stdout_json(&listed)["count"], 1, "the read still answers");

    // The commands that work with the registry still refuse it.
    let projects = run(dir, &["--json", "project", "list"]);
    assert!(
        !projects.status.success(),
        "the registry commands must still fail loudly: {}",
        stderr_of(&projects)
    );
}
