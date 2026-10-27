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

use crate::application::git_state::git_output;
use crate::config::projects::{ProjectRegistry, registry_path, validate_namespace};
use crate::domain::error::MnexisError;
use crate::domain::scope::AccessScope;

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

/// What git says about a directory: the repository root (when there is
/// one), the remote of that root, and the key the two produce together.
struct Probe {
    key: String,
    repo_root: Option<PathBuf>,
    remote: Option<String>,
}

/// The workspace key of `cwd`, probed from git exactly like the extension:
/// `git rev-parse --show-toplevel` for the root and
/// `git config --get remote.origin.url` for the remote of that root.
///
/// A missing or failing `git` is not an error: the key falls back to
/// `path:<cwd>`, and the marker (if any) still decides the namespace.
fn probe(cwd: &Path) -> Probe {
    let repo_root = git_output(cwd, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    let remote = git_output(
        repo_root.as_deref().unwrap_or(cwd),
        &["config", "--get", "remote.origin.url"],
    );
    Probe {
        key: workspace_key(remote.as_deref(), repo_root.as_deref(), cwd),
        repo_root,
        remote,
    }
}

/// The last segment of a remote URL, lowercased: the name a workspace would
/// offer as its own namespace (`https://github.com/Xeworg/Nodexar.git` ->
/// `nodexar`, `git@github.com:Xeworg/my-project.git` -> `my-project`).
///
/// The raw remote is split, never the normalized key: the key has already
/// turned every separator into a dash, so its last dash-segment of
/// `github.com-xeworg-my-project` would be `project`.
fn remote_tail(remote: &str) -> Option<String> {
    let trimmed = remote.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    let tail = trimmed.rsplit(['/', ':']).find(|part| !part.is_empty())?;
    let tail = tail
        .strip_suffix(".git")
        .or_else(|| tail.strip_suffix(".GIT"))
        .unwrap_or(tail)
        .trim()
        .to_lowercase();
    if tail.is_empty() || tail == "." || tail == ".." {
        None
    } else {
        Some(tail)
    }
}

/// The namespace a human would be asked to grant for this workspace
/// (auto-namespace S2): the last segment of the remote, or the repository
/// directory's own name when the repository has no remote.
///
/// `Ok(None)` means the process is not inside a repository: there is no
/// project to name. The reserved `global` namespace, a name the workspace
/// validator refuses, and a repository with no usable name are errors, so
/// the caller can name the reason instead of quietly offering nothing.
/// Nothing here writes: deriving a candidate is not binding it.
pub fn candidate_namespace(
    remote: Option<&str>,
    repo_root: Option<&Path>,
) -> Result<Option<String>, MnexisError> {
    let Some(root) = repo_root else {
        return Ok(None);
    };
    let derived = match remote.and_then(remote_tail) {
        Some(tail) => tail,
        None => root
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default(),
    };
    if derived.trim().is_empty() {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "this workspace has no name to offer: name its namespace yourself with \
             `mnexis project bind <namespace> --key <key>`",
        ));
    }
    if derived == "global" {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "`global` is reserved and a workspace cannot be bound to it: pick another \
             namespace with `mnexis project bind <namespace> --key <key>`",
        ));
    }
    validate_namespace(&derived)?;
    Ok(Some(derived))
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

/// The namespace the binding for `key` names, when it names one.
///
/// Only the registry is consulted: the `.mnexis-namespace` marker is a
/// different rule and is never a permission. A binding with a blank
/// namespace names nothing.
pub fn bound_namespace(registry: &ProjectRegistry, key: &str) -> Option<String> {
    registry
        .find(key)
        .filter(|binding| !binding.namespace.trim().is_empty())
        .map(|binding| binding.namespace.clone())
}

/// Which namespace a workspace works in: the binding for `key`, else the
/// marker, else none.
pub fn resolve_namespace(
    registry: &ProjectRegistry,
    key: &str,
    marker: Option<String>,
) -> Option<String> {
    bound_namespace(registry, key).or(marker)
}

