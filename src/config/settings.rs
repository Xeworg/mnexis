//! Profile settings: the closed `mnexis.toml` schema (e2-core §6/§7).
//!
//! Loads the process [`AccessScope`] and service limits from the profile
//! data directory. When the file is absent the permissive default scope
//! from the design applies (`allow_global = true`, all operations,
//! namespace `default`), so a bare `mnexis init` works with no config
//! editing. Nothing here touches stored memories.

use std::path::Path;

use crate::application::ServiceConfig;
use crate::domain::error::MnexisError;
use crate::domain::scope::{AccessScope, ExistenceDisclosure, Operation};

/// A parsed TOML-subset value (what the writer emits, the reader accepts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TomlValue {
    Str(String),
    Bool(bool),
    Int(i64),
    StrList(Vec<String>),
}

/// Effective profile settings. Field names are the `mnexis.toml` keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileSettings {
    pub default_namespace: String,
    pub read_namespaces: Vec<String>,
    pub write_namespaces: Vec<String>,
    pub allow_global: bool,
    /// Reads are open across namespaces; `global` stays reserved.
    pub allow_read_all: bool,
    pub existence_disclosure: ExistenceDisclosure,
    /// `["all"]` expands to every [`Operation`] variant.
    pub allowed_operations: Vec<String>,
    pub max_content_bytes: Option<usize>,
    pub max_query_bytes: Option<usize>,
    pub max_tags: Option<usize>,
    pub max_topic_key_bytes: Option<usize>,
    pub default_retention_days: Option<i64>,
    pub idempotency_retention_days: Option<i64>,
}

impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            default_namespace: "default".to_string(),
            read_namespaces: vec!["default".to_string()],
            write_namespaces: vec!["default".to_string()],
            allow_global: false,
            allow_read_all: true,
            existence_disclosure: ExistenceDisclosure::NotFound,
            allowed_operations: vec!["all".to_string()],
            max_content_bytes: None,
            max_query_bytes: None,
            max_tags: None,
            max_topic_key_bytes: None,
            default_retention_days: None,
            idempotency_retention_days: None,
        }
    }
}

impl ProfileSettings {
    /// The base a profile *file* is parsed on top of. It differs from
    /// [`ProfileSettings::default`] (what `init` writes) in the two
    /// permission flags: a file that does not mention them keeps the
    /// permissions closed, so an existing profile never opens silently.
    fn file_base() -> Self {
        Self {
            allow_global: false,
            allow_read_all: false,
            ..Self::default()
        }
    }

