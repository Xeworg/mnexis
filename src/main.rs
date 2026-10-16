use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "mnexis",
    version,
    about = "Local-first memory engine for AI agents"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the minimal MCP stdio server.
    McpServe,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Args::parse().command {
        Command::McpServe => mnexis_core::mcp::serve_stdio().await?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_cli_does_not_expose_the_retired_vector_spike() {
        use clap::CommandFactory;

        let help = Args::command().render_help().to_string();
        assert!(!help.contains("spike"));
        assert!(help.contains("mcp-serve"));
    }
}
