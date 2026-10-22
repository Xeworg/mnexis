//! opencode's MCP configuration: the `mcp` entry that points an opencode
//! host at the mnexis MCP server.
//!
//! opencode reads its global configuration from
//! `~/.config/opencode/opencode.json`. The file belongs to the user, not to
//! mnexis, so this module never rewrites it blindly:
//!
//! - when the file does not exist, mnexis creates it with the entry;
//! - when the file exists and has no `mcp` member, the `"mcp": { "mnexis":
//!   { ... } }` member is inserted textually;
//! - when the file already has an `mcp` object, only the `"mnexis": { ... }`
//!   member is inserted inside it, textually;
//! - when the file is JSONC (opencode also reads `opencode.jsonc`), is not
//!   strict JSON, has a root that is not an object, has an `mcp` member that
//!   is not an object, is a symlink or is not writable by its owner, mnexis
//!   refuses to touch it and prints the block to add by hand.
//!
//! Every other byte of the file is preserved: its key order, its number
//! formatting, its duplicate keys and its own indentation. Nothing is ever
//! re-serialised, because a re-serialisation is a rewrite of the user's
//! document.
//!
//! Uninstalling removes only the entry mnexis writes today; an entry the
//! user wrote or edited is left alone.

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
    /// `$XDG_CONFIG_HOME`, when the environment sets it.
    config_home: Option<PathBuf>,
}

impl OpencodeConfig {
    /// Build a configuration handle resolving paths under `home`.
    pub fn new(home: impl Into<PathBuf>) -> Self {
        Self::with_config_home(home.into(), None)
    }

    /// Build a configuration handle with an explicit configuration home.
    fn with_config_home(home: PathBuf, config_home: Option<PathBuf>) -> Self {
        Self { home, config_home }
    }

