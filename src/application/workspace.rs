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

pub use crate::domain::workspace::{canonical_workspace_key, normalize_remote_url};

/// The marker file a workspace can drop to name its namespace without a
/// binding.
pub const MARKER_FILE: &str = ".mnexis-namespace";

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
    let canonical = canonical_workspace_key(key);
    // The binding is looked up by the canonical key, and a binding that was
    // recorded before the rule existed is compared in its canonical form
    // too: both sides have to mean the same workspace.
    let found = registry
        .bindings
        .iter()
        .find(|b| canonical_workspace_key(&b.key) == canonical);
    match found {
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
