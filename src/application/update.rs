//! The update service: is there a newer release, and can this binary
//! install it?
//!
//! The service does not speak HTTP itself: it drives the system tools
//! (`curl`, `sha256sum`, `tar`), the same ones the installer uses. The
//! binary keeps no HTTP client and no TLS stack of its own, and the
//! machine that runs the update is the machine that already has the
//! tools to install it.
//!
//! The version check reads the `Location` header of the redirect that
//! `releases/latest` answers with, instead of asking the GitHub API:
//! the API allows 60 requests per hour per IP without a token, and a TUI
//! opened many times a day would spend that budget on nothing.
//!
//! Nothing here is authoritative about what to install: the download is
//! verified against the checksum the release publishes, and the artifact
//! has to carry the version that was asked for.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crate::domain::error::MnexisError;

/// The repository the releases live in.
pub const REPO_URL: &str = "https://github.com/Xeworg/mnexis";

/// How long a version check is reused before asking again (24 hours).
pub const CHECK_INTERVAL_SECS: i64 = 24 * 60 * 60;

/// The output of a command the service ran.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// The outside world, behind a trait so the tests can drive it.
pub trait System {
    /// Run a command and return its output.
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, MnexisError>;
    /// The current time, in epoch seconds.
    fn now(&self) -> i64;
    /// The path of the running binary.
    fn current_exe(&self) -> Result<PathBuf, MnexisError>;
}

/// The real system.
pub struct RealSystem;

impl System for RealSystem {
    fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, MnexisError> {
        let output = std::process::Command::new(program)
            .args(args)
            .output()
            .map_err(|e| MnexisError::busy(format!("cannot run {program}: {e}")))?;
        Ok(CommandOutput {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn current_exe(&self) -> Result<PathBuf, MnexisError> {
        std::env::current_exe()
            .map_err(|e| MnexisError::busy(format!("cannot resolve the running binary: {e}")))
    }
}

/// What the version check found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateStatus {
    /// The version of the running binary.
    pub current_version: String,
    /// The newest published version, when it could be resolved.
    pub latest_version: Option<String>,
    /// Whether the published version is newer than the running one.
    pub update_available: bool,
    /// When the check ran, in epoch seconds.
    pub checked_at: i64,
}

/// What an update did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOutcome {
    /// Whether a new binary was installed.
    pub updated: bool,
    /// The version that was running.
    pub from_version: String,
    /// The version that is running now.
    pub to_version: String,
    /// The path of the binary.
    pub binary: PathBuf,
    /// A line for the user, when there is something to say.
    pub note: String,
}

/// The update service.
pub struct UpdateService {
    system: Box<dyn System>,
    repo_url: String,
    current_version: String,
    cache_path: PathBuf,
    work_dir: PathBuf,
}

impl UpdateService {
    /// A service for the real system, for the given profile directory.
    pub fn new(data_dir: &Path) -> Self {
        Self::with_system(
            data_dir,
            Box::new(RealSystem),
            REPO_URL,
            env!("CARGO_PKG_VERSION").to_string(),
        )
    }

    /// A service with an injected system, for the tests.
    pub fn with_system(
        data_dir: &Path,
        system: Box<dyn System>,
        repo_url: &str,
        current_version: String,
    ) -> Self {
        Self {
            system,
            repo_url: repo_url.to_string(),
            current_version,
            cache_path: data_dir.join("update-check"),
            work_dir: data_dir.join("update-work"),
        }
    }

    /// The version of the running binary.
    pub fn current_version(&self) -> &str {
        &self.current_version
    }

    /// Is there a newer release?
    ///
    /// The answer is cached for a day: the check is a network call and
    /// the answer does not change faster than that. `force` asks again
    /// anyway.
    pub fn check(&self, force: bool) -> Result<UpdateStatus, MnexisError> {
        if !force && let Some(cached) = self.read_cache()? {
            return Ok(cached);
        }
        let latest = self.latest_version()?;
        let status = UpdateStatus {
            current_version: self.current_version.clone(),
            update_available: match &latest {
                Some(latest) => {
                    compare_versions(latest, &self.current_version) == Ordering::Greater
                }
                None => false,
            },
            latest_version: latest,
            checked_at: self.system.now(),
        };
        self.write_cache(&status)?;
        Ok(status)
    }

