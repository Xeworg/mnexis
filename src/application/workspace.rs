//! Workspace → namespace resolution (project-isolation S5, S9).
//!
//! The CLI refuses to write outside the namespace of the workspace it runs
//! in. Resolving that namespace needs two inputs: the **workspace key**, an
//! identity for the directory the process runs in, and the namespace bound
//! to it in the project registry (`projects.toml`).
//!
//! The key must be byte-identical to the one the agent extension computes
//! (`extensions/mnexis-memory/lib/workspace.js`), otherwise the extension
//! and the CLI disagree about which project a directory is and the guard
//! refuses writes the agent believes are allowed. The rules, in the order
//! the extension applies them:
//!
//! 1. `git:<normalized remote>` when the directory is in a repository with
//!    a remote,
//! 2. `path:<repository root>` when it is in a repository without one,
//! 3. `path:<directory>` otherwise.
//!
//! The namespace resolves in a second, separate order:
//!
//! 1. the binding recorded for the key,
//! 2. the nearest `.mnexis-namespace` marker at or above the directory,
//! 3. nothing.
//!
//! "Nothing" is not an error: it means the process runs outside any known
//! project, so the profile allowlist keeps deciding — the behaviour before
//! this rule existed.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::projects::{ProjectRegistry, registry_path};
use crate::domain::error::MnexisError;

/// The marker file a workspace can drop to name its namespace without a
/// binding.
pub const MARKER_FILE: &str = ".mnexis-namespace";

/// Normalize a git remote URL into the canonical key suffix.
///
/// Drop a `git+` prefix, drop the protocol, drop the `user@` part of an
/// scp-style remote, drop a trailing `.git`, lowercase everything, replace
/// `/` and `:` runs with `-`, collapse repeated dashes, and trim the dashes
/// at both ends. The host's dots survive:
///
/// ```text
/// https://github.com/Xeworg/AegisPI.git  ->  github.com-xeworg-aegispi
/// git@github.com:Xeworg/AegisPI.git      ->  github.com-xeworg-aegispi
/// ```
///
/// `None` means the remote has no usable path left (empty or only
/// separators); the caller falls back to the next key rule.
pub fn normalize_remote_url(remote: &str) -> Option<String> {
    let mut url = remote.trim();
    if url.is_empty() {
        return None;
    }
    if let Some(rest) = strip_prefix_ci(url, "git+") {
        url = rest;
    }
    if let Some(rest) = after_protocol(url) {
        url = rest;
    }
    if let Some(rest) = after_user_prefix(url) {
        url = rest;
    }
    if let Some(rest) = strip_suffix_ci(url, ".git") {
        url = rest;
    }

    let mut out = String::with_capacity(url.len());
    for c in url.to_lowercase().chars() {
        if matches!(c, '/' | ':' | '-') {
            // A run of separators becomes one dash, and no dash opens the key.
            if out.is_empty() || out.ends_with('-') {
                continue;
            }
            out.push('-');
        } else {
            out.push(c);
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { None } else { Some(out) }
}

/// `s` without a leading `prefix`, ASCII-case-insensitively.
fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let p = prefix.as_bytes();
    if bytes.len() >= p.len() && bytes[..p.len()].eq_ignore_ascii_case(p) {
        Some(&s[p.len()..])
    } else {
        None
    }
}

/// `s` without a trailing `suffix`, ASCII-case-insensitively.
fn strip_suffix_ci<'a>(s: &'a str, suffix: &str) -> Option<&'a str> {
    let bytes = s.as_bytes();
    let suf = suffix.as_bytes();
    if bytes.len() >= suf.len() && bytes[bytes.len() - suf.len()..].eq_ignore_ascii_case(suf) {
        Some(&s[..bytes.len() - suf.len()])
    } else {
        None
    }
}

/// `url` without a `scheme://` prefix, when the prefix is a real scheme.
fn after_protocol(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return None;
    }
    Some(rest)
}

/// `url` without a leading `user@` part (scp-style remotes).
fn after_user_prefix(url: &str) -> Option<&str> {
    let at = url.find('@')?;
    if at == 0 || url[..at].contains('/') {
        return None;
    }
    Some(&url[at + 1..])
}