    /// Parse the closed TOML subset. Unknown keys are rejected so typos
    /// fail loudly instead of silently changing the effective scope.
    pub fn from_toml(src: &str) -> Result<Self, MnexisError> {
        let mut settings = Self::file_base();
        for (lineno, line) in src.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| invalid_line(lineno, "expected `key = value`"))?;
            let key = key.trim();
            let value = parse_value(value.trim(), lineno)?;
            settings.apply(key, value, lineno)?;
        }
        Ok(settings)
    }

    fn apply(&mut self, key: &str, value: TomlValue, lineno: usize) -> Result<(), MnexisError> {
        let type_err = |expected: &str| {
            Err(MnexisError::invalid_argument(
                "mnexis.toml",
                format!("line {}: `{key}` expects {expected}", lineno + 1),
            ))
        };
        match key {
            "default_namespace" => match value {
                TomlValue::Str(s) => {
                    self.default_namespace = s;
                    Ok(())
                }
                _ => type_err("a string"),
            },
            "read_namespaces" => match value {
                TomlValue::StrList(v) => {
                    self.read_namespaces = v;
                    Ok(())
                }
                _ => type_err("an array of strings"),
            },
            "write_namespaces" => match value {
                TomlValue::StrList(v) => {
                    self.write_namespaces = v;
                    Ok(())
                }
                _ => type_err("an array of strings"),
            },
            "allow_global" => match value {
                TomlValue::Bool(b) => {
                    self.allow_global = b;
                    Ok(())
                }
                _ => type_err("a boolean"),
            },
            "allow_read_all" => match value {
                TomlValue::Bool(b) => {
                    self.allow_read_all = b;
                    Ok(())
                }
                _ => type_err("a boolean"),
            },
            "existence_disclosure" => match value {
                TomlValue::Str(s) => {
                    self.existence_disclosure = parse_disclosure(&s, lineno)?;
                    Ok(())
                }
                _ => type_err("`not_found` or `forbid`"),
            },
            "allowed_operations" => match value {
                TomlValue::StrList(v) => {
                    for op in &v {
                        if op != "all" {
                            parse_operation(op, lineno)?;
                        }
                    }
                    self.allowed_operations = v;
                    Ok(())
                }
                _ => type_err("an array of strings"),
            },
            "max_content_bytes" => {
                Self::apply_usize(&mut self.max_content_bytes, key, value, lineno)
            }
            "max_query_bytes" => Self::apply_usize(&mut self.max_query_bytes, key, value, lineno),
            "max_tags" => Self::apply_usize(&mut self.max_tags, key, value, lineno),
            "max_topic_key_bytes" => {
                Self::apply_usize(&mut self.max_topic_key_bytes, key, value, lineno)
            }
            "default_retention_days" => match value {
                TomlValue::Int(v) => {
                    self.default_retention_days = Some(v);
                    Ok(())
                }
                _ => type_err("an integer"),
            },
            "idempotency_retention_days" => match value {
                TomlValue::Int(v) => {
                    self.idempotency_retention_days = Some(v);
                    Ok(())
                }
                _ => type_err("an integer"),
            },
            other => Err(MnexisError::invalid_argument(
                "mnexis.toml",
                format!("line {}: unknown key `{other}`", lineno + 1),
            )),
        }
    }

    fn apply_usize(
        slot: &mut Option<usize>,
        key: &str,
        value: TomlValue,
        lineno: usize,
    ) -> Result<(), MnexisError> {
        match value {
            TomlValue::Int(v) if v >= 0 => {
                *slot = Some(v as usize);
                Ok(())
            }
            TomlValue::Int(_) => Err(MnexisError::invalid_argument(
                "mnexis.toml",
                format!("line {}: `{key}` must be >= 0", lineno + 1),
            )),
            _ => Err(MnexisError::invalid_argument(
                "mnexis.toml",
                format!("line {}: `{key}` expects an integer", lineno + 1),
            )),
        }
    }

    /// Serialize to the same closed subset. `init` writes this when the
    /// profile has no `mnexis.toml` yet.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# mnexis profile settings (closed flat schema)\n");
        out.push_str("# existence_disclosure: not_found | forbid\n");
        out.push_str("# allowed_operations: [\"all\"] or a list of operation names\n");
        out.push_str(&format!(
            "default_namespace = {}\n",
            quote(&self.default_namespace)
        ));
        out.push_str(&format!(
            "read_namespaces = {}\n",
            quote_list(&self.read_namespaces)
        ));
        out.push_str(&format!(
            "write_namespaces = {}\n",
            quote_list(&self.write_namespaces)
        ));
        out.push_str(&format!("allow_global = {}\n", self.allow_global));
        out.push_str(&format!("allow_read_all = {}\n", self.allow_read_all));
        out.push_str(&format!(
            "existence_disclosure = {}\n",
            quote(match self.existence_disclosure {
                ExistenceDisclosure::NotFound => "not_found",
                ExistenceDisclosure::Forbid => "forbid",
            })
        ));
        out.push_str(&format!(
            "allowed_operations = {}\n",
            quote_list(&self.allowed_operations)
        ));
        let defaults = ServiceConfig::default();
        out.push_str(&format!(
            "max_content_bytes = {}\n",
            self.max_content_bytes.unwrap_or(defaults.max_content_bytes)
        ));
        out.push_str(&format!(
            "max_query_bytes = {}\n",
            self.max_query_bytes.unwrap_or(defaults.max_query_bytes)
        ));
        out.push_str(&format!(
            "max_tags = {}\n",
            self.max_tags.unwrap_or(defaults.max_tags)
        ));
        out.push_str(&format!(
            "max_topic_key_bytes = {}\n",
            self.max_topic_key_bytes
                .unwrap_or(defaults.max_topic_key_bytes)
        ));
        out.push_str(&format!(
            "default_retention_days = {}\n",
            self.default_retention_days
                .unwrap_or(defaults.default_retention_days)
        ));
        out.push_str(&format!(
            "idempotency_retention_days = {}\n",
            self.idempotency_retention_days
                .unwrap_or(defaults.idempotency_retention_days)
        ));
        out
    }

    /// Add a namespace to the read and write allowlists (project-bindings
    /// S3). Returns whether anything changed. The allowlists only ever
    /// grow: this command authorizes, it never revokes.
    pub fn allow_namespace(&mut self, namespace: &str) -> bool {
        let mut changed = false;
        if !self.read_namespaces.iter().any(|n| n == namespace) {
            self.read_namespaces.push(namespace.to_string());
            changed = true;
        }
        if !self.write_namespaces.iter().any(|n| n == namespace) {
            self.write_namespaces.push(namespace.to_string());
            changed = true;
        }
        changed
    }

    /// Write the settings to `path` atomically.
    pub fn save_to(&self, path: &Path) -> Result<(), MnexisError> {
        crate::config::write_atomic(path, &self.to_toml())
    }

    /// Load settings from `path`, falling back to the permissive default
    /// when the file does not exist (a profile being created by `init`,
    /// or an operator who never wrote one). Returns the settings and
    /// whether the fallback was used.
    pub fn load_or_default(path: &Path) -> Result<(Self, bool), MnexisError> {
        match std::fs::read_to_string(path) {
            Ok(src) => Ok((Self::from_toml(&src)?, false)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Self::default(), true)),
            Err(e) => Err(MnexisError::invalid_argument(
                "mnexis.toml",
                format!("cannot read config: {e}"),
            )),
        }
    }

    /// Expand to the immutable process [`AccessScope`]. `["all"]` becomes
    /// every operation; anything else is parsed name by name.
    pub fn to_access_scope(&self) -> Result<AccessScope, MnexisError> {
        let ops = if self.allowed_operations.iter().any(|o| o == "all") {
            ALL_OPERATIONS.to_vec()
        } else {
            self.allowed_operations
                .iter()
                .map(|o| parse_operation(o, 0).copied())
                .collect::<Result<Vec<Operation>, MnexisError>>()?
        };
        Ok(AccessScope {
            read_namespaces: self.read_namespaces.clone(),
            write_namespaces: self.write_namespaces.clone(),
            default_namespace: self.default_namespace.clone(),
            allow_global: self.allow_global,
            allow_read_all: self.allow_read_all,
            allowed_operations: ops,
            existence_disclosure: self.existence_disclosure,
        })
    }

    /// Service limits with unset fields falling back to the defaults.
    pub fn to_service_config(&self) -> ServiceConfig {
        let defaults = ServiceConfig::default();
        ServiceConfig {
            max_content_bytes: self.max_content_bytes.unwrap_or(defaults.max_content_bytes),
            max_query_bytes: self.max_query_bytes.unwrap_or(defaults.max_query_bytes),
            max_tags: self.max_tags.unwrap_or(defaults.max_tags),
            max_keywords: defaults.max_keywords,
            max_topic_key_bytes: self
                .max_topic_key_bytes
                .unwrap_or(defaults.max_topic_key_bytes),
            default_retention_days: self
                .default_retention_days
                .unwrap_or(defaults.default_retention_days),
            idempotency_retention_days: self
                .idempotency_retention_days
                .unwrap_or(defaults.idempotency_retention_days),
        }
    }
}

