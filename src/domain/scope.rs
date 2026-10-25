//! Process access scope (PRD §7).

use serde::{Deserialize, Serialize};

use crate::domain::error::MnexisError;

/// The closed set of operations (PRD §E2 use-case surface + admin).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    // Core use cases
    Save,
    Get,
    List,
    Update,
    History,
    Supersede,
    SoftDelete,
    Restore,
    Purge,
    Export,
    Import,
    Reindex,
    Search,
    Context,
    // Admin
    Init,
    Doctor,
    ConfigShow,
    McpServe,
    Eval,
}

impl Operation {
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::Save => "save",
            Operation::Get => "get",
            Operation::List => "list",
            Operation::Update => "update",
            Operation::History => "history",
            Operation::Supersede => "supersede",
            Operation::SoftDelete => "soft_delete",
            Operation::Restore => "restore",
            Operation::Purge => "purge",
            Operation::Export => "export",
            Operation::Import => "import",
            Operation::Reindex => "reindex",
            Operation::Search => "search",
            Operation::Context => "context",
            Operation::Init => "init",
            Operation::Doctor => "doctor",
            Operation::ConfigShow => "config_show",
            Operation::McpServe => "mcp_serve",
            Operation::Eval => "eval",
        }
    }
}

/// How the service reports an out-of-scope access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExistenceDisclosure {
    /// Return `NotFound` to avoid disclosing the existence of records the
    /// caller cannot see. This is the default.
    NotFound,
    /// Return `Forbidden` when the operator explicitly wants the caller to
    /// know they were denied.
    Forbid,
}

impl Default for ExistenceDisclosure {
    fn default() -> Self {
        ExistenceDisclosure::NotFound
    }
}

/// How a read without an explicit namespace is filtered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NamespaceScope {
    /// Every namespace.
    All,
    /// Every namespace except these.
    AllExcept(Vec<String>),
    /// Only these namespaces.
    Only(Vec<String>),
}

/// The immutable access scope for a process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessScope {
    pub read_namespaces: Vec<String>,
    pub write_namespaces: Vec<String>,
    pub default_namespace: String,
    pub allow_global: bool,
    /// Reads are open across namespaces. The reserved literal `global`
    /// stays governed by `allow_global`: it is never covered by this.
    pub allow_read_all: bool,
    pub allowed_operations: Vec<Operation>,
    pub existence_disclosure: ExistenceDisclosure,
}

impl AccessScope {
    pub fn new(
        read_namespaces: Vec<String>,
        write_namespaces: Vec<String>,
        default_namespace: String,
        allow_global: bool,
        allowed_operations: Vec<Operation>,
    ) -> Self {
        Self {
            read_namespaces,
            write_namespaces,
            default_namespace,
            allow_global,
            allow_read_all: false,
            allowed_operations,
            existence_disclosure: ExistenceDisclosure::NotFound,
        }
    }

    pub fn with_disclosure(mut self, disclosure: ExistenceDisclosure) -> Self {
        self.existence_disclosure = disclosure;
        self
    }

    /// Open the reads across every namespace (except the reserved
    /// `global` literal, which keeps its own permission).
    pub fn with_read_all(mut self, allow_read_all: bool) -> Self {
        self.allow_read_all = allow_read_all;
        self
    }

    pub fn can_read(&self, namespace: &str) -> bool {
        if namespace == "global" {
            return self.allow_global;
        }
        if self.allow_read_all {
            return true;
        }
        self.read_namespaces.iter().any(|n| n == namespace)
    }

    pub fn can_write(&self, namespace: &str) -> bool {
        if namespace == "global" {
            return self.allow_global;
        }
        self.write_namespaces.iter().any(|n| n == namespace)
    }

    /// The filter a read without an explicit namespace uses.
    ///
    /// `allow_read_all` opens the reads across projects; the reserved
    /// `global` literal stays outside them until `allow_global` permits
    /// it. A scoped profile reads only its read namespaces.
    pub fn read_scope(&self) -> NamespaceScope {
        match (self.allow_read_all, self.allow_global) {
            (true, true) => NamespaceScope::All,
            (true, false) => NamespaceScope::AllExcept(vec!["global".to_string()]),
            (false, _) => {
                let mut names = self.read_namespaces.clone();
                if self.allow_global && !names.iter().any(|n| n == "global") {
                    names.push("global".to_string());
                }
                NamespaceScope::Only(names)
            }
        }
    }

