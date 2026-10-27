# Installing mnexis

There are two ways to install mnexis on Linux:

- **from a link**: the installer published in every release downloads the
  binary for your platform, verifies it against the checksum the release
  publishes and installs it;
- **from the sources**: the same script builds the working tree with
  cargo.

Both paths install the same binary to `~/.local/bin/mnexis` and both are
the same `install.sh` — the mode follows the script itself, not the
working directory.

## From a link

```sh
curl -fsSL https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | bash
```

The script needs `bash` (it is a bash script), `curl`, `sha256sum` and
`tar`. It downloads the artifact for your platform
(`mnexis-linux-x86_64.tar.gz` or `mnexis-linux-aarch64.tar.gz`), verifies
it against the `SHA256SUMS` the release publishes, installs it and opens
the TUI.

Pin a version with the environment (the script installs the version you
ask for, and refuses an artifact that carries another one):

```sh
MNEXIS_VERSION=0.4.0 \
  curl -fsSL https://github.com/Xeworg/mnexis/releases/latest/download/install.sh | bash
```

You can also fetch the script first and read it before running it:

```sh
curl -fsSLO https://github.com/Xeworg/mnexis/releases/latest/download/install.sh
less install.sh
bash install.sh
```

The release artifacts are **not signed**: the checksum proves the file is
the one that was published, not that the release was not tampered with.
See [release.md](release.md).

## From the sources

`install.sh` builds the `mnexis` binary from source with cargo and installs
it for the current user.

## Requirements

