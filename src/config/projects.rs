//! Project bindings: which namespace a workspace writes into
//! (project-bindings S2).
//!
//! The registry lives next to the profile settings (`projects.toml` in the
//! data directory) and maps a **workspace identity** (the key) to one
//! namespace. The identity is computed by the agent (a git remote, a
//! repository root, or a canonical path), so this module never needs to
//! know about git: it only stores and enforces the mapping.
//!
//! Stability is the invariant. A key that is already bound keeps its
//! namespace unless the caller forces the change, because silently moving
//! a workspace to another namespace would split its memories in two.

use std::path::{Path, PathBuf};

use serde_json::json;

use crate::config::write_atomic;
use crate::domain::error::MnexisError;

/// The registry file inside a profile data directory.
///
/// Every reader and writer resolves the path through this function, so the
/// commands that write bindings and the workspace guard that reads them can
/// never disagree about where the bindings live.
pub fn registry_path(data_dir: &Path) -> PathBuf {
    data_dir.join("projects.toml")
}

/// One workspace → namespace binding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectBinding {
    /// Workspace identity: `git:<remote>`, or `path:<absolute path>`.
    pub key: String,
    /// The namespace the agent writes into for this workspace.
    pub namespace: String,
    /// Where the binding was recorded (informational).
    pub path: Option<String>,
}

/// What a `bind` call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindOutcome {
    /// The key was not bound yet.
    Added,
    /// The key was bound to the same namespace already.
    Unchanged,
    /// The key was bound to another namespace and the change was forced.
    Rebound,
}

/// The parsed registry.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectRegistry {
    pub bindings: Vec<ProjectBinding>,
}

