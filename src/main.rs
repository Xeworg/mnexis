use clap::Parser;
use mnexis_core::cli::{self, Cli};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let code = cli::run(cli).await;
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn binary_help_shows_the_nested_mcp_serve_command() {
        let help = Cli::command().render_help().to_string();
        assert!(help.contains("mcp"), "help: {help}");
        assert!(!help.contains("mcp-serve"), "old spelling is gone");
    }
}
