//! Installing the agent integration (skill and extension) into an agent
//! environment.
//!
//! This is the only place that writes those files. The CLI and the TUI both
//! go through it, so an installation performed from the terminal and one
//! performed from the TUI are the same operation.
//!
//! Rules:
//!
//! - The embedded assets ([`crate::assets`]) are the only source; there is
//!   no second copy of the skill or the extension anywhere.
//! - Installing is idempotent and never destructive: it replaces only the
//!   files mnexis ships and leaves every foreign file alone.
//! - The profile files (`mnexis.toml`, `projects.toml`) are never touched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::assets::{ALL_ASSETS, EmbeddedAsset};

/// An agent environment mnexis can install its integration into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentEnvironment {
    /// pi: the skill goes to `<agent-dir>/skills`, the extension to
    /// `<agent-dir>/extensions`.
    Pi,
    /// opencode: the skill goes to `~/.config/opencode/skills`.
    Opencode,
}

impl AgentEnvironment {
    /// Every supported environment, in presentation order.
    pub const ALL: [AgentEnvironment; 2] = [AgentEnvironment::Pi, AgentEnvironment::Opencode];

    /// The environment's identifier, as used on the command line.
    pub fn id(self) -> &'static str {
        match self {
            AgentEnvironment::Pi => "pi",
            AgentEnvironment::Opencode => "opencode",
        }
    }

    /// A short human-readable name for the TUI.
    pub fn display_name(self) -> &'static str {
        match self {
            AgentEnvironment::Pi => "pi",
            AgentEnvironment::Opencode => "opencode",
        }
    }

    /// Parse an environment identifier.
    pub fn parse(value: &str) -> Option<AgentEnvironment> {
        match value.trim().to_lowercase().as_str() {
            "pi" => Some(AgentEnvironment::Pi),
            "opencode" => Some(AgentEnvironment::Opencode),
            _ => None,
        }
    }

    /// The supported identifiers, for help and error messages.
    pub fn supported_ids() -> String {
        AgentEnvironment::ALL
            .iter()
            .map(|env| env.id())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// Which kinds of asset an operation touches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFilter {
    /// Every asset that applies to the environment.
    All,
    /// Only the agent skill.
    Skill,
    /// Only the agent extension.
    Extension,
}

impl AssetFilter {
    /// Parse a filter name.
    pub fn parse(value: &str) -> Option<AssetFilter> {
        match value.trim().to_lowercase().as_str() {
            "all" => Some(AssetFilter::All),
            "skill" | "skills" => Some(AssetFilter::Skill),
            "extension" | "extensions" => Some(AssetFilter::Extension),
            _ => None,
        }
    }

    /// The filter names, for help and error messages.
    pub fn supported_ids() -> String {
        "all, skill, extension".to_string()
    }

    /// Whether this filter includes a kind.
    pub fn includes(self, kind: AssetKind) -> bool {
        match self {
            AssetFilter::All => true,
            AssetFilter::Skill => matches!(kind, AssetKind::Skill),
            AssetFilter::Extension => matches!(kind, AssetKind::Extension),
        }
    }
}

/// What kind of asset an installation writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetKind {
    /// An agent skill (`SKILL.md`).
    Skill,
    /// An agent extension (an executable module for the host).
    Extension,
}

impl AssetKind {
    /// The plural noun used in messages.
    pub fn label(self) -> &'static str {
        match self {
            AssetKind::Skill => "skill",
            AssetKind::Extension => "extension",
        }
    }
}

/// Where an asset is installed for a given environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetTarget {
    /// The environment.
    pub environment: AgentEnvironment,
    /// The kind of asset.
    pub kind: AssetKind,
    /// The asset name (the directory name under the target).
    pub name: &'static str,
    /// The directory the asset is written to.
    pub directory: PathBuf,
}

/// The home directory used to resolve the targets.
#[derive(Debug, Clone)]
pub struct AgentInstaller {
    home: PathBuf,
    /// pi's agent directory, when overridden by `PI_CODING_AGENT_DIR`.
    agent_dir_override: Option<PathBuf>,
}

