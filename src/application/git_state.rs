//! The git state of a branch: the anchor a session records, and what became
//! of it when it is read (session-recall S2, S3).
//!
//! A session records the **anchor** of the repository it happened in — the
//! branch, the commit and the canonical remote (`git:<normalized>`). The
//! anchor is the part that does not rot: the commit is still that commit, even
//! when the branch has moved on or disappeared.
//!
//! The **state** of that branch — whether the work reached the base — is a
//! fact about the repository *now*, so it is derived when the anchor is read
//! and never stored: a stored state would rot exactly like a stored branch
//! name. The answer is one of three, and `unknown` is a first-class one: the
//! repository may not be on this machine, the base may be undecidable, git
//! may be missing. The engine reports what it could not check and never turns
//! it into a negative. There is no "pending" answer either: whether the work
//! is still pending is a judgment about evidence, and the engine does not make
//! it.
//!
//! Every git query here is best effort. No git, no repository, or a failing
//! git is not an error: it is an `unknown`, and the answer says why.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::domain::workspace::normalize_remote_url;

/// One git query in `dir`. Anything but a clean, non-empty answer is `None`:
/// a workspace without git is a normal workspace, not a failure.
pub fn git_output(dir: &Path, args: &[&str]) -> Option<String> {
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

/// The exit status of one git query in `dir`, or `None` when git could not be
/// run at all.
///
/// [`git_output`] collapses every failure into `None`, which is the right
/// answer for a probe. The comparison needs the distinction: `merge-base
/// --is-ancestor` answers with its exit code, and `1` means "not an ancestor"
/// while anything else means "the question could not be asked".
fn git_status(dir: &Path, args: &[&str]) -> Option<std::process::ExitStatus> {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()
        .map(|out| out.status)
}

/// The git anchor of a directory: where the work happened.
///
/// The **anchor** is the part of the context that does not rot: the commit
/// is still that commit, even when the branch has moved on. The branch name
/// is recorded next to it because it is what a human reads, and the remote is
/// recorded in its canonical form (`git:<normalized>`) because a raw URL can
/// carry credentials.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitAnchor {
    /// The branch the directory is on.
    pub branch: Option<String>,
    /// The commit the directory is at.
    pub sha: Option<String>,
    /// The canonical key of the repository (`git:<normalized>`).
    pub remote: Option<String>,
}

/// The anchor of `directory`, as git reports it.
///
/// Everything but a clean answer is `None`: a directory that is not a
/// repository, a git that is not installed, a repository with no commits, a
/// detached HEAD, a remote that cannot be normalized. **No git is not an
/// error** — a session that did not happen in a repository is a session, and
/// the engine records what it can and nothing more.
pub fn git_anchor(directory: &Path) -> GitAnchor {
    // The root of the repository, not the directory: a session that happens
    // in a subdirectory of a repository is anchored to that repository.
    let Some(root) = repository_root(directory) else {
        return GitAnchor::default();
    };
    // `symbolic-ref` fails on a detached HEAD, which is the honest answer:
    // there is no branch there.
    let branch = git_output(&root, &["symbolic-ref", "--short", "HEAD"]);
    let sha = git_output(&root, &["rev-parse", "HEAD"]);
    GitAnchor {
        branch,
        sha,
        remote: canonical_remote_key(&root),
    }
}

/// Whether the recorded commit reached the base.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchVerdict {
    /// The compared commit is an ancestor of the base.
    Merged,
    /// It is not.
    NotMerged,
    /// It could not be checked. Never a negative: see the module docs.
    Unknown,
}

impl std::fmt::Display for BranchVerdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Merged => "merged",
            Self::NotMerged => "not_merged",
            Self::Unknown => "unknown",
        })
    }
}

/// What the comparison used as the younger side of the ancestry check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComparedKind {
    /// The branch the session recorded still exists, and its current tip was
    /// compared: the branch moved on, and the tip is what matters now.
    BranchTip,
    /// The branch is gone, so the commit the session recorded — the last
    /// evidence of the work — was compared.
    RecordedCommit,
}

