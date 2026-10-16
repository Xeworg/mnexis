//! Application service: the use-case surface for the durable SQL core.
//! Orchestrates the `MemoryRepository` port with validation, state transitions,
//! scope enforcement, and idempotency.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::domain::error::MnexisError;
use crate::domain::idempotency::{IdempotencyKey, IdempotencyRecord};
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink, MemoryState};
use crate::domain::repository::MemoryRepository;
use crate::domain::scope::{AccessScope, Operation};
use crate::retrieval::SearchHit;

pub struct ServiceConfig {
    pub max_content_bytes: usize,
    pub max_query_bytes: usize,
    pub max_tags: usize,
    pub max_topic_key_bytes: usize,
    pub default_retention_days: i64,
    pub idempotency_retention_days: i64,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            max_content_bytes: 16_384,
            max_query_bytes: 8_192,
            max_tags: 32,
            max_topic_key_bytes: 128,
            default_retention_days: 30,
            idempotency_retention_days: 7,
        }
    }
}

pub struct SaveRequest {
    pub uuid: String,
    pub topic_key: String,
    pub namespace: String,
    pub memory_type: String,
    pub content: String,
    pub actor: String,
    pub channel: String,
    pub assertion_basis: String,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub tags: Vec<String>,
    pub idempotency_key: Option<IdempotencyKey>,
    pub state: MemoryState,
    pub now: String,
}

pub struct UpdateRequest {
    pub uuid: String,
    pub expected_revision: i32,
    pub content: Option<String>,
    pub state: Option<MemoryState>,
    pub tags: Option<Vec<String>>,
    pub assertion_basis: Option<String>,
    pub reason: String,
    pub now: String,
}

pub struct SupersedeRequest {
    pub older_uuid: String,
    pub newer_uuid: String,
    pub expected_old_revision: i32,
    pub expected_new_revision: i32,
    pub reason: String,
    pub now: String,
}

pub struct ListFilter {
    pub namespace: Option<String>,
    pub state: Option<MemoryState>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for ListFilter {
    fn default() -> Self {
        Self {
            namespace: None,
            state: None,
            limit: 100,
            offset: 0,
        }
    }
}

pub struct MemoryService<R: MemoryRepository> {
    repo: R,
    scope: AccessScope,
    config: ServiceConfig,
}

impl<R: MemoryRepository> MemoryService<R> {
    pub fn new(repo: R, scope: AccessScope, config: ServiceConfig) -> Self {
        Self {
            repo,
            scope,
            config,
        }
    }

    pub fn scope(&self) -> &AccessScope {
        &self.scope
    }
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    // --- save ---
    pub fn save(&mut self, req: SaveRequest) -> Result<i64, MnexisError> {
        self.scope.authorize_write(&req.namespace)?;
        self.scope.authorize_operation(Operation::Save)?;

        validate_content(&req.content, self.config.max_content_bytes)?;
        validate_topic_key(&req.topic_key, self.config.max_topic_key_bytes)?;
        validate_namespace(&req.namespace)?;
        validate_tags(&req.tags, self.config.max_tags)?;

        // Compute the idempotency hash BEFORE any moves of req fields, so
        // the subsequent borrow-and-move pattern doesn't trigger E0382.
        let idem_hash = req
            .idempotency_key
            .as_ref()
            .map(|_| hash_save_request(&req));

        // Idempotency check (borrow, no moves).
        if let (Some(idem_key), Some(hash)) = (req.idempotency_key.as_ref(), idem_hash.as_ref()) {
            if let Some(existing) = self
                .repo
                .get_idempotency(idem_key.as_str(), &req.namespace)?
            {
                if existing.request_hash == *hash {
                    if let Some(m) = self.repo.get_by_uuid(&existing.result_uuid)? {
                        return Ok(m.id.unwrap_or(0));
                    }
                    return Err(MnexisError::not_found(format!(
                        "idempotency result uuid:{}",
                        existing.result_uuid
                    )));
                } else {
                    return Err(MnexisError::topic_conflict(
                        idem_key.as_str().to_string(),
                        req.namespace.clone(),
                    ));
                }
            }
        }

        let mem = Memory {
            id: None,
            uuid: req.uuid.clone(),
            topic_key: req.topic_key.clone(),
            namespace: req.namespace.clone(),
            memory_type: req.memory_type.clone(),
            content: req.content.clone(),
            state: req.state,
            revision: 1,
            deleted: false,
            created_at: req.now.clone(),
            updated_at: req.now.clone(),
            deleted_at: None,
            purge_after: None,
            valid_from: req.valid_from.clone(),
            valid_until: req.valid_until.clone(),
            actor: req.actor.clone(),
            channel: req.channel.clone(),
            assertion_basis: req.assertion_basis.clone(),
        };

        let row_id = self.repo.save(&mem)?;

        if !req.tags.is_empty() {
            self.repo.set_tags(&req.uuid, &req.tags)?;
        }

        // Move idempotency_key only after all borrows of req are done.
        if let Some(idem_key) = req.idempotency_key {
            let record = IdempotencyRecord {
                key: idem_key,
                namespace: req.namespace.clone(),
                request_hash: idem_hash.unwrap(), // safe: idempotency_key was Some
                result_uuid: req.uuid.clone(),
                created_at: req.now,
            };
            self.repo.save_idempotency(&record)?;
        }

        Ok(row_id)
    }