impl AgentInstaller {
    /// Build an installer resolving paths under `home`.
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            agent_dir_override: None,
        }
    }

    /// Build an installer from the process environment.
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let agent_dir_override = std::env::var_os("PI_CODING_AGENT_DIR").map(PathBuf::from);
        Self {
            home,
            agent_dir_override,
        }
    }

    /// Override pi's agent directory (used by the tests).
    pub fn with_agent_dir(mut self, agent_dir: impl Into<PathBuf>) -> Self {
        self.agent_dir_override = Some(agent_dir.into());
        self
    }

    /// pi's agent directory: `PI_CODING_AGENT_DIR` when set, else
    /// `~/.pi/agent`.
    pub fn pi_agent_dir(&self) -> PathBuf {
        self.agent_dir_override
            .clone()
            .unwrap_or_else(|| self.home.join(".pi").join("agent"))
    }

    /// The directory where the environment looks for skills.
    pub fn skills_dir(&self, environment: AgentEnvironment) -> PathBuf {
        match environment {
            AgentEnvironment::Pi => self.pi_agent_dir().join("skills"),
            AgentEnvironment::Opencode => self.home.join(".config").join("opencode").join("skills"),
        }
    }

    /// The directory where the environment looks for extensions, when it
    /// supports them at all.
    pub fn extensions_dir(&self, environment: AgentEnvironment) -> Option<PathBuf> {
        match environment {
            AgentEnvironment::Pi => Some(self.pi_agent_dir().join("extensions")),
            AgentEnvironment::Opencode => None,
        }
    }

    /// Every asset that can be installed into an environment.
    pub fn targets(&self, environment: AgentEnvironment) -> Vec<AssetTarget> {
        let mut targets = vec![AssetTarget {
            environment,
            kind: AssetKind::Skill,
            name: "mnexis-memory",
            directory: self.skills_dir(environment).join("mnexis-memory"),
        }];
        if let Some(dir) = self.extensions_dir(environment) {
            targets.push(AssetTarget {
                environment,
                kind: AssetKind::Extension,
                name: "mnexis-memory",
                directory: dir.join("mnexis-memory"),
            });
        }
        targets
    }

    /// Whether an asset is currently installed for an environment.
    pub fn is_installed(&self, target: &AssetTarget) -> bool {
        let asset = asset_for(target.kind).unwrap_or(&ALL_ASSETS[0]);
        asset
            .files
            .iter()
            .all(|file| target.directory.join(file.path).is_file())
    }

    /// Install every asset of an environment that the filter selects.
    ///
    /// Files mnexis does not ship are never removed; the files it does ship
    /// are replaced with the embedded content.
    pub fn install(
        &self,
        environment: AgentEnvironment,
        filter: AssetFilter,
    ) -> io::Result<InstallReport> {
        let mut report = InstallReport::new(environment);
        for target in self.targets(environment) {
            if !filter.includes(target.kind) {
                continue;
            }
            let asset = asset_for(target.kind).expect("target kind has an asset");
            let was_installed = self.is_installed(&target);
            for file in asset.files {
                let path = target.directory.join(file.path);
                write_file_atomically(&path, file.contents)?;
                report.files.push(InstalledFile {
                    path,
                    action: FileAction::Written,
                });
            }
            report.assets.push(InstalledAsset {
                kind: target.kind,
                name: target.name,
                directory: target.directory.clone(),
                action: if was_installed {
                    AssetAction::Updated
                } else {
                    AssetAction::Installed
                },
            });
        }
        Ok(report)
    }

    /// Remove every asset mnexis installed for an environment.
    ///
    /// Only the asset directories are removed; anything else in the target
    /// directory is left untouched.
    pub fn uninstall(
        &self,
        environment: AgentEnvironment,
        filter: AssetFilter,
    ) -> io::Result<InstallReport> {
        let mut report = InstallReport::new(environment);
        for target in self.targets(environment) {
            if !filter.includes(target.kind) {
                continue;
            }
            if !target.directory.is_dir() {
                continue;
            }
            // Remove only the files mnexis ships, then the directories it
            // created, from the deepest one up, and only while they are
            // empty: a foreign file anywhere inside must survive.
            let mut protected: Vec<PathBuf> = Vec::new();
            if let Some(asset) = asset_for(target.kind) {
                for file in asset.files {
                    let path = target.directory.join(file.path);
                    // symlink_metadata does not follow links: a symlink is
                    // removed as a link, never as its target. A directory in
                    // the way is not ours to remove: skip it instead of
                    // failing the whole uninstall.
                    match fs::symlink_metadata(&path) {
                        Ok(metadata) if metadata.is_dir() => {
                            // Not ours: it must survive, even as an empty
                            // directory the cleanup would otherwise remove.
                            protected.push(path);
                        }
                        Ok(_) => {
                            fs::remove_file(&path)?;
                            report.files.push(InstalledFile {
                                path,
                                action: FileAction::Removed,
                            });
                        }
                        Err(_) => {}
                    }
                }
            }
            remove_empty_directories(&target.directory, &protected);
            report.assets.push(InstalledAsset {
                kind: target.kind,
                name: target.name,
                directory: target.directory.clone(),
                action: AssetAction::Removed,
            });
        }
        Ok(report)
    }
}

