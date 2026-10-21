//! `mnexis upgrade` — the version check, against a faked network.
//!
//! The check is a `curl` call, so the test puts a fake `curl` on the
//! PATH of the spawned binary: that is the only thing the service asks
//! the outside world for.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::tempdir_in;

struct Profile {
    data_dir: PathBuf,
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
        profile.run(&["init"]).unwrap_status("init bootstrap");
        profile
    }

    /// Run the binary with a PATH that starts with the fake tools.
    fn run_with_path(&self, path: &PathBuf, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--json")
            .args(args)
            .env("PATH", path)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_with_path(&real_path(), args)
    }
}

/// The PATH of the test runner: the fake tools go in front of it, not in
/// place of it (a fake shell script still needs a shell to run).
fn real_path() -> PathBuf {
    PathBuf::from(std::env::var("PATH").unwrap_or_default())
}

/// The PATH with the fake tools in front of the real one.
fn path_with(dir: &PathBuf) -> PathBuf {
    let mut path = std::ffi::OsString::from(dir);
    path.push(":");
    path.push(real_path());
    PathBuf::from(path)
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

fn stdout_json(out: &Output) -> serde_json::Value {
    let text = String::from_utf8(out.stdout.clone()).expect("utf-8 stdout");
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!("stdout is not JSON ({e}): {text:?}");
    })
}

/// A fake `curl` that answers the redirect with the given tag and records
/// how many times it was called.
fn fake_curl(dir: &Path, tag: &str, fail: bool) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("create the fake bin dir");
    let script = format!(
        r#"#!/usr/bin/env bash
printf 'call\n' >>"{log}"
if [[ "$1" == "-fsSI" ]]; then
  if [[ "{fail}" == "1" ]]; then
    printf 'curl: (6) Could not resolve host\n' >&2
    exit 6
  fi
  printf 'HTTP/2 302\nlocation: https://github.com/Xeworg/mnexis/releases/tag/{tag}\ncontent-length: 0\n\n'
  exit 0
fi
printf 'curl: (22) unsupported in the test\n' >&2
exit 22
"#,
        log = dir.join("curl.log").display(),
        tag = tag,
        fail = if fail { "1" } else { "0" },
    );
    let path = bin.join("curl");
    std::fs::write(&path, script).expect("write the fake curl");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&path).expect("stat").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("chmod");
    }
    path_with(&bin)
}

fn calls(dir: &Path) -> usize {
    std::fs::read_to_string(dir.join("curl.log"))
        .map(|raw| raw.lines().filter(|line| !line.trim().is_empty()).count())
        .unwrap_or(0)
}

#[test]
fn the_check_reports_a_newer_release() {
    let scratch = tempdir_in("target").expect("tempdir");
    let dir = scratch.path().to_path_buf();
    let path = fake_curl(&dir, "v0.2.0", false);
    let p = Profile::new();

    let out = p
        .run_with_path(&path, &["upgrade", "--check"])
        .unwrap_status("upgrade --check");
    let json = stdout_json(&out);
    assert_eq!(json["latest_version"], "0.2.0");
    assert_eq!(json["update_available"], true);
    assert!(json["current_version"].is_string());
}

#[test]
fn the_check_is_cached_and_force_asks_again() {
    let scratch = tempdir_in("target").expect("tempdir");
    let dir = scratch.path().to_path_buf();
    let path = fake_curl(&dir, "v0.2.0", false);
    let p = Profile::new();

    p.run_with_path(&path, &["upgrade", "--check"])
        .unwrap_status("the first check");
    assert_eq!(calls(&dir), 1);

    p.run_with_path(&path, &["upgrade", "--check"])
        .unwrap_status("the cached check");
    assert_eq!(calls(&dir), 1, "the cached answer must not ask again");

    p.run_with_path(&path, &["upgrade", "--check", "--force"])
        .unwrap_status("the forced check");
    assert_eq!(calls(&dir), 2, "--force must ask again");
}

#[test]
fn a_network_failure_is_reported_as_no_update() {
    let scratch = tempdir_in("target").expect("tempdir");
    let dir = scratch.path().to_path_buf();
    let path = fake_curl(&dir, "v0.2.0", true);
    let p = Profile::new();

    let out = p
        .run_with_path(&path, &["upgrade", "--check"])
        .unwrap_status("the check without network");
    let json = stdout_json(&out);
    assert_eq!(json["latest_version"], serde_json::Value::Null);
    assert_eq!(json["update_available"], false);
}

#[test]
fn the_check_does_not_need_a_profile() {
    // The command updates the binary, not the memories: it works without
    // an initialised profile.
    let scratch = tempdir_in("target").expect("tempdir");
    let dir = scratch.path().to_path_buf();
    let path = fake_curl(&dir, "v0.2.0", false);
    let guard = tempdir_in("target").expect("tempdir");
    let data_dir = guard.path().to_path_buf();

    let out = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("--data-dir")
        .arg(&data_dir)
        .arg("--json")
        .args(["upgrade", "--check"])
        .env("PATH", &path)
        .stdin(Stdio::null())
        .output()
        .expect("spawn mnexis binary");
    let json = stdout_json(&out.clone().unwrap_status("the check without a profile"));
    assert_eq!(json["update_available"], true);
}
