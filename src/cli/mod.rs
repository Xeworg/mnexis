//! The `mnexis` command-line interface (PRD §8, e2-core §7).
//!
//! Every command is a thin wrapper over [`MemoryService`] under the scope
//! loaded from `mnexis.toml`. Exit codes 2–10 map to the stable error set
//! (e2-core §8); `--json` switches output to structured envelopes.

pub mod agent_cmd;
pub mod commands;
pub mod config_cmd;
pub mod output;
pub mod profile;
pub mod project_cmd;
pub mod update_cmd;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::cli::output::CommandOutcome;
use crate::domain::error::MnexisError;
use profile::Profile;

#[derive(Debug, Parser)]
#[command(
    name = "mnexis",
    version,
    about = "Local-first memory engine for AI agents"
)]
pub struct Cli {
    /// Profile data directory (default: $MNEXIS_DATA_DIR or
    /// $HOME/.local/share/mnexis)
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,

    /// Structured JSON output
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print version and exit
    Version,

    /// Initialise a new profile (mnexis.db + mnexis.toml)
    Init,

    /// Run the MCP stdio server
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },

    /// Retrieve a budgeted context for a task (PRD §9, E3)
    Context {
        /// Namespace to search within (default: scope's first read namespace)
        #[arg(long)]
        namespace: Option<String>,
        /// Token budget for the response envelope
        #[arg(long, default_value_t = 1024)]
        token_budget: i64,
        /// Supported tokenizer id (default: supported-tokenizer-v1)
        #[arg(long, value_name = "ID")]
        tokenizer_id: Option<String>,
        /// Maximum items to consider (default 20, capped at 100)
        #[arg(long, default_value_t = 20)]
        max_items: usize,
        /// Budget enforcement: estimated (default) | exact
        #[arg(long, default_value = "estimated", value_name = "MODE")]
        budget_mode: String,
        /// Task / query (positional)
        task: String,
    },

    /// Save a new memory
    Save {
        /// Namespace (default: the profile's default_namespace); a bound
        /// workspace only accepts its own namespace
        #[arg(long)]
        namespace: Option<String>,
        /// Memory type (default: fact)
        #[arg(long)]
        r#type: Option<String>,
        /// Assertion basis (default: user_stated)
        #[arg(long)]
        basis: Option<String>,
        /// Topic key (unique per namespace when non-empty; allowed: lowercase letters, digits, '.' and '-')
        #[arg(long)]
        topic_key: Option<String>,
        /// Idempotency key for safe retries
        #[arg(long)]
        idempotency_key: Option<String>,
        /// Valid-from timestamp (RFC3339)
        #[arg(long)]
        valid_from: Option<String>,
        /// Valid-until timestamp (RFC3339)
        #[arg(long)]
        valid_until: Option<String>,
        /// Comma-separated tags (max 32)
        #[arg(long, value_name = "TAG[,...]")]
        tags: Option<String>,
        /// Initial state: active (default) or proposed
        #[arg(long)]
        state: Option<String>,
        /// Memory content (positional)
        content: String,
    },

    /// Update a memory (optimistic revision)
    Update {
        uuid: String,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long, value_name = "TAG[,...]")]
        tags: Option<String>,
        #[arg(long)]
        basis: Option<String>,
        #[arg(long)]
        expected_revision: i32,
        #[arg(long)]
        reason: String,
    },

    /// Mark an older memory as superseded by a newer one
    Supersede {
        #[arg(long)]
        older: String,
        #[arg(long)]
        newer: String,
        #[arg(long)]
        expected_old_revision: Option<i32>,
        #[arg(long)]
        expected_new_revision: Option<i32>,
        #[arg(long)]
        reason: String,
    },

    /// FTS5 ranked search
    Search {
        #[arg(long)]
        namespace: Option<String>,
        /// Max results (default 20, capped at 100)
        #[arg(long, default_value_t = commands::DEFAULT_LIMIT)]
        limit: usize,
        /// Per-hit rank and matched terms
        #[arg(long)]
        explain: bool,
        query: String,
    },

    /// List memories
    List {
        #[arg(long)]
        namespace: Option<String>,
        #[arg(long)]
        state: Option<String>,
        #[arg(long, default_value_t = commands::DEFAULT_LIMIT)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },

    /// Show a memory (current or specific revision)
    Show {
        uuid: String,
        #[arg(long)]
        revision: Option<i32>,
    },

    /// Show revision history
    History {
        uuid: String,
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Soft-delete a memory (prompts unless --force)
    Delete {
        #[arg(long)]
        force: bool,
        #[arg(long)]
        expected_revision: Option<i32>,
        uuid: String,
    },

    /// Restore a soft-deleted memory
    Restore { uuid: String },

    /// Purge soft-deleted memories (dry-run unless --apply)
    Purge {
        /// Actually purge (default is a dry-run)
        #[arg(long)]
        apply: bool,
        /// Purge all soft-deleted memories regardless of purge_after
        #[arg(long)]
        all: bool,
        /// ISO-8601 cutoff: purge soft-deleted with purge_after <= DATE
        #[arg(long)]
        before: Option<String>,
    },

    /// Rebuild the FTS5 index from canonical data
    Reindex,

    /// Check SQLite and FTS5 health; optionally repair the FTS index
    Doctor {
        /// Drop and rebuild memories_fts, re-create all FTS5 triggers,
        /// then reindex every active+non-deleted row.
        #[arg(long)]
        repair: bool,
    },

    /// Browse and search memories in a terminal UI
    Tui,

    /// Check for a newer release and install it
    ///
    /// The name is `upgrade` and not `update`: `mnexis update` already
    /// updates a memory in place.
    Upgrade {
        /// Only report whether a newer release exists
        #[arg(long)]
        check: bool,
        /// Install that version instead of the latest one
        #[arg(long, value_name = "VERSION")]
        version: Option<String>,
        /// Ask again even when the check is cached
        #[arg(long)]
        force: bool,
    },

    /// Show effective configuration and scope
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Manage workspace → namespace bindings
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },

    /// Install the agent integration (skill and pi extension) from the binary
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },

    /// Export memories as JSONL (excludes deleted by default)
    Export {
        /// Also include soft-deleted records
        #[arg(long)]
        include_deleted: bool,
        /// Output file (default: stdout)
        #[arg(long, value_name = "FILE")]
        output: Option<std::path::PathBuf>,
    },

    /// Import memories from JSONL (default policy: reject on conflict)
    Import {
        /// Validate and report, do not write
        #[arg(long)]
        dry_run: bool,
        /// Conflict policy: reject | skip | remap (default: reject)
        #[arg(long, default_value = "reject")]
        policy: String,
        /// Input file (default: stdin)
        #[arg(long, value_name = "FILE")]
        input: Option<std::path::PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum McpAction {
    /// Run the MCP stdio server
    Serve,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Print the effective configuration
    Show,
    /// Authorize a namespace for read and write in the profile
    AllowNamespace {
        /// Namespace to authorize
        namespace: String,
    },
    /// Allow (or revoke) the reserved `global` namespace
    AllowGlobal {
        /// Revoke the permission instead of granting it
        #[arg(long)]
        revoke: bool,
    },
    /// Allow (or revoke) reads across every namespace
    AllowReadAll {
        /// Revoke the permission instead of granting it
        #[arg(long)]
        revoke: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    /// Bind a workspace key to a namespace
    Bind {
        /// Namespace the workspace writes into
        namespace: String,
        /// Workspace identity (e.g. `git:github.com/owner/repo` or `path:/abs/dir`)
        #[arg(long, value_name = "KEY")]
        key: String,
        /// Workspace path (informational)
        #[arg(long, value_name = "DIR")]
        path: Option<String>,
        /// Move an already-bound key to another namespace
        #[arg(long)]
        force: bool,
    },
    /// List the recorded bindings
    List,
    /// Remove the bindings for a namespace (memories are untouched)
    Unbind {
        /// Namespace to unbind
        namespace: String,
    },
}

/// The `mnexis agent` actions.
#[derive(Debug, Subcommand)]
pub enum AgentAction {
    /// Install the skill (and the extension, where supported) for an environment
    Install {
        /// Target environment (pi, opencode)
        #[arg(long, value_name = "NAME")]
        env: Option<String>,
        /// Which assets to install (all, skill, extension)
        #[arg(long, value_name = "WHAT")]
        only: Option<String>,
    },
    /// Remove the installed assets for an environment
    Uninstall {
        /// Target environment (pi, opencode)
        #[arg(long, value_name = "NAME")]
        env: Option<String>,
        /// Which assets to remove (all, skill, extension)
        #[arg(long, value_name = "WHAT")]
        only: Option<String>,
    },
    /// Show what is installed for every environment
    Status,
}

/// Parse and run. Returns the process exit code.
pub async fn run(cli: Cli) -> i32 {
    let mode = output::OutputMode::from_flag(cli.json);
    // The MCP server owns its own lifecycle; it is not a service command.
    // A startup failure is infrastructure outside the stable error set
    // (the dominant case: missing profile), so it exits 1 with the raw cause.
    if matches!(
        &cli.command,
        Command::Mcp {
            action: McpAction::Serve
        }
    ) {
        let data_dir = match profile::resolve_data_dir(cli.data_dir.clone()) {
            Ok(dir) => dir,
            Err(err) => {
                eprintln!("{}", output::render_error(&err, mode));
                return output::exit_code(&err);
            }
        };
        return match crate::mcp::serve_stdio(data_dir).await {
            Ok(()) => {
                eprintln!("mcp serve stopped");
                0
            }
            Err(e) => {
                eprintln!("error: mcp serve: {e}");
                1
            }
        };
    }
    match execute(&cli).await {
        Ok(outcome) => {
            println!("{}", outcome.render(mode));
            outcome.exit_code
        }
        Err(err) => {
            eprintln!("{}", output::render_error(&err, mode));
            output::exit_code(&err)
        }
    }
}

async fn execute(cli: &Cli) -> Result<CommandOutcome, MnexisError> {
    let data_dir = profile::resolve_data_dir(cli.data_dir.clone())?;
    match &cli.command {
        Command::Version => Ok(commands::version()),
        Command::Init => commands::init(data_dir),
        Command::Mcp {
            action: McpAction::Serve,
        } => {
            // Unreachable: run() intercepts the server lifecycle before
            // dispatching (see run). Kept for exhaustiveness.
            unreachable!("mcp serve is handled in run()")
        }
        Command::Context {
            namespace,
            token_budget,
            tokenizer_id,
            max_items,
            budget_mode,
            task,
        } => {
            let profile = Profile::open(data_dir)?;
            commands::context(
                &profile,
                namespace.clone(),
                *token_budget,
                tokenizer_id.clone(),
                *max_items,
                budget_mode,
                task.clone(),
            )
        }
        Command::Save {
            namespace,
            r#type,
            basis,
            topic_key,
            idempotency_key,
            valid_from,
            valid_until,
            tags,
            state,
            content,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::save(
                &mut profile,
                namespace.clone(),
                r#type.clone(),
                basis.clone(),
                topic_key.clone(),
                idempotency_key.clone(),
                valid_from.clone(),
                valid_until.clone(),
                tags.clone(),
                state.clone(),
                content.clone(),
            )
        }
        Command::Update {
            uuid,
            content,
            state,
            tags,
            basis,
            expected_revision,
            reason,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::update(
                &mut profile,
                uuid.clone(),
                content.clone(),
                state.clone(),
                tags.clone(),
                basis.clone(),
                *expected_revision,
                reason.clone(),
            )
        }
        Command::Supersede {
            older,
            newer,
            expected_old_revision,
            expected_new_revision,
            reason,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::supersede(
                &mut profile,
                older.clone(),
                newer.clone(),
                *expected_old_revision,
                *expected_new_revision,
                reason.clone(),
            )
        }
        Command::Search {
            namespace,
            limit,
            explain,
            query,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::search(
                &mut profile,
                query.clone(),
                namespace.clone(),
                *limit,
                *explain,
            )
        }
        Command::List {
            namespace,
            state,
            limit,
            offset,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::list(
                &mut profile,
                namespace.clone(),
                state.clone(),
                *limit,
                *offset,
            )
        }
        Command::Show { uuid, revision } => {
            let mut profile = Profile::open(data_dir)?;
            commands::show(&mut profile, uuid.clone(), *revision)
        }
        Command::History { uuid, limit } => {
            let mut profile = Profile::open(data_dir)?;
            commands::history(&mut profile, uuid.clone(), *limit)
        }
        Command::Delete {
            force,
            expected_revision,
            uuid,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::delete(&mut profile, uuid.clone(), *force, *expected_revision)
        }
        Command::Restore { uuid } => {
            let mut profile = Profile::open(data_dir)?;
            commands::restore(&mut profile, uuid.clone())
        }
        Command::Purge { apply, before, all } => {
            let mut profile = Profile::open(data_dir)?;
            commands::purge(&mut profile, *apply, before.clone(), *all)
        }
        Command::Reindex => {
            let mut profile = Profile::open(data_dir)?;
            commands::reindex(&mut profile)
        }
        Command::Doctor { repair } => {
            let profile = Profile::open(data_dir)?;
            commands::doctor(&profile, *repair)
        }
        Command::Export {
            include_deleted,
            output,
        } => {
            let profile = Profile::open(data_dir)?;
            commands::export(&profile, *include_deleted, output.clone())
        }
        Command::Import {
            dry_run,
            policy,
            input,
        } => {
            let mut profile = Profile::open(data_dir)?;
            commands::import(&mut profile, *dry_run, policy, input.clone())
        }
        Command::Config {
            action: ConfigAction::Show,
        } => config_cmd::show(data_dir),
        Command::Config {
            action: ConfigAction::AllowNamespace { namespace },
        } => config_cmd::allow_namespace(data_dir, namespace.clone()),
        Command::Config {
            action: ConfigAction::AllowGlobal { revoke },
        } => config_cmd::set_permission(data_dir, config_cmd::Permission::Global, !*revoke),
        Command::Config {
            action: ConfigAction::AllowReadAll { revoke },
        } => config_cmd::set_permission(data_dir, config_cmd::Permission::ReadAll, !*revoke),
        Command::Project {
            action:
                ProjectAction::Bind {
                    namespace,
                    key,
                    path,
                    force,
                },
        } => project_cmd::bind(
            data_dir,
            namespace.clone(),
            key.clone(),
            path.clone(),
            *force,
        ),
        Command::Agent {
            action: AgentAction::Install { env, only },
        } => agent_cmd::install(env.as_ref(), only.as_ref()),
        Command::Agent {
            action: AgentAction::Uninstall { env, only },
        } => agent_cmd::uninstall(env.as_ref(), only.as_ref()),
        Command::Agent {
            action: AgentAction::Status,
        } => agent_cmd::show(),
        Command::Project {
            action: ProjectAction::List,
        } => project_cmd::list(data_dir),
        Command::Project {
            action: ProjectAction::Unbind { namespace },
        } => project_cmd::unbind(data_dir, namespace.clone()),
        Command::Tui => {
            // The TUI owns the terminal lifecycle; profile resolution and
            // error reporting stay identical to every other command.
            let mut profile = Profile::open(data_dir)?;
            crate::tui::run(&mut profile)?;
            Ok(CommandOutcome::new(
                serde_json::json!({ "closed": true }),
                "tui closed",
            ))
        }
        Command::Upgrade {
            check,
            version,
            force,
        } => update_cmd::update(data_dir, *check, version.clone(), *force),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_help_lists_every_e2_command_group() {
        let help = Cli::command().render_help().to_string();
        for word in [
            "version",
            "init",
            "mcp",
            "save",
            "update",
            "supersede",
            "search",
            "list",
            "show",
            "history",
            "delete",
            "restore",
            "purge",
            "reindex",
            "tui",
            "config",
            "project",
        ] {
            assert!(help.contains(word), "help must mention {word}: {help}");
        }
    }

    #[test]
    fn mcp_serve_is_a_nested_subcommand() {
        let cmd = Cli::command();
        let mcp = cmd.find_subcommand("mcp").expect("mcp subcommand");
        assert!(mcp.get_subcommands().any(|s| s.get_name() == "serve"));
        assert!(
            cmd.find_subcommand("mcp-serve").is_none(),
            "old spelling must be gone"
        );
    }

    #[test]
    fn data_dir_and_json_are_global_flags() {
        let cmd = Cli::command();
        let data_dir = cmd
            .get_arguments()
            .any(|a| a.get_id().as_str() == "data_dir" && a.is_global_set());
        let json = cmd
            .get_arguments()
            .any(|a| a.get_id().as_str() == "json" && a.is_global_set());
        assert!(data_dir && json);
    }
}
