//! opencode's MCP configuration: the `mcp` entry that points an opencode
//! host at the mnexis MCP server.
//!
//! opencode reads its global configuration from
//! `~/.config/opencode/opencode.json`. The file belongs to the user, not to
//! mnexis, so this module never rewrites it blindly:
//!
//! - when the file does not exist, mnexis creates it with the entry;
//! - when the file is strict JSON without an `mcp` key, the entry is
//!   inserted textually, so the rest of the file keeps its exact bytes;
//! - when the file is strict JSON with an `mcp` key, the file is rewritten
//!   as JSON, keeping every other key and value;
//! - when the file is JSONC (opencode also reads `opencode.jsonc`) or
//!   invalid, mnexis refuses to touch it and prints the snippet to add.
//!
//! Uninstalling removes only the entry mnexis added.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Map, Value};

/// The name of the MCP server entry mnexis manages.
pub const MCP_SERVER_NAME: &str = "mnexis";

/// What an operation did to opencode's configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigAction {
    /// The configuration file did not exist and was created.
    Created,
    /// The configuration file was changed.
    Updated,
    /// The configuration already had the entry (or lacked what was asked
    /// for); nothing was written.
    Unchanged,
    /// The entry was removed.
    Removed,
    /// The configuration was left alone because mnexis cannot write it
    /// safely; the outcome carries the instructions.
    Refused,
}

impl ConfigAction {
    /// The lowercase name used in reports and JSON output.
    pub fn label(self) -> &'static str {
        match self {
            ConfigAction::Created => "created",
            ConfigAction::Updated => "updated",
            ConfigAction::Unchanged => "unchanged",
            ConfigAction::Removed => "removed",
            ConfigAction::Refused => "refused",
        }
    }
}

/// The outcome of an operation on opencode's configuration.
#[derive(Debug, Clone)]
pub struct ConfigOutcome {
    /// The configuration file the operation was about.
    pub path: PathBuf,
    /// What happened.
    pub action: ConfigAction,
    /// A human-readable explanation, always filled: what was done, or what
    /// to do by hand when the action is `Refused`.
    pub detail: String,
}

/// opencode's global configuration, resolved under a home directory.
#[derive(Debug, Clone)]
pub struct OpencodeConfig {
    home: PathBuf,
}