/// The younger side of the comparison, and why it was the one chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ComparedAnchor {
    /// Which of the two the comparison used.
    pub kind: ComparedKind,
    /// The name the answer reports: the branch, or the recorded commit.
    pub name: String,
    /// The commit that was compared against the base.
    pub commit: String,
}

/// The state of the branch the session was anchored to, validated at read
/// time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BranchState {
    /// Whether the work reached the base.
    pub state: BranchVerdict,
    /// The base ref the comparison used, as git and a human read it
    /// (`origin/main`, `main`, `master`); `None` when no base was found.
    pub base: Option<String>,
    /// What was compared; `None` when nothing could be.
    pub compared: Option<ComparedAnchor>,
    /// When the check happened (`RFC3339`): a state is a fact about the
    /// repository at that instant, not about the session.
    pub checked_at: String,
    /// Why the answer is what it is, when it is not a plain verdict.
    pub reason: Option<String>,
}

impl BranchState {
    /// An `unknown` answer with the words that say why.
    fn unknown(
        base: Option<String>,
        compared: Option<ComparedAnchor>,
        checked_at: &str,
        reason: &str,
    ) -> Self {
        Self {
            state: BranchVerdict::Unknown,
            base,
            compared,
            checked_at: checked_at.to_string(),
            reason: Some(reason.to_string()),
        }
    }
}

/// Validate the state of the anchor's branch, here and now.
///
/// The repository is the one the anchor names: the canonical remote is the
/// identity, so a directory only counts as the repository when its
/// `git:<normalized>` key equals the recorded one. A session that happened in
/// a repository **without** a remote can only be answered by the directory it
/// recorded — the reader's own repository is a different one, and substituting
/// it would answer a question nobody asked.
///
/// The base is the branch the work would land on: `origin/HEAD` when the
/// repository knows it, else `main`, else `master`. The younger side is the
/// branch's current tip when the branch still exists, and the recorded commit
/// when it does not.
///
/// Every step that fails lands on `unknown` with a reason, never on
/// `not_merged`.
pub fn validate(
    anchor: &GitAnchor,
    session_directory: &str,
    reader_directory: &Path,
    checked_at: &str,
) -> BranchState {
    let Some(root) = find_repository(anchor, session_directory, reader_directory) else {
        return BranchState::unknown(None, None, checked_at, "the repository is not here");
    };
    let Some(base) = find_base(&root) else {
        return BranchState::unknown(None, None, checked_at, "the base could not be determined");
    };
    let compared = match compared_anchor(anchor, &root) {
        Ok(compared) => compared,
        Err(reason) => return BranchState::unknown(Some(base), None, checked_at, reason),
    };
    // A commit this repository does not know cannot be asked about: it may
    // belong to a repository that is not here, or to a rewrite.
    if !commit_exists(&root, &compared.commit) {
        return BranchState::unknown(
            Some(base),
            Some(compared),
            checked_at,
            "the commit is unknown to this repository",
        );
    }
    let args = [
        "merge-base",
        "--is-ancestor",
        compared.commit.as_str(),
        base.as_str(),
    ];
    let state = match git_status(&root, &args) {
        Some(status) if status.success() => BranchVerdict::Merged,
        Some(status) if status.code() == Some(1) => BranchVerdict::NotMerged,
        _ => {
            return BranchState::unknown(
                Some(base),
                Some(compared),
                checked_at,
                "the comparison could not be made",
            );
        }
    };
    BranchState {
        state,
        base: Some(base),
        compared: Some(compared),
        checked_at: checked_at.to_string(),
        reason: None,
    }
}

