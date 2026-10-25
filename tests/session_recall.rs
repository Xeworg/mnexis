//! session-recall T2 — the session carries its working context (S2).
//!
//! A session is the unit of work, and the working context of a session is
//! the branch it happened on. The engine records the **anchor** — the
//! branch, the commit and the canonical remote — when the session starts,
//! and a session that did not happen in a repository records nothing at all.
//!
//! The rules this file pins:
//!
//! - a session started inside a repository records the branch, the HEAD
//!   commit and the canonical remote (`git:<normalized>`);
//! - the canonical remote is never the raw URL: a URL can carry credentials;
//! - a repository without a remote records the branch and the commit, and no
//!   remote;
//! - a directory that is not a repository records nothing, and the session is
//!   still a session: no git is not an error;
//! - the anchor survives the store: it is read back unchanged.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use tempfile::{TempDir, tempdir_in};

/// A profile on a fresh database, driven through the real binary.
struct Profile {
    data_dir: PathBuf,
    #[allow(dead_code)]
    guard: TempDir,
}

impl Profile {
    fn new() -> Self {
        let guard = tempdir_in("target").expect("tempdir under target/");
        let profile = Self {
            data_dir: guard.path().to_path_buf(),
            guard,
        };
        let init = profile.run_silent(&["init"]);
        assert!(
            init.status.success(),
            "init bootstrap: {}",
            String::from_utf8_lossy(&init.stderr)
        );
        profile
    }

    fn run_silent(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_mnexis"))
            .arg("--data-dir")
            .arg(&self.data_dir)
            .arg("--json")
            .args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }
}

fn stdout_json(out: &Output) -> serde_json::Value {
    let text = String::from_utf8(out.stdout.clone()).expect("utf-8 stdout");
    serde_json::from_str(text.trim()).unwrap_or_else(|e| {
        panic!("stdout is not JSON ({e}): {text:?}\nstderr: {:?}", {
            String::from_utf8_lossy(&out.stderr)
        })
    })
}

/// Run one git command in `dir` and return its output.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// A repository with one commit on `main`, with or without a remote.
struct Repo {
    #[allow(dead_code)]
    guard: TempDir,
    path: PathBuf,
}

impl Repo {
    fn new(remote: Option<&str>) -> Self {
        let guard = tempdir_in("target").expect("tempdir under target/");
        let path = guard.path().to_path_buf();
        git(&path, &["init", "-q", "-b", "main", "."]);
        git(&path, &["config", "user.email", "test@example.com"]);
        git(&path, &["config", "user.name", "Test"]);
        git(&path, &["config", "commit.gpgsign", "false"]);
        std::fs::write(path.join("README.md"), "# fixture\n").expect("write the fixture");
        git(&path, &["add", "."]);
        git(&path, &["commit", "-q", "-m", "first"]);
        if let Some(remote) = remote {
            git(&path, &["remote", "add", "origin", remote]);
        }
        Self { guard, path }
    }

    fn sha(&self) -> String {
        git(&self.path, &["rev-parse", "HEAD"]).trim().to_string()
    }

    fn dir(&self) -> &str {
        self.path.to_str().expect("utf-8 path")
    }
}

/// A directory that is **not** inside any repository.
///
/// The fixture has to be outside every repository: a directory under this
/// repository's `target/` would resolve to this repository, and the test
/// would pass for the wrong reason. The guard makes that failure loud.
fn outside_any_repository() -> TempDir {
    let dir = tempfile::TempDir::new_in("/tmp").expect("a tempdir outside any repository");
    let probe = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir.path())
        .output()
        .expect("run git");
    assert!(
        !probe.status.success(),
        "the fixture must not be inside a repository"
    );
    dir
}

#[test]
fn a_session_in_a_repository_records_the_branch_the_commit_and_the_canonical_remote() {
    let repo = Repo::new(Some("git@github.com:Xeworg/mnexis.git"));
    let profile = Profile::new();

    let out = profile.run_silent(&["session", "--id", "01JX", "--directory", repo.dir()]);
    assert!(
        out.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(json["id"], "01JX", "{json}");
    assert_eq!(json["branch"], "main", "{json}");
    assert_eq!(json["branch_sha"], repo.sha(), "{json}");
    assert_eq!(
        json["branch_remote"], "git:github.com-xeworg-mnexis",
        "the remote is the canonical key, not the URL: {json}"
    );
}

#[test]
fn the_recorded_remote_never_carries_the_credentials_of_the_url() {
    let repo = Repo::new(Some("https://user:token@github.com/Xeworg/mnexis.git"));
    let profile = Profile::new();

    let out = profile.run_silent(&["session", "--id", "01JX", "--directory", repo.dir()]);
    assert!(
        out.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(
        json["branch_remote"], "git:github.com-xeworg-mnexis",
        "{json}"
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains("token"),
        "the raw URL must never be stored: {text}"
    );
}

#[test]
fn a_repository_without_a_remote_records_the_branch_and_the_commit_and_no_remote() {
    let repo = Repo::new(None);
    let profile = Profile::new();

    let out = profile.run_silent(&["session", "--id", "01JX", "--directory", repo.dir()]);
    assert!(
        out.status.success(),
        "session failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(json["branch"], "main", "{json}");
    assert_eq!(json["branch_sha"], repo.sha(), "{json}");
    assert_eq!(
        json["branch_remote"],
        serde_json::Value::Null,
        "there is no remote to record: {json}"
    );
}

#[test]
fn a_directory_that_is_not_a_repository_records_nothing_and_the_session_is_still_a_session() {
    let dir = outside_any_repository();
    let profile = Profile::new();

    let out = profile.run_silent(&[
        "session",
        "--id",
        "SIN-REPO",
        "--directory",
        dir.path().to_str().expect("utf-8 path"),
    ]);
    assert!(
        out.status.success(),
        "no git is not an error: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let json = stdout_json(&out);
    assert_eq!(json["id"], "SIN-REPO", "{json}");
    assert_eq!(json["branch"], serde_json::Value::Null, "{json}");
    assert_eq!(json["branch_sha"], serde_json::Value::Null, "{json}");
    assert_eq!(json["branch_remote"], serde_json::Value::Null, "{json}");

    // The session exists all the same.
    let sessions = profile.run_silent(&["sessions"]);
    assert!(sessions.status.success(), "sessions failed");
    let listed = stdout_json(&sessions);
    let ids: Vec<&str> = listed["items"]
        .as_array()
        .expect("a list of sessions")
        .iter()
        .map(|session| session["id"].as_str().expect("a session id"))
        .collect();
    assert!(ids.contains(&"SIN-REPO"), "{ids:?}");
}

#[test]
fn the_anchor_survives_the_store() {
    let repo = Repo::new(Some("https://github.com/Xeworg/mnexis.git"));
    let profile = Profile::new();

    profile.run_silent(&["session", "--id", "01JX", "--directory", repo.dir()]);

    // Read back through a different command: the anchor is in the store, not
    // in the answer of the command that wrote it.
    let sessions = profile.run_silent(&["sessions"]);
    assert!(sessions.status.success(), "sessions failed");
    let listed = stdout_json(&sessions);
    let session = listed["items"]
        .as_array()
        .expect("a list of sessions")
        .iter()
        .find(|session| session["id"] == "01JX")
        .expect("the session is listed")
        .clone();
    assert_eq!(session["branch"], "main", "{session}");
    assert_eq!(session["branch_sha"], repo.sha(), "{session}");
    assert_eq!(
        session["branch_remote"], "git:github.com-xeworg-mnexis",
        "{session}"
    );
}