impl OpencodeConfig {
    /// Build a configuration handle resolving paths under `home`.
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self { home: home.into() }
    }

    /// Build a configuration handle from the process environment.
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self { home }
    }

    /// The directory opencode keeps its configuration in.
    pub fn directory(&self) -> PathBuf {
        self.home.join(".config").join("opencode")
    }

    /// The strict-JSON configuration file.
    pub fn config_path(&self) -> PathBuf {
        self.directory().join("opencode.json")
    }

    /// The JSONC configuration file. When it exists, mnexis does not write
    /// anything: a file with comments cannot be rewritten without losing
    /// them.
    pub fn jsonc_path(&self) -> PathBuf {
        self.directory().join("opencode.jsonc")
    }

    /// The entry mnexis writes: a local stdio server running the MCP
    /// server of the installed binary.
    pub fn mcp_entry(&self) -> Value {
        let command = self.server_command();
        let mut server = Map::new();
        server.insert("type".to_string(), Value::String("local".to_string()));
        server.insert("command".to_string(), Value::Array(command));
        server.insert("enabled".to_string(), Value::Bool(true));
        let mut mcp = Map::new();
        mcp.insert(MCP_SERVER_NAME.to_string(), Value::Object(server));
        let mut root = Map::new();
        root.insert("mcp".to_string(), Value::Object(mcp));
        Value::Object(root)
    }

    /// The command that runs the MCP server: the installed binary, with
    /// `mcp serve`.
    fn server_command(&self) -> Vec<Value> {
        let binary = std::env::current_exe()
            .ok()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "mnexis".to_string());
        vec![
            Value::String(binary),
            Value::String("mcp".to_string()),
            Value::String("serve".to_string()),
        ]
    }

    /// The snippet a user has to add by hand when mnexis refuses to write
    /// the configuration.
    pub fn manual_snippet(&self) -> String {
        let command = self.server_command();
        let command = command
            .iter()
            .map(|value| match value.as_str() {
                Some(text) => format!("\"{text}\""),
                None => "null".to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "  \"mcp\": {{\n    \"{MCP_SERVER_NAME}\": {{\n      \"type\": \"local\",\n      \"command\": [{command}],\n      \"enabled\": true\n    }}\n  }}"
        )
    }

    /// The entry as a member of the root object, as it is written into
    /// opencode's configuration.
    fn member_text(&self) -> Option<String> {
        let entry = self.mcp_entry();
        let mcp = entry.get("mcp")?.clone();
        let text = serde_json::to_string_pretty(&mcp).ok()?;
        Some(format!("\"mcp\": {text}"))
    }

    /// Whether the configuration already carries the entry.
    pub fn is_configured(&self) -> bool {
        let Ok(text) = fs::read_to_string(self.config_path()) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return false;
        };
        value
            .get("mcp")
            .and_then(|mcp| mcp.get(MCP_SERVER_NAME))
            .is_some()
    }

    /// Write the entry into opencode's configuration.
    pub fn install(&self) -> io::Result<ConfigOutcome> {
        let path = self.config_path();
        let jsonc = self.jsonc_path();
        if jsonc.exists() {
            return Ok(ConfigOutcome {
                path: jsonc,
                action: ConfigAction::Refused,
                detail: format!(
                    "opencode's configuration is JSONC; add this to it by hand:\n{}",
                    self.manual_snippet()
                ),
            });
        }
        let entry = self.mcp_entry();
        let entry_text = {
            let mut text = serde_json::to_string_pretty(&entry)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
            text.push('\n');
            text
        };
        // The same entry as a member of the root object, for the textual
        // insertion that keeps the user's own bytes.
        let member_text = self.member_text().unwrap_or_default();
        match fs::read_to_string(&path) {
            Ok(existing) => {
                let value: Value = match serde_json::from_str(&existing) {
                    Ok(value) => value,
                    Err(_) => {
                        return Ok(ConfigOutcome {
                            path: path.clone(),
                            action: ConfigAction::Refused,
                            detail: format!(
                                "opencode's configuration is not valid JSON; add this to it by hand:\n{}",
                                self.manual_snippet()
                            ),
                        });
                    }
                };
                if value
                    .get("mcp")
                    .and_then(|mcp| mcp.get(MCP_SERVER_NAME))
                    .is_some()
                {
                    return Ok(ConfigOutcome {
                        path,
                        action: ConfigAction::Unchanged,
                        detail: format!(
                            "opencode already runs the {MCP_SERVER_NAME} MCP server; left it as it is"
                        ),
                    });
                }
                let rewritten = if value.get("mcp").is_none() {
                    // The fast path: the user's file has no "mcp" key, so
                    // the entry can be inserted without touching the rest of
                    // the file's bytes.
                    match insert_entry_textually(&existing, &member_text) {
                        Some(text) => text,
                        None => merge_and_render(&value, &entry)?,
                    }
                } else {
                    merge_and_render(&value, &entry)?
                };
                write_atomically(&path, &rewritten)?;
                Ok(ConfigOutcome {
                    path,
                    action: ConfigAction::Updated,
                    detail: "added the mnexis MCP server to opencode's configuration".to_string(),
                })
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(self.directory())?;
                write_atomically(&path, &entry_text)?;
                Ok(ConfigOutcome {
                    path,
                    action: ConfigAction::Created,
                    detail: "created opencode's configuration with the mnexis MCP server"
                        .to_string(),
                })
            }
            Err(error) => Err(error),
        }
    }

    /// Remove the entry mnexis added from opencode's configuration.
    pub fn uninstall(&self) -> io::Result<ConfigOutcome> {
        let path = self.config_path();
        let existing = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(ConfigOutcome {
                    path,
                    action: ConfigAction::Unchanged,
                    detail: "opencode has no configuration file; nothing to remove".to_string(),
                });
            }
            Err(error) => return Err(error),
        };
        let value: Value = match serde_json::from_str(&existing) {
            Ok(value) => value,
            Err(_) => {
                return Ok(ConfigOutcome {
                    path: path.clone(),
                    action: ConfigAction::Refused,
                    detail: "opencode's configuration is not valid JSON; left it as it is"
                        .to_string(),
                });
            }
        };
        let has_entry = value
            .get("mcp")
            .and_then(|mcp| mcp.get(MCP_SERVER_NAME))
            .is_some();
        if !has_entry {
            return Ok(ConfigOutcome {
                path,
                action: ConfigAction::Unchanged,
                detail: "opencode's configuration has no mnexis MCP server; left it as it is"
                    .to_string(),
            });
        }
        // The textual removal keeps the user's own bytes: the block mnexis
        // wrote is removed exactly as it was written. When the user edited
        // that block, the file is rewritten as JSON instead (every value is
        // preserved, the formatting is normalised).
        let member_text = self.member_text().unwrap_or_default();
        let rewritten = match remove_entry_textually(&existing, &member_text) {
            Some(text) => text,
            None => {
                let mut value = value;
                remove_entry_from_value(&mut value);
                let mut rendered = serde_json::to_string_pretty(&value)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                rendered.push('\n');
                rendered
            }
        };
        write_atomically(&path, &rewritten)?;
        Ok(ConfigOutcome {
            path,
            action: ConfigAction::Removed,
            detail: "removed the mnexis MCP server from opencode's configuration".to_string(),
        })
    }
}