    /// Build a configuration handle from the process environment.
    pub fn from_env() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let config_home = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        Self::with_config_home(home, config_home)
    }

    /// The directory opencode keeps its configuration in:
    /// `$XDG_CONFIG_HOME/opencode` when `XDG_CONFIG_HOME` is set,
    /// `$HOME/.config/opencode` otherwise.
    pub fn directory(&self) -> PathBuf {
        config_directory(&self.home, self.config_home.as_deref())
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
    /// server of the installed binary, expressed as the root member it is
    /// written as (`{"mcp": {"mnexis": ...}}`).
    pub fn mcp_entry(&self) -> Value {
        let mut root = Map::new();
        root.insert("mcp".to_string(), self.mcp_value());
        Value::Object(root)
    }

    /// The `mcp` object mnexis writes: the one `mnexis` server inside it.
    fn mcp_value(&self) -> Value {
        let mut mcp = Map::new();
        mcp.insert(MCP_SERVER_NAME.to_string(), self.server_object());
        Value::Object(mcp)
    }

    /// The server entry mnexis writes. Uninstalling compares the file's
    /// entry against exactly this value.
    fn server_object(&self) -> Value {
        let mut server = Map::new();
        server.insert("type".to_string(), Value::String("local".to_string()));
        server.insert("command".to_string(), Value::Array(self.server_command()));
        server.insert("enabled".to_string(), Value::Bool(true));
        Value::Object(server)
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

    /// The command as a JSON array literal, on one line.
    fn command_literal(&self) -> String {
        self.server_command()
            .iter()
            .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The snippet a user has to add by hand when mnexis refuses to write
    /// the configuration: the whole `mcp` member.
    pub fn manual_snippet(&self) -> String {
        let command = self.command_literal();
        format!(
            "  \"mcp\": {{\n    \"{MCP_SERVER_NAME}\": {{\n      \"type\": \"local\",\n      \"command\": [{command}],\n      \"enabled\": true\n    }}\n  }}"
        )
    }

    /// The snippet for a file whose `mcp` member is not an object: the
    /// server member that goes inside it once it is one.
    fn manual_server_snippet(&self) -> String {
        let command = self.command_literal();
        format!(
            "    \"{MCP_SERVER_NAME}\": {{\n      \"type\": \"local\",\n      \"command\": [{command}],\n      \"enabled\": true\n    }}"
        )
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
    ///
    /// The file is only ever changed by adding the one block mnexis owns.
    /// When it cannot be changed safely it is left byte-identical and the
    /// outcome carries the block to add by hand.
    pub fn install(&self) -> io::Result<ConfigOutcome> {
        let path = self.config_path();
        let jsonc = self.jsonc_path();
        if jsonc.exists() {
            return Ok(refused(
                &jsonc,
                format!(
                    "opencode's configuration is JSONC; mnexis did not touch it; add this to it by hand:\n{}",
                    self.manual_snippet()
                ),
            ));
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let mut text = serde_json::to_string_pretty(&self.mcp_entry())
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                text.push('\n');
                write_atomically(&path, &text)?;
                return Ok(ConfigOutcome {
                    path: path.clone(),
                    action: ConfigAction::Created,
                    detail: "created opencode's configuration with the mnexis MCP server"
                        .to_string(),
                });
            }
            Err(error) => return Err(error),
        };
        // A symlink belongs to the user's dotfiles: renaming over it would
        // replace the link and the file it points to would stop being the
        // live configuration.
        if metadata.file_type().is_symlink() {
            return Ok(refused(
                &path,
                format!(
                    "opencode's configuration is a symbolic link; mnexis never replaces a symlink, so the file it points to is untouched; add this to it by hand:\n{}",
                    self.manual_snippet()
                ),
            ));
        }
        let existing = match fs::read_to_string(&path) {
            Ok(text) => text,
            // A file that is not even UTF-8 is not valid strict JSON.
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                return Ok(refused(
                    &path,
                    format!(
                        "opencode's configuration is not valid strict JSON; mnexis did not touch it; add this to it by hand:\n{}",
                        self.manual_snippet()
                    ),
                ));
            }
            Err(error) => return Err(error),
        };
        let value: Value = match serde_json::from_str(&existing) {
            Ok(value) => value,
            Err(_) => {
                return Ok(refused(
                    &path,
                    format!(
                        "opencode's configuration is not valid strict JSON; mnexis did not touch it; add this to it by hand:\n{}",
                        self.manual_snippet()
                    ),
                ));
            }
        };
        let Some(root) = value.as_object() else {
            return Ok(refused(
                &path,
                format!(
                    "opencode's configuration root is {}, not a JSON object; mnexis did not touch it; add this to it by hand:\n{}",
                    value_kind(&value),
                    self.manual_snippet()
                ),
            ));
        };
        match root.get("mcp") {
            // Whatever that entry says, it is not mnexis's to overwrite.
            Some(Value::Object(mcp)) if mcp.contains_key(MCP_SERVER_NAME) => {
                return Ok(unchanged(
                    &path,
                    format!(
                        "opencode already runs the {MCP_SERVER_NAME} MCP server; left it as it is"
                    ),
                ));
            }
            Some(Value::Object(_)) => {}
            Some(other) => {
                return Ok(refused(
                    &path,
                    format!(
                        "opencode's configuration has an \"mcp\" member that is {}, not an object; mnexis did not touch it; make it an object and add this member to it by hand:\n{}",
                        value_kind(other),
                        self.manual_server_snippet()
                    ),
                ));
            }
            None => {}
        }
        // A read-only file is refused whatever this process may bypass: the
        // owner did not ask for the change, and a rename would grant one.
        if !owner_writable(&metadata) {
            return Ok(refused(
                &path,
                format!(
                    "opencode's configuration is not writable by its owner; mnexis did not touch it; make it writable and run the command again, or add this to it by hand:\n{}",
                    self.manual_snippet()
                ),
            ));
        }
        // The file is only ever changed by adding one textual block: the
        // `mnexis` member inside an existing `mcp` object, or the whole `mcp`
        // member when the file has none. Everything else keeps its bytes.
        let rewritten = if root.contains_key("mcp") {
            let member = member_text(MCP_SERVER_NAME, &self.server_object())
                .ok_or_else(|| textual_failure("the mcp member"))?;
            let mcp = root_member(&existing, "mcp")?;
            insert_member_textually(&existing, mcp.value_start, &member)
                .ok_or_else(|| textual_failure("the mcp member"))?
        } else {
            let member = member_text("mcp", &self.mcp_value())
                .ok_or_else(|| textual_failure("the root object"))?;
            let open =
                root_object_open(&existing).ok_or_else(|| textual_failure("the root object"))?;
            insert_member_textually(&existing, open, &member)
                .ok_or_else(|| textual_failure("the root object"))?
        };
        write_atomically(&path, &rewritten)?;
        Ok(ConfigOutcome {
            path,
            action: ConfigAction::Updated,
            detail: "added the mnexis MCP server to opencode's configuration".to_string(),
        })
    }

    /// Remove the entry mnexis added from opencode's configuration.
    ///
    /// Only the entry mnexis writes today is removed: an entry the user
    /// wrote or edited is left byte-identical, and so is every other byte of
    /// the file.
    pub fn uninstall(&self) -> io::Result<ConfigOutcome> {
        let path = self.config_path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(unchanged(
                    &path,
                    "opencode has no configuration file; nothing to remove".to_string(),
                ));
            }
            Err(error) => return Err(error),
        };
        if metadata.file_type().is_symlink() {
            return Ok(refused(
                &path,
                "opencode's configuration is a symbolic link; mnexis never replaces a symlink, so the file it points to is untouched; remove the \"mcp\".\"mnexis\" entry from the file you manage by hand".to_string(),
            ));
        }
        let existing = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                return Ok(refused(
                    &path,
                    "opencode's configuration is not valid JSON; left it as it is".to_string(),
                ));
            }
            Err(error) => return Err(error),
        };
        let value: Value = match serde_json::from_str(&existing) {
            Ok(value) => value,
            Err(_) => {
                return Ok(refused(
                    &path,
                    "opencode's configuration is not valid JSON; left it as it is".to_string(),
                ));
            }
        };
        let Some(server) = value.get("mcp").and_then(|mcp| mcp.get(MCP_SERVER_NAME)) else {
            return Ok(unchanged(
                &path,
                "opencode's configuration has no mnexis MCP server; left it as it is".to_string(),
            ));
        };
        // Only the entry mnexis writes today is mnexis's to remove; a
        // hand-written or edited one belongs to the user.
        if *server != self.server_object() {
            return Ok(unchanged(
                &path,
                format!(
                    "opencode's configuration has an \"mcp\".\"{MCP_SERVER_NAME}\" entry that was not written by mnexis; left it as it is"
                ),
            ));
        }
        if !owner_writable(&metadata) {
            return Ok(refused(
                &path,
                "opencode's configuration is not writable by its owner; mnexis did not touch it; make it writable and run the command again, or remove the \"mcp\".\"mnexis\" entry by hand".to_string(),
            ));
        }
        let rewritten =
            remove_mnexis_member(&existing).ok_or_else(|| textual_failure("the mnexis member"))?;
        write_atomically(&path, &rewritten)?;
        Ok(ConfigOutcome {
            path,
            action: ConfigAction::Removed,
            detail: "removed the mnexis MCP server from opencode's configuration".to_string(),
        })
    }
}

