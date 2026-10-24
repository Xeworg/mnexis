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

    #[test]
    fn doctor_help_lists_the_repair_modes_and_the_check_filter() {
        let mut root = Cli::command();
        let doctor_help = root
            .find_subcommand_mut("doctor")
            .expect("the doctor subcommand exists")
            .render_help()
            .to_string();
        assert!(
            doctor_help.contains("--check"),
            "doctor help: {doctor_help}"
        );

        // The mode flags live one level down, on the repair subcommand.
        let mut root = Cli::command();
        let repair_help = root
            .find_subcommand_mut("doctor")
            .expect("the doctor subcommand exists")
            .find_subcommand_mut("repair")
            .expect("the repair subcommand exists")
            .render_help()
            .to_string();
        for expected in ["--plan", "--dry-run", "--apply"] {
            assert!(
                repair_help.contains(expected),
                "doctor repair help must show {expected}: {repair_help}"
            );
        }
    }
}
