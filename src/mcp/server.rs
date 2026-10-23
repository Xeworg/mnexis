//! MCP server implementation (T20; e2-core §3, §8).
//!
//! McpServer wraps MemoryService and exposes the 9 memory_* tools via the
//! rmcp ServerHandler interface. Every tool call maps domain errors to
//! JSON-RPC code -32050 with data.error_code set to the stable label.

use std::sync::{Arc, Mutex};

use rmcp::{
    ServerHandler, ServiceExt, handler::server::wrapper::Parameters, tool, tool_handler,
    tool_router,
};

use crate::ApplicationService;
use crate::MnexisError;
use crate::application::{ListFilter, clamp_duplicate_limit, duplicate_item, duplicates_payload};
use crate::domain::duplicates::DEFAULT_DUPLICATE_LIMIT;
use crate::domain::idempotency::IdempotencyKey;
use crate::domain::memory::MemoryLink;
use crate::mcp::tools::{
    DeleteArgs, GetArgs, HistoryArgs, ListArgs, McpError, RestoreArgs, SaveArgs, SavePromptArgs,
    ScanDuplicatesArgs, SearchArgs, SearchPromptsArgs, SessionEndArgs, SessionStartArgs,
    SuggestTopicKeyArgs, SupersedeArgs, UpdateArgs, clamp_limit, parse_state, parse_tags,
};
use crate::mcp::transport::stdio;
use crate::storage::memory_repo::SqliteMemoryRepository;
use crate::storage::sqlite::open_sqlite;

/// The relations between the memories a caller is reading, in the shape
/// the agent reads them.
fn relations_payload(links: &[MemoryLink]) -> Vec<serde_json::Value> {
    links
        .iter()
        .map(|link| {
            serde_json::json!({
                "older": link.older_uuid,
                "newer": link.newer_uuid,
                "type": link.link_type,
                "reason": link.reason,
            })
        })
        .collect()
}

/// How many recent memories the `memory_workspace` tool reports.
const WORKSPACE_RECENT_LIMIT: usize = 3;

/// The MCP server — owns the MemoryService and exposes 16 tools.
///
/// All tool functions take `&self`; the Arc<Mutex> provides interior
/// mutability so the service can be mutated without needing `&mut self`.
/// (`MemoryService<SqliteMemoryRepository>` is Send via rusqlite's
/// Connection, so Arc<Mutex<..>> is Send + Sync by the auto traits.)
#[derive(Clone)]
pub struct McpServer {
    service: Arc<Mutex<ApplicationService>>,
    default_namespace: String,
    /// The profile directory the workspace bindings live in. The status
    /// tool resolves the workspace of the process from it, exactly like the
    /// CLI does.
    data_dir: std::path::PathBuf,
    /// The namespace the workspace this server runs in writes into, and
    /// the key that identifies it. `None` means the server runs outside
    /// any known project: the profile allowlist keeps deciding.
    workspace_namespace: Option<String>,
    workspace_key: String,
}
impl McpServer {
    /// Build a McpServer from a data directory.
    ///
    /// Opens the SQLite database, loads the profile settings to construct
    /// the AccessScope, and builds the MemoryService. Fails fast with
    /// the service's NotFound error if the database is missing.
    pub fn new(data_dir: std::path::PathBuf) -> Result<Self, MnexisError> {
        let db_path = data_dir.join("mnexis.db");
        if !db_path.exists() {
            // Same contract as the CLI: the profile must be initialised
            // explicitly (`mnexis init`); a server must not silently create
            // one under an unintended data directory.
            return Err(MnexisError::not_found(format!(
                "profile at {} (run `mnexis init` first)",
                data_dir.display()
            )));
        }
        let conn = open_sqlite(&db_path)?;
        let (settings, _) =
            crate::config::ProfileSettings::load_or_default(&data_dir.join("mnexis.toml"))?;
        let repo = SqliteMemoryRepository::new(conn);
        let default_namespace = settings.default_namespace.clone();
        let service = crate::application::MemoryService::new(
            repo,
            settings.to_access_scope()?,
            settings.to_service_config(),
        );
        // The workspace this server runs in: the same rule the CLI uses
        // for its own writes (project-isolation S5, S9).
        let workspace = crate::application::workspace::current_workspace(&data_dir)?;
        let (workspace_namespace, workspace_key) = match workspace {
            Some(workspace) => (workspace.namespace, workspace.key),
            None => (None, String::new()),
        };
        Ok(Self {
            service: Arc::new(Mutex::new(service)),
            default_namespace,
            data_dir,
            workspace_namespace,
            workspace_key,
        })
    }