/// The directory opencode keeps its configuration in.
fn config_directory(home: &Path, config_home: Option<&Path>) -> PathBuf {
    match config_home {
        Some(path) => path.join("opencode"),
        None => home.join(".config").join("opencode"),
    }
}

/// A member of a JSON object, located in the text it was scanned from.
///
/// The offsets are byte offsets, so a caller can slice the exact bytes the
/// member covers and edit the text without re-serialising it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MemberSpan {
    /// The first byte of the key's opening quote.
    key_start: usize,
    /// One past the key's closing quote.
    key_end: usize,
    /// The first byte of the value.
    value_start: usize,
    /// One past the last byte of the value.
    value_end: usize,
    /// The byte of the comma that follows the member, when another member
    /// follows it.
    comma_start: Option<usize>,
}

/// Whether `byte` is JSON whitespace.
fn is_json_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// The first offset at or after `index` that is not whitespace.
fn skip_space(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && is_json_space(bytes[index]) {
        index += 1;
    }
    index
}

/// One past the last offset before `index` that is not whitespace.
fn skip_space_back(bytes: &[u8], mut index: usize) -> usize {
    while index > 0 && is_json_space(bytes[index - 1]) {
        index -= 1;
    }
    index
}

/// One past the closing quote of the JSON string that starts at `start`.
///
/// Escapes are skipped two bytes at a time: every escaped byte is a single
/// ASCII character (the `u` of a `\uXXXX` escape included), so no escape can
/// hide a quote.
fn scan_string(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// One past the last byte of the JSON value that starts at `start`.
fn scan_value(bytes: &[u8], start: usize) -> Option<usize> {
    match *bytes.get(start)? {
        b'"' => scan_string(bytes, start),
        b'{' | b'[' => scan_container(bytes, start),
        _ => {
            // A literal: a number, `true`, `false` or `null`.
            let mut index = start;
            while index < bytes.len()
                && !is_json_space(bytes[index])
                && !matches!(bytes[index], b',' | b'}' | b']')
            {
                index += 1;
            }
            (index > start).then_some(index)
        }
    }
}

/// One past the closing bracket of the object or array that starts at
/// `start`, with every nested brace, bracket and string skipped.
fn scan_container(bytes: &[u8], start: usize) -> Option<usize> {
    let first = *bytes.get(start)?;
    if closer_of(first) == first {
        return None;
    }
    let mut open = vec![first];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index = scan_string(bytes, index)?;
                continue;
            }
            opener @ (b'{' | b'[') => open.push(opener),
            closer @ (b'}' | b']') => {
                if closer != closer_of(*open.last()?) {
                    return None;
                }
                open.pop();
                if open.is_empty() {
                    return Some(index + 1);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

/// The bracket that closes `opener`; `opener` itself when it opens nothing.
fn closer_of(opener: u8) -> u8 {
    match opener {
        b'{' => b'}',
        b'[' => b']',
        other => other,
    }
}

/// The members of the object whose opening brace is at `open`, in the order
/// they appear in the text. Duplicate keys are kept as they are written.
fn object_members(text: &str, open: usize) -> Option<Vec<(String, MemberSpan)>> {
    let bytes = text.as_bytes();
    if bytes.get(open) != Some(&b'{') {
        return None;
    }
    let end = scan_container(bytes, open)?.checked_sub(1)?;
    let mut members = Vec::new();
    let mut index = skip_space(bytes, open + 1);
    while index < end {
        let key_start = index;
        let key_end = scan_string(bytes, key_start)?;
        let key: String = serde_json::from_str(text.get(key_start..key_end)?).ok()?;
        let colon = skip_space(bytes, key_end);
        if bytes.get(colon) != Some(&b':') {
            return None;
        }
        let value_start = skip_space(bytes, colon + 1);
        let value_end = scan_value(bytes, value_start)?;
        let next = skip_space(bytes, value_end);
        let boundary = *bytes.get(next)?;
        members.push((
            key,
            MemberSpan {
                key_start,
                key_end,
                value_start,
                value_end,
                comma_start: (boundary == b',').then_some(next),
            },
        ));
        match boundary {
            b',' => index = skip_space(bytes, next + 1),
            b'}' => break,
            _ => return None,
        }
    }
    Some(members)
}

/// The span of a member by key. A duplicate key resolves to its last
/// occurrence, which is the value a JSON parser keeps.
fn find_member<'a>(members: &'a [(String, MemberSpan)], key: &str) -> Option<&'a MemberSpan> {
    members
        .iter()
        .rev()
        .find(|(name, _)| name == key)
        .map(|(_, span)| span)
}

/// The offset of the root object's opening brace.
fn root_object_open(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let index = skip_space(bytes, 0);
    (bytes.get(index) == Some(&b'{')).then_some(index)
}

/// Locate a member of the root object in the file's own text.
fn root_member(text: &str, key: &str) -> io::Result<MemberSpan> {
    let open = root_object_open(text).ok_or_else(|| textual_failure("the root object"))?;
    let members = object_members(text, open).ok_or_else(|| textual_failure(key))?;
    find_member(&members, key)
        .copied()
        .ok_or_else(|| textual_failure(key))
}

/// Insert `member` as the last member of the object whose opening brace is
/// at `open`, keeping every other byte of the text.
fn insert_member_textually(text: &str, open: usize, member: &str) -> Option<String> {
    let members = object_members(text, open)?;
    let base = line_indent(text, open);
    let indented = indent(member.trim_end(), &format!("{base}  "));
    let (position, insertion) = match members.last() {
        Some((_, span)) => (span.value_end, format!(",\n{indented}")),
        None => (open + 1, format!("\n{indented}\n{base}")),
    };
    let mut out = String::with_capacity(text.len() + insertion.len());
    out.push_str(&text[..position]);
    out.push_str(&insertion);
    out.push_str(&text[position..]);
    Some(out)
}

/// Remove the member at `span` from the object whose opening brace is at
/// `open`, keeping every other byte of the text.
fn remove_member_textually(text: &str, open: usize, span: MemberSpan) -> Option<String> {
    let bytes = text.as_bytes();
    let close = scan_container(bytes, open)?.checked_sub(1)?;
    let mut out = String::with_capacity(text.len());
    let previous = skip_space_back(bytes, span.key_start);
    if previous > open + 1 && bytes.get(previous - 1) == Some(&b',') {
        // The member follows another one: drop the comma before it.
        out.push_str(&text[..previous - 1]);
        out.push_str(&text[span.value_end..]);
        return Some(out);
    }
    if let Some(comma) = span.comma_start {
        // The member is the first of several: drop it and the comma that
        // follows it.
        out.push_str(&text[..previous]);
        out.push_str(&text[comma + 1..]);
        return Some(out);
    }
    // The member is the only one: the object becomes `{}`, keeping its own
    // closing bracket and whatever surrounded it (the file is never
    // deleted).
    out.push_str(&text[..open + 1]);
    out.push_str(&text[close..]);
    Some(out)
}

/// Remove the `mcp.mnexis` member from the text, then the `mcp` member
/// itself when its object becomes empty. Every other byte survives.
fn remove_mnexis_member(text: &str) -> Option<String> {
    let root_open = root_object_open(text)?;
    let root_members = object_members(text, root_open)?;
    let mcp = *find_member(&root_members, "mcp")?;
    let mcp_members = object_members(text, mcp.value_start)?;
    let mnexis = *find_member(&mcp_members, MCP_SERVER_NAME)?;
    let without_entry = remove_member_textually(text, mcp.value_start, mnexis)?;
    let root_open = root_object_open(&without_entry)?;
    let root_members = object_members(&without_entry, root_open)?;
    let mcp = *find_member(&root_members, "mcp")?;
    if !object_members(&without_entry, mcp.value_start)?.is_empty() {
        return Some(without_entry);
    }
    remove_member_textually(&without_entry, root_open, mcp)
}

/// The leading whitespace of the line `index` is on.
fn line_indent(text: &str, index: usize) -> &str {
    let start = text[..index].rfind('\n').map_or(0, |line| line + 1);
    let rest = &text[start..];
    let end = rest
        .bytes()
        .position(|byte| byte != b' ' && byte != b'\t')
        .unwrap_or(rest.len());
    &rest[..end]
}

/// A member of an object as text: `"key": <pretty value>`, where the value
/// is rendered with two-space indentation and the caller indents the member
/// to the depth it is inserted at.
fn member_text(key: &str, value: &Value) -> Option<String> {
    let rendered = serde_json::to_string_pretty(value).ok()?;
    Some(format!("\"{key}\": {rendered}"))
}

/// How a JSON value is named in a report.
fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Whether the file's owner may write it. A read-only file is refused before
/// anything is written, whatever this process may bypass.
fn owner_writable(metadata: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o200 != 0
    }
    #[cfg(not(unix))]
    {
        !metadata.permissions().readonly()
    }
}