    /// The newest published version, when it can be resolved.
    ///
    /// No network, no `curl`, no published release: there is nothing to
    /// report, and that is not an error.
    pub fn latest_version(&self) -> Result<Option<String>, MnexisError> {
        let url = format!("{}/releases/latest", self.repo_url);
        let output = self
            .system
            .run("curl", &["-fsSI", "--max-time", "10", &url])?;
        if output.code != 0 {
            return Ok(None);
        }
        Ok(parse_latest_tag(&output.stdout))
    }

    /// Install a published version.
    ///
    /// The artifact is downloaded, checked against the checksum the
    /// release publishes, and only then put in place of the running
    /// binary. The replacement is atomic: a temporary file next to the
    /// binary, then a rename, so an interrupted update never leaves a
    /// half-written binary behind.
    pub fn install(&self, version: Option<&str>) -> Result<UpdateOutcome, MnexisError> {
        let target = match version {
            Some(version) => version.trim().trim_start_matches('v').to_string(),
            None => match self.latest_version()? {
                Some(latest) => latest,
                None => {
                    return Err(MnexisError::not_found(
                        "a published release (the latest one could not be resolved)",
                    ));
                }
            },
        };
        if target.is_empty() {
            return Err(MnexisError::invalid_argument(
                "version",
                "the version is empty",
            ));
        }

        let exe = self.system.current_exe()?;
        let asset = format!("mnexis-{}.tar.gz", release_platform()?);
        let base = format!("{}/releases/download/v{}", self.repo_url, target);

        let work = self.work_dir.join(&target);
        let _ = std::fs::remove_dir_all(&work);
        std::fs::create_dir_all(&work)
            .map_err(|e| MnexisError::busy(format!("cannot create the work directory: {e}")))?;

        let archive = work.join(&asset);
        let checksums = work.join("SHA256SUMS");
        self.download(&format!("{base}/{asset}"), &archive)?;
        self.download(&format!("{base}/SHA256SUMS"), &checksums)?;

        let expected = expected_checksum(&checksums, &asset)?;
        let actual = self.sha256(&archive)?;
        if expected != actual {
            let _ = std::fs::remove_dir_all(&work);
            return Err(MnexisError::invalid_argument(
                "checksum",
                format!("{asset} does not match the published checksum; nothing was installed"),
            ));
        }

        let extract = work.join("extract");
        std::fs::create_dir_all(&extract).map_err(|e| {
            MnexisError::busy(format!("cannot create the extraction directory: {e}"))
        })?;
        self.system.run(
            "tar",
            &[
                "-xzf",
                &archive.to_string_lossy(),
                "-C",
                &extract.to_string_lossy(),
            ],
        )?;
        let unpacked = extract.join("mnexis");
        if !unpacked.exists() {
            let _ = std::fs::remove_dir_all(&work);
            return Err(MnexisError::not_found(format!("mnexis in {asset}")));
        }
        // The artifact has to be the version that was asked for.
        let packaged = extract.join("VERSION");
        if packaged.exists() {
            let found = std::fs::read_to_string(&packaged)
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            if !found.is_empty() && found != target {
                let _ = std::fs::remove_dir_all(&work);
                return Err(MnexisError::invalid_argument(
                    "version",
                    format!("the artifact is version {found} but {target} was requested"),
                ));
            }
        }

        let binary = self.replace_binary(&unpacked, &exe)?;
        let _ = std::fs::remove_dir_all(&work);

        Ok(UpdateOutcome {
            updated: true,
            from_version: self.current_version.clone(),
            to_version: target,
            binary,
            note: "restart the terminal (or the agent) to run the new version".to_string(),
        })
    }