/// The workspace key for a directory, from what git reports about it.
///
/// See the module docs for the three rules. A remote that cannot be used
/// falls through to the path rules instead of failing the command.
pub fn workspace_key(remote: Option<&str>, repo_root: Option<&Path>, cwd: &Path) -> String {
    if let Some(remote) = remote
        && let Some(normalized) = normalize_remote_url(remote)
    {
        return format!("git:{normalized}");
    }
    match repo_root {
        Some(root) => format!("path:{}", root.display()),
        None => format!("path:{}", cwd.display()),
    }
}

/// The workspace key of `cwd`, probed from git exactly like the extension:
/// `git rev-parse --show-toplevel` for the root and
/// `git config --get remote.origin.url` for the remote of that root.
///
/// A missing or failing `git` is not an error: the key falls back to
/// `path:<cwd>`, and the marker (if any) still decides the namespace.
fn current_key(cwd: &Path) -> String {
    let repo_root = git_output(cwd, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    let remote = git_output(
        repo_root.as_deref().unwrap_or(cwd),
        &["config", "--get", "remote.origin.url"],
    );
    workspace_key(remote.as_deref(), repo_root.as_deref(), cwd)
}

/// One git query in `dir`. Anything but a clean, non-empty answer is `None`:
/// a workspace without git is a normal workspace, not a failure.
fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() { None } else { Some(text) }
}

/// The namespace named by the nearest `.mnexis-namespace` marker at or
/// above `start`.
///
/// The marker holds one namespace name, trimmed. A marker with no name
/// names nothing and the walk continues upwards, so an accidentally empty
/// file cannot silently unbind a nested project.
pub fn find_marker_namespace(start: &Path) -> Option<String> {
    let mut current = Some(start);
    while let Some(dir) = current {
        if let Ok(contents) = std::fs::read_to_string(dir.join(MARKER_FILE)) {
            let name = contents.trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
        current = dir.parent();
    }
    None
}

/// Which namespace a workspace works in: the binding for `key`, else the
/// marker, else none.
pub fn resolve_namespace(
    registry: &ProjectRegistry,
    key: &str,
    marker: Option<String>,
) -> Option<String> {
    match registry.find(key) {
        Some(binding) if !binding.namespace.trim().is_empty() => Some(binding.namespace.clone()),
        _ => marker,
    }
}

/// The workspace the process runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    /// The workspace key: `git:<normalized remote>` or `path:<directory>`.
    pub key: String,
    /// The namespace the workspace writes into; `None` when the workspace
    /// resolves to none and the profile allowlist keeps deciding.
    pub namespace: Option<String>,
}