/// The outcome for a file mnexis will not touch.
fn refused(path: &Path, detail: String) -> ConfigOutcome {
    ConfigOutcome {
        path: path.to_path_buf(),
        action: ConfigAction::Refused,
        detail,
    }
}

/// The outcome for a file that needed no change.
fn unchanged(path: &Path, detail: String) -> ConfigOutcome {
    ConfigOutcome {
        path: path.to_path_buf(),
        action: ConfigAction::Unchanged,
        detail,
    }
}

/// The error for valid JSON the textual scanner could not follow. It means a
/// bug in this module, so it is reported instead of guessed at, and the file
/// is left as it is.
fn textual_failure(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "could not locate {what} in opencode's configuration textually; the file was left as it is"
        ),
    )
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
/// Write a file by replacing it, never through a symlink.
///
/// The new content is written into a temporary file created with
/// `create_new`, so it can never be a pre-existing file or symlink and two
/// concurrent installs cannot collide on it. The temporary file is given the
/// exact mode of the file it replaces (0644 for a new file) before it is
/// renamed over it, because a rename creates a new inode.
fn write_atomically(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let previous = fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.permissions());
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "opencode".to_string());
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_file_name(format!(
        ".{file_name}.{}.{unique}.mnexis-tmp",
        std::process::id()
    ));
    let write_result = (|| -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        std::io::Write::write_all(&mut file, contents.as_bytes())?;
        match previous {
            Some(permissions) => fs::set_permissions(&temporary, permissions)?,
            None => set_new_file_mode(&temporary)?,
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

/// Give a newly created file the usual mode, whatever the process umask is.
#[cfg(unix)]
fn set_new_file_mode(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o644))
}

