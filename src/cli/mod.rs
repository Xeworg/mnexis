//! The `mnexis` command-line interface (PRD §8, e2-core §7).
//!
//! Every command is a thin wrapper over [`MemoryService`] under the scope
//! loaded from `mnexis.toml`. Exit codes 2–10 map to the stable error set
//! (e2-core §8); `--json` switches output to structured envelopes.

pub mod commands;
pub mod config_cmd;
pub mod output;
pub mod profile;

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

    /// Save a new memory
    Save {
        /// Namespace (default: the profile's default_namespace)
        #[arg(long)]
        namespace: Option<String>,
        /// Memory type (default: fact)
        #[arg(long)]
        r#type: Option<String>,
        /// Assertion basis (default: user_stated)
        #[arg(long)]
        basis: Option<String>,
        /// Topic key (unique per namespace when non-empty)
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

    /// Show effective configuration and scope
    Config {
        #[command(subcommand)]
        action: ConfigAction,
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
}

/// Parse and run. Returns the process exit code.
pub async fn run(cli: Cli) -> i32 {
    let mode = output::OutputMode::from_flag(cli.json);
    match execute(&cli).await {
        Ok(outcome) => {
            println!("{}", outcome.render(mode));
            0
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
            // The MCP server owns its own lifecycle; it is not a service
            // wrapper. Failures surface as exit 1 with the raw error.
            if let Err(e) = crate::mcp::serve_stdio().await {
                eprintln!("mcp serve error: {e}");
            }
            Ok(CommandOutcome::new(
                serde_json::json!({ "mcp": "serve", "status": "stopped" }),
                "mcp serve stopped",
            ))
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
        Command::Config {
            action: ConfigAction::Show,
        } => config_cmd::show(data_dir),
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
            "config",
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