- Linux (the MVP is Linux-first).
- Rust 1.95 or newer with `cargo` and `rustc` on `PATH`; the exact pin lives
  in [`rust-toolchain.toml`](../rust-toolchain.toml). If the toolchain is
  missing, the script exits with a pointer to [rustup](https://rustup.rs).
- A C toolchain for the bundled SQLite (`rusqlite` builds SQLite from
  source).
- `bash` plus the usual coreutils (`mkdir`, `cp`, `chmod`, `rm`).
- For the online path: `curl`, `sha256sum` and `tar`.

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

The online path does the same thing with the published artifact instead
of a local build, and it opens the TUI when the output is a terminal (it
is not: in a script nobody is watching, and `MNEXIS_NO_TUI=1` turns it off).

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

## Agent skills

The repository ships agent skills in [`skills/`](../skills): each
subdirectory is one installable skill (Agent Skills format — a `SKILL.md`
plus optional supporting files). The first skill, `mnexis-memory`, teaches
AI agents the mnexis memory loop (recall, save, supersede) so they can use
mnexis without per-session prompting.

```sh
./install.sh skills            # interactive environment selection
./install.sh skills --env pi   # unattended, explicit environment
```

Behaviour:

- Supported environments and their skills directories:
  - `pi` → `<agent-dir>/skills/<skill-name>/`, where `<agent-dir>` is
    `$PI_CODING_AGENT_DIR` or `$HOME/.pi/agent` by default. This is the
    directory pi scans at startup to advertise skills to the model;
    `~/.pi/skills/` is a *project*-level location for pi, so a skill
    installed there is never advertised.
  - `opencode` → `~/.config/opencode/skills/<skill-name>/` (opencode's
    global skills directory).
  Adding more environments extends the registry in `install.sh`.
- Running `skills` is a pure file copy: no Rust toolchain, no build, and
  the `mnexis` binary is not required or installed by it.
- Re-running overwrites the installed skills with the shipped content.
- Skill directories that mnexis does not ship are never touched.
- Without `--env`, the selector requires an interactive terminal; from a
  pipe it aborts (exit `1`) with the supported environment list, mirroring
  the `--purge` confirmation policy. An unknown environment exits `2`.

## Installing from the binary

The binary carries the agent assets inside itself, so an installed `mnexis`
can set up the integration without a source checkout:

```sh
mnexis agent status                  # what is installed, per environment
mnexis agent install --env pi        # skill + pi extension
mnexis agent install --env opencode  # skill + opencode's MCP entry
mnexis agent uninstall --env pi
```

`--only skill|extension|all` narrows what an operation touches. The same
operations are available from the TUI (`mnexis tui` → *Install for agents*),
which reports what it installed.

This is the only implementation: `install.sh skills|extensions` forwards to
`mnexis agent install`, so both paths write the same embedded assets. Those
assets are a snapshot of the tree the binary was compiled from, so when the
script runs from a repository and the working tree is newer than the
installed binary, it rebuilds the binary before forwarding — the order of
`./install.sh` and `./install.sh skills` never matters. A piped
(standalone) script never rebuilds: it has no tree, and the binary it
forwards to is the one it just installed.

### opencode's MCP entry

opencode can reach the memory engine as native tools, through the MCP
server the binary already ships. Installing the opencode integration wires
that server into opencode's own configuration
(`~/.config/opencode/opencode.json`):

```json
{
  "mcp": {
    "mnexis": {
      "type": "local",
      "command": ["/home/you/.local/bin/mnexis", "mcp", "serve"],
      "enabled": true
    }
  }
}
```

The file belongs to you, so mnexis is careful with it:

- the file is created when it does not exist;
- when it exists, mnexis only ever adds or removes **one textual block**: the
  `"mnexis": { … }` member, or the `"mcp": { "mnexis": { … } }` member when
  the file has no `mcp` key. The rest of the file keeps its exact bytes, its
  key order, its number formatting, its duplicate keys and its formatting —
  mnexis never re-serialises your document;
- an `mcp.mnexis` entry that is already there is left alone, and an
  `mcp.mnexis` entry that is **not** the one mnexis wrote (you wrote it, or
  you edited it) is never removed by the uninstall;
- the file mode is preserved, and a configuration that is a symlink (you
  manage it with dotfiles), that is read-only, that is `opencode.jsonc`
  (opencode also reads JSONC), that is not valid JSON, whose root is not an
  object, or whose `mcp` member is not an object, is **not touched at all**:
  the command reports `refused` and prints the exact snippet to add by hand;
- `XDG_CONFIG_HOME` is honoured when it is set
  (`$XDG_CONFIG_HOME/opencode/opencode.json`).

A refusal is not an error: the command exits 0 and reports what it did
(`host config refused: …`), so a script can read the outcome from the
`host_config` field of `mnexis --json agent install`.

`mnexis agent status` reports whether the entry is configured.

## Agent extensions

The repository ships agent extensions in [`extensions/`](../extensions): each
subdirectory is one installable extension for an agent that supports them.
The first one, `mnexis-memory`, exposes the memory engine as native tools
(`mnexis_save`, `mnexis_search`, `mnexis_list`, `mnexis_show`,
`mnexis_supersede`) and resolves the workspace → namespace binding itself,
so the agent never has to be told to remember anything.

```sh
./install.sh extensions --env pi   # → <agent-dir>/extensions/mnexis-memory/
```

Behaviour:

- `pi` → `<agent-dir>/extensions/<extension-name>/`, where `<agent-dir>` is
  `$PI_CODING_AGENT_DIR` or `$HOME/.pi/agent` by default. `--env` is
  optional here and defaults to `pi`; an unknown environment exits `2`.
- Like `skills`, this is a pure file copy: no build step, no toolchain. The
  extension's `test/` directory is not shipped.
- Extension directories that mnexis does not ship are never touched.
- `--skills`, `--extensions` and `--purge` are separate scopes and cannot be
  combined in one `uninstall` run.

The extension resolves the workspace key canonically
(`git:github.com-xeworg-mnexis`), binds a namespace on first use, and asks
for your confirmation **once per namespace** before it may write there. After
that grant the agent stores memories on its own. In a headless session
(`pi -p`) it never authorizes anything by itself: it reports that
authorization is pending and which command to run.

## Uninstall

```sh
./install.sh uninstall
```

This removes `~/.local/bin/mnexis` and nothing else: the profile data and
settings stay in place. It is idempotent — running it when the binary is
already gone is a clean no-op success.

## Uninstall agent skills

```sh
./install.sh uninstall --skills            # interactive environment selection
./install.sh uninstall --skills --env pi   # unattended, explicit environment
```

This removes exactly the skill directories mnexis ships (`skills/*` with a
`SKILL.md`) from the selected environment's skills directory — foreign
skills are kept. When nothing is installed it prints
`No mnexis skills are installed ...` and exits `0`. `--skills` cannot be
combined with `--purge`: they are separate scopes (skills are not profile
data), so run them separately.

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