/// Other platforms keep the mode the operating system gave the file.
#[cfg(not(unix))]
fn set_new_file_mode(_path: &Path) -> io::Result<()> {
    Ok(())
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

    fn write_config(config: &OpencodeConfig, text: &str) {
        std::fs::create_dir_all(config.directory()).expect("dir");
        std::fs::write(config.config_path(), text).expect("write");
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("chmod");
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::symlink_metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[cfg(unix)]
    fn is_symlink(path: &Path) -> bool {
        fs::symlink_metadata(path)
            .expect("metadata")
            .file_type()
            .is_symlink()
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

    #[test]
    fn install_refuses_a_configuration_that_is_not_utf8() {
        let (_dir, config) = scratch();
        let bytes = b"{\n  \"theme\": \"dark\",\n  \"note\": \"\xff\xfe\"\n}\n";
        write_config(&config, "");
        std::fs::write(config.config_path(), bytes).expect("write");
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert!(
            outcome.detail.contains("add this to it by hand"),
            "{}",
            outcome.detail
        );
        assert_eq!(std::fs::read(config.config_path()).expect("file"), bytes);
    }

    #[test]
    fn uninstall_refuses_a_configuration_that_is_not_utf8() {
        let (_dir, config) = scratch();
        let bytes = b"{\n  \"note\": \"\xff\xfe\"\n}\n";
        write_config(&config, "");
        std::fs::write(config.config_path(), bytes).expect("write");
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert_eq!(std::fs::read(config.config_path()).expect("file"), bytes);
    }

    // --- a root that is not an object is refused (B2) -------------------

    #[test]
    fn install_refuses_a_root_that_is_not_an_object() {
        for original in ["[]", "[1, 2, 3]", "\"a string\"", "42", "null"] {
            let (_dir, config) = scratch();
            write_config(&config, original);
            let outcome = config.install().expect("install");
            assert_eq!(outcome.action, ConfigAction::Refused, "root {original}");
            assert!(
                outcome.detail.contains("add this to it by hand"),
                "root {original}: {}",
                outcome.detail
            );
            assert!(
                outcome.detail.contains("\"mcp\""),
                "root {original}: {}",
                outcome.detail
            );
            assert_eq!(read(&config), original, "root {original}");
        }
    }

    // --- a non-object `mcp` member is refused (B3) ----------------------

    #[test]
    fn install_refuses_an_mcp_member_that_is_not_an_object() {
        for mcp in ["[]", "null", "\"text\"", "42"] {
            let original = format!("{{\n  \"theme\": \"dark\",\n  \"mcp\": {mcp}\n}}\n");
            let (_dir, config) = scratch();
            write_config(&config, &original);
            let outcome = config.install().expect("install");
            assert_eq!(outcome.action, ConfigAction::Refused, "mcp {mcp}");
            assert!(
                outcome.detail.contains("not an object"),
                "mcp {mcp}: {}",
                outcome.detail
            );
            assert!(
                outcome.detail.contains("\"mnexis\""),
                "mcp {mcp}: {}",
                outcome.detail
            );
            assert_eq!(read(&config), original, "mcp {mcp}");
        }
    }

    // --- a symlinked configuration is refused and stays a symlink (A1) ---

    #[cfg(unix)]
    #[test]
    fn install_refuses_a_symlinked_configuration() {
        let (dir, config) = scratch();
        let dotfiles = dir.path().join("dotfiles");
        std::fs::create_dir_all(&dotfiles).expect("dir");
        let target = dotfiles.join("opencode.json");
        let original = "{\n  \"theme\": \"dark\"\n}\n";
        std::fs::write(&target, original).expect("write");
        std::fs::create_dir_all(config.directory()).expect("dir");
        std::os::unix::fs::symlink(&target, config.config_path()).expect("link");
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert!(
            outcome.detail.contains("symbolic link"),
            "{}",
            outcome.detail
        );
        assert!(
            outcome.detail.contains("add this to it by hand"),
            "{}",
            outcome.detail
        );
        assert!(
            is_symlink(&config.config_path()),
            "the symlink must survive the refusal"
        );
        assert_eq!(std::fs::read_to_string(&target).expect("target"), original);
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_refuses_a_symlinked_configuration() {
        let (dir, config) = scratch();
        let dotfiles = dir.path().join("dotfiles");
        std::fs::create_dir_all(&dotfiles).expect("dir");
        let target = dotfiles.join("opencode.json");
        let entry = serde_json::to_string_pretty(&config.mcp_entry()).expect("entry");
        std::fs::write(&target, format!("{entry}\n")).expect("write");
        std::fs::create_dir_all(config.directory()).expect("dir");
        std::os::unix::fs::symlink(&target, config.config_path()).expect("link");
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert!(is_symlink(&config.config_path()), "{entry}");
        assert!(
            std::fs::read_to_string(&target)
                .expect("target")
                .contains(MCP_SERVER_NAME)
        );
    }

    // --- a read-only configuration is refused (A2) ----------------------

    #[cfg(unix)]
    #[test]
    fn install_refuses_a_read_only_configuration() {
        let (_dir, config) = scratch();
        let original = "{\n  \"theme\": \"dark\"\n}\n";
        write_config(&config, original);
        set_mode(&config.config_path(), 0o444);
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert!(
            outcome.detail.contains("not writable"),
            "{}",
            outcome.detail
        );
        assert_eq!(read(&config), original);
        assert_eq!(mode_of(&config.config_path()), 0o444);
    }

    #[cfg(unix)]
    #[test]
    fn uninstall_refuses_a_read_only_configuration() {
        let (_dir, config) = scratch();
        let entry = serde_json::to_string_pretty(&config.mcp_entry()).expect("entry");
        let original = format!("{entry}\n");
        write_config(&config, &original);
        set_mode(&config.config_path(), 0o444);
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Refused);
        assert!(
            outcome.detail.contains("not writable"),
            "{}",
            outcome.detail
        );
        assert_eq!(read(&config), original);
        assert_eq!(mode_of(&config.config_path()), 0o444);
    }

    // --- the file mode is preserved (A2) --------------------------------

    #[cfg(unix)]
    #[test]
    fn install_and_uninstall_preserve_the_file_mode() {
        let (_dir, config) = scratch();
        write_config(&config, "{\n  \"theme\": \"dark\"\n}\n");
        set_mode(&config.config_path(), 0o600);
        config.install().expect("install");
        assert_eq!(mode_of(&config.config_path()), 0o600, "after install");
        config.uninstall().expect("uninstall");
        assert_eq!(mode_of(&config.config_path()), 0o600, "after uninstall");
    }

    #[cfg(unix)]
    #[test]
    fn install_creates_a_new_file_with_the_usual_mode() {
        let (_dir, config) = scratch();
        config.install().expect("install");
        assert_eq!(mode_of(&config.config_path()), 0o644);
    }

    // --- a user's own `mcp.mnexis` entry is never overwritten (S5.3) ----

    #[test]
    fn install_does_not_overwrite_the_users_own_entry() {
        for entry in [
            "{\"type\": \"remote\", \"url\": \"https://example.test/mcp\"}",
            "\"text\"",
            "null",
        ] {
            let original = format!("{{\n  \"mcp\": {{\n    \"mnexis\": {entry}\n  }}\n}}\n");
            let (_dir, config) = scratch();
            write_config(&config, &original);
            let outcome = config.install().expect("install");
            assert_eq!(outcome.action, ConfigAction::Unchanged, "entry {entry}");
            assert!(
                outcome.detail.contains("left it as it is"),
                "entry {entry}: {}",
                outcome.detail
            );
            assert_eq!(read(&config), original, "entry {entry}");
        }
    }

    // --- installing into an existing `mcp` object edits one member only -

    #[test]
    fn install_inserts_only_the_mnexis_member_into_an_existing_mcp_object() {
        let user_member =
            "    \"other\": {\"type\": \"local\", \"command\": [\"other\"], \"enabled\": false}";
        let original =
            format!("{{\n  \"theme\": \"dark\",\n  \"mcp\": {{\n{user_member}\n  }}\n}}\n");
        let (_dir, config) = scratch();
        write_config(&config, &original);
        let outcome = config.install().expect("install");
        assert_eq!(outcome.action, ConfigAction::Updated);
        let text = read(&config);
        assert!(
            text.contains(user_member),
            "the user's own member must keep its bytes: {text}"
        );
        assert!(
            text.contains("  \"theme\": \"dark\",\n"),
            "the user's own formatting must survive: {text}"
        );
        let value: Value = serde_json::from_str(&text).expect("json");
        assert!(value["mcp"]["other"].is_object());
        assert!(value["mcp"][MCP_SERVER_NAME].is_object());
    }

    // --- install + uninstall is a byte-identical round trip (A4) --------

    #[test]
    fn install_and_uninstall_round_trip_preserves_the_users_bytes() {
        let documents = [
            "{\n  \"theme\": \"dark\",\n  \"model\": \"x\"\n}\n",
            "{\n    \"mcp\": {\n        \"other\": {\"type\": \"local\", \"command\": [\"other\"], \"enabled\": false}\n    },\n    \"theme\": \"dark\"\n}\n",
            "{\n  \"note\": \"a } brace, a , comma, a \\\" quote and a \\\\ slash\",\n  \"list\": [\"}\", \"]\"],\n  \"mcp\": {\"other\": {}}\n}\n",
            "{}",
            "{}\n",
            "{\"a\":1,\"mcp\":{\"other\":{\"k\":[1,2,{\"z\":null}]}}}",
        ];
        for original in documents {
            let (_dir, config) = scratch();
            write_config(&config, original);
            let installed = config.install().expect("install");
            assert_eq!(installed.action, ConfigAction::Updated, "{original}");
            assert!(
                config.is_configured(),
                "the entry must be there: {original}"
            );
            let removed = config.uninstall().expect("uninstall");
            assert_eq!(removed.action, ConfigAction::Removed, "{original}");
            assert_eq!(read(&config), original, "round trip of {original}");
        }
    }

    #[test]
    fn install_and_uninstall_round_trip_preserves_a_big_number_and_a_duplicate_key() {
        let big = "123456789012345678901234567890";
        let original = format!(
            "{{\n  \"big\": {big},\n  \"theme\": \"dark\",\n  \"theme\": \"light\",\n  \"mcp\": {{\n    \"other\": {{\"type\": \"local\"}}\n  }}\n}}\n"
        );
        let (_dir, config) = scratch();
        write_config(&config, &original);
        config.install().expect("install");
        let installed = read(&config);
        assert!(
            installed.contains(big),
            "the big number must keep its digits: {installed}"
        );
        assert_eq!(
            installed.matches("\"theme\"").count(),
            2,
            "the duplicate key must survive: {installed}"
        );
        config.uninstall().expect("uninstall");
        assert_eq!(read(&config), original);
    }

    // --- uninstall removes only what mnexis wrote (B1) -------------------

    #[test]
    fn uninstall_keeps_a_hand_written_mnexis_entry() {
        let original = "{\n  \"mcp\": {\n    \"mnexis\": {\n      \"type\": \"local\",\n      \"command\": [\"/usr/local/bin/other\", \"serve\"],\n      \"enabled\": true\n    }\n  }\n}\n";
        let (_dir, config) = scratch();
        write_config(&config, original);
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Unchanged);
        assert!(
            outcome.detail.contains("not written by mnexis"),
            "{}",
            outcome.detail
        );
        assert_eq!(read(&config), original);
    }

    #[test]
    fn uninstall_keeps_an_entry_the_user_edited() {
        let (_dir, config) = scratch();
        write_config(&config, "{\n  \"theme\": \"dark\"\n}\n");
        config.install().expect("install");
        let edited = read(&config).replace("\"enabled\": true", "\"enabled\": false");
        write_config(&config, &edited);
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Unchanged);
        assert!(
            outcome.detail.contains("not written by mnexis"),
            "{}",
            outcome.detail
        );
        assert_eq!(read(&config), edited);
    }

    #[test]
    fn uninstall_leaves_the_root_as_an_empty_object() {
        for original in ["{}", "{}\n"] {
            let (_dir, config) = scratch();
            write_config(&config, original);
            config.install().expect("install");
            let outcome = config.uninstall().expect("uninstall");
            assert_eq!(outcome.action, ConfigAction::Removed, "{original}");
            assert_eq!(read(&config), original, "with original {original}");
        }
    }

    #[test]
    fn uninstall_removes_the_mcp_member_when_its_object_becomes_empty() {
        let (_dir, config) = scratch();
        write_config(&config, "{\n  \"theme\": \"dark\",\n  \"mcp\": {}\n}\n");
        config.install().expect("install");
        let outcome = config.uninstall().expect("uninstall");
        assert_eq!(outcome.action, ConfigAction::Removed);
        let value: Value = serde_json::from_str(&read(&config)).expect("json");
        assert_eq!(value["theme"].as_str(), Some("dark"));
        assert!(
            value.get("mcp").is_none(),
            "the empty mcp member must go: {value}"
        );
    }

    // --- the scanner ----------------------------------------------------

    /// A document whose values hold the characters the scanner must not
    /// mistake for structure: braces, brackets, quotes, commas and escapes.
    const SCANNER_DOCUMENT: &str = r#"{
  "note": "a { brace, a } brace, a \"quote\", a comma, and a \\ slash",
  "nested": {"a": {"b": [1, 2, {"c": "}"}]}},
  "mcp": {
    "other": {"type": "local", "command": ["x", "serve"]}
  }
}
"#;

    #[test]
    fn scanner_finds_every_member_of_the_root_object() {
        let open = root_object_open(SCANNER_DOCUMENT).expect("root object");
        let members = object_members(SCANNER_DOCUMENT, open).expect("members");
        let keys: Vec<&str> = members.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["note", "nested", "mcp"]);

        let (_, note) = members.iter().find(|(key, _)| key == "note").expect("note");
        assert_eq!(
            &SCANNER_DOCUMENT[note.value_start..note.value_end],
            "\"a { brace, a } brace, a \\\"quote\\\", a comma, and a \\\\ slash\""
        );
        assert_eq!(&SCANNER_DOCUMENT[note.key_start..note.key_end], "\"note\"");
        // The comma that follows a member is part of its span; the last
        // member has none.
        let comma = note.comma_start.expect("the comma after the member");
        assert_eq!(&SCANNER_DOCUMENT[comma..comma + 1], ",");

        let (_, mcp) = members.iter().find(|(key, _)| key == "mcp").expect("mcp");
        assert!(mcp.comma_start.is_none(), "the last member has no comma");
        assert_eq!(
            &SCANNER_DOCUMENT[mcp.value_start..mcp.value_end],
            "{\n    \"other\": {\"type\": \"local\", \"command\": [\"x\", \"serve\"]}\n  }"
        );
    }

    #[test]
    fn scanner_descends_into_a_nested_object() {
        let open = root_object_open(SCANNER_DOCUMENT).expect("root object");
        let members = object_members(SCANNER_DOCUMENT, open).expect("members");
        let mcp = *find_member(&members, "mcp").expect("mcp");
        let inner = object_members(SCANNER_DOCUMENT, mcp.value_start).expect("mcp members");
        let keys: Vec<&str> = inner.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(keys, ["other"]);
        let (_, other) = &inner[0];
        assert_eq!(
            &SCANNER_DOCUMENT[other.value_start..other.value_end],
            "{\"type\": \"local\", \"command\": [\"x\", \"serve\"]}"
        );
    }

    #[test]
    fn scanner_reports_no_members_for_an_empty_object() {
        for text in ["{}", "{  }", "{\n}"] {
            assert!(
                object_members(text, 0).expect("members").is_empty(),
                "{text}"
            );
        }
    }

    #[test]
    fn scanner_keeps_duplicate_keys_and_the_last_one_wins() {
        let text = "{\"mcp\": {\"other\": {}}, \"mcp\": {\"second\": {}}}";
        let open = root_object_open(text).expect("root object");
        let members = object_members(text, open).expect("members");
        assert_eq!(members.len(), 2);
        let span = find_member(&members, "mcp").expect("mcp");
        assert_eq!(&text[span.value_start..span.value_end], "{\"second\": {}}");
    }

    // --- the configuration directory ------------------------------------

    #[test]
    fn directory_follows_xdg_config_home() {
        let config = OpencodeConfig::with_config_home(
            PathBuf::from("/home/user"),
            Some(PathBuf::from("/xdg/config")),
        );
        assert_eq!(config.directory(), PathBuf::from("/xdg/config/opencode"));
        assert_eq!(
            config.config_path(),
            PathBuf::from("/xdg/config/opencode/opencode.json")
        );
        assert_eq!(
            config.jsonc_path(),
            PathBuf::from("/xdg/config/opencode/opencode.jsonc")
        );
    }

    #[test]
    fn directory_defaults_to_the_home_config_directory() {
        let config = OpencodeConfig::new("/home/user");
        assert_eq!(
            config.directory(),
            PathBuf::from("/home/user/.config/opencode")
        );
    }
}
