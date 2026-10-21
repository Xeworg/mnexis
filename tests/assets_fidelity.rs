//! The embedded assets must be exactly the repository files: an asset in
//! `skills/` or `extensions/` that is not embedded can never reach a user's
//! agent, and an embedded copy that drifts is a second source of truth (S7).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use mnexis_core::assets::{EXTENSION_MNEXIS_MEMORY, EmbeddedAsset, SKILL_MNEXIS_MEMORY};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Every file below `dir`, relative and with `/` separators.
fn shipped_files(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let entries =
            fs::read_dir(&current).unwrap_or_else(|error| panic!("{}: {error}", current.display()));
        for entry in entries {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(dir)
                    .expect("below the asset")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(relative);
            }
        }
    }
    out
}

fn assert_asset_matches_the_repository(asset: &EmbeddedAsset, dir: PathBuf, skip: &[&str]) {
    let embedded: BTreeSet<String> = asset
        .files
        .iter()
        .map(|file| file.path.to_string())
        .collect();
    let shipped: BTreeSet<String> = shipped_files(&dir)
        .into_iter()
        .filter(|path| !skip.iter().any(|prefix| path.starts_with(prefix)))
        .collect();
    assert_eq!(
        shipped,
        embedded,
        "{}: the embedded files differ from {}\n  only in the repository: {:?}\n  only embedded: {:?}",
        asset.name,
        dir.display(),
        shipped.difference(&embedded),
        embedded.difference(&shipped),
    );
    for file in asset.files {
        let on_disk = fs::read_to_string(dir.join(file.path)).expect("read the shipped file");
        assert_eq!(
            on_disk, file.contents,
            "{}/{} drifted from the embedded copy",
            asset.name, file.path
        );
    }
}

#[test]
fn the_embedded_skill_is_the_repository_skill() {
    assert_asset_matches_the_repository(
        &SKILL_MNEXIS_MEMORY,
        repo().join("skills/mnexis-memory"),
        &[],
    );
}

#[test]
fn the_embedded_extension_is_the_repository_extension() {
    // `test/` is development-only and is deliberately not shipped.
    assert_asset_matches_the_repository(
        &EXTENSION_MNEXIS_MEMORY,
        repo().join("extensions/mnexis-memory"),
        &["test/"],
    );
}