    /// Refuse a write that leaves the workspace this server runs in.
    ///
    /// The scope check still applies on top of this one: this guard
    /// answers FORBIDDEN with the namespace the workspace accepts, and
    /// the service still authorizes the write itself.
    fn authorize_write(&self, namespace: &str) -> Result<(), McpError> {
        let Some(allowed) = self.workspace_namespace.as_deref() else {
            return Ok(());
        };
        if allowed == namespace {
            return Ok(());
        }
        Err(MnexisError::forbidden(
            "write",
            format!(
                "this workspace writes only into namespace '{allowed}'; \
                 run `mnexis project bind <namespace> --key {} --force` to change it",
                self.workspace_key
            ),
        )
        .into())
    }

    /// Refuse a write whose target is the namespace of `uuid`.
    ///
    /// An unknown uuid is left to the service, which answers NOT_FOUND on
    /// its own.
    fn authorize_write_memory(&self, svc: &ApplicationService, uuid: &str) -> Result<(), McpError> {
        match svc.namespace_of(uuid)? {
            Some(namespace) => self.authorize_write(&namespace),
            None => Ok(()),
        }
    }

    /// Access the service for direct use (e.g. in tests).
    #[cfg(test)]
    pub fn service(&self) -> &Arc<Mutex<ApplicationService>> {
        &self.service
    }

    // -------------------------------------------------------------------------
    // Tool implementations — all take `&self`; interior mutability via Arc<Mutex>
    // -------------------------------------------------------------------------