/// Add the entry to a parsed configuration and render it back as JSON.
fn merge_and_render(value: &Value, entry: &Value) -> io::Result<String> {
    let mut root = match value.as_object() {
        Some(object) => object.clone(),
        None => Map::new(),
    };
    let mut mcp = match root.get("mcp").and_then(Value::as_object) {
        Some(object) => object.clone(),
        None => Map::new(),
    };
    if let Some(server) = entry
        .get("mcp")
        .and_then(|mcp| mcp.get(MCP_SERVER_NAME))
        .cloned()
    {
        mcp.insert(MCP_SERVER_NAME.to_string(), server);
    }
    root.insert("mcp".to_string(), Value::Object(mcp));
    let mut rendered = serde_json::to_string_pretty(&Value::Object(root))
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    rendered.push('\n');
    Ok(rendered)
}

/// Insert the entry textually, keeping the rest of the file's bytes.
///
/// The entry is a member of the root object (`"mcp": { ... }`) and it is
/// placed right after the opening brace, so a file that has no `mcp` key
/// keeps its own formatting, its own key order and its own text exactly as
/// the user wrote it.
fn insert_entry_textually(existing: &str, member_text: &str) -> Option<String> {
    let open = existing.find('{')?;
    let rest = &existing[open + 1..];
    let body_is_empty = rest.trim_start().starts_with('}');
    let mut entry = member_text.trim_end().to_string();
    if !body_is_empty {
        entry.push(',');
    }
    let mut out = String::with_capacity(existing.len() + entry.len() + 4);
    out.push_str(&existing[..=open]);
    if body_is_empty {
        out.push('\n');
        out.push_str(&indent(&entry, "  "));
        out.push('\n');
        out.push('}');
        // Keep whatever followed the closing brace (a trailing newline).
        let after = rest.trim_start().strip_prefix('}')?;
        out.push_str(after);
    } else {
        out.push('\n');
        out.push_str(&indent(&entry, "  "));
        out.push_str(rest);
    }
    Some(out)
}

/// Remove the entry from a parsed configuration.
fn remove_entry_from_value(value: &mut Value) {
    let Some(mcp) = value.get_mut("mcp").and_then(Value::as_object_mut) else {
        return;
    };
    mcp.remove(MCP_SERVER_NAME);
    if mcp.is_empty()
        && let Some(root) = value.as_object_mut()
    {
        root.remove("mcp");
    }
}

/// Remove the entry textually, keeping the rest of the file's bytes.
///
/// Only the exact block mnexis wrote is removed, so a file the user edited
/// by hand is left to the JSON path (which preserves every value).
fn remove_entry_textually(existing: &str, member_text: &str) -> Option<String> {
    let indented = indent(member_text.trim_end(), "  ");
    // The member was inserted before other members: the block it wrote is
    // the newline, the member and the comma that follows it.
    let span = format!("\n{indented},");
    if let Some(position) = existing.find(&span) {
        let mut out = String::with_capacity(existing.len() - span.len());
        out.push_str(&existing[..position]);
        out.push_str(&existing[position + span.len()..]);
        return Some(out);
    }
    // The member was the only one: the file was an empty object.
    let span = format!("\n{indented}\n");
    if let Some(position) = existing.find(&span) {
        let mut out = String::with_capacity(existing.len() - span.len());
        out.push_str(&existing[..position]);
        out.push('}');
        out.push_str(&existing[position + span.len() + 1..]);
        return Some(out);
    }
    None
}

/// Indent every line of `text` with `prefix`.
fn indent(text: &str, prefix: &str) -> String {
    let mut out = String::with_capacity(text.len() + prefix.len());
    for (index, line) in text.lines().enumerate() {
        if index > 0 {
            out.push('\n');
        }
        out.push_str(prefix);
        out.push_str(line);
    }
    out
}