    /// Put the new binary in place of the running one.
    fn replace_binary(&self, source: &Path, exe: &Path) -> Result<PathBuf, MnexisError> {
        let dir = exe.parent().unwrap_or_else(|| Path::new("."));
        let tmp = dir.join(format!(".mnexis.update.{}", std::process::id()));
        std::fs::copy(source, &tmp).map_err(|e| {
            MnexisError::busy(format!(
                "cannot write the new binary next to the old one: {e}"
            ))
        })?;
        set_executable(&tmp)?;
        std::fs::rename(&tmp, exe).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            MnexisError::busy(format!("cannot replace the running binary: {e}"))
        })?;
        Ok(exe.to_path_buf())
    }

    /// Download a file with `curl`.
    fn download(&self, url: &str, dest: &Path) -> Result<(), MnexisError> {
        let output = self.system.run(
            "curl",
            &[
                "-fsSL",
                "--retry",
                "3",
                "--retry-delay",
                "1",
                "-o",
                &dest.to_string_lossy(),
                url,
            ],
        )?;
        if output.code != 0 {
            let _ = std::fs::remove_dir_all(&self.work_dir);
            return Err(MnexisError::busy(format!(
                "the download failed: {url} ({})",
                output.stderr.trim()
            )));
        }
        Ok(())
    }

    /// The SHA-256 of a file, computed by the system tool.
    fn sha256(&self, path: &Path) -> Result<String, MnexisError> {
        let output = self.system.run("sha256sum", &[&path.to_string_lossy()])?;
        if output.code != 0 {
            return Err(MnexisError::busy(format!(
                "cannot compute the checksum: {}",
                output.stderr.trim()
            )));
        }
        Ok(output
            .stdout
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string())
    }

    /// The cached check, when it is still fresh.
    fn read_cache(&self) -> Result<Option<UpdateStatus>, MnexisError> {
        let Ok(raw) = std::fs::read_to_string(&self.cache_path) else {
            return Ok(None);
        };
        let mut lines = raw.lines();
        let Some(checked_at) = lines
            .next()
            .and_then(|line| line.trim().parse::<i64>().ok())
        else {
            return Ok(None);
        };
        if self.system.now() - checked_at >= CHECK_INTERVAL_SECS {
            return Ok(None);
        }
        let latest = lines.next().unwrap_or("").trim().to_string();
        let latest = if latest.is_empty() {
            None
        } else {
            Some(latest)
        };
        let update_available = match &latest {
            Some(latest) => compare_versions(latest, &self.current_version) == Ordering::Greater,
            None => false,
        };
        Ok(Some(UpdateStatus {
            current_version: self.current_version.clone(),
            latest_version: latest,
            update_available,
            checked_at,
        }))
    }

    /// Cache the check so the next start does not ask again.
    fn write_cache(&self, status: &UpdateStatus) -> Result<(), MnexisError> {
        let body = format!(
            "{}\n{}\n",
            status.checked_at,
            status.latest_version.clone().unwrap_or_default()
        );
        if let Some(dir) = self.cache_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        std::fs::write(&self.cache_path, body)
            .map_err(|e| MnexisError::busy(format!("cannot write the update cache: {e}")))
    }
}

/// The version in the `Location` header of the redirect.
pub fn parse_latest_tag(headers: &str) -> Option<String> {
    for line in headers.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.trim().eq_ignore_ascii_case("location") {
            continue;
        }
        let tag = value.trim().rsplit('/').next()?.trim();
        if tag.is_empty() {
            return None;
        }
        return Some(tag.trim_start_matches('v').to_string());
    }
    None
}

