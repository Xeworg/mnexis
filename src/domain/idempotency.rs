//! Idempotency key and record types (PRD §10 `idempotency_records`).

use serde::{Deserialize, Serialize};

/// A client-provided idempotency key. Unique per (namespace, actor).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IdempotencyKey(pub String);

impl IdempotencyKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stored idempotency record. Maps to `idempotency_records`.
///
/// Retention is bounded to 7 days by the application service (configurable);
/// old records are pruned on startup and after each write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub key: IdempotencyKey,
    pub namespace: String,
    /// SHA-256 of the canonical request JSON.
    pub request_hash: String,
    /// UUID of the memory that was (or would be) created.
    pub result_uuid: String,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_wraps_a_string() {
        let k = IdempotencyKey::new("abc");
        assert_eq!(k.as_str(), "abc");
        assert_eq!(k.0, "abc");
    }
}