impl ProjectRegistry {
    /// Read the registry. A missing file is an empty registry (not an
    /// error): the file only exists once a binding is recorded.
    pub fn load(path: &Path) -> Result<Self, MnexisError> {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => {
                return Err(MnexisError::invalid_argument(
                    "projects.toml",
                    format!("cannot read: {e}"),
                ));
            }
        };
        Self::from_toml(&src)
    }

    /// Write the registry atomically.
    pub fn save(&self, path: &Path) -> Result<(), MnexisError> {
        write_atomic(path, &self.to_toml())
    }

    /// The binding for a workspace key, if any.
    pub fn find(&self, key: &str) -> Option<&ProjectBinding> {
        self.bindings.iter().find(|b| b.key == key)
    }

    /// Bind a workspace key to a namespace.
    ///
    /// Refuses to move an already-bound key to a different namespace
    /// unless `force` is set: the namespace holds memories, and a silent
    /// move would orphan them.
    pub fn bind(
        &mut self,
        key: &str,
        namespace: &str,
        path: Option<&str>,
        force: bool,
    ) -> Result<BindOutcome, MnexisError> {
        validate_key(key)?;
        validate_namespace(namespace)?;
        match self.bindings.iter_mut().find(|b| b.key == key) {
            Some(existing) => {
                if existing.namespace == namespace {
                    if let Some(p) = path {
                        existing.path = Some(p.to_string());
                    }
                    Ok(BindOutcome::Unchanged)
                } else if force {
                    existing.namespace = namespace.to_string();
                    if let Some(p) = path {
                        existing.path = Some(p.to_string());
                    }
                    Ok(BindOutcome::Rebound)
                } else {
                    Err(MnexisError::invalid_argument(
                        "key",
                        format!(
                            "workspace key `{key}` is already bound to namespace `{}`; \
                             pass --force to move it to `{namespace}`",
                            existing.namespace
                        ),
                    ))
                }
            }
            None => {
                self.bindings.push(ProjectBinding {
                    key: key.to_string(),
                    namespace: namespace.to_string(),
                    path: path.map(|p| p.to_string()),
                });
                Ok(BindOutcome::Added)
            }
        }
    }

    /// Remove every binding for a namespace. Returns whether anything was
    /// removed; the memories themselves are never touched.
    pub fn unbind(&mut self, namespace: &str) -> bool {
        let before = self.bindings.len();
        self.bindings.retain(|b| b.namespace != namespace);
        before != self.bindings.len()
    }

    /// Parse the registry file. The format is a flat list of `[[project]]`
    /// tables with string values, written and read by this module only.
    pub fn from_toml(src: &str) -> Result<Self, MnexisError> {
        let mut registry = Self::default();
        let mut current: Option<ProjectBinding> = None;

        for (lineno, raw) in src.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[project]]" {
                if let Some(entry) = current.take() {
                    registry.bindings.push(entry);
                }
                current = Some(ProjectBinding {
                    key: String::new(),
                    namespace: String::new(),
                    path: None,
                });
                continue;
            }
            let (key, value) = line.split_once('=').ok_or_else(|| {
                MnexisError::invalid_argument(
                    "projects.toml",
                    format!("line {}: expected `key = value`", lineno + 1),
                )
            })?;
            let value = parse_string(value.trim(), lineno)?;
            let entry = current.as_mut().ok_or_else(|| {
                MnexisError::invalid_argument(
                    "projects.toml",
                    format!("line {}: value outside a [[project]] table", lineno + 1),
                )
            })?;
            match key.trim() {
                "key" => entry.key = value,
                "namespace" => entry.namespace = value,
                "path" => entry.path = Some(value),
                other => {
                    return Err(MnexisError::invalid_argument(
                        "projects.toml",
                        format!("line {}: unknown key `{other}`", lineno + 1),
                    ));
                }
            }
        }
        if let Some(entry) = current.take() {
            registry.bindings.push(entry);
        }
        for entry in &registry.bindings {
            validate_key(&entry.key)?;
            validate_namespace(&entry.namespace)?;
        }
        Ok(registry)
    }

    /// Serialize to the same format.
    pub fn to_toml(&self) -> String {
        let mut out = String::new();
        out.push_str("# mnexis project bindings (workspace key -> namespace)\n");
        out.push_str("# Written by `mnexis project bind`; safe to edit by hand.\n");
        for binding in &self.bindings {
            out.push_str("\n[[project]]\n");
            out.push_str(&format!("key = {}\n", quote(&binding.key)));
            out.push_str(&format!("namespace = {}\n", quote(&binding.namespace)));
            if let Some(path) = &binding.path {
                out.push_str(&format!("path = {}\n", quote(path)));
            }
        }
        out
    }

    /// The JSON envelope for `project list`.
    pub fn to_json(&self) -> serde_json::Value {
        json!({
            "bindings": self
                .bindings
                .iter()
                .map(|b| json!({
                    "key": b.key,
                    "namespace": b.namespace,
                    "path": b.path,
                }))
                .collect::<Vec<_>>(),
        })
    }
}

fn validate_key(key: &str) -> Result<(), MnexisError> {
    if key.trim().is_empty() {
        return Err(MnexisError::invalid_argument(
            "key",
            "key must not be empty",
        ));
    }
    if key.contains('\n') || key.contains('\r') {
        return Err(MnexisError::invalid_argument(
            "key",
            "key must not contain line breaks",
        ));
    }
    Ok(())
}

fn validate_namespace(namespace: &str) -> Result<(), MnexisError> {
    if namespace.trim().is_empty() {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "namespace must not be empty",
        ));
    }
    if namespace.contains('\n') || namespace.contains('\r') {
        return Err(MnexisError::invalid_argument(
            "namespace",
            "namespace must not contain line breaks",
        ));
    }
    Ok(())
}