/// Resolve the workspace of the process from its working directory and the
/// bindings recorded in `data_dir`.
///
/// `None` means the working directory could not be read, so the process has
/// no workspace at all. A malformed `projects.toml` is an error: a registry
/// nobody can read must not silently reopen every namespace.
pub fn current_workspace(data_dir: &Path) -> Result<Option<WorkspaceInfo>, MnexisError> {
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(None);
    };
    let registry = ProjectRegistry::load(&registry_path(data_dir))?;
    let key = current_key(&cwd);
    let namespace = resolve_namespace(&registry, &key, find_marker_namespace(&cwd));
    Ok(Some(WorkspaceInfo { key, namespace }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir_in;

    fn workspace_dir() -> tempfile::TempDir {
        tempdir_in("target").expect("tempdir under target/")
    }

    // --- normalization: the vectors the extension's tests pin ---

    #[test]
    fn normalization_strips_the_protocol_the_git_suffix_and_lowercases() {
        assert_eq!(
            normalize_remote_url("https://github.com/Xeworg/AegisPI.git").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_handles_scp_style_ssh_remotes() {
        assert_eq!(
            normalize_remote_url("git@github.com:Xeworg/AegisPI.git").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_handles_the_ssh_protocol() {
        assert_eq!(
            normalize_remote_url("ssh://git@github.com/Xeworg/AegisPI.git").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_keeps_the_host_dots_and_replaces_slashes_and_colons() {
        assert_eq!(
            normalize_remote_url("https://gitlab.com/group/sub/proj").as_deref(),
            Some("gitlab.com-group-sub-proj")
        );
    }

    #[test]
    fn normalization_collapses_repeated_separators() {
        assert_eq!(
            normalize_remote_url("https://github.com//Xeworg//AegisPI").as_deref(),
            Some("github.com-xeworg-aegispi")
        );
    }

    #[test]
    fn normalization_rejects_a_remote_without_a_usable_path() {
        // The extension throws on exactly these inputs, so the CLI has to
        // answer `None` for them: a key the two sides disagree on is worse
        // than no key at all.
        assert_eq!(normalize_remote_url(""), None);
        assert_eq!(normalize_remote_url("   "), None);
        assert_eq!(normalize_remote_url("https://"), None);
        assert_eq!(normalize_remote_url("/"), None);
        assert_eq!(normalize_remote_url("::::"), None);
    }

    // --- the key: the same three cases, in the same order, as the extension ---

    #[test]
    fn the_key_prefers_the_git_remote() {
        assert_eq!(
            workspace_key(
                Some("https://github.com/Xeworg/AegisPI.git"),
                Some(Path::new("/w")),
                Path::new("/w/sub")
            ),
            "git:github.com-xeworg-aegispi"
        );
    }

    #[test]
    fn the_key_falls_back_to_the_repo_root_then_to_the_directory() {
        assert_eq!(
            workspace_key(None, Some(Path::new("/w")), Path::new("/w/sub")),
            "path:/w"
        );
        assert_eq!(
            workspace_key(None, None, Path::new("/tmp/x")),
            "path:/tmp/x"
        );
    }

    // --- the marker ---

    #[test]
    fn the_marker_is_found_by_walking_up_and_is_trimmed() {
        let root = workspace_dir();
        std::fs::write(root.path().join(MARKER_FILE), "  marcado \n").unwrap();
        let nested = root.path().join("a/b");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_marker_namespace(&nested).as_deref(), Some("marcado"));
    }

    #[test]
    fn the_nearest_marker_wins() {
        let root = workspace_dir();
        std::fs::write(root.path().join(MARKER_FILE), "outer\n").unwrap();
        let inner = root.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join(MARKER_FILE), "inner-ns\n").unwrap();
        assert_eq!(find_marker_namespace(&inner).as_deref(), Some("inner-ns"));
    }

    #[test]
    fn an_empty_marker_names_nothing_and_walking_continues() {
        let root = workspace_dir();
        std::fs::write(root.path().join(MARKER_FILE), "outer\n").unwrap();
        let inner = root.path().join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join(MARKER_FILE), "   \n").unwrap();
        assert_eq!(find_marker_namespace(&inner).as_deref(), Some("outer"));
    }

    #[test]
    fn a_directory_without_a_marker_resolves_to_nothing() {
        // Neither the repository tree nor `target/` carries a marker today;
        // if one ever appears above this tempdir the test fails loudly
        // instead of passing for the wrong reason.
        let root = workspace_dir();
        assert_eq!(find_marker_namespace(root.path()), None);
    }

    // --- the resolution order ---

    #[test]
    fn the_binding_wins_over_the_marker() {
        let mut registry = ProjectRegistry::default();
        registry
            .bind("git:github.com-xeworg-mnexis", "bound", None, false)
            .unwrap();
        assert_eq!(
            resolve_namespace(
                &registry,
                "git:github.com-xeworg-mnexis",
                Some("marker".to_string())
            )
            .as_deref(),
            Some("bound")
        );
    }

    #[test]
    fn the_marker_is_used_when_the_key_is_not_bound() {
        let registry = ProjectRegistry::default();
        assert_eq!(
            resolve_namespace(
                &registry,
                "git:github.com-xeworg-mnexis",
                Some("marker".into())
            )
            .as_deref(),
            Some("marker")
        );
    }

    #[test]
    fn nothing_resolves_without_a_binding_and_without_a_marker() {
        let registry = ProjectRegistry::default();
        assert_eq!(
            resolve_namespace(&registry, "git:github.com-xeworg-mnexis", None),
            None
        );
    }
}
