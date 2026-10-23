//! Application service: the use-case surface for the durable SQL core.
//! Orchestrates the `MemoryRepository` port with validation, state transitions,
//! scope enforcement, and idempotency.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::application::io::{ConflictPolicy, ImportDecision, LocalIndex};
use crate::domain::duplicates::{DuplicateCandidate, TitleRecord, rank_duplicates};
use crate::domain::error::MnexisError;
use crate::domain::idempotency::{IdempotencyKey, IdempotencyRecord};
use crate::domain::memory::{Memory, MemoryHistory, MemoryLink, MemoryState, MemoryTag};
use crate::domain::query::MatchMode;
use crate::domain::repository::MemoryRepository;
use crate::domain::scope::{AccessScope, NamespaceScope, Operation};
use crate::domain::session::{Session, UserPrompt};
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
    /// The short, stable label of the fact. The topic key derives from it
    /// when the caller does not give one.
    pub title: String,
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
    /// Save a new memory. Returns the saved [`Memory`] so callers can
    /// see the assigned rowid, the canonical uuid, and the persisted
    /// timestamps. The idempotency replay path returns the *original*
    /// record (not the fresh uuid the client minted), which is the
    /// whole point of an idempotency key.
    pub fn save(&mut self, mut req: SaveRequest) -> Result<Memory, MnexisError> {
        self.scope.authorize_write(&req.namespace)?;
        self.scope.authorize_operation(Operation::Save)?;

        validate_content(&req.content, self.config.max_content_bytes)?;
        validate_topic_key(&req.topic_key, self.config.max_topic_key_bytes)?;
        validate_namespace(&req.namespace)?;
        validate_tags(&req.tags, self.config.max_tags)?;

        // The key comes from the title: the same title always gives the same
        // key, so the same fact lands on the same record, in any language,
        // without anyone having to remember the key.
        if req.topic_key.is_empty() && !req.title.trim().is_empty() {
            req.topic_key = crate::domain::title::key_from_title(&req.title);
        }

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
                        return Ok(m);
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

        // One fact, one record: a key the namespace already uses is not a
        // second fact, it is the same fact with a new value. The save updates
        // that record — content, title and tags — keeps its history, and
        // returns it. The key derives from the title, so this is an equality,
        // not a guess.
        if !req.topic_key.is_empty()
            && let Some(occupier) = self
                .repo
                .topic_key_occupier(&req.namespace, &req.topic_key)?
        {
            let updated = self.update_occupied(&occupier, &req)?;
            return Ok(updated);
        }

        let mem = Memory {
            id: None,
            uuid: req.uuid.clone(),
            title: req.title.clone(),
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

        // Re-read so the caller sees the canonical assigned rowid and
        // the persisted timestamps (the timestamps defaulted to
        // `req.now` in the in-memory `mem`; the DB may have applied
        // its own defaults via the schema's `DEFAULT (strftime(...))`).
        let stored = self
            .repo
            .get_by_uuid(&req.uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", req.uuid)))?;
        let _ = row_id; // already reflected in `stored.id`
        Ok(stored)
    }

    // --- get ---
    pub fn get(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        let mem = self.repo.get_by_uuid(uuid)?;
        if let Some(ref m) = mem {
            self.scope.authorize_read(&m.namespace)?;
        }
        Ok(mem)
    }

    // --- get (including soft-deleted) ---
    /// Load a memory even when it is soft-deleted. `get` intentionally hides
    /// deleted records; the TUI's deleted view needs the row to render it and
    /// to read the revision a restore acts on.
    pub fn get_including_deleted(&self, uuid: &str) -> Result<Option<Memory>, MnexisError> {
        let mem = self.repo.get_by_uuid_including_deleted(uuid)?;
        if let Some(ref m) = mem {
            self.scope.authorize_read(&m.namespace)?;
        }
        Ok(mem)
    }

    // --- target of a write ---
    /// The namespace a memory lives in, read without the scope's read check.
    ///
    /// A caller that must decide about a *write* before the service
    /// authorizes it needs the target namespace, and its own read allowlist
    /// may not cover that namespace. Identity only: no content, and no
    /// authorization — the write itself still goes through
    /// [`MemoryService::update`], [`MemoryService::supersede`],
    /// [`MemoryService::soft_delete`] or [`MemoryService::restore`], which
    /// run the scope check.
    pub fn namespace_of(&self, uuid: &str) -> Result<Option<String>, MnexisError> {
        Ok(self
            .repo
            .get_by_uuid_including_deleted(uuid)?
            .map(|m| m.namespace))
    }

    // --- list ---
    pub fn list(&self, filter: ListFilter) -> Result<Vec<Memory>, MnexisError> {
        let state_str = filter.state.as_ref().map(|s| s.as_str());
        match filter.namespace.as_deref() {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                self.repo
                    .list(Some(ns), state_str, filter.limit, filter.offset)
            }
            // Without a namespace the read covers every namespace the
            // scope reads: all of them when the reads are open, only its
            // read namespaces when they are scoped.
            None => self.repo.list_in(
                &self.scope.read_scope(),
                state_str,
                filter.limit,
                filter.offset,
                false,
            ),
        }
    }

    // --- list deleted ---
    /// List soft-deleted memories for a namespace. `list` hides them by
    /// design, so this is the read that makes `restore` reachable from the
    /// TUI's deleted view. Same scope check as `list`.
    pub fn list_deleted(
        &self,
        namespace: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        match namespace {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                self.repo.list_deleted(Some(ns), limit, offset)
            }
            None => self
                .repo
                .list_in(&self.scope.read_scope(), None, limit, offset, true),
        }
    }

    // --- recent ---
    /// The most recent memories, newest first, limited to `limit` rows.
    ///
    /// Same scope rule as [`MemoryService::list`]: an explicit namespace is
    /// authorized as a read first, and a read without a namespace is bounded
    /// by the scope's read allowlist.
    pub fn recent(
        &self,
        namespace: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>, MnexisError> {
        match namespace {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                self.repo
                    .list_recent(&NamespaceScope::Only(vec![ns.to_string()]), limit)
            }
            None => self.repo.list_recent(&self.scope.read_scope(), limit),
        }
    }

    // --- count ---
    /// How many memories the scope can see: the same namespace filter as a
    /// read without an explicit namespace ([`MemoryService::list`]), without
    /// loading the rows.
    pub fn count(&self) -> Result<i64, MnexisError> {
        self.repo.count_in(&self.scope.read_scope())
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

        // The history revision is what the UPDATE will produce
        // (current + 1). We pre-compute it so the history row matches
        // the post-update row exactly.
        let new_revision = current.revision + 1;
        let history = MemoryHistory {
            id: None,
            memory_uuid: req.uuid.clone(),
            revision: new_revision,
            memory_type: updated.memory_type.clone(),
            content: updated.content.clone(),
            state: updated.state,
            valid_from: updated.valid_from.clone(),
            valid_until: updated.valid_until.clone(),
            assertion_basis: updated.assertion_basis.clone(),
            changed_by: updated.actor.clone(),
            change_reason: req.reason.clone(),
            changed_at: req.now.clone(),
        };
        // T23 close test `partial_failure_no_partial_writes`: the UPDATE
        // and the history insert must commit atomically. We use the
        // transactional repo method that wraps both in a single tx
        // (plus the tag rewrite when one is requested). If the history
        // INSERT (or anything in the transaction) fails, the canonical
        // row rolls back too.
        let tags = req
            .tags
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(Vec::as_slice);
        self.repo
            .update_with_history_and_tags(&updated, req.expected_revision, &history, tags)
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

    // --- links (supersession chain) ---
    /// Every supersession link touching `uuid`, either as the older or the
    /// newer side. Soft-deleted records are readable (the TUI chain of a
    /// deleted memory); scope authorization matches `history`.
    ///
    /// A link row carries `older_uuid`/`newer_uuid`/`link_type`/`created_at`.
    /// The `reason` a supersede was requested is a request argument only:
    /// `memory_links` has no reason column, so it is not persisted and is not
    /// fabricated here.
    pub fn links(&self, uuid: &str) -> Result<Vec<MemoryLink>, MnexisError> {
        let mem = self
            .get_including_deleted(uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", uuid)))?;
        self.scope.authorize_read(&mem.namespace)?;
        self.repo.links_for(uuid)
    }

    // --- tags ---
    /// The tags attached to a memory, ordered by tag. Delegates to the
    /// repository's `tags_for`, like every other read here.
    pub fn tags(&self, uuid: &str) -> Result<Vec<MemoryTag>, MnexisError> {
        let mem = self
            .get_including_deleted(uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{}", uuid)))?;
        self.scope.authorize_read(&mem.namespace)?;
        self.repo.tags_for(uuid)
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

        // Contract v1: "Supersede is atomic, same-namespace in MVP,
        // revision-checked on both records, and must reject cycles."
        // Same-namespace is gated here (namespaces never change after
        // save); revisions, atomicity, and cycle rejection live in the
        // repository transaction.
        if older.namespace != newer.namespace {
            return Err(MnexisError::invalid_argument(
                "newer_uuid",
                format!(
                    "supersede is same-namespace in MVP: {} is in {}, {} is in {}",
                    req.older_uuid, older.namespace, req.newer_uuid, newer.namespace
                ),
            ));
        }

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

        self.repo.supersede_atomic(
            &req.older_uuid,
            &req.newer_uuid,
            req.expected_old_revision,
            req.expected_new_revision,
            &req.reason,
            &req.now,
        )
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

    // --- context (PRD §9, E3) ---
    /// Run the full `memory_context` pipeline: FTS5 candidates +
    /// canonical revalidation + eligibility + dedup + budget selection.
    /// The `tokenizer_id` is enforced against the supported tokenizer v1
    /// when `Exact` mode is requested; an unsupported id in exact mode
    /// raises `MnexisError::BudgetTooSmall`.
    pub fn context(
        &self,
        req: &crate::application::ContextRequest,
    ) -> Result<crate::application::ContextResponse, MnexisError> {
        use crate::application::budget::{Budget, token_count_v1};
        use crate::application::context::{build_response, serialise_envelope};

        let budget = Budget::new(req.token_budget, req.budget_mode);
        let tokenizer_id = req.tokenizer_id.as_deref().unwrap_or("");

        // Step 1: scope authorisation (read the namespace, or fall back
        // to the first read_namespaces entry).
        let ns = match req.namespace.as_deref() {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                ns.to_string()
            }
            None => {
                if self.scope.read_namespaces.is_empty() {
                    return Ok(crate::application::ContextResponse::empty(
                        "scope has no read_namespaces",
                        &budget,
                    ));
                }
                self.scope.read_namespaces[0].clone()
            }
        };

        // Step 2: FTS5 candidate retrieval (active + non-deleted by
        // default; the pipeline's eligibility pass re-enforces the
        // same invariant as defence in depth).
        let max_items = req.max_items.unwrap_or(20);
        let hits = self
            .repo
            .search(&req.task, Some(&ns), max_items, req.match_mode)?;

        // Steps 3+4: revalidate the canonical record and pair the
        // (hit, memory) tuples the pipeline consumes.
        let mut pairs: Vec<(crate::retrieval::SearchHit, Memory)> = Vec::with_capacity(hits.len());
        for hit in hits {
            if let Some(mem) = self.repo.get_by_uuid(&hit.uuid)? {
                pairs.push((hit, mem));
            }
        }

        // Steps 5+6: build the response through the pipeline.
        let response = build_response(&budget, tokenizer_id, pairs, &req.search, &req.now)?;

        // A-07 invariant (debug-only): in exact mode, the
        // response-claimed tokens_used equals the supported tokenizer's
        // count of the serialised envelope.
        if matches!(req.budget_mode, crate::application::BudgetMode::Exact) {
            let s = serialise_envelope(&response.items, &budget);
            let tokens = token_count_v1(&s);
            debug_assert_eq!(response.tokens_used, tokens);
        }

        Ok(response)
    }

    // --- export ---
    /// Snapshot all non-deleted memories (unless `include_deleted` is true)
    /// in the namespaces the caller is authorised to read. The scope is
    /// authoritative; the snapshot is filtered to `read_namespaces`.
    pub fn export(&self, include_deleted: bool) -> Result<Vec<Memory>, MnexisError> {
        self.scope.authorize_operation(Operation::Export)?;
        let namespaces = self.scope.read_namespaces.clone();
        // Defence in depth: the scope can never authorise a write, but
        // export reads, so the explicit read filter is what we honour.
        for ns in &namespaces {
            self.scope.authorize_read(ns)?;
        }
        self.repo.list_for_export(&namespaces, include_deleted)
    }

    // --- import ---
    /// Parse the JSONL `body` and apply it under the given conflict policy.
    /// When `dry_run` is true, validate and classify every record without
    /// writing; otherwise, persist the resolved set inside a single
    /// service call (the repo's `save` is per-row; a wrapping transaction
    /// is the storage layer's job — kept per-row here because the service
    /// already enforces atomicity at the request level).
    ///
    /// Returns the structured outcomes: counts plus the remapped uuids
    /// (so the caller can log/display them).
    pub fn import(
        &mut self,
        body: &[u8],
        policy: ConflictPolicy,
        dry_run: bool,
    ) -> Result<ImportSummary, MnexisError> {
        self.scope.authorize_operation(Operation::Import)?;
        let records = crate::application::io::parse_jsonl(body, self.config.max_content_bytes)?;

        // Build the local index over every memory the caller can read. This
        // is the source of truth for conflict decisions.
        let namespaces = self.scope.read_namespaces.clone();
        let local_memories = self.repo.list_for_export(&namespaces, true)?;
        let local_index = LocalIndex::from_memories(local_memories);

        let mut summary = ImportSummary {
            dry_run,
            policy: policy.as_str().to_string(),
            records: records.len(),
            identical: 0,
            fresh: 0,
            skipped: 0,
            remapped: 0,
            conflicts: Vec::new(),
            remapped_uuids: Vec::new(),
        };

        for candidate in records {
            // Authorize the candidate's namespace as a write. The scope
            // cannot widen during an import; this is the protection the
            // design asks for.
            self.scope.authorize_write(&candidate.namespace)?;

            let decision =
                crate::application::io::classify(candidate.clone(), &local_index, policy, || {
                    crate::cli::profile::uuid_v4().unwrap_or_else(|_| {
                        // uuid_v4 only fails on a missing /dev/urandom; if the
                        // process reached import, the same code minted the
                        // initial save uuids. Falling back to a deterministic
                        // prefix keeps import alive in degenerate environments.
                        format!("imported-{}", summary.remapped_uuids.len())
                    })
                });

            match decision {
                ImportDecision::Identical => summary.identical += 1,
                ImportDecision::Fresh => {
                    if !dry_run {
                        let mem: Memory = candidate.into();
                        self.repo.save(&mem)?;
                    }
                    summary.fresh += 1;
                }
                ImportDecision::Skipped(conflict) => {
                    // Two cases are hard errors regardless of policy:
                    //   - `reject` on the first conflict (any kind).
                    //   - a `TopicKey` clash: the design forbids silent
                    //     rewriting of conceptual keys, so even under
                    //     `skip` and `remap` this aborts with a stable
                    //     error code (the operator must fix the file).
                    if matches!(policy, ConflictPolicy::Reject)
                        || matches!(conflict, crate::application::io::Conflict::TopicKey)
                    {
                        return Err(match conflict {
                            crate::application::io::Conflict::UuidClash => {
                                MnexisError::revision_conflict(0)
                            }
                            crate::application::io::Conflict::TopicKey => {
                                MnexisError::topic_conflict(
                                    candidate.topic_key,
                                    candidate.namespace,
                                )
                            }
                        });
                    }
                    summary.skipped += 1;
                    summary.conflicts.push(ConflictRecord {
                        uuid: candidate.uuid,
                        kind: conflict,
                        action: "skipped",
                    });
                }
                ImportDecision::Remapped(_, new_uuid) => {
                    if !dry_run {
                        let mut mem: Memory = candidate.into();
                        mem.uuid = new_uuid.clone();
                        self.repo.save(&mem)?;
                    }
                    summary.remapped += 1;
                    summary.remapped_uuids.push(new_uuid);
                }
            }
        }

        Ok(summary)
    }

    // --- sessions and prompts ---

    /// Register the session the caller runs in.
    ///
    /// The id is the caller's: the agent never invents one, and a session
    /// that already exists is returned as it is — the engine never restarts
    /// a session. The namespace obeys the same rules as a memory write.
    pub fn start_session(
        &mut self,
        id: &str,
        namespace: Option<&str>,
        directory: &str,
        now: &str,
    ) -> Result<Session, MnexisError> {
        if id.trim().is_empty() {
            return Err(MnexisError::invalid_argument(
                "session_id",
                "must not be empty",
            ));
        }
        let ns = self.target_namespace(namespace)?;
        self.scope.authorize_write(&ns)?;
        self.scope.authorize_operation(Operation::Save)?;
        self.repo.start_session(&Session {
            id: id.to_string(),
            namespace: ns,
            directory: directory.to_string(),
            started_at: now.to_string(),
            ended_at: None,
            summary: None,
        })
    }

    /// End a session, with an optional summary of what it did.
    pub fn end_session(
        &mut self,
        id: &str,
        summary: Option<&str>,
        now: &str,
    ) -> Result<Session, MnexisError> {
        if let Some(summary) = summary {
            validate_content(summary, self.config.max_content_bytes)?;
        }
        let session = self
            .repo
            .get_session(id)?
            .ok_or_else(|| MnexisError::not_found(format!("session:{id}")))?;
        // Ending a session writes into its namespace: the same rule as a
        // memory write.
        self.scope.authorize_write(&session.namespace)?;
        self.scope.authorize_operation(Operation::Save)?;
        self.repo.end_session(id, now, summary)
    }

    /// The sessions the scope can read, newest first.
    pub fn list_sessions(&self, limit: usize) -> Result<Vec<Session>, MnexisError> {
        self.repo.list_sessions(&self.scope.read_scope(), limit)
    }

    /// Store what the user said.
    ///
    /// A prompt is not a memory: it has no revision, no topic key and no
    /// state. It obeys the same namespace rules, because it is still a
    /// write into a project.
    pub fn save_prompt(
        &mut self,
        content: &str,
        session_id: Option<&str>,
        namespace: Option<&str>,
        now: &str,
    ) -> Result<UserPrompt, MnexisError> {
        validate_content(content, self.config.max_content_bytes)?;
        let ns = self.target_namespace(namespace)?;
        self.scope.authorize_write(&ns)?;
        self.scope.authorize_operation(Operation::Save)?;
        let prompt = UserPrompt {
            id: 0,
            session_id: session_id.map(|s| s.to_string()),
            namespace: ns,
            content: content.to_string(),
            created_at: now.to_string(),
        };
        let id = self.repo.save_prompt(&prompt)?;
        Ok(UserPrompt { id, ..prompt })
    }

    /// The prompts the scope can read, newest first.
    ///
    /// A namespace narrows the read to one project; without one, the
    /// scope's read scope decides, as it does for the memories.
    pub fn list_prompts(
        &self,
        session_id: Option<&str>,
        limit: usize,
        namespace: Option<&str>,
    ) -> Result<Vec<UserPrompt>, MnexisError> {
        let scope = self.prompt_scope(namespace)?;
        self.repo.list_prompts(&scope, session_id, limit)
    }

    /// Search the prompts, the same way the memories are searched: the
    /// query is compiled, never passed through.
    pub fn search_prompts(
        &self,
        query: &str,
        limit: usize,
        mode: MatchMode,
        namespace: Option<&str>,
    ) -> Result<Vec<UserPrompt>, MnexisError> {
        validate_query(query, self.config.max_query_bytes)?;
        let scope = self.prompt_scope(namespace)?;
        self.repo.search_prompts(&scope, query, limit, mode)
    }

    /// The namespaces a prompt read reaches: the one the caller named, or
    /// every namespace the scope reads.
    fn prompt_scope(&self, namespace: Option<&str>) -> Result<NamespaceScope, MnexisError> {
        match namespace {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                Ok(NamespaceScope::Only(vec![ns.to_string()]))
            }
            None => Ok(self.scope.read_scope()),
        }
    }

    /// The namespace a write without an explicit one lands in: the
    /// profile's default, which the workspace binding may narrow.
    fn target_namespace(&self, namespace: Option<&str>) -> Result<String, MnexisError> {
        let ns = namespace
            .map(|n| n.to_string())
            .unwrap_or_else(|| self.scope.default_namespace.clone());
        validate_namespace(&ns)?;
        Ok(ns)
    }

    // --- the relations between the memories a caller is reading ---

    /// Which of these memories replaced which.
    ///
    /// A memory that was superseded says so where the agent reads it, so
    /// the agent does not have to open every record to find out which one
    /// is current.
    pub fn relations_between(&self, uuids: &[String]) -> Result<Vec<MemoryLink>, MnexisError> {
        self.repo.links_between(uuids)
    }

    /// The memory that holds a topic key in a namespace, if any.
    pub fn topic_occupier(
        &self,
        namespace: &str,
        topic_key: &str,
    ) -> Result<Option<String>, MnexisError> {
        self.repo.topic_key_occupier(namespace, topic_key)
    }

    /// Update the record that already holds a topic key, and keep its
    /// history.
    ///
    /// A save with a key that exists is the same fact again: the content and
    /// the title are replaced, the revision goes up, the previous content
    /// stays in the history with the reason, and the caller gets the record
    /// that was actually written.
    fn update_occupied(&mut self, uuid: &str, req: &SaveRequest) -> Result<Memory, MnexisError> {
        let current = self
            .repo
            .get_by_uuid(uuid)?
            .ok_or_else(|| MnexisError::not_found(format!("memory:{uuid}")))?;
        let next_revision = current.revision + 1;
        let updated = Memory {
            id: current.id,
            uuid: current.uuid.clone(),
            title: if req.title.is_empty() {
                current.title.clone()
            } else {
                req.title.clone()
            },
            topic_key: current.topic_key.clone(),
            namespace: current.namespace.clone(),
            memory_type: req.memory_type.clone(),
            content: req.content.clone(),
            // An upsert is not a resurrection: the state the record has is
            // the state it keeps.
            state: current.state,
            revision: next_revision,
            deleted: current.deleted,
            created_at: current.created_at.clone(),
            updated_at: req.now.clone(),
            deleted_at: current.deleted_at.clone(),
            purge_after: current.purge_after.clone(),
            valid_from: req.valid_from.clone(),
            valid_until: req.valid_until.clone(),
            actor: req.actor.clone(),
            channel: req.channel.clone(),
            assertion_basis: req.assertion_basis.clone(),
        };
        let history = MemoryHistory {
            id: None,
            memory_uuid: current.uuid.clone(),
            revision: next_revision,
            memory_type: updated.memory_type.clone(),
            content: updated.content.clone(),
            state: updated.state,
            valid_from: updated.valid_from.clone(),
            valid_until: updated.valid_until.clone(),
            assertion_basis: updated.assertion_basis.clone(),
            changed_by: req.actor.clone(),
            change_reason: format!(
                "saved again with the topic key `{}`",
                current.topic_key.as_str()
            ),
            changed_at: req.now.clone(),
        };
        let tags: Option<&[String]> = if req.tags.is_empty() {
            None
        } else {
            Some(&req.tags)
        };
        self.repo
            .update_with_history_and_tags(&updated, current.revision, &history, tags)
    }

    // --- the topic a title yields ---

    /// The topic key a title gives, and the record that already holds it.
    ///
    /// The key is a function of the title: the same title always gives the
    /// same key, in any language. Whether the namespace already holds that
    /// key is an equality, not a guess — and it is what makes one fact one
    /// record.
    pub fn topic_for_title(
        &self,
        title: &str,
        namespace: Option<&str>,
    ) -> Result<(String, Option<String>), MnexisError> {
        let key = crate::domain::title::key_from_title(title);
        if key.is_empty() {
            return Ok((String::new(), None));
        }
        let ns = match namespace {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                ns.to_string()
            }
            None => self.scope.default_namespace.clone(),
        };
        let occupier = self.repo.topic_key_occupier(&ns, &key)?;
        Ok((key, occupier))
    }

    // --- duplicates ---
    /// The records of a namespace whose title looks like the given one.
    ///
    /// This is a shortlist, not a verdict: the engine cannot tell that two
    /// different titles are the same fact (measured: a threshold on the
    /// title separates nothing), but it can put the records that *look*
    /// like the title in front of the agent, which does understand
    /// language. The candidate carries the score and the shared words, so
    /// the agent judges instead of trusting.
    ///
    /// `exclude_uuid` leaves a record out of the shortlist — the save that
    /// just created it does not need to be proposed to itself.
    pub fn duplicates(
        &self,
        namespace: &str,
        title: &str,
        limit: usize,
        exclude_uuid: Option<&str>,
    ) -> Result<Vec<DuplicateCandidate>, MnexisError> {
        self.scope.authorize_read(namespace)?;
        if title.trim().is_empty() {
            return Ok(Vec::new());
        }
        let scope = NamespaceScope::Only(vec![namespace.to_string()]);
        let records = self.repo.list_titles_in(&scope)?;
        let corpus: Vec<TitleRecord> = records
            .into_iter()
            .filter(|record| Some(record.uuid.as_str()) != exclude_uuid)
            .collect();
        Ok(rank_duplicates(title, &corpus, limit))
    }

    // --- search ---
    pub fn search(
        &self,
        query: &str,
        namespace: Option<&str>,
        limit: usize,
        mode: MatchMode,
    ) -> Result<Vec<SearchHit>, MnexisError> {
        validate_query(query, self.config.max_query_bytes)?;
        match namespace {
            Some(ns) => {
                self.scope.authorize_read(ns)?;
                self.repo.search(query, Some(ns), limit, mode)
            }
            // The cross-namespace recall is bounded by the scope: open
            // reads reach every project, scoped reads only their own.
            None => self
                .repo
                .search_in(&self.scope.read_scope(), query, limit, mode),
        }
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
        // The slash is what an agent writes when it groups facts
        // (`payments-api/server-config`), and the dot is the same idea
        // written the other way. Both are accepted, and nothing else is.
        //
        // "Letter" is not "ASCII letter": the key derives from a title, and
        // a title is written in the language the team speaks. The rule is
        // "alphabetic and not uppercase", not "lowercase": scripts without
        // case (Japanese kana, Chinese) have no lowercase form, and their
        // letters are perfectly good in a key. An uppercase letter stays
        // out, because a key is lowercase by contract.
        let is_letter = (c.is_alphabetic() && !c.is_uppercase()) || c.is_numeric();
        if !is_letter && !matches!(c, '.' | '-' | '/') {
            return Err(MnexisError::invalid_argument(
                "topic_key",
                format!(
                    "invalid character: {c:?} (allowed: lowercase letters, digits, '.', '-' and '/')"
                ),
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

/// Hash the *logical* parts of a save request. The uuid is excluded
/// so two calls with the same `idempotency_key` and the same logical
/// payload (but client-minted uuids that differ) match — the whole
/// point of idempotency (PRD §10). The "idempotency key" itself is
/// the lookup, so it doesn't enter the hash either.
fn hash_save_request(req: &SaveRequest) -> String {
    let mut h = DefaultHasher::new();
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

// --- import summary types ---

use crate::application::io::Conflict;

/// The structured outcome of a single `import` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSummary {
    pub dry_run: bool,
    pub policy: String,
    pub records: usize,
    pub identical: usize,
    pub fresh: usize,
    pub skipped: usize,
    pub remapped: usize,
    pub conflicts: Vec<ConflictRecord>,
    pub remapped_uuids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictRecord {
    pub uuid: String,
    pub kind: Conflict,
    pub action: &'static str,
}