/// The workspace the process runs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceInfo {
    /// The workspace key: `git:<normalized remote>` or `path:<directory>`.
    pub key: String,
    /// The namespace the workspace writes into; `None` when the workspace
    /// resolves to none and the profile allowlist keeps deciding.
    pub namespace: Option<String>,
    /// The namespace the workspace is **bound** to. `None` when no binding
    /// exists, even when a marker names one: only a binding is a
    /// permission, so this is what widens the read scope.
    pub bound_namespace: Option<String>,
    /// Whether the process runs inside a git repository. A repository is a
    /// project even before it is bound: a pointer outside one keeps letting
    /// the profile allowlist decide, one inside does not (auto-namespace S5).
    pub in_repo: bool,
    /// The namespace a human would be asked to grant for this workspace
    /// (auto-namespace S2): the last segment of its remote, or the
    /// repository directory's own name. `None` outside a repository.
    pub candidate_namespace: Option<String>,
    /// Why the workspace offers no candidate although it is inside a
    /// repository: the derived name was reserved or refused.
    pub candidate_refusal: Option<String>,
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
    // A registry that cannot be read grants NOTHING: it must not widen the
    // scope. But it must not stop the reads either — a broken
    // `projects.toml` is a broken file, not a broken engine. The commands
    // that work WITH the registry (`project list`, `project bind`) still
    // fail loudly on it.
    let registry = ProjectRegistry::load(&registry_path(data_dir)).unwrap_or_default();
    let probe = probe(&cwd);
    let bound_namespace = bound_namespace(&registry, &probe.key);
    let namespace = bound_namespace
        .clone()
        .or_else(|| find_marker_namespace(&cwd));
    // Deriving a candidate is part of the answer, never an action: nothing
    // is bound, allowed, or written here.
    let candidate = candidate_namespace(probe.remote.as_deref(), probe.repo_root.as_deref());
    let (candidate_namespace, candidate_refusal) = match candidate {
        Ok(candidate) => (candidate, None),
        Err(error) => (None, Some(error.to_string())),
    };
    Ok(Some(WorkspaceInfo {
        key: probe.key,
        namespace,
        bound_namespace,
        in_repo: probe.repo_root.is_some(),
        candidate_namespace,
        candidate_refusal,
    }))
}

/// The namespace a read without an explicit namespace searches: the
/// workspace's own namespace — the one a save lands in — when the scope
/// may read it.
///
/// `None` means the caller keeps its own fallback. Two cases lead here:
/// the workspace names no namespace at all, or it names one this profile
/// may not read (a marker is a file in the repository, not a permission,
/// so the namespace it names can be unreadable).
///
/// A read never widens itself: the namespace is only used when the scope
/// already allows reading it.
pub fn read_namespace(scope: &AccessScope, workspace: Option<&WorkspaceInfo>) -> Option<String> {
    let namespace = workspace.and_then(|workspace| workspace.namespace.as_deref())?;
    if scope.can_read(namespace) {
        Some(namespace.to_string())
    } else {
        None
    }
}

/// Widen `scope`'s read allowlist with the namespace the workspace is
/// **bound** to (read-granularity S2).
///
/// Only a real binding counts: the `.mnexis-namespace` marker is a file in
/// the repository, not a permission, so a workspace that only has a marker
/// changes nothing. Writes are untouched; `can_write` keeps consulting
/// `write_namespaces`.
///
/// The reserved `global` namespace keeps its own permission: a binding must
/// not open it through the read allowlist.
pub fn allow_bound_read_namespace(scope: &mut AccessScope, workspace: Option<&WorkspaceInfo>) {
    let Some(namespace) = workspace.and_then(|workspace| workspace.bound_namespace.as_deref())
    else {
        return;
    };
    if namespace == "global" {
        return;
    }
    if !scope.read_namespaces.iter().any(|read| read == namespace) {
        scope.read_namespaces.push(namespace.to_string());
    }
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

#[cfg(test)]
mod candidate_tests {
    use super::*;

    // --- the candidate namespace (auto-namespace S2) ---

    #[test]
    fn the_candidate_is_the_last_segment_of_the_remote() {
        let root = Path::new("/w/nodexar");
        assert_eq!(
            candidate_namespace(Some("https://github.com/Xeworg/Nodexar.git"), Some(root))
                .unwrap()
                .as_deref(),
            Some("nodexar")
        );
        assert_eq!(
            candidate_namespace(Some("git@github.com:Xeworg/AegisPI.git"), Some(root))
                .unwrap()
                .as_deref(),
            Some("aegispi")
        );
        // The remote is split, never the normalized key: every separator of
        // `github.com-xeworg-my-project` is a dash, so the key's last
        // dash-segment would be `project`.
        assert_eq!(
            candidate_namespace(Some("https://github.com/Xeworg/my-project.git"), Some(root))
                .unwrap()
                .as_deref(),
            Some("my-project")
        );
    }

    #[test]
    fn the_candidate_falls_back_to_the_repository_directory() {
        assert_eq!(
            candidate_namespace(None, Some(Path::new("/w/MyProject")))
                .unwrap()
                .as_deref(),
            Some("myproject")
        );
    }

    #[test]
    fn there_is_no_candidate_outside_a_repository() {
        assert_eq!(candidate_namespace(None, None).unwrap(), None);
        assert_eq!(
            candidate_namespace(Some("https://github.com/Xeworg/Nodexar.git"), None).unwrap(),
            None
        );
    }

    #[test]
    fn the_reserved_global_namespace_is_refused_with_its_reason() {
        let error = candidate_namespace(
            Some("https://github.com/someone/Global.git"),
            Some(Path::new("/w/global")),
        )
        .expect_err("`global` is reserved");
        assert!(error.to_string().contains("global"), "{error}");
    }

    #[test]
    fn a_repository_with_no_usable_name_is_refused() {
        let error = candidate_namespace(None, Some(Path::new("/")))
            .expect_err("a root with no name offers nothing");
        assert!(error.to_string().contains("no name to offer"), "{error}");
    }
}