/// Write a file by replacing it, never through a symlink.
///
/// Writing in place would follow a symlink and destroy the file it points
/// to; renaming over it replaces the link itself and leaves the target
/// alone.
///
/// The temporary name is unique and created with `create_new`, so it can
/// never be a pre-existing symlink either, and the permissions of the file
/// being replaced are preserved (a rename creates a new inode).
fn write_file_atomically(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "asset".to_string());
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        ".{file_name}.{}.{unique}.mnexis-tmp",
        std::process::id()
    ));

    let previous_mode = fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.permissions());

    let write_result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        std::io::Write::write_all(&mut file, contents.as_bytes())?;
        if let Some(mode) = previous_mode {
            fs::set_permissions(&temporary, mode)?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        fs::remove_file(&temporary).ok();
        return Err(error);
    }

    match fs::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            fs::remove_file(&temporary).ok();
            Err(error)
        }
    }
}

/// Counter that makes the temporary file names unique.
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Remove the directories under `root` that are empty, deepest first.
///
/// The root itself is removed when it is empty, so an asset directory with
/// no foreign file left is removed as a whole. A directory holding a
/// foreign file is kept, and so is a directory in `protected`: those stand
/// in the path of a shipped file and are not ours to remove.
fn remove_empty_directories(root: &Path, protected: &[PathBuf]) {
    let mut directories = Vec::new();
    collect_directories(root, &mut directories);
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        if protected.iter().any(|kept| kept == &directory) {
            continue;
        }
        let empty = fs::read_dir(&directory)
            .map(|mut entries| entries.next().is_none())
            .unwrap_or(false);
        if empty {
            fs::remove_dir(&directory).ok();
        }
    }
}

/// Collect `root` and every directory below it.
fn collect_directories(root: &Path, out: &mut Vec<PathBuf>) {
    if !root.is_dir() {
        return;
    }
    out.push(root.to_path_buf());
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                collect_directories(&entry.path(), out);
            }
        }
    }
}

/// The embedded asset of a kind.
fn asset_for(kind: AssetKind) -> Option<&'static EmbeddedAsset> {
    ALL_ASSETS.iter().find(|asset| {
        matches!(
            (kind, asset.name),
            (AssetKind::Skill, "mnexis-memory") | (AssetKind::Extension, "mnexis-memory")
        ) && match kind {
            AssetKind::Skill => asset.files.iter().any(|f| f.path == "SKILL.md"),
            AssetKind::Extension => asset.files.iter().any(|f| f.path == "index.js"),
        }
    })
}

/// What happened to an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetAction {
    /// The asset was not there before.
    Installed,
    /// The asset was there and its files were replaced.
    Updated,
    /// The asset was removed.
    Removed,
}

/// What happened to a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    /// The file was written.
    Written,
    /// The file was removed.
    Removed,
}

/// One asset touched by an installation.
#[derive(Debug, Clone)]
pub struct InstalledAsset {
    /// The kind of asset.
    pub kind: AssetKind,
    /// The asset name.
    pub name: &'static str,
    /// Where the asset lives.
    pub directory: PathBuf,
    /// What happened.
    pub action: AssetAction,
}

/// One file touched by an installation.
#[derive(Debug, Clone)]
pub struct InstalledFile {
    /// The file path.
    pub path: PathBuf,
    /// What happened.
    pub action: FileAction,
}

/// The outcome of an install, uninstall or status operation.
#[derive(Debug, Clone)]
pub struct InstallReport {
    /// The environment the report is about.
    pub environment: AgentEnvironment,
    /// The assets touched.
    pub assets: Vec<InstalledAsset>,
    /// The files touched.
    pub files: Vec<InstalledFile>,
}

