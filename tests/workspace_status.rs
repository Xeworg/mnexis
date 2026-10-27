//! agent-memory-ergonomics S4 — `mnexis status` answers, in one call,
//! "where may I write, and what is already there?".
//!
//! The payload is the same one the MCP `memory_workspace` tool returns, so
//! these tests drive the real binary like `tests/cli_commands.rs` does: the
//! workspace the process runs in, the profile's allowlists, how many
//! memories the scope can see, and the most recent of them, newest first.

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
        profile
            .run_silent(&["init"])
            .unwrap_status("init bootstrap");
        profile
    }

    /// A profile directory that does not exist yet: `status` (like every
    /// other data command) must refuse it instead of creating one.
    fn without_init() -> Self {
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

    /// Run with stdin not inherited (default: null) so confirmation
    /// prompts never block the test runner.
    fn run_silent(&self, args: &[&str]) -> Output {
        self.spawn(None, true, args)
    }

    /// Run in a chosen working directory: the workspace guard, the project
    /// registry, and `mnexis status` all read the workspace from there.
    fn run_in_dir(&self, dir: &Path, args: &[&str]) -> Output {
        self.spawn(Some(dir), true, args)
    }

    /// Run in a chosen working directory without `--json`, for the
    /// human-readable rendering.
    fn run_text_in_dir(&self, dir: &Path, args: &[&str]) -> Output {
        self.spawn(Some(dir), false, args)
    }

    fn spawn(&self, dir: Option<&Path>, json: bool, args: &[&str]) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_mnexis"));
        if let Some(dir) = dir {
            cmd.current_dir(dir);
        }
        cmd.arg("--data-dir").arg(&self.data_dir);
        if json {
            cmd.arg("--json");
        }
        cmd.args(args)
            .stdin(Stdio::null())
            .output()
            .expect("spawn mnexis binary")
    }
}

/// A profile whose allowlists are exactly `namespaces`, in that order, and
/// whose default namespace is the first one.
fn config(namespaces: &[&str], allow_global: bool, allow_read_all: bool) -> String {
    let list = namespaces
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "default_namespace = \"{first}\"\nread_namespaces = [{list}]\nwrite_namespaces = [{list}]\n\
         allow_global = {allow_global}\nallow_read_all = {allow_read_all}\n\
         existence_disclosure = \"not_found\"\nallowed_operations = [\"all\"]\n",
        first = namespaces[0],
    )
}

/// A working directory for the binary, so `mnexis status` can resolve a
/// workspace. The repository tree, `target/`, and the temp root carry no
/// `.mnexis-namespace` marker, so a bare directory resolves to a key with no
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
        panic!("stdout is not JSON ({e}): {text:?}\nstderr: {:?}", {
            String::from_utf8_lossy(&out.stderr)
        })
    })
}

fn stdout_text(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf-8 stdout")
}