const ALL_OPERATIONS: &[Operation] = &[
    Operation::Save,
    Operation::Get,
    Operation::List,
    Operation::Update,
    Operation::History,
    Operation::Supersede,
    Operation::SoftDelete,
    Operation::Restore,
    Operation::Purge,
    Operation::Export,
    Operation::Import,
    Operation::Reindex,
    Operation::Search,
    Operation::Context,
    Operation::Init,
    Operation::Doctor,
    Operation::ConfigShow,
    Operation::McpServe,
    Operation::Eval,
];

fn parse_operation(name: &str, lineno: usize) -> Result<&'static Operation, MnexisError> {
    let op = match name {
        "save" => &Operation::Save,
        "get" => &Operation::Get,
        "list" => &Operation::List,
        "update" => &Operation::Update,
        "history" => &Operation::History,
        "supersede" => &Operation::Supersede,
        "soft_delete" => &Operation::SoftDelete,
        "restore" => &Operation::Restore,
        "purge" => &Operation::Purge,
        "export" => &Operation::Export,
        "import" => &Operation::Import,
        "reindex" => &Operation::Reindex,
        "search" => &Operation::Search,
        "context" => &Operation::Context,
        "init" => &Operation::Init,
        "doctor" => &Operation::Doctor,
        "config_show" => &Operation::ConfigShow,
        "mcp_serve" => &Operation::McpServe,
        "eval" => &Operation::Eval,
        other => {
            return Err(MnexisError::invalid_argument(
                "mnexis.toml",
                format!(
                    "line {}: unknown operation `{other}` in allowed_operations",
                    lineno + 1
                ),
            ));
        }
    };
    Ok(op)
}