impl InstallReport {
    fn new(environment: AgentEnvironment) -> Self {
        Self {
            environment,
            assets: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Whether the operation changed anything.
    pub fn changed(&self) -> bool {
        !self.files.is_empty()
    }
}

/// The installation state of one environment.
#[derive(Debug, Clone)]
pub struct EnvironmentStatus {
    /// The environment.
    pub environment: AgentEnvironment,
    /// One entry per asset.
    pub assets: Vec<AssetStatus>,
}

/// The state of one asset.
#[derive(Debug, Clone)]
pub struct AssetStatus {
    /// The kind of asset.
    pub kind: AssetKind,
    /// The asset name.
    pub name: &'static str,
    /// Where the asset would live.
    pub directory: PathBuf,
    /// Whether it is installed.
    pub installed: bool,
}

/// The installation state of every supported environment.
#[derive(Debug, Clone)]
pub struct StatusReport {
    /// One entry per environment.
    pub environments: Vec<EnvironmentStatus>,
}

/// Collect the installation state of every supported environment.
pub fn status(installer: &AgentInstaller) -> StatusReport {
    let mut environments = Vec::new();
    for environment in AgentEnvironment::ALL {
        let assets = installer
            .targets(environment)
            .into_iter()
            .map(|target| {
                let installed = installer.is_installed(&target);
                AssetStatus {
                    kind: target.kind,
                    name: target.name,
                    directory: target.directory,
                    installed,
                }
            })
            .collect();
        environments.push(EnvironmentStatus {
            environment,
            assets,
        });
    }
    StatusReport { environments }
}

/// The directory a path is inside, used for display.
pub fn display_path(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn temp_home() -> PathBuf {
        let mut base = std::env::temp_dir();
        let unique = format!(
            "mnexis-agents-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        );
        base.push(unique);
        fs::create_dir_all(&base).expect("temp home");
        base
    }

    #[test]
    fn environments_parse_and_render() {
        assert_eq!(AgentEnvironment::parse("pi"), Some(AgentEnvironment::Pi));
        assert_eq!(
            AgentEnvironment::parse(" OpenCode "),
            Some(AgentEnvironment::Opencode)
        );
        assert_eq!(AgentEnvironment::parse("claude"), None);
        assert!(AgentEnvironment::supported_ids().contains("opencode"));
    }

    #[test]
    fn pi_targets_include_the_skill_and_the_extension() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let targets = installer.targets(AgentEnvironment::Pi);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].kind, AssetKind::Skill);
        assert_eq!(targets[1].kind, AssetKind::Extension);
        assert!(
            targets[0]
                .directory
                .starts_with(home.join(".pi").join("agent").join("skills"))
        );
        assert!(
            targets[1]
                .directory
                .starts_with(home.join(".pi").join("agent").join("extensions"))
        );
    }

