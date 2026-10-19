# Installing mnexis

`install.sh` builds the `mnexis` binary from source with cargo and installs
it for the current user. There is no binary-download path: the script only
uses the local Rust toolchain (S1 non-goal: no GitHub Releases / `curl | sh`).

## Requirements

- Linux (the MVP is Linux-first).
- Rust 1.95 or newer with `cargo` and `rustc` on `PATH`; the exact pin lives
  in [`rust-toolchain.toml`](../rust-toolchain.toml). If the toolchain is
  missing, the script exits with a pointer to [rustup](https://rustup.rs).
- A C toolchain for the bundled SQLite (`rusqlite` builds SQLite from
  source).
- `bash` plus the usual coreutils (`mkdir`, `cp`, `chmod`, `rm`).

## Install

From the repository root:

```sh
./install.sh
```

The default action:

1. runs `cargo build --release` from the repository root (a relative
   `CARGO_TARGET_DIR` is resolved against the repository root, exactly as
   cargo itself resolves it),
2. creates `~/.local/bin` when missing,
3. copies the built binary to `~/.local/bin/mnexis`,
4. prints PATH guidance when `~/.local/bin` is not on `PATH`.

Example of the PATH guidance:

```text
/home/you/.local/bin is not on your PATH.
Add it to your shell profile and restart the shell, for example:
  export PATH="/home/you/.local/bin:$PATH"
```

Re-running the script overwrites the installed binary cleanly, so it is the
normal way to upgrade after pulling new sources. `./install.sh install` is
the explicit spelling of the default action; `./install.sh --help` prints
usage.

### Verify

```sh
mnexis --version
```

Then create the default profile (only the first time):

```sh
mnexis init
```

`mnexis init` creates the data directory `~/.local/share/mnexis` with
`mnexis.db` and `mnexis.toml`. The CLI also accepts `--data-dir DIR` and the
`MNEXIS_DATA_DIR` environment variable if you want the profile elsewhere.

## Uninstall

```sh
./install.sh uninstall
```

This removes `~/.local/bin/mnexis` and nothing else: the profile data and
settings stay in place. It is idempotent — running it when the binary is
already gone is a clean no-op success.

## Uninstall and purge profile data

```sh
./install.sh uninstall --purge
```

`--purge` additionally deletes the profile data directory, which also holds
`mnexis.toml` (the settings live inside the data directory; see below).
Because this permanently deletes memories, the script asks for an explicit
`y/N` confirmation **before removing anything**.

Purge behaviour:

- The confirmation prompt is interactive. If stdin is not a terminal, the
  script aborts with instructions and leaves the binary and profile data
  untouched.
- Declining the prompt (`n`, empty input, or anything but `y/Y/yes`) also
  leaves everything untouched.
- Aborting or declining exits with status `1`; a completed purge exits `0`.
- Unsafe targets are refused: a relative `MNEXIS_DATA_DIR`, `/`, or the home
  directory itself.

### Exact purge paths

The installer does not guess. The profile location follows
`resolve_data_dir` in [`src/cli/profile.rs`](../src/cli/profile.rs):

| Priority | Path | Source |
| --- | --- | --- |
| 1 (CLI only) | `--data-dir DIR` | `src/cli/profile.rs:76-79` |
| 2 | `$MNEXIS_DATA_DIR` when set | `src/cli/profile.rs:80-82` |
| 3 | `$HOME/.local/share/mnexis` | `src/cli/profile.rs:83-84` |

The installer has no `--data-dir` flag, so `--purge` resolves step 2 and
then step 3. Inside the data directory:

| File | Meaning | Source |
| --- | --- | --- |
| `mnexis.db` (+ SQLite `-wal`/`-shm` sidecars) | memory database | `src/cli/profile.rs:59-65`, `src/cli/commands.rs:555-563` (`init()` joins) |
| `mnexis.toml` | profile settings / access scope | `src/cli/profile.rs:63-65`, `src/cli/commands.rs:555-588` (`init()`), `src/config/mod.rs:1-9` |

`mnexis config show` also reads `mnexis.toml` from the data directory
(`src/cli/config_cmd.rs:13-15`), confirming there is no separate config
directory. The application does **not** read `XDG_DATA_HOME` or
`XDG_CONFIG_HOME`; `~/.local/share/mnexis` is only the XDG-style default
location, hardcoded in `resolve_data_dir`. Purging therefore removes exactly
one directory: the resolved data directory (default
`~/.local/share/mnexis`), and nothing outside it.

The installed binary lives at `$HOME/.local/bin/mnexis` (the installer's
`INSTALL_DIR`), independent of `MNEXIS_DATA_DIR`.

If you initialised a profile somewhere else with `--data-dir`, the
installer cannot discover it; remove that directory yourself.

## Installer tests

The installer has its own bash test suite; it is not part of `cargo test`:

```sh
bash scripts/test-installer.sh
```

The runner builds the release binary once into the gitignored
`tmp/installer-target` (override with `CARGO_TARGET_DIR`), then runs every
`tests/installer/*_test.sh`. Each test creates a scratch `HOME` with
`mktemp -d`, so the real user profile is never read or modified. The
interactive `y/N` cases use util-linux `script` to allocate a pty; when it
is unavailable those cases are reported as skipped instead of failing.

Run a single test directly if needed:

```sh
bash tests/installer/03_uninstall_test.sh
```