    fn do_save(&self, args: SaveArgs) -> Result<String, McpError> {
        let uuid = uuid_v4_mcp()?;
        let now = now_rfc3339_mcp();
        let state = parse_state(args.state.clone())?;
        let namespace = args
            .namespace
            .unwrap_or_else(|| self.default_namespace.clone());
        self.authorize_write(&namespace)?;
        let req = crate::application::SaveRequest {
            uuid: uuid.clone(),
            title: args.title.unwrap_or_default(),
            topic_key: args.topic_key.unwrap_or_default(),
            namespace,
            memory_type: args.memory_type.unwrap_or_else(|| "fact".to_string()),
            content: args.content,
            actor: "mcp".to_string(),
            channel: "mcp".to_string(),
            assertion_basis: args
                .assertion_basis
                .unwrap_or_else(|| "user_stated".to_string()),
            valid_from: args.valid_from,
            valid_until: args.valid_until,
            tags: parse_tags(args.tags),
            idempotency_key: args.idempotency_key.map(IdempotencyKey::new),
            state,
            now,
        };
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        // The service honours the idempotency-key contract: if the
        // call is a replay, the returned Memory is the *original*
        // record (canonical uuid + rowid), not a fresh one. We use
        // the returned values so the JSON envelope matches what a
        // real client expects to see.
        let stored = svc.save(req)?;
        // The save says what the namespace already holds: the record it may
        // duplicate, at the moment it would create it.
        let candidates = svc.duplicates_of(&stored)?;
        let (duplicates, _) = duplicates_payload(&stored, &candidates);
        let returned_uuid = stored.uuid.clone();
        let returned_row_id = stored.id.unwrap_or(0);
        let json = serde_json::json!({
            "uuid": returned_uuid,
            "row_id": returned_row_id,
            "duplicates": duplicates,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    fn do_get(&self, args: GetArgs) -> Result<String, McpError> {
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        match svc.get(&args.uuid)? {
            Some(mem) => Ok(serde_json::to_string_pretty(&mem).unwrap_or_default()),
            // NOT_FOUND must not leak the UUID in the response per S10/A-04.
            None => Err(MnexisError::not_found("memory").into()),
        }
    }

    fn do_list(&self, args: ListArgs) -> Result<String, McpError> {
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let state = args
            .state
            .clone()
            .map(|s| parse_state(Some(s)))
            .transpose()?;
        let filter = ListFilter {
            namespace: args.namespace.clone(),
            state,
            limit: clamp_limit(args.limit),
            offset: args.offset,
        };
        let memories = svc.list(filter)?;
        // Which of these replaced which: the agent sees it in the same
        // answer, so it does not have to open every record to know which
        // one is current.
        let uuids: Vec<String> = memories.iter().map(|m| m.uuid.clone()).collect();
        let relations = svc.relations_between(&uuids)?;
        drop(svc);
        // A list is a list: the identity of every memory and a preview of
        // its content. `full` asks for the whole shape.
        let items = if args.full.unwrap_or(false) {
            serde_json::to_value(&memories).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::to_value(crate::application::previews(&memories))
                .unwrap_or(serde_json::Value::Null)
        };
        let json = serde_json::json!({
            "count": memories.len(),
            "items": items,
            "relations": relations_payload(&relations),
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    /// Register the session the caller runs in.
    fn do_session_start(&self, args: SessionStartArgs) -> Result<String, McpError> {
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let now = crate::cli::profile::now_rfc3339();
        let session = svc.start_session(
            &args.session_id,
            args.namespace.as_deref(),
            args.directory.as_deref().unwrap_or_default(),
            &now,
        )?;
        Ok(serde_json::to_string_pretty(&session).unwrap_or_default())
    }

    /// End a session, with an optional summary.
    fn do_session_end(&self, args: SessionEndArgs) -> Result<String, McpError> {
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let now = crate::cli::profile::now_rfc3339();
        let session = svc.end_session(&args.session_id, args.summary.as_deref(), &now)?;
        Ok(serde_json::to_string_pretty(&session).unwrap_or_default())
    }

    /// Store what the user said.
    fn do_save_prompt(&self, args: SavePromptArgs) -> Result<String, McpError> {
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let now = crate::cli::profile::now_rfc3339();
        let prompt = svc.save_prompt(
            &args.content,
            args.session_id.as_deref(),
            args.namespace.as_deref(),
            &now,
        )?;
        Ok(serde_json::to_string_pretty(&prompt).unwrap_or_default())
    }

    /// The topic key a title yields, and the record that already holds it.
    fn do_suggest_topic_key(&self, args: SuggestTopicKeyArgs) -> Result<String, McpError> {
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let (key, occupier) = svc.topic_for_title(&args.title, args.namespace.as_deref())?;
        drop(svc);
        let json = serde_json::json!({
            "title": args.title,
            "topic_key": key,
            "recorded_by": occupier,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    /// The records of a namespace whose titles look like the given one.
    ///
    /// The same answer the save carries, asked on demand: the agent can look
    /// before it saves, instead of learning about the record it duplicated
    /// afterwards.
    fn do_scan_duplicates(&self, args: ScanDuplicatesArgs) -> Result<String, McpError> {
        let namespace = args
            .namespace
            .unwrap_or_else(|| self.default_namespace.clone());
        let limit = clamp_duplicate_limit(args.limit.unwrap_or(DEFAULT_DUPLICATE_LIMIT));
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        // The shortlist is a read of the namespace: the caller's scope
        // decides, the same way it decides every other read of that
        // namespace.
        let candidates = svc.duplicates(&namespace, &args.title, limit, None)?;
        drop(svc);
        let items: Vec<serde_json::Value> = candidates.iter().map(duplicate_item).collect();
        let json = serde_json::json!({
            "title": args.title,
            "namespace": namespace,
            "count": items.len(),
            "items": items,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    /// Search what the user said.
    fn do_search_prompts(&self, args: SearchPromptsArgs) -> Result<String, McpError> {
        let limit = clamp_limit(args.limit);
        let mode = match args.match_mode.as_deref() {
            Some(name) => crate::domain::query::MatchMode::parse(name)?,
            None => crate::domain::query::MatchMode::default(),
        };
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let prompts = svc.search_prompts(&args.query, limit, mode, args.namespace.as_deref())?;
        drop(svc);
        let json = serde_json::json!({
            "count": prompts.len(),
            "items": prompts,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    /// The status payload: where this workspace may write, and what is
    /// already there.
    ///
    /// The payload is built by the same function the `mnexis status`
    /// command renders, so the CLI and the agent cannot drift.
    fn do_workspace(&self) -> Result<String, McpError> {
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let status = crate::application::workspace_status::workspace_status(
            &svc,
            &self.data_dir,
            WORKSPACE_RECENT_LIMIT,
        )?;
        drop(svc);
        Ok(serde_json::to_string_pretty(&status).unwrap_or_default())
    }

    fn do_update(&self, args: UpdateArgs) -> Result<String, McpError> {
        let now = now_rfc3339_mcp();
        let state = args
            .state
            .clone()
            .map(|s| parse_state(Some(s)))
            .transpose()?;
        let req = crate::application::UpdateRequest {
            uuid: args.uuid.clone(),
            expected_revision: args.expected_revision,
            content: args.content,
            state,
            tags: args.tags.as_ref().map(|s| parse_tags(Some(s.clone()))),
            assertion_basis: args.assertion_basis,
            reason: args.reason,
            now,
        };
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        self.authorize_write_memory(&svc, &args.uuid)?;
        let updated = svc.update(req)?;
        drop(svc);
        let json = serde_json::json!({
            "uuid": updated.uuid,
            "revision": updated.revision,
            "state": updated.state.as_str(),
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    fn do_history(&self, args: HistoryArgs) -> Result<String, McpError> {
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        let entries = svc.history(&args.uuid)?;
        drop(svc);
        let shown: Vec<_> = match args.limit {
            Some(l) => entries.iter().rev().take(l).cloned().collect(),
            None => entries,
        };
        let items: Vec<_> = shown
            .iter()
            .map(|h| {
                serde_json::json!({
                    "revision": h.revision,
                    "memory_type": h.memory_type,
                    "state": h.state.as_str(),
                    "content": h.content,
                    "assertion_basis": h.assertion_basis,
                    "valid_from": h.valid_from,
                    "valid_until": h.valid_until,
                    "changed_by": h.changed_by,
                    "change_reason": h.change_reason,
                    "changed_at": h.changed_at,
                })
            })
            .collect();
        let json = serde_json::json!({
            "uuid": args.uuid,
            "count": items.len(),
            "items": items,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    fn do_supersede(&self, args: SupersedeArgs) -> Result<String, McpError> {
        let now = now_rfc3339_mcp();
        let req = crate::application::SupersedeRequest {
            older_uuid: args.older_uuid.clone(),
            newer_uuid: args.newer_uuid.clone(),
            expected_old_revision: args.expected_old_revision,
            expected_new_revision: args.expected_new_revision,
            reason: args.reason,
            now,
        };
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        self.authorize_write_memory(&svc, &args.older_uuid)?;
        self.authorize_write_memory(&svc, &args.newer_uuid)?;
        svc.supersede(req)?;
        drop(svc);
        let json = serde_json::json!({
            "older": args.older_uuid,
            "newer": args.newer_uuid,
            "ok": true,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    fn do_delete(&self, args: DeleteArgs) -> Result<String, McpError> {
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        self.authorize_write_memory(&svc, &args.uuid)?;
        svc.soft_delete(&args.uuid, args.expected_revision)?;
        drop(svc);
        let json = serde_json::json!({
            "deleted": args.uuid,
            "expected_revision": args.expected_revision,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    fn do_restore(&self, args: RestoreArgs) -> Result<String, McpError> {
        let mut svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        self.authorize_write_memory(&svc, &args.uuid)?;
        svc.restore(&args.uuid)?;
        drop(svc);
        let json = serde_json::json!({
            "restored": args.uuid,
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }

    fn do_search(&self, args: SearchArgs) -> Result<String, McpError> {
        let limit = clamp_limit(args.limit);
        let svc = self
            .service
            .lock()
            .map_err(|e| MnexisError::busy(format!("service lock poisoned: {e}")))?;
        // Out-of-scope namespace denial follows the profile's configured
        // existence_disclosure (service-enforced); no layer-2 remapping.
        let mode = match args.match_mode.as_deref() {
            Some(name) => crate::domain::query::MatchMode::parse(name)?,
            None => crate::domain::query::MatchMode::default(),
        };
        let hits = svc.search(&args.query, args.namespace.as_deref(), limit, mode)?;
        // Which of the hits replaced which.
        let hit_uuids: Vec<String> = hits.iter().map(|h| h.uuid.clone()).collect();
        let relations = svc.relations_between(&hit_uuids)?;
        drop(svc);
        let items: Vec<_> = hits
            .iter()
            .map(|h| {
                serde_json::json!({
                    "uuid": h.uuid,
                    "rank": h.rank,
                    "namespace": h.namespace,
                    "memory_type": h.memory_type,
                    "state": h.state,
                    "revision": h.revision,
                    "created_at": h.created_at,
                    "valid_from": h.valid_from,
                    "valid_until": h.valid_until,
                })
            })
            .collect();
        let json = serde_json::json!({
            "query": args.query,
            "namespace": args.namespace,
            "count": hits.len(),
            "items": items,
            "relations": relations_payload(&relations),
        });
        Ok(serde_json::to_string_pretty(&json).unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------
// rmcp ServerHandler
// ---------------------------------------------------------------------------

#[tool_router]
impl McpServer {
    #[tool(
        name = "memory_save",
        description = "Save a memory that must survive the session: a decision, a discovery, a convention, a user preference. Call it right after such a milestone, and store what the user asked to remember, in the namespace of this workspace."
    )]
    fn memory_save(&self, Parameters(args): Parameters<SaveArgs>) -> Result<String, McpError> {
        self.do_save(args)
    }

    #[tool(name = "memory_get", description = "Get a memory by UUID.")]
    fn memory_get(&self, Parameters(args): Parameters<GetArgs>) -> Result<String, McpError> {
        self.do_get(args)
    }

    #[tool(
        name = "memory_list",
        description = "List the memories this project already holds, with optional namespace/state filters and pagination. Use it at the start of a session, together with memory_workspace, to see what the project knows."
    )]
    fn memory_list(&self, Parameters(args): Parameters<ListArgs>) -> Result<String, McpError> {
        self.do_list(args)
    }

    #[tool(
        name = "memory_workspace",
        description = "Call this FIRST, at the start of every session and before you change code that already exists: it returns the namespace this workspace writes into, the read and write allowlists, how many memories the scope can see, and the most recent of them. The decisions behind the code live here, not in the files."
    )]
    fn memory_workspace(&self) -> Result<String, McpError> {
        self.do_workspace()
    }

    #[tool(
        name = "memory_session_start",
        description = "Register the session you are running in, so the engine knows its boundaries. Call it once, at the start of the session, with the session id you were given; the id is yours, never invented. The engine never ends a session for you."
    )]
    fn memory_session_start(
        &self,
        Parameters(args): Parameters<SessionStartArgs>,
    ) -> Result<String, McpError> {
        self.do_session_start(args)
    }

    #[tool(
        name = "memory_session_end",
        description = "End the session you are running in, with a short summary of what it did. The summary is what the next session reads first."
    )]
    fn memory_session_end(
        &self,
        Parameters(args): Parameters<SessionEndArgs>,
    ) -> Result<String, McpError> {
        self.do_session_end(args)
    }

    #[tool(
        name = "memory_save_prompt",
        description = "Store what the user said, verbatim. Use it when the user states a constraint, a preference or a goal that has to survive the session: the conversation is not in the repository, and this is where it goes."
    )]
    fn memory_save_prompt(
        &self,
        Parameters(args): Parameters<SavePromptArgs>,
    ) -> Result<String, McpError> {
        self.do_save_prompt(args)
    }

    #[tool(
        name = "memory_search_prompts",
        description = "Search what the user said before, across sessions. Use it when you need the request behind the work: the prompt is not in the files."
    )]
    fn memory_search_prompts(
        &self,
        Parameters(args): Parameters<SearchPromptsArgs>,
    ) -> Result<String, McpError> {
        self.do_search_prompts(args)
    }

    #[tool(
        name = "memory_suggest_topic_key",
        description = "Call this BEFORE you save a fact: it answers which topic key the title you are about to use yields, and whether the namespace already records it. The same title always gives the same key, so the same fact lands on the same record."
    )]
    fn memory_suggest_topic_key(
        &self,
        Parameters(args): Parameters<SuggestTopicKeyArgs>,
    ) -> Result<String, McpError> {
        self.do_suggest_topic_key(args)
    }

    #[tool(
        name = "memory_scan_duplicates",
        description = "Call this BEFORE you save a fact you are not sure about, or when a save returned duplicates: it returns the records of the namespace whose titles look like the one you give, each with a score and the words it shares with your title. The engine proposes and you decide: if one of them is the same fact, link them with `mnexis supersede --older <candidate> --newer <saved> --reason \"same fact, different title\"`."
    )]
    fn memory_scan_duplicates(
        &self,
        Parameters(args): Parameters<ScanDuplicatesArgs>,
    ) -> Result<String, McpError> {
        self.do_scan_duplicates(args)
    }

    #[tool(
        name = "memory_update",
        description = "Update a memory's content/state/tags. Requires expected_revision for optimistic locking."
    )]
    fn memory_update(&self, Parameters(args): Parameters<UpdateArgs>) -> Result<String, McpError> {
        self.do_update(args)
    }

    #[tool(
        name = "memory_history",
        description = "Return the revision history for a memory."
    )]
    fn memory_history(
        &self,
        Parameters(args): Parameters<HistoryArgs>,
    ) -> Result<String, McpError> {
        self.do_history(args)
    }

    #[tool(
        name = "memory_supersede",
        description = "Atomically link an older memory as superseded by a newer one."
    )]
    fn memory_supersede(
        &self,
        Parameters(args): Parameters<SupersedeArgs>,
    ) -> Result<String, McpError> {
        self.do_supersede(args)
    }

    #[tool(
        name = "memory_delete",
        description = "Soft-delete a memory (sets deleted=1, removes from FTS5)."
    )]
    fn memory_delete(&self, Parameters(args): Parameters<DeleteArgs>) -> Result<String, McpError> {
        self.do_delete(args)
    }

    #[tool(
        name = "memory_restore",
        description = "Restore a soft-deleted memory (clears deleted fields, re-indexes in FTS5)."
    )]
    fn memory_restore(
        &self,
        Parameters(args): Parameters<RestoreArgs>,
    ) -> Result<String, McpError> {
        self.do_restore(args)
    }

    #[tool(
        name = "memory_search",
        description = "FTS5 ranked search over the memories of this project. Use it BEFORE changing existing code: the decisions, conventions and rejected approaches behind that code are recorded here and are not visible in the files. Returns the memories ordered by relevance."
    )]
    fn memory_search(&self, Parameters(args): Parameters<SearchArgs>) -> Result<String, McpError> {
        self.do_search(args)
    }
}

#[tool_handler]
impl ServerHandler for McpServer {}

/// Start the MCP stdio server.
///
/// Fails fast with the service's NotFound error if the database is missing.
pub async fn serve_stdio(
    data_dir: std::path::PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = McpServer::new(data_dir)?;
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Local UUIDv4 and RFC3339 helpers (duplicated from cli::profile;
// consolidation is a later refactor — noted in T20)
// ---------------------------------------------------------------------------

fn uuid_v4_mcp() -> Result<String, MnexisError> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .map_err(|e| {
            MnexisError::invalid_argument("uuid", format!("cannot read /dev/urandom: {e}"))
        })?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    ))
}

fn now_rfc3339_mcp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v4_mcp_produces_valid_uuid() {
        let uuid = uuid_v4_mcp().unwrap();
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(
            parts.iter().map(|p| p.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert_eq!(&parts[2][0..1], "4"); // version nibble
    }

    #[test]
    fn now_rfc3339_mcp_format() {
        let ts = now_rfc3339_mcp();
        assert!(ts.ends_with('Z'));
        assert!(ts.len() == 20);
        assert!(ts.contains('T'));
    }
}