    pub fn allows(&self, op: Operation) -> bool {
        self.allowed_operations.contains(&op)
    }

    pub fn authorize_read(&self, namespace: &str) -> Result<(), MnexisError> {
        if self.can_read(namespace) {
            Ok(())
        } else {
            Err(self.deny_error("read", "allow-read-namespace", namespace))
        }
    }

    pub fn authorize_write(&self, namespace: &str) -> Result<(), MnexisError> {
        if self.can_write(namespace) {
            Ok(())
        } else {
            Err(self.deny_error("write", "allow-namespace", namespace))
        }
    }

    pub fn authorize_operation(&self, op: Operation) -> Result<(), MnexisError> {
        if self.allows(op) {
            Ok(())
        } else {
            Err(MnexisError::forbidden(
                op.as_str(),
                "operation not allowed for this process",
            ))
        }
    }

    /// Build the denial for an out-of-scope namespace.
    ///
    /// `grant` is the `config` command that grants the access, and the
    /// answer names it (S3): a route the human can follow, never a
    /// statement about what the namespace holds. The only namespace the
    /// message mentions is the one that was asked for, so neither form
    /// discloses the allowlists or the existence of records.
    fn deny_error(&self, op: &str, grant: &str, namespace: &str) -> MnexisError {
        // The reserved `global` namespace is governed by its own
        // permission, so the route must be the command that actually
        // grants it: `allow-read-namespace global` would not open it.
        let route = if namespace == "global" {
            format!(
                "run `mnexis config allow-global` to grant {op} access \
                 (the reserved 'global' namespace)"
            )
        } else {
            format!("run `mnexis config {grant} {namespace}` to grant {op} access")
        };
        match self.existence_disclosure {
            ExistenceDisclosure::NotFound => {
                MnexisError::not_found(format!("namespace:{namespace} ({route})"))
            }
            ExistenceDisclosure::Forbid => MnexisError::forbidden(
                op,
                format!("namespace not allowed for this process ({route})"),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> AccessScope {
        AccessScope::new(
            vec!["proyecto-a".into(), "proyecto-b".into()],
            vec!["proyecto-a".into()],
            "proyecto-a".into(),
            false,
            vec![Operation::Save, Operation::Search],
        )
    }

    #[test]
    fn read_and_write_follow_distinct_allowlists() {
        let s = scope();
        assert!(s.can_read("proyecto-a"));
        assert!(s.can_read("proyecto-b"));
        assert!(!s.can_read("proyecto-c"));
        assert!(s.can_write("proyecto-a"));
        assert!(!s.can_write("proyecto-b"));
    }

    #[test]
    fn global_namespace_requires_explicit_permission() {
        let mut s = scope();
        assert!(!s.can_read("global"));
        assert!(!s.can_write("global"));
        s.allow_global = true;
        assert!(s.can_read("global"));
        assert!(s.can_write("global"));
    }

    #[test]
    fn read_all_opens_other_namespaces_but_keeps_global_reserved() {
        let mut s = scope().with_read_all(true);
        assert!(s.can_read("proyecto-c"));
        // The reserved literal stays governed by `allow_global`.
        assert!(!s.can_read("global"));
        s.allow_global = true;
        assert!(s.can_read("global"));
    }

    #[test]
    fn read_all_never_opens_writes() {
        let s = scope().with_read_all(true);
        assert!(!s.can_write("proyecto-b"));
        assert!(!s.can_write("proyecto-c"));
        assert!(!s.can_write("global"));
    }

    #[test]
    fn read_all_is_off_by_default_in_the_constructor() {
        let s = scope();
        assert!(!s.can_read("proyecto-c"));
    }

    #[test]
    fn the_read_scope_follows_the_two_permissions() {
        // Open reads, reserved namespace closed: everything but `global`.
        let s = scope().with_read_all(true);
        assert_eq!(
            s.read_scope(),
            NamespaceScope::AllExcept(vec!["global".to_string()])
        );
        // Open reads and the reserved namespace permitted: everything.
        let mut s = scope().with_read_all(true);
        s.allow_global = true;
        assert_eq!(s.read_scope(), NamespaceScope::All);
        // Scoped reads: only the read namespaces (plus `global` when it is
        // permitted).
        let mut s = scope();
        assert_eq!(
            s.read_scope(),
            NamespaceScope::Only(vec!["proyecto-a".to_string(), "proyecto-b".to_string()])
        );
        s.allow_global = true;
        assert_eq!(
            s.read_scope(),
            NamespaceScope::Only(vec![
                "proyecto-a".to_string(),
                "proyecto-b".to_string(),
                "global".to_string()
            ])
        );
    }

    #[test]
    fn operation_allowlist_is_enforced() {
        let s = scope();
        assert!(s.allows(Operation::Save));
        assert!(s.allows(Operation::Search));
        assert!(!s.allows(Operation::Purge));
    }

    #[test]
    fn default_disclosure_is_not_found() {
        let s = scope();
        let err = s.authorize_read("proyecto-c").unwrap_err();
        assert!(matches!(err, MnexisError::NotFound { .. }));
    }

    #[test]
    fn opt_in_forbid_disclosure() {
        let s = scope().with_disclosure(ExistenceDisclosure::Forbid);
        let err = s.authorize_read("proyecto-c").unwrap_err();
        assert!(matches!(err, MnexisError::Forbidden { .. }));
    }

    #[test]
    fn authorize_write_returns_error_for_unauthorized_namespace() {
        let s = scope();
        let err = s.authorize_write("proyecto-b").unwrap_err();
        assert!(matches!(err, MnexisError::NotFound { .. }));
    }

    #[test]
    fn authorize_read_and_write_accept_allowed_namespaces() {
        let s = scope();
        assert_eq!(s.authorize_read("proyecto-a"), Ok(()));
        assert_eq!(s.authorize_read("proyecto-b"), Ok(()));
        assert_eq!(s.authorize_write("proyecto-a"), Ok(()));
    }

    #[test]
    fn read_denial_names_the_read_route_and_stays_not_found() {
        let s = scope();
        let err = s.authorize_read("proyecto-c").unwrap_err();
        assert!(matches!(err, MnexisError::NotFound { .. }));
        assert_eq!(
            err.to_string(),
            "not found: namespace:proyecto-c (run `mnexis config allow-read-namespace \
             proyecto-c` to grant read access)"
        );
    }

    #[test]
    fn write_denial_names_the_write_route_and_stays_not_found() {
        let s = scope();
        // `proyecto-b` is readable but not writable: the denial must name
        // the write route, never the read one.
        let err = s.authorize_write("proyecto-b").unwrap_err();
        assert!(matches!(err, MnexisError::NotFound { .. }));
        assert_eq!(
            err.to_string(),
            "not found: namespace:proyecto-b (run `mnexis config allow-namespace \
             proyecto-b` to grant write access)"
        );
    }

    #[test]
    fn forbid_disclosure_keeps_the_kind_and_names_the_route() {
        let s = scope().with_disclosure(ExistenceDisclosure::Forbid);
        let read = s.authorize_read("proyecto-c").unwrap_err();
        assert!(matches!(read, MnexisError::Forbidden { .. }));
        assert_eq!(
            read.to_string(),
            "forbidden: read: namespace not allowed for this process (run `mnexis \
             config allow-read-namespace proyecto-c` to grant read access)"
        );
        let write = s.authorize_write("proyecto-c").unwrap_err();
        assert!(matches!(write, MnexisError::Forbidden { .. }));
        assert_eq!(
            write.to_string(),
            "forbidden: write: namespace not allowed for this process (run \
             `mnexis config allow-namespace proyecto-c` to grant write access)"
        );
    }

    #[test]
    fn the_denial_is_a_route_and_carries_nothing_about_the_namespace() {
        let s = scope();
        // The answer differs only in the name that was asked for: it is a
        // route, never a statement about what the namespace holds.
        let render = |namespace: &str| {
            s.authorize_read(namespace)
                .unwrap_err()
                .to_string()
                .replace(namespace, "<ns>")
        };
        assert_eq!(render("proyecto-c"), render("otro-proyecto"));
        // And it mentions no other namespace than the one requested.
        let message = s.authorize_read("proyecto-c").unwrap_err().to_string();
        assert!(message.contains("proyecto-c"), "message: {message}");
        assert!(!message.contains("proyecto-a"), "message: {message}");
        assert!(!message.contains("proyecto-b"), "message: {message}");
    }

    #[test]
    fn authorize_operation_returns_forbidden_for_unauthorized_op() {
        let s = scope();
        let err = s.authorize_operation(Operation::Purge).unwrap_err();
        assert!(matches!(err, MnexisError::Forbidden { .. }));
    }
}