fn parse_disclosure(s: &str, lineno: usize) -> Result<ExistenceDisclosure, MnexisError> {
    match s {
        "not_found" => Ok(ExistenceDisclosure::NotFound),
        "forbid" => Ok(ExistenceDisclosure::Forbid),
        other => Err(MnexisError::invalid_argument(
            "mnexis.toml",
            format!(
                "line {}: unknown existence_disclosure `{other}`",
                lineno + 1
            ),
        )),
    }
}

fn parse_value(src: &str, lineno: usize) -> Result<TomlValue, MnexisError> {
    if let Some(body) = src.strip_prefix('"') {
        let end = body.find('"').ok_or_else(|| {
            MnexisError::invalid_argument(
                "mnexis.toml",
                format!("line {}: unterminated string", lineno + 1),
            )
        })?;
        return Ok(TomlValue::Str(body[..end].to_string()));
    }
    if src.starts_with('[') {
        let inner = src
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or_else(|| {
                MnexisError::invalid_argument(
                    "mnexis.toml",
                    format!("line {}: malformed array", lineno + 1),
                )
            })?;
        let mut items = Vec::new();
        for part in inner.split(',') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            match parse_value(part, lineno)? {
                TomlValue::Str(s) => items.push(s),
                _ => {
                    return Err(MnexisError::invalid_argument(
                        "mnexis.toml",
                        format!("line {}: arrays hold strings only", lineno + 1),
                    ));
                }
            }
        }
        return Ok(TomlValue::StrList(items));
    }
    match src {
        "true" => Ok(TomlValue::Bool(true)),
        "false" => Ok(TomlValue::Bool(false)),
        digits => digits
            .parse::<i64>()
            .map(TomlValue::Int)
            .map_err(|_| invalid_line(lineno, "unsupported value")),
    }
}

fn invalid_line(lineno: usize, why: &str) -> MnexisError {
    MnexisError::invalid_argument("mnexis.toml", format!("line {}: {why}", lineno + 1))
}

fn quote(s: &str) -> String {
    format!("\"{s}\"")
}

