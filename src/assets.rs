//! The agent integration assets, embedded in the binary.
//!
//! The skill (`skills/mnexis-memory/`) and the pi extension
//! (`extensions/mnexis-memory/`) are compiled into the executable, so an
//! installed `mnexis` can install them without a source checkout. The
//! embedded copies come from the repository files at build time, so the two
//! can never diverge.
//!
//! The extension's `test/` directory is deliberately not embedded: it is
//! development-only.

/// One embedded file: its path relative to the install target and its bytes.
pub struct EmbeddedFile {
    /// Path relative to the installation root, using `/` separators.
    pub path: &'static str,
    /// File contents.
    pub contents: &'static str,
}

/// One installable asset: a name and the files that make it up.
pub struct EmbeddedAsset {
    /// Asset name, used as the directory name under the target.
    pub name: &'static str,
    /// Files of the asset, relative to the asset directory.
    pub files: &'static [EmbeddedFile],
}

/// The agent skill (`mnexis-memory`), installable in every environment.
pub const SKILL_MNEXIS_MEMORY: EmbeddedAsset = EmbeddedAsset {
    name: "mnexis-memory",
    files: &[EmbeddedFile {
        path: "SKILL.md",
        contents: include_str!("../skills/mnexis-memory/SKILL.md"),
    }],
};

/// The pi extension (`mnexis-memory`), installable only where pi loads it.
pub const EXTENSION_MNEXIS_MEMORY: EmbeddedAsset = EmbeddedAsset {
    name: "mnexis-memory",
    files: &[
        EmbeddedFile {
            path: "index.js",
            contents: include_str!("../extensions/mnexis-memory/index.js"),
        },
        EmbeddedFile {
            path: "lib/cli.js",
            contents: include_str!("../extensions/mnexis-memory/lib/cli.js"),
        },
        EmbeddedFile {
            path: "lib/gate.js",
            contents: include_str!("../extensions/mnexis-memory/lib/gate.js"),
        },
        EmbeddedFile {
            path: "lib/workspace.js",
            contents: include_str!("../extensions/mnexis-memory/lib/workspace.js"),
        },
        EmbeddedFile {
            path: "package.json",
            contents: include_str!("../extensions/mnexis-memory/package.json"),
        },
        EmbeddedFile {
            path: "README.md",
            contents: include_str!("../extensions/mnexis-memory/README.md"),
        },
    ],
};

/// Every asset the binary can install, in installation order.
pub const ALL_ASSETS: &[EmbeddedAsset] = &[SKILL_MNEXIS_MEMORY, EXTENSION_MNEXIS_MEMORY];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asset_has_files_and_a_name() {
        for asset in ALL_ASSETS {
            assert!(!asset.name.is_empty());
            assert!(!asset.files.is_empty(), "{} has no files", asset.name);
        }
    }

    #[test]
    fn asset_files_are_not_empty() {
        for asset in ALL_ASSETS {
            for file in asset.files {
                assert!(
                    !file.contents.trim().is_empty(),
                    "{}/{} is empty",
                    asset.name,
                    file.path
                );
            }
        }
    }

    #[test]
    fn the_extension_does_not_ship_its_tests() {
        for file in EXTENSION_MNEXIS_MEMORY.files {
            assert!(
                !file.path.starts_with("test/"),
                "test files must not be embedded: {}",
                file.path
            );
        }
    }

    #[test]
    fn the_skill_carries_the_frontmatter_name() {
        let skill = SKILL_MNEXIS_MEMORY.files[0].contents;
        assert!(skill.starts_with("---\n"));
        assert!(skill.contains("name: mnexis-memory"));
    }
}