/// Parse a quoted TOML string, resolving the escapes this module writes.
fn parse_string(src: &str, lineno: usize) -> Result<String, MnexisError> {
    let body = src.strip_prefix('"').ok_or_else(|| {
        MnexisError::invalid_argument(
            "projects.toml",
            format!("line {}: expected a quoted string", lineno + 1),
        )
    })?;
    let mut out = String::new();
    let mut chars = body.chars();
    loop {
        match chars.next() {
            None => {
                return Err(MnexisError::invalid_argument(
                    "projects.toml",
                    format!("line {}: unterminated string", lineno + 1),
                ));
            }
            Some('"') => return Ok(out),
            Some('\\') => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => {
                    return Err(MnexisError::invalid_argument(
                        "projects.toml",
                        format!("line {}: unsupported escape `\\{other}`", lineno + 1),
                    ));
                }
                None => {
                    return Err(MnexisError::invalid_argument(
                        "projects.toml",
                        format!("line {}: unterminated escape", lineno + 1),
                    ));
                }
            },
            Some(c) => out.push(c),
        }
    }
}

/// Quote a string, escaping what a quoted TOML string cannot hold raw.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trips_through_toml() {
        let mut registry = ProjectRegistry::default();
        registry
            .bind("git:github.com/Xeworg/mnexis", "mnexis", None, false)
            .unwrap();
        registry
            .bind(
                "path:/home/xeworg/notas",
                "notas",
                Some("/home/xeworg/notas"),
                false,
            )
            .unwrap();
        let text = registry.to_toml();
        let parsed = ProjectRegistry::from_toml(&text).unwrap();
        assert_eq!(parsed, registry);
    }

    #[test]
    fn a_key_keeps_its_namespace_unless_the_change_is_forced() {
        let mut registry = ProjectRegistry::default();
        registry.bind("k", "one", None, false).unwrap();
        let err = registry.bind("k", "two", None, false).unwrap_err();
        assert!(matches!(err, MnexisError::InvalidArgument { .. }));
        assert_eq!(registry.find("k").unwrap().namespace, "one");
        assert_eq!(
            registry.bind("k", "two", None, true).unwrap(),
            BindOutcome::Rebound
        );
        assert_eq!(registry.find("k").unwrap().namespace, "two");
    }

    #[test]
    fn binding_the_same_key_and_namespace_again_is_unchanged() {
        let mut registry = ProjectRegistry::default();
        registry.bind("k", "one", None, false).unwrap();
        assert_eq!(
            registry.bind("k", "one", None, false).unwrap(),
            BindOutcome::Unchanged
        );
        assert_eq!(registry.bindings.len(), 1);
    }

    #[test]
    fn unbind_removes_only_the_named_namespace() {
        let mut registry = ProjectRegistry::default();
        registry.bind("k1", "one", None, false).unwrap();
        registry.bind("k2", "two", None, false).unwrap();
        assert!(registry.unbind("one"));
        assert_eq!(registry.bindings.len(), 1);
        assert_eq!(registry.bindings[0].namespace, "two");
        assert!(!registry.unbind("one"));
    }

    #[test]
    fn an_empty_key_or_namespace_is_rejected() {
        let mut registry = ProjectRegistry::default();
        assert!(registry.bind("", "one", None, false).is_err());
        assert!(registry.bind("k", "", None, false).is_err());
    }

    #[test]
    fn quoted_values_round_trip_through_escapes() {
        let mut registry = ProjectRegistry::default();
        registry
            .bind("path:/tmp/a\"b\\c", "we\"ird", Some("/tmp/x\ty"), false)
            .unwrap();
        let parsed = ProjectRegistry::from_toml(&registry.to_toml()).unwrap();
        assert_eq!(parsed, registry);
    }

    #[test]
    fn an_unknown_key_in_the_file_is_rejected() {
        let src = "[[project]]\nkey = \"k\"\nnamespace = \"n\"\nbogus = \"x\"\n";
        assert!(ProjectRegistry::from_toml(src).is_err());
    }

    #[test]
    fn a_value_outside_a_table_is_rejected() {
        assert!(ProjectRegistry::from_toml("key = \"k\"\n").is_err());
    }

    #[test]
    fn the_fixture_holds_two_bindings() {
        let mut registry = ProjectRegistry::default();
        registry.bind("k1", "one", None, false).unwrap();
        registry.bind("k2", "two", None, false).unwrap();
        assert_eq!(registry.bindings.len(), 2);
    }
}