/// The status payload for a working directory.
fn status_json(profile: &Profile, dir: &Path) -> serde_json::Value {
    let out = profile.run_in_dir(dir, &["status"]);
    assert!(
        out.status.success(),
        "status failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    stdout_json(&out)
}

/// Bind the workspace the binary resolves in `dir` to `namespace` and return
/// that workspace key. The key comes from the status payload itself: the test
/// never has to recompute what git says about the directory.
fn bind_workspace(profile: &Profile, dir: &Path, namespace: &str) -> String {
    let key = status_json(profile, dir)["workspace"]["key"]
        .as_str()
        .expect("workspace key in the status payload")
        .to_string();
    profile
        .run_silent(&["project", "bind", namespace, "--key", &key])
        .unwrap_status("project bind");
    key
}

/// Save one memory through a working directory and return its uuid.
fn save(profile: &Profile, dir: &Path, namespace: &str, content: &str) -> String {
    save_with_title(profile, dir, namespace, content, None)
}

/// A save with a title of its own: the status line prints it.
fn save_with_title(
    profile: &Profile,
    dir: &Path,
    namespace: &str,
    content: &str,
    title: Option<&str>,
) -> String {
    let mut args = vec!["save", "--namespace", namespace, content];
    if let Some(title) = title {
        args.push("--title");
        args.push(title);
    }
    let out = profile.run_in_dir(dir, &args);
    assert!(
        out.status.success(),
        "save failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    stdout_json(&out)["uuid"]
        .as_str()
        .expect("uuid in save output")
        .to_string()
}

/// Every file of the profile with its bytes, so a command that writes to the
/// profile (or creates a file in it) cannot pass a read-only assertion.
fn snapshot(data_dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(data_dir)
        .expect("read the data directory")
        .map(|entry| {
            let entry = entry.expect("directory entry");
            let name = entry.file_name().to_string_lossy().to_string();
            let bytes = std::fs::read(entry.path()).expect("read profile file");
            (name, bytes)
        })
        .collect();
    files.sort();
    files
}

// --- the payload ---

#[test]
fn status_reports_where_to_write_and_the_newest_memories_first() {
    let p = Profile::new().with_config(&config(&["default", "snake"], false, true));
    let ws = Workspace::bare();
    let key = bind_workspace(&p, ws.path(), "snake");

    for content in ["la primera", "la segunda", "la tercera", "la cuarta"] {
        save(&p, ws.path(), "snake", content);
    }

    let status = status_json(&p, ws.path());

    assert_eq!(
        status["workspace"]["key"].as_str(),
        Some(key.as_str()),
        "the payload reports the workspace the process resolved"
    );
    assert_eq!(
        status["workspace"]["namespace"].as_str(),
        Some("snake"),
        "the payload reports the namespace the workspace is bound to"
    );
    assert_eq!(
        status["scope"]["default_namespace"].as_str(),
        Some("default")
    );
    assert_eq!(
        status["scope"]["read_namespaces"],
        serde_json::json!(["default", "snake"])
    );
    assert_eq!(
        status["scope"]["write_namespaces"],
        serde_json::json!(["default", "snake"])
    );
    assert_eq!(
        status["scope"]["allow_global"].as_bool(),
        Some(false),
        "the profile's allow_global flag is reported"
    );
    assert_eq!(
        status["scope"]["allow_read_all"].as_bool(),
        Some(true),
        "the profile's allow_read_all flag is reported"
    );
    assert_eq!(
        status["memory_count"].as_i64(),
        Some(4),
        "every memory the scope can see is counted"
    );

    let recent = status["recent"].as_array().expect("recent array");
    // The recent list is priced (#53): identity and preview, never the
    // whole record — for these short contents the preview is the full text.
    assert!(
        recent.iter().all(|m| m.get("content").is_none()),
        "the recent list must not paste the full record: {status}"
    );
    let contents: Vec<&str> = recent
        .iter()
        .map(|m| m["preview"].as_str().expect("recent preview"))
        .collect();
    assert_eq!(
        contents,
        vec!["la cuarta", "la tercera", "la segunda"],
        "the recent list is the newest first, not the oldest first"
    );
    for item in recent {
        assert_eq!(item["namespace"].as_str(), Some("snake"));
        assert!(
            item["uuid"].as_str().is_some_and(|u| !u.is_empty()),
            "each recent memory carries its uuid: {item}"
        );
        assert!(
            item["revision"].as_i64().is_some_and(|r| r >= 1),
            "each recent memory carries its revision: {item}"
        );
        assert!(
            item["updated_at"].as_str().is_some_and(|t| !t.is_empty()),
            "each recent memory carries its updated_at: {item}"
        );
    }
}

#[test]
fn status_prints_one_line_per_fact() {
    let p = Profile::new().with_config(&config(&["default", "snake"], false, false));
    let ws = Workspace::bare();
    let key = bind_workspace(&p, ws.path(), "snake");
    let uuid = save(&p, ws.path(), "snake", "la primera");

    let out = p.run_text_in_dir(ws.path(), &["status"]);
    assert!(
        out.status.success(),
        "status failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout_text(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some(format!("workspace: {key} -> snake").as_str()),
        "line 1 is the workspace: {text:?}"
    );
    assert_eq!(
        lines.get(1).copied(),
        Some(
            "scope: default=default, read=[\"default\",\"snake\"], \
             write=[\"default\",\"snake\"], global=false, read_all=false"
        ),
        "line 2 is the scope: {text:?}"
    );
    assert_eq!(
        lines.get(2).copied(),
        Some("memories: 1 (1 most recent, previews)"),
        "line 3 is the count, and says the list is priced: {text:?}"
    );
    assert_eq!(
        lines.get(3).copied(),
        Some(format!("  [snake] {uuid} \"\" — la primera").as_str()),
        "line 4 is the most recent memory (title, then preview): {text:?}"
    );
    assert_eq!(
        lines.get(4).copied(),
        Some("full record: mnexis show <uuid>"),
        "line 5 names how to open a record: {text:?}"
    );
    assert_eq!(lines.len(), 5, "no other lines: {text:?}");
}

/// A title or a preview can carry a line break (the store accepts them),
/// and the status line prints one fact per line: the break becomes a
/// space, never a second physical line.
#[test]
fn a_title_with_a_line_break_stays_on_one_line() {
    let p = Profile::new().with_config(&config(&["default", "snake"], false, false));
    let ws = Workspace::bare();
    bind_workspace(&p, ws.path(), "snake");
    save_with_title(
        &p,
        ws.path(),
        "snake",
        // A carriage return in the content too: the preview carries the
        // content's first words, and it goes through the same helper.
        "el contenido\rcon un salto",
        // A carriage return too: on its own it does not end the line, but a
        // terminal reads it as "back to the start of the line".
        Some("titulo\r\ncon un salto"),
    );

    let out = p.run_text_in_dir(ws.path(), &["status"]);
    assert!(
        out.status.success(),
        "status failed ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout_text(&out);
    // The whole answer has a known shape: the workspace line, the scope
    // line, the count, one line per fact, and the way to open a record.
    // Counting the fact lines alone would not prove the claim: a fact whose
    // title wraps onto a second physical line would still leave exactly one
    // line starting with '['. The number of physical lines is the claim.
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "one fact is one physical line, got: {text:?}"
    );
    assert!(
        lines[3].contains("titulo"),
        "the break became a space: {text:?}"
    );
    assert!(
        lines[3].contains("con un salto"),
        "the title's words are all in the same line: {text:?}"
    );
    assert!(
        lines[3].contains("el contenido con un salto"),
        "the preview's break became a space too: {text:?}"
    );
    assert!(
        !text.contains('\r'),
        "no carriage return survives into the line: {text:?}"
    );
}

/// The workspace namespace is what the agent needs to know even when its
/// profile cannot read it: the payload still reports it, and the recent list
/// falls back to the namespace the scope does read.
///
/// A workspace that only a `.mnexis-namespace` marker names is exactly that
/// case: a file in the repository is not a permission (read-granularity S2).
#[test]
fn a_workspace_namespace_that_only_a_marker_names_is_not_readable() {
    let p = Profile::new().with_config(&config(&["default"], false, false));
    // Save before the workspace exists: a bound workspace only accepts its
    // own namespace, and this memory lives in the readable one.
    let unbound = Workspace::bare();
    save(&p, unbound.path(), "default", "memoria visible");

    // A workspace that only a marker names: no binding, so nothing grants
    // reading 'snake'.
    let ws = Workspace::bare();
    std::fs::write(ws.path().join(".mnexis-namespace"), "snake")
        .expect("write the namespace marker");

    let status = status_json(&p, ws.path());
    assert_eq!(
        status["workspace"]["namespace"].as_str(),
        Some("snake"),
        "a write would go into the workspace namespace, so the payload names it"
    );
    assert_eq!(
        status["memory_count"].as_i64(),
        Some(1),
        "the count is what the scope can see, not what the workspace would write"
    );
    let recent = status["recent"].as_array().expect("recent array");
    assert_eq!(
        recent.len(),
        1,
        "the recent list falls back to the readable scope: {status}"
    );
    assert_eq!(recent[0]["namespace"].as_str(), Some("default"));
    assert_eq!(recent[0]["preview"].as_str(), Some("memoria visible"));
}

/// A bound workspace may READ its namespace: the binding is a human
/// command on the local registry, so it grants reading (and only
/// reading). The profile below can WRITE 'snake' but not READ it, so
/// everything visible here comes from the binding.
#[test]
fn a_bound_workspace_namespace_is_readable() {
    let p = Profile::new().with_config(
        "default_namespace = \"default\"\n\
         read_namespaces = [\"default\"]\n\
         write_namespaces = [\"default\", \"snake\"]\n\
         allow_global = false\n\
         allow_read_all = false\n\
         existence_disclosure = \"not_found\"\n\
         allowed_operations = [\"all\"]\n",
    );

    let ws = Workspace::bare();
    bind_workspace(&p, ws.path(), "snake");
    // The write is allowed by the profile; the read is not.
    save(&p, ws.path(), "snake", "memoria del workspace");

    // From the bound workspace, the namespace is readable.
    let out = p.run_in_dir(ws.path(), &["list", "--namespace", "snake", "--full"]);
    assert!(
        out.status.success(),
        "the binding grants reading: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("the list answers JSON");
    let items = json["items"].as_array().expect("items array");
    assert_eq!(
        items.len(),
        1,
        "the memory of the bound namespace is visible: {json}"
    );
    assert_eq!(items[0]["content"].as_str(), Some("memoria del workspace"));

    // From another workspace, the same namespace is not readable: the
    // grant belongs to the workspace, not to the process. The other
    // workspace is a git repository with no remote, so its key is its own
    // path and not this repository's key.
    let other = Workspace::bare();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(other.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("run git");
        assert!(status.success(), "git {args:?} failed");
    };
    git(&["init", "-q", "."]);
    let denied = p.run_in_dir(other.path(), &["list", "--namespace", "snake", "--full"]);
    assert_eq!(
        denied.status.code(),
        Some(3),
        "without the binding the namespace stays unreadable: {:?}",
        String::from_utf8_lossy(&denied.stderr)
    );
    assert!(
        String::from_utf8_lossy(&denied.stderr).contains("allow-read-namespace snake"),
        "the refusal names the way to grant it: {:?}",
        String::from_utf8_lossy(&denied.stderr)
    );
}

// --- the profile contract ---

#[test]
fn status_requires_an_initialised_profile() {
    let p = Profile::without_init();
    let out = p.run_silent(&["status"]);

    assert_eq!(
        out.status.code(),
        Some(3),
        "a missing profile is NOT_FOUND: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("NOT_FOUND"), "stderr: {stderr}");
    assert!(
        stderr.contains("mnexis init"),
        "the error says how to create the profile: {stderr}"
    );
    assert!(
        !p.data_dir.join("mnexis.db").exists(),
        "status must not create a profile"
    );
}

#[test]
fn status_never_writes_to_the_profile() {
    let p = Profile::new().with_config(&config(&["default", "snake"], false, true));
    let ws = Workspace::bare();
    bind_workspace(&p, ws.path(), "snake");
    save(&p, ws.path(), "snake", "no se toca");

    let before = snapshot(&p.data_dir);
    status_json(&p, ws.path());
    let after = snapshot(&p.data_dir);

    assert_eq!(
        before, after,
        "status only reads: no file in the profile may change or appear"
    );
}

// --- the candidate namespace (auto-namespace S1, S2, S5) ---

impl Workspace {
    /// A git repository with an `origin` remote: the key is
    /// `git:<normalized remote>`, the identity a binding is recorded under,
    /// and the remote is what a candidate namespace is derived from.
    fn with_remote(url: &str) -> Self {
        let workspace = Self::bare();
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(workspace.path())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .expect("run git");
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "."]);
        git(&["remote", "add", "origin", url]);
        workspace
    }
}

#[test]
fn status_offers_the_candidate_namespace_of_an_unbound_repository() {
    let p = Profile::new();
    let ws = Workspace::with_remote("https://github.com/Xeworg/Nodexar.git");

    let before = snapshot(&p.data_dir);
    let status = status_json(&p, ws.path());
    let after = snapshot(&p.data_dir);

    assert_eq!(
        status["workspace"]["key"].as_str(),
        Some("git:github.com-xeworg-nodexar"),
        "the key is the git remote: {status}"
    );
    assert!(
        status["workspace"]["namespace"].is_null(),
        "nothing is bound yet: {status}"
    );
    assert_eq!(
        status["workspace"]["candidate_namespace"].as_str(),
        Some("nodexar"),
        "the last segment of the remote is the name to offer: {status}"
    );
    assert!(
        status["workspace"]["candidate_refusal"].is_null(),
        "a usable name is not a refusal: {status}"
    );
    assert_eq!(
        before, after,
        "offering a candidate is not binding it: the profile stays byte-identical"
    );
}

#[test]
fn status_offers_the_folders_own_name_outside_a_repository() {
    let p = Profile::new();
    // A new project before `git init`: no repository, and still a name a
    // human can be asked about — the directory's own.
    let folder = tempfile::Builder::new()
        .prefix("mnexis-nuevo-")
        .tempdir_in("/tmp")
        .expect("a directory outside any repository");
    let name = folder
        .path()
        .file_name()
        .expect("the folder has a name")
        .to_string_lossy()
        .to_lowercase();

    let status = status_json(&p, folder.path());

    assert_eq!(
        status["workspace"]["candidate_namespace"].as_str(),
        Some(name.as_str()),
        "the folder names the workspace it would become: {status}"
    );
    assert!(
        status["workspace"]["namespace"].is_null(),
        "offering the name is not binding it: {status}"
    );
}

#[test]
fn a_save_without_namespace_in_a_bound_repository_lands_in_its_namespace() {
    let p = Profile::new();
    let ws = Workspace::with_remote("https://github.com/Xeworg/Nodexar.git");
    bind_workspace(&p, ws.path(), "nodexar");
    p.run_silent(&["config", "allow-namespace", "nodexar"])
        .unwrap_status("config allow-namespace");

    let out = p.run_in_dir(ws.path(), &["save", "la memoria del repositorio"]);
    assert!(
        out.status.success(),
        "a bound workspace owns the destination ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let own = p.run_in_dir(ws.path(), &["list", "--namespace", "nodexar", "--full"]);
    let own = stdout_json(&own);
    assert_eq!(
        own["items"].as_array().map(Vec::len),
        Some(1),
        "the record is in the workspace's namespace: {own}"
    );
    let default = p.run_in_dir(ws.path(), &["list", "--namespace", "default", "--full"]);
    let default = stdout_json(&default);
    assert_eq!(
        default["items"].as_array().map(Vec::len),
        Some(0),
        "the shared drawer stays untouched: {default}"
    );
}

#[test]
fn a_save_without_namespace_in_an_unbound_repository_is_refused_with_the_route() {
    let p = Profile::new();
    let ws = Workspace::with_remote("https://github.com/Xeworg/Nodexar.git");

    let out = p.run_in_dir(ws.path(), &["save", "una memoria sin namespace"]);
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert_eq!(
        out.status.code(),
        Some(4),
        "a namespace-less save does not pool in `default`: {stderr}"
    );
    assert!(
        stderr.contains("project bind nodexar --key git:github.com-xeworg-nodexar"),
        "the refusal names the exact binding call: {stderr}"
    );
    assert!(
        stderr.contains("config allow-namespace nodexar"),
        "the refusal names the grant that must follow: {stderr}"
    );

    let status = status_json(&p, ws.path());
    assert_eq!(
        status["memory_count"].as_i64(),
        Some(0),
        "the refused save left no record anywhere: {status}"
    );
}

#[test]
fn a_save_without_namespace_outside_a_repository_keeps_the_profiles_default() {
    let p = Profile::new();
    let outside = tempfile::Builder::new()
        .prefix("mnexis-no-repo-")
        .tempdir_in("/tmp")
        .expect("a directory outside any repository");

    let out = p.run_in_dir(outside.path(), &["save", "fuera de todo repositorio"]);
    assert!(
        out.status.success(),
        "outside a repository the allowlist keeps deciding ({}): {}",
        out.status,
        String::from_utf8_lossy(&out.stderr)
    );

    let listed = p.run_in_dir(
        outside.path(),
        &["list", "--namespace", "default", "--full"],
    );
    let listed = stdout_json(&listed);
    assert_eq!(
        listed["items"].as_array().map(Vec::len),
        Some(1),
        "the record lands in the profile's default namespace: {listed}"
    );
}