/// The repository the anchor names, among the directories that could hold it.
///
/// The session's recorded directory is tried first, the reader's own
/// directory second. See [`validate`] for what makes a directory count.
fn find_repository(
    anchor: &GitAnchor,
    session_directory: &str,
    reader_directory: &Path,
) -> Option<PathBuf> {
    let recorded = session_directory.trim();
    let session_candidate = (!recorded.is_empty()).then(|| Path::new(recorded));
    let reader_candidate = (!reader_directory.as_os_str().is_empty()).then_some(reader_directory);
    for (index, candidate) in [session_candidate, reader_candidate]
        .into_iter()
        .enumerate()
    {
        let Some(candidate) = candidate else {
            continue;
        };
        let Some(root) = repository_root(candidate) else {
            continue;
        };
        let qualifies = match anchor.remote.as_deref() {
            Some(key) => canonical_remote_key(&root).as_deref() == Some(key),
            // The session happened in a repository without a remote: only the
            // directory it recorded can be that repository.
            None => index == 0,
        };
        if qualifies {
            return Some(root);
        }
    }
    None
}

/// The root of the repository `directory` belongs to, when it belongs to one.
fn repository_root(directory: &Path) -> Option<PathBuf> {
    git_output(directory, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

/// The canonical key of the repository at `root` (`git:<normalized>`), when
/// it has a remote that can be normalized.
///
/// The raw URL is never kept, let alone stored: a URL can carry credentials
/// (`https://user:token@host/...`), and the key is the identity the workspace
/// already uses.
fn canonical_remote_key(root: &Path) -> Option<String> {
    git_output(root, &["config", "--get", "remote.origin.url"])
        .and_then(|url| normalize_remote_url(&url))
        .map(|normalized| format!("git:{normalized}"))
}

/// The branch the recorded work would land on, as the answer reports it.
///
/// `origin/HEAD` says what the remote considers its trunk, which is the
/// honest base when the repository has one; without it, the local trunk is
/// `main`, and then `master`. `None` means the repository does not say.
fn find_base(root: &Path) -> Option<String> {
    if let Some(head) = git_output(
        root,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) {
        // `refs/remotes/origin/main` is reported as `origin/main`: the last
        // two segments name the ref, and the ref is what git compares against.
        let parts: Vec<&str> = head.split('/').collect();
        if parts.len() >= 2 {
            return Some(format!(
                "{}/{}",
                parts[parts.len() - 2],
                parts[parts.len() - 1]
            ));
        }
    }
    for name in ["main", "master"] {
        let reference = format!("refs/heads/{name}^{{commit}}");
        if git_output(root, &["rev-parse", "--verify", "--quiet", &reference]).is_some() {
            return Some(name.to_string());
        }
    }
    None
}

/// What to compare: the branch's current tip when the branch still exists,
/// else the recorded commit.
///
/// The error is the reason the answer carries when there is nothing to
/// compare at all.
fn compared_anchor(anchor: &GitAnchor, root: &Path) -> Result<ComparedAnchor, &'static str> {
    let branch = anchor
        .branch
        .as_deref()
        .map(str::trim)
        .filter(|branch| !branch.is_empty());
    if let Some(branch) = branch {
        // The local branch first: a branch that is still here is the work
        // as it is now. A branch pushed from another machine is the same
        // work, seen through the remote-tracking ref.
        for reference in [
            format!("refs/heads/{branch}^{{commit}}"),
            format!("refs/remotes/origin/{branch}^{{commit}}"),
        ] {
            if let Some(commit) =
                git_output(root, &["rev-parse", "--verify", "--quiet", &reference])
            {
                return Ok(ComparedAnchor {
                    kind: ComparedKind::BranchTip,
                    name: branch.to_string(),
                    commit,
                });
            }
        }
    }
    let sha = anchor
        .sha
        .as_deref()
        .map(str::trim)
        .filter(|sha| !sha.is_empty());
    if let Some(sha) = sha {
        // The branch is gone: the recorded commit is the last evidence of the
        // work, and it is still that commit.
        return Ok(ComparedAnchor {
            kind: ComparedKind::RecordedCommit,
            name: sha.to_string(),
            commit: sha.to_string(),
        });
    }
    if anchor.branch.is_none() && anchor.sha.is_none() {
        return Err("the session recorded no branch and no commit");
    }
    Err("the comparison could not be made")
}

/// Whether this repository knows `commit` as a commit.
fn commit_exists(root: &Path, commit: &str) -> bool {
    let reference = format!("{commit}^{{commit}}");
    git_output(root, &["rev-parse", "--verify", "--quiet", &reference]).is_some()
}