/// Write a file by replacing it, never through a symlink.
///
/// The temporary name is unique (pid + counter) and created with
/// `create_new`, so it can never be a pre-existing file and two
/// concurrent installs cannot collide on it.
fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "opencode".to_string());
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        ".{file_name}.{}.{unique}.mnexis-tmp",
        std::process::id()
    ));
    {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        std::io::Write::write_all(&mut file, contents.as_bytes())?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, OpencodeConfig) {
        let dir = tempfile::tempdir().expect("tempdir");
        let config = OpencodeConfig::new(dir.path());
        (dir, config)
    }

    fn read(config: &OpencodeConfig) -> String {
        std::fs::read_to_string(config.config_path()).expect("config file")
    }

    #[test]
    fn install_creates_the_configuration_with_the_entry() {
        let (_dir, config) = scratch();
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Created);
        let value: Value = serde_json::from_str(&read(&config)).expect("json");
        assert_eq!(
            value["mcp"][MCP_SERVER_NAME]["type"].as_str(),
            Some("local")
        );
        assert!(value["mcp"][MCP_SERVER_NAME]["command"].is_array());
        assert_eq!(
            value["mcp"][MCP_SERVER_NAME]["enabled"].as_bool(),
            Some(true)
        );
    }

    #[test]
    fn install_is_idempotent() {
        let (_dir, config) = scratch();
        config.install().expect("install");
        let outcome = config.install().expect("install again");
        assert_eq!(outcome.action, ConfigAction::Unchanged);
    }

    #[test]
    fn install_keeps_the_other_keys_and_their_order() {
        let (_dir, config) = scratch();
        std::fs::create_dir_all(config.directory()).expect("dir");
        let original = "{\n  \"theme\": \"dark\",\n  \"model\": \"x\"\n}\n";
        std::fs::write(config.config_path(), original).expect("write");
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Updated);
        let text = read(&config);
        assert!(text.contains("\"theme\": \"dark\""));
        assert!(text.contains("\"model\": \"x\""));
        // The user's own bytes survive: the entry was inserted textually.
        assert!(text.contains("\"theme\": \"dark\","));
        let value: Value = serde_json::from_str(&text).expect("json");
        assert!(value["mcp"][MCP_SERVER_NAME].is_object());
    }

    #[test]
    fn install_refuses_a_jsonc_configuration() {
        let (_dir, config) = scratch();
        std::fs::create_dir_all(config.directory()).expect("dir");
        std::fs::write(config.jsonc_path(), "{\n  // a comment\n}\n").expect("write");
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert!(outcome.detail.contains("mcp"));
        // The JSONC file is untouched.
        assert!(
            std::fs::read_to_string(config.jsonc_path())
                .expect("jsonc")
                .contains("a comment")
        );
        assert!(!config.config_path().exists());
    }

    #[test]
    fn install_refuses_an_invalid_configuration() {
        let (_dir, config) = scratch();
        std::fs::create_dir_all(config.directory()).expect("dir");
        std::fs::write(config.config_path(), "{not json").expect("write");
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert_eq!(
            std::fs::read_to_string(config.config_path()).expect("file"),
            "{not json"
        );
    }

    #[test]
    fn uninstall_removes_only_the_entry() {
        let (_dir, config) = scratch();
        std::fs::create_dir_all(config.directory()).expect("dir");
        std::fs::write(
            config.config_path(),
            "{\n  \"theme\": \"dark\",\n  \"mcp\": {\n    \"other\": {\"type\": \"local\"}\n  }\n}\n",
        )
        .expect("write");
        config.install().expect("install");
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Removed);
        let value: Value = serde_json::from_str(&read(&config)).expect("json");
        assert!(value["mcp"]["other"].is_object());
        assert!(value["mcp"].get(MCP_SERVER_NAME).is_none());
        assert_eq!(value["theme"].as_str(), Some("dark"));
    }

    #[test]
    fn uninstall_restores_the_users_bytes_after_a_textual_install() {
        let (_dir, config) = scratch();
        std::fs::create_dir_all(config.directory()).expect("dir");
        let original = "{\n  \"theme\": \"dark\",\n  \"model\": \"x\"\n}\n";
        std::fs::write(config.config_path(), original).expect("write");
        config.install().expect("install");
        assert_ne!(read(&config), original);
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Removed);
        assert_eq!(read(&config), original);
    }

    #[test]
    fn uninstall_leaves_a_configuration_without_the_entry_alone() {
        let (_dir, config) = scratch();
        std::fs::create_dir_all(config.directory()).expect("dir");
        let original = "{\n  \"theme\": \"dark\"\n}\n";
        std::fs::write(config.config_path(), original).expect("write");
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Unchanged);
        assert_eq!(
            std::fs::read_to_string(config.config_path()).expect("file"),
            original
        );
    }

    #[test]
    fn is_configured_reports_the_entry() {
        let (_dir, config) = scratch();
        assert!(!config.is_configured());
        config.install().expect("install");
        assert!(config.is_configured());
    }
}