    // --- get ---
    pub fn get(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        let mem = self.repo.get_by_uuid(uuid)?;
        if let Some(ref m) = mem {
            self.scope.authorize_read(&m.namespace)?;
        }
        Ok(mem)
    }

    // --- list ---
    pub fn list(&self, filter: ListFilter) -> Result<Vec<Memory>, MnexisError> {
        if let Some(ref ns) = filter.namespace {
            self.scope.authorize_read(ns)?;
        }
        let state_str = filter.state.as_ref().map(|s| s.as_str());
        self.repo.list(
            filter.namespace.as_deref(),
            state_str,
            filter.limit,
            filter.offset,
        )
    }

    // --- update ---
    pub fn update(&mut self, req: UpdateRequest) -> Result<Memory, MnexisError> {
        let current = self
            .repo
            .get_by_uuid(&req.uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", req.uuid)))?;

        self.scope.authorize_write(&current.namespace)?;
        self.scope.authorize_operation(Operation::Update)?;

        if let Some(new_state) = req.state {
            if new_state != current.state && !current.state.can_transition_to(new_state) {
                return Err(MnexisError::invalid_argument(
                    "state",
                    format!(
                        "cannot transition from {} to {}",
                        current.state.as_str(),
                        new_state.as_str()
                    ),
                ));
            }
        }

        let mut updated = current.clone();
        if let Some(content) = req.content.as_ref() {
            validate_content(content, self.config.max_content_bytes)?;
            updated.content = content.clone();
        }
        if let Some(new_state) = req.state {
            updated.state = new_state;
        }
        if let Some(basis) = req.assertion_basis {
            updated.assertion_basis = basis;
        }
        updated.updated_at = req.now.clone();

        let result = self.repo.update(&updated, req.expected_revision)?;

        if let Some(tags) = req.tags.as_ref() {
            if !tags.is_empty() {
                validate_tags(tags, self.config.max_tags)?;
                self.repo.set_tags(&req.uuid, tags)?;
            }
        }

        let history = MemoryHistory {
            id: None,
            memory_uuid: req.uuid.clone(),
            revision: result.revision,
            memory_type: result.memory_type.clone(),
            content: result.content.clone(),
            state: result.state,
            valid_from: result.valid_from.clone(),
            valid_until: result.valid_until.clone(),
            assertion_basis: result.assertion_basis.clone(),
            changed_by: result.actor.clone(),
            change_reason: req.reason,
            changed_at: req.now,
        };
        self.repo.save_history(&history)?;

        Ok(result)
    }

    // --- history ---
    pub fn history(&self, uuid: &str) -> Result<Vec<MemoryHistory>, MnexisError> {
        let mem = self
            .repo
            .get_by_uuid(uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", uuid)))?;
        self.scope.authorize_read(&mem.namespace)?;
        self.repo.history(uuid)
    }

    // --- supersede ---
    pub fn supersede(&mut self, req: SupersedeRequest) -> Result<(), MnexisError> {
        let older = self
            .repo
            .get_by_uuid(&req.older_uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", req.older_uuid)))?;
        let newer = self
            .repo
            .get_by_uuid(&req.newer_uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", req.newer_uuid)))?;

        self.scope.authorize_write(&older.namespace)?;
        self.scope.authorize_write(&newer.namespace)?;
        self.scope.authorize_operation(Operation::Supersede)?;

        if older.state != MemoryState::Active {
            return Err(MnexisError::invalid_argument(
                "older",
                format!("must be active, is {}", older.state.as_str()),
            ));
        }
        if newer.state != MemoryState::Active {
            return Err(MnexisError::invalid_argument(
                "newer",
                format!("must be active, is {}", newer.state.as_str()),
            ));
        }

        let mut older_updated = older.clone();
        older_updated.state = MemoryState::Superseded;
        older_updated.updated_at = req.now.clone();
        self.repo
            .update(&older_updated, req.expected_old_revision)?;

        let link = MemoryLink {
            id: None,
            older_uuid: req.older_uuid.clone(),
            newer_uuid: req.newer_uuid.clone(),
            link_type: "supersedes".to_string(),
            created_at: req.now,
        };
        self.repo.save_link(&link)?;

        Ok(())
    }

    // --- soft_delete ---
    pub fn soft_delete(&mut self, uuid: &str, expected_revision: i32) -> Result<(), MnexisError> {
        let mem = self
            .repo
            .get_by_uuid_including_deleted(uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", uuid)))?;
        self.scope.authorize_write(&mem.namespace)?;
        self.scope.authorize_operation(Operation::SoftDelete)?;
        self.repo.soft_delete(uuid, expected_revision)
    }

    // --- restore ---
    pub fn restore(&mut self, uuid: &str) -> Result<(), MnexisError> {
        let mem = self
            .repo
            .get_by_uuid_including_deleted(uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", uuid)))?;
        self.scope.authorize_write(&mem.namespace)?;
        self.scope.authorize_operation(Operation::Restore)?;
        self.repo.restore(uuid)
    }

    // --- purge ---
    pub fn purge(&mut self, before: Option<&str>, all: bool) -> Result<usize, MnexisError> {
        self.scope.authorize_operation(Operation::Purge)?;
        self.repo.purge(before, all)
    }

    // --- reindex ---
    /// Rebuild the FTS5 index from the canonical `memories` table. Returns
    /// the number of rows re-indexed. Requires `Operation::Reindex` on the
    /// scope's `allowed_operations` allowlist.
    pub fn reindex(&mut self) -> Result<usize, MnexisError> {
        self.scope.authorize_operation(Operation::Reindex)?;
        self.repo.reindex()
    }

    // --- search ---
    pub fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
    ) -> Result<Vec<SearchHit>, MnexisError> {
        validate_query(query, self.config.max_query_bytes)?;
        if let Some(ns) = namespace {
            self.scope.authorize_read(ns)?;
        }
        self.repo.search(query, namespace, limit)
    }
}

// --- validation helpers ---

fn validate_content(content: &str, max_bytes: usize) -> Result<(), MnexisError> {
    if content.len() > max_bytes {
        Err(MnexisError::invalid_argument(
            "content",
            format!("exceeds max bytes: {} > {}", content.len(), max_bytes),
        ))
    } else {
        Ok(())
    }
}

fn validate_topic_key(topic_key: &str, max_bytes: usize) -> Result<(), MnexisError> {
    if topic_key.len() > max_bytes {
        return Err(MnexisError::invalid_argument(
            "topic_key",
            format!("exceeds max bytes: {} > {}", topic_key.len(), max_bytes),
        ));
    }
    for c in topic_key.chars() {
        if !c.is_ascii_lowercase() && !c.is_ascii_digit() && c != '.' && c != '-' {
            return Err(MnexisError::invalid_argument(
                "topic_key",
                format!("invalid character: {c:?}"),
            ));
        }
    }
    Ok(())
}

fn validate_namespace(namespace: &str) -> Result<(), MnexisError> {
    if namespace.is_empty() {
        Err(MnexisError::invalid_argument(
            "namespace",
            "must not be empty",
        ))
    } else {
        Ok(())
    }
}

fn validate_tags(tags: &[String], max_tags: usize) -> Result<(), MnexisError> {
    if tags.len() > max_tags {
        return Err(MnexisError::invalid_argument(
            "tags",
            format!("exceeds max count: {} > {}", tags.len(), max_tags),
        ));
    }
    for tag in tags {
        if tag.is_empty() {
            return Err(MnexisError::invalid_argument(
                "tags",
                "tag must not be empty",
            ));
        }
    }
    Ok(())
}

fn validate_query(query: &str, max_bytes: usize) -> Result<(), MnexisError> {
    if query.len() > max_bytes {
        Err(MnexisError::invalid_argument(
            "query",
            format!("exceeds max bytes: {} > {}", query.len(), max_bytes),
        ))
    } else {
        Ok(())
    }
}

fn hash_save_request(req: &SaveRequest) -> String {
    let mut h = DefaultHasher::new();
    req.uuid.hash(&mut h);
    req.topic_key.hash(&mut h);
    req.namespace.hash(&mut h);
    req.memory_type.hash(&mut h);
    req.content.hash(&mut h);
    req.actor.hash(&mut h);
    req.channel.hash(&mut h);
    req.assertion_basis.hash(&mut h);
    req.valid_from.hash(&mut h);
    req.valid_until.hash(&mut h);
    for tag in &req.tags {
        tag.hash(&mut h);
    }
    req.state.as_str().hash(&mut h);
    format!("{:x}", h.finish())
}