    #[test]
    fn opencode_gets_the_skill_only() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let targets = installer.targets(AgentEnvironment::Opencode);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, AssetKind::Skill);
        assert!(
            targets[0]
                .directory
                .ends_with("opencode/skills/mnexis-memory")
        );
    }

    #[test]
    fn the_pi_agent_directory_can_be_overridden() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home).with_agent_dir(home.join("custom-agent"));
        assert_eq!(installer.pi_agent_dir(), home.join("custom-agent"));
    }

    #[test]
    fn install_writes_every_shipped_file_and_is_idempotent() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);

        let report = installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");
        assert!(report.changed());
        let skill = installer.targets(AgentEnvironment::Pi)[0].directory.clone();
        assert!(skill.join("SKILL.md").is_file());

        // Drift the installed copy: reinstalling restores the shipped bytes.
        fs::write(skill.join("SKILL.md"), "drift").expect("write drift");
        let report = installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("reinstall");
        assert!(report.changed());
        let contents = fs::read_to_string(skill.join("SKILL.md")).expect("read");
        assert!(contents.starts_with("---"));

        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn install_never_ships_the_extension_tests() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");
        let extension = installer.targets(AgentEnvironment::Pi)[1].directory.clone();
        assert!(extension.join("index.js").is_file());
        assert!(!extension.join("test").exists());
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn uninstall_removes_only_the_shipped_files() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");

        let skills = installer
            .skills_dir(AgentEnvironment::Pi)
            .join("mnexis-memory");
        fs::write(skills.join("foreign.md"), "not ours").expect("write foreign");

        installer
            .uninstall(AgentEnvironment::Pi, AssetFilter::All)
            .expect("uninstall");
        assert!(!skills.join("SKILL.md").exists());
        assert!(
            skills.join("foreign.md").is_file(),
            "foreign file must survive"
        );

        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn uninstall_on_a_clean_environment_is_not_an_error() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let report = installer
            .uninstall(AgentEnvironment::Opencode, AssetFilter::All)
            .expect("uninstall");
        assert!(!report.changed());
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn status_reports_every_environment_and_asset() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");

        let report = status(&installer);
        assert_eq!(report.environments.len(), 2);
        let pi = &report.environments[0];
        assert_eq!(pi.assets.len(), 2);
        assert!(pi.assets.iter().all(|asset| asset.installed));
        let opencode = &report.environments[1];
        assert!(!opencode.assets[0].installed);

        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn install_replaces_a_symlinked_file_without_touching_its_target() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let skills = installer.targets(AgentEnvironment::Pi)[0].directory.clone();
        fs::create_dir_all(&skills).expect("skills dir");
        let target = home.join("dotfile");
        fs::write(&target, "my own dotfile").expect("write target");
        std::os::unix::fs::symlink(&target, skills.join("SKILL.md")).expect("link");

        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");

        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "my own dotfile",
            "install must never write through a symlink"
        );
        let installed = skills.join("SKILL.md");
        assert!(
            fs::symlink_metadata(&installed)
                .unwrap()
                .file_type()
                .is_file(),
            "the symlink itself is replaced by a regular file"
        );
        let leftovers: Vec<String> = fs::read_dir(&skills)
            .expect("read dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".mnexis-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temporary file is left behind: {leftovers:?}"
        );
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn uninstall_removes_a_symlink_as_a_link() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let skills = installer.targets(AgentEnvironment::Pi)[0].directory.clone();
        fs::create_dir_all(&skills).expect("skills dir");
        let target = home.join("dotfile");
        fs::write(&target, "my own dotfile").expect("write target");
        std::os::unix::fs::symlink(&target, skills.join("SKILL.md")).expect("link");

        installer
            .uninstall(AgentEnvironment::Pi, AssetFilter::All)
            .expect("uninstall");

        assert!(
            fs::symlink_metadata(skills.join("SKILL.md")).is_err(),
            "the link is gone"
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            "my own dotfile",
            "uninstall must not delete the symlink target"
        );
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let skills = installer.targets(AgentEnvironment::Pi)[0].directory.clone();
        fs::create_dir_all(skills.join("SKILL.md")).expect("dir in the way");

        assert!(
            installer
                .install(AgentEnvironment::Pi, AssetFilter::All)
                .is_err(),
            "renaming over a directory must fail"
        );
        let leftovers: Vec<String> = fs::read_dir(&skills)
            .expect("read dir")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".mnexis-tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no temporary file may survive a failed write: {leftovers:?}"
        );
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn reinstalling_preserves_the_mode_of_the_replaced_file() {
        use std::os::unix::fs::PermissionsExt;

        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let skills = installer.targets(AgentEnvironment::Pi)[0].directory.clone();
        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");
        let installed = skills.join("SKILL.md");
        fs::set_permissions(&installed, fs::Permissions::from_mode(0o755)).expect("chmod");

        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("reinstall");

        let mode = fs::metadata(&installed)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o777,
            0o755,
            "the mode of the replaced file survives the reinstall"
        );
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn uninstall_skips_a_directory_standing_in_the_path_of_a_shipped_file() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        let skills = installer.targets(AgentEnvironment::Pi)[0].directory.clone();
        fs::create_dir_all(skills.join("SKILL.md")).expect("dir in the way");

        installer
            .uninstall(AgentEnvironment::Pi, AssetFilter::All)
            .expect("uninstall must not fail on a directory in the way");

        assert!(
            skills.join("SKILL.md").is_dir(),
            "the directory that is not ours survives"
        );
        fs::remove_dir_all(&home).ok();
    }

    #[test]
    fn install_and_uninstall_touch_distinct_directories() {
        let home = temp_home();
        let installer = AgentInstaller::new(&home);
        installer
            .install(AgentEnvironment::Pi, AssetFilter::All)
            .expect("install");
        let mut seen: HashSet<PathBuf> = HashSet::new();
        for target in installer.targets(AgentEnvironment::Pi) {
            assert!(seen.insert(target.directory.clone()));
        }
        fs::remove_dir_all(&home).ok();
    }
}
