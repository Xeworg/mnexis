//! CLI wiring for `mnexis tui`: help text and profile resolution.

use std::process::Command;

use clap::CommandFactory;
use mnexis_core::cli::Cli;

#[test]
fn help_advertises_the_tui_subcommand() {
    let help = Cli::command().render_help().to_string();
    assert!(help.contains("tui"), "help must mention tui: {help}");
    assert!(
        help.contains("terminal UI"),
        "tui must carry a description: {help}"
    );
}

#[test]
fn tui_requires_an_initialized_profile() {
    let guard = tempfile::tempdir_in("target").expect("tempdir under target/");
    let out = Command::new(env!("CARGO_BIN_EXE_mnexis"))
        .arg("--data-dir")
        .arg(guard.path())
        .arg("tui")
        .output()
        .expect("spawn mnexis");
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("NOT_FOUND"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