/// The checksum a `SHA256SUMS` file publishes for an asset.
pub fn expected_checksum(checksums: &Path, asset: &str) -> Result<String, MnexisError> {
    let raw = std::fs::read_to_string(checksums)
        .map_err(|e| MnexisError::not_found(format!("the published checksums ({e})")))?;
    for line in raw.lines() {
        let mut parts = line.split_whitespace();
        let Some(digest) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if name.trim_start_matches('*') == asset {
            return Ok(digest.to_string());
        }
    }
    Err(MnexisError::not_found(format!(
        "a published checksum for {asset}"
    )))
}

/// Compare two versions.
///
/// `1.2.3` style, with an optional pre-release suffix that is older than
/// the release itself (`1.2.3-rc1` is older than `1.2.3`).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a_parts, a_pre) = split_version(a);
    let (b_parts, b_pre) = split_version(b);
    for (x, y) in a_parts.iter().zip(b_parts.iter()) {
        match x.cmp(y) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match a_parts.len().cmp(&b_parts.len()) {
        Ordering::Equal => {}
        other => return other,
    }
    match (a_pre.as_deref(), b_pre.as_deref()) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => x.cmp(y),
    }
}

/// The numeric parts of a version and its pre-release suffix.
fn split_version(version: &str) -> (Vec<u64>, Option<String>) {
    let version = version.trim().trim_start_matches('v');
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre.to_string())),
        None => (version, None),
    };
    let parts = core
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect();
    (parts, pre)
}

/// The platform slug the releases publish artifacts for.
pub fn release_platform() -> Result<&'static str, MnexisError> {
    #[cfg(target_arch = "x86_64")]
    {
        Ok("linux-x86_64")
    }
    #[cfg(target_arch = "aarch64")]
    {
        Ok("linux-aarch64")
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        Err(MnexisError::invalid_argument(
            "platform",
            "mnexis publishes binaries for linux-x86_64 and linux-aarch64 only",
        ))
    }
}

