use std::path::PathBuf;

/// Pins the repository-level TMPDIR redirection configured in
/// `.cargo/config.toml`.
///
/// The development host's `/tmp` is a tmpfs with a hard per-user quota that
/// parallel compiler temporaries exhausted twice (SQLite extended error 778
/// "disk I/O error" and libsqlite3-sys cc "Disk quota exceeded"; see
/// `odd/tasks/e0-e1-recovery.md` L71). Cargo-spawned processes must
/// therefore use the repository's own `tmp/` directory, which lives on the
/// project filesystem and is not quota-bound.
#[test]
fn cargo_spawned_processes_use_the_repository_tmp_directory() {
    let tmpdir = std::env::var("TMPDIR")
        .expect("TMPDIR must be set by .cargo/config.toml for cargo-spawned processes");
    let expected = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tmp");
    let actual = PathBuf::from(&tmpdir);
    assert_eq!(
        actual, expected,
        "TMPDIR must point at the repository tmp/ directory (got {tmpdir}); \
         check .cargo/config.toml"
    );
}