fn quote_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| quote(s)).collect();
    format!("[{}]", quoted.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip_preserves_every_field() {
        let src = ProfileSettings::default().to_toml();
        let parsed = ProfileSettings::from_toml(&src).unwrap();
        // The writer materialises effective limits, so compare the derived
        // values (what the process actually runs with), not Option-ness.
        assert_eq!(
            parsed.to_access_scope().unwrap(),
            ProfileSettings::default().to_access_scope().unwrap()
        );
        assert_eq!(parsed.max_content_bytes, Some(16_384));
        assert_eq!(parsed.max_query_bytes, Some(8_192));
        assert_eq!(parsed.max_tags, Some(32));
        assert_eq!(parsed.default_retention_days, Some(30));
        assert_eq!(parsed.default_namespace, "default");
        assert_eq!(parsed.allowed_operations, vec!["all".to_string()]);
    }

    #[test]
    fn default_scope_is_permissive_all_operations() {
        let scope = ProfileSettings::default().to_access_scope().unwrap();
        assert!(!scope.allow_global);
        assert_eq!(scope.default_namespace, "default");
        assert_eq!(scope.allowed_operations.len(), ALL_OPERATIONS.len());
        assert!(scope.allows(crate::domain::scope::Operation::Save));
        assert!(scope.allows(crate::domain::scope::Operation::Eval));
    }

    #[test]
    fn default_profile_keeps_global_closed_and_reads_open() {
        let settings = ProfileSettings::default();
        assert!(!settings.allow_global);
        assert!(settings.allow_read_all);
        let scope = settings.to_access_scope().unwrap();
        assert!(!scope.allow_global);
        assert!(scope.allow_read_all);
        assert!(scope.can_read("otro-proyecto"));
        assert!(!scope.can_read("global"));
        assert!(!scope.can_write("otro-proyecto"));
    }

    #[test]
    fn a_profile_without_the_keys_keeps_reads_scoped() {
        // A profile written before `allow_read_all` existed must not open
        // its reads silently: the human authorizes it explicitly.
        let src = "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\"]\n";
        let s = ProfileSettings::from_toml(src).unwrap();
        assert!(!s.allow_read_all);
        assert!(!s.allow_global);
        let scope = s.to_access_scope().unwrap();
        assert!(!scope.can_read("otro-proyecto"));
    }

    #[test]
    fn allow_read_all_parses_and_round_trips() {
        let s = ProfileSettings::from_toml("allow_read_all = false\n").unwrap();
        assert!(!s.allow_read_all);
        assert!(s.to_toml().contains("allow_read_all = false"));
        let s = ProfileSettings::from_toml("allow_read_all = true\n").unwrap();
        assert!(s.allow_read_all);
        assert!(s.to_toml().contains("allow_read_all = true"));
    }

    #[test]
    fn restricted_scope_parses_and_expands() {
        let src = "default_namespace = \"ns-a\"\nread_namespaces = [\"ns-a\"]\nwrite_namespaces = [\"ns-a\"]\nallow_global = false\nexistence_disclosure = \"forbid\"\nallowed_operations = [\"save\", \"get\"]\n";
        let s = ProfileSettings::from_toml(src).unwrap();
        let scope = s.to_access_scope().unwrap();
        assert!(!scope.allow_global);
        assert!(scope.allows(Operation::Save));
        assert!(!scope.allows(Operation::Purge));
        assert_eq!(scope.existence_disclosure, ExistenceDisclosure::Forbid);
    }

    #[test]
    fn unknown_keys_and_values_are_rejected() {
        let bad_key = "bogus_key = true\n";
        assert!(ProfileSettings::from_toml(bad_key).is_err());
        let bad_op = "allowed_operations = [\"save\", \"nope\"]\n";
        assert!(ProfileSettings::from_toml(bad_op).is_err());
        let bad_type = "allow_global = \"yes\"\n";
        assert!(ProfileSettings::from_toml(bad_type).is_err());
        let bad_syntax = "no_equals_sign_here\n";
        assert!(ProfileSettings::from_toml(bad_syntax).is_err());
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let src = "# a comment\n\nallow_global = false\n# trailing\n";
        let s = ProfileSettings::from_toml(src).unwrap();
        assert!(!s.allow_global);
    }

    #[test]
    fn load_or_default_falls_back_when_file_missing() {
        let dir = tempfile::tempdir_in("target").unwrap();
        let (settings, fell_back) =
            ProfileSettings::load_or_default(&dir.path().join("mnexis.toml")).unwrap();
        assert!(fell_back);
        assert_eq!(settings, ProfileSettings::default());
    }

    #[test]
    fn service_config_overrides_unset_fields_with_defaults() {
        let s = ProfileSettings {
            max_tags: Some(4),
            ..ProfileSettings::default()
        };
        let cfg = s.to_service_config();
        assert_eq!(cfg.max_tags, 4);
        assert_eq!(cfg.max_content_bytes, 16_384);
    }
}