/// Make a file executable by everybody.
fn set_executable(path: &Path) -> Result<(), MnexisError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)
            .map_err(|e| MnexisError::busy(format!("cannot read the new binary: {e}")))?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(path, permissions).map_err(|e| {
            MnexisError::busy(format!("cannot make the new binary executable: {e}"))
        })?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A system that answers from a script instead of the network.
    struct FakeSystem {
        now: i64,
        exe: PathBuf,
        answers: RefCell<Vec<(String, Vec<String>, CommandOutput)>>,
        calls: RefCell<Vec<(String, Vec<String>)>>,
    }

    impl FakeSystem {
        fn new(now: i64) -> Self {
            Self {
                now,
                exe: PathBuf::from("/tmp/mnexis"),
                answers: RefCell::new(Vec::new()),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn answer(self, program: &str, args: &[&str], output: CommandOutput) -> Self {
            self.answers.borrow_mut().push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
                output,
            ));
            self
        }
    }

    impl System for FakeSystem {
        fn run(&self, program: &str, args: &[&str]) -> Result<CommandOutput, MnexisError> {
            self.calls.borrow_mut().push((
                program.to_string(),
                args.iter().map(|a| a.to_string()).collect(),
            ));
            for (name, wanted, output) in self.answers.borrow().iter() {
                if name == program
                    && wanted == &args.iter().map(|a| a.to_string()).collect::<Vec<_>>()
                {
                    return Ok(output.clone());
                }
            }
            Ok(CommandOutput {
                code: 127,
                stdout: String::new(),
                stderr: format!("{program}: not found"),
            })
        }

        fn now(&self) -> i64 {
            self.now
        }

        fn current_exe(&self) -> Result<PathBuf, MnexisError> {
            Ok(self.exe.clone())
        }
    }

    fn headers(location: &str) -> CommandOutput {
        CommandOutput {
            code: 0,
            stdout: format!("HTTP/2 302\nlocation: {location}\ncontent-length: 0\n"),
            stderr: String::new(),
        }
    }

    #[test]
    fn the_location_header_carries_the_version() {
        assert_eq!(
            parse_latest_tag(
                "HTTP/2 302\nlocation: https://github.com/Xeworg/mnexis/releases/tag/v0.2.0\n"
            ),
            Some("0.2.0".to_string())
        );
        assert_eq!(parse_latest_tag("HTTP/2 200\ncontent-length: 0\n"), None);
    }

    #[test]
    fn versions_compare_by_number_then_by_precedence() {
        assert_eq!(compare_versions("0.2.0", "0.1.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.1.0", "0.2.0"), Ordering::Less);
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Ordering::Greater);
        assert_eq!(compare_versions("0.2.0", "0.2.0-rc1"), Ordering::Greater);
        assert_eq!(compare_versions("0.2.0-rc1", "0.2.0"), Ordering::Less);
        assert_eq!(compare_versions("v1.0.0", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn the_check_reports_a_newer_release() {
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        let system = FakeSystem::new(1_000).answer(
            "curl",
            &[
                "-fsSI",
                "--max-time",
                "10",
                "https://github.com/Xeworg/mnexis/releases/latest",
            ],
            headers("https://github.com/Xeworg/mnexis/releases/tag/v0.2.0"),
        );
        let service =
            UpdateService::with_system(dir.path(), Box::new(system), REPO_URL, "0.1.0".to_string());
        let status = service.check(true).expect("check");
        assert_eq!(status.latest_version, Some("0.2.0".to_string()));
        assert!(status.update_available);
        assert_eq!(status.current_version, "0.1.0");
    }

    #[test]
    fn a_release_without_a_newer_version_reports_no_update() {
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        let system = FakeSystem::new(1_000).answer(
            "curl",
            &[
                "-fsSI",
                "--max-time",
                "10",
                "https://github.com/Xeworg/mnexis/releases/latest",
            ],
            headers("https://github.com/Xeworg/mnexis/releases/tag/v0.1.0"),
        );
        let service =
            UpdateService::with_system(dir.path(), Box::new(system), REPO_URL, "0.1.0".to_string());
        let status = service.check(true).expect("check");
        assert!(!status.update_available);
    }

    #[test]
    fn a_network_failure_is_not_an_error() {
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        let system = FakeSystem::new(1_000);
        let service =
            UpdateService::with_system(dir.path(), Box::new(system), REPO_URL, "0.1.0".to_string());
        let status = service.check(true).expect("check");
        assert_eq!(status.latest_version, None);
        assert!(!status.update_available);
    }

    #[test]
    fn the_check_is_cached_for_a_day() {
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        let system = FakeSystem::new(1_000).answer(
            "curl",
            &[
                "-fsSI",
                "--max-time",
                "10",
                "https://github.com/Xeworg/mnexis/releases/latest",
            ],
            headers("https://github.com/Xeworg/mnexis/releases/tag/v0.2.0"),
        );
        let service =
            UpdateService::with_system(dir.path(), Box::new(system), REPO_URL, "0.1.0".to_string());
        service.check(true).expect("first check");
        // The cached answer answers without asking again.
        let status = service.check(false).expect("cached check");
        assert_eq!(status.latest_version, Some("0.2.0".to_string()));
    }

    #[test]
    fn a_stale_cache_is_asked_again() {
        let dir = tempfile::tempdir_in("target").expect("tempdir");
        // Now is far enough that a cache written at the epoch is stale.
        let system = FakeSystem::new(200_000).answer(
            "curl",
            &[
                "-fsSI",
                "--max-time",
                "10",
                "https://github.com/Xeworg/mnexis/releases/latest",
            ],
            headers("https://github.com/Xeworg/mnexis/releases/tag/v0.2.0"),
        );
        let service =
            UpdateService::with_system(dir.path(), Box::new(system), REPO_URL, "0.1.0".to_string());
        service.check(true).expect("first check");
        std::fs::write(dir.path().join("update-check"), "0\n0.9.0\n").expect("write cache");
        let status = service
            .check(false)
            .expect("check after the cache went stale");
        assert_eq!(status.latest_version, Some("0.2.0".to_string()));
    }
}
