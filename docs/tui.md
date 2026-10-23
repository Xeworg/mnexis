# The mnexis TUI

`mnexis tui` is a full-CRUD terminal UI over the same `MemoryService` the
CLI and the MCP server use. It is a pure UI layer: no domain rule, SQL, or
validation lives in the TUI, so a memory saved from the TUI is exactly a
memory saved from `mnexis save`, including optimistic-revision checks and
scope enforcement.

## Launch

```sh
mnexis tui
```

The TUI opens on a **menu** with the available actions:

```
┌mnexis──────────────────────────────────────────┐
│                                              │
│  Memories                                    │
│  Browse and edit your memories               │
│                                              │
│▸ Install for agents                         │
│  Install the mnexis skill and extension      │
│                                              │
│  Quit                                        │
│  Close the TUI                               │
│                                              │
│↑/↓ or j/k move · Enter choose · Esc/q quit    │
└──────────────────────────────────────────────┘
```

- `↑`/`↓` (or `j`/`k`) move between the options, `Enter` opens the
  highlighted one, `Esc`/`q` leaves the TUI.
- **Memories** opens the three-pane view described below, unchanged.
- **Install for agents** opens the agents view, where the mnexis skill and
  the pi extension are installed for the supported environments.
- **Quit** ends the TUI.

When a newer release is published, the menu says so and `u` installs it:

```
│  mnexis 0.2.0 is published · press u to install it │
```

The check runs in the background when the TUI opens (its answer is cached
for a day, so the network is not touched on every start) and the
installation runs the same way: the interface never freezes while a
binary is downloaded. The new binary waits for a restart.

## Agents view

The agents view lists every supported environment (`pi`, `opencode`) with
the state of each asset — the skill and, where the host supports it, the
extension — and the directory each one lives in:

```
┌Agents────────────────────────────────────────────────────────┐
│pi — skill: installed, extension: not installed              │
│    skill → ~/.pi/agent/skills/mnexis-memory                 │
│    extension → ~/.pi/agent/extensions/mnexis-memory         │
│                                                            │
│opencode — skill: not installed                             │
│    skill → ~/.config/opencode/skills/mnexis-memory         │
└────────────────────────────────────────────────────────────┘
```

| Key           | Action                                                  |
| ------------- | ------------------------------------------------------- |
| `j` / `Down`  | next environment                                        |
| `k` / `Up`    | previous environment                                    |
| `i`           | install the integration for the selected environment     |
| `u`           | uninstall it                                            |
| `r`           | refresh the installation state                           |
| `Esc`         | back to the menu                                        |
| `q` / `Ctrl-C`| quit                                                    |

The status line reports what the last operation did (the assets and the
files it wrote or removed). The installation is the same operation the CLI
performs (`mnexis agent install`): the assets are embedded in the binary,
only what mnexis ships is replaced, and foreign skills or extensions are
never touched. Installing never grants the agent write permissions by
itself — that decision belongs to you, and the agent asks for it once per
namespace when it first writes.

The profile resolves exactly like every other command: `mnexis init` must
have run first (or `--data-dir DIR` points at an initialised profile);
otherwise the command fails with the same `NOT_FOUND` error (exit 3) as
the rest of the CLI. The TUI takes over the terminal (alternate screen +
raw mode) and restores it on exit, including on panic.

## Layout

Three panes:

```
┌Namespaces────┐┌Memories──────────────┐┌Detail──────────────────────┐
│ > default    ││ > uuid fact/active…  ││ uuid, type/state, revision │
│   project-x  ││   uuid fact/active…  ││ content, history, chain    │
└──────────────┘└──────────────────────┘└────────────────────────────┘
status line
footer help line
```

- **Namespaces** — the read scope of the profile (`mnexis config show`).
- **Memories** — the rows of the selected namespace, the FTS5 results while
  searching (`/`), or the soft-deleted rows in the deleted view (`x`).
- **Detail** — the selected memory: metadata, content, tags, revision
  history (newest first), and its supersession chain (each link shows the
  reason it was superseded with, when recorded).

The yellow border marks the focused pane. The status line above the footer
carries transient results and errors; the footer always shows the keys of
the current mode.

## Keys

### Normal mode

| Key           | Action                                              |
| ------------- | --------------------------------------------------- |
| `j` / `Down`  | next row (namespace or memory, per focused pane)    |
| `k` / `Up`    | previous row                                        |
| `Tab`         | next pane                                           |
| `Shift-Tab`   | previous pane                                       |
| `Enter`       | open the memory (from the namespace pane: jump to the memory list) |
| `/`           | Search in the selected namespace                    |
| `Esc`         | clear the search / selection; leave the detail pane |
| `x`           | toggle the soft-deleted view of the namespace       |
| `q` / `Ctrl-C`| quit                                                |

### Write flows

| Key | Action                                                    |
| --- | --------------------------------------------------------- |
| `n` | save form (prefilled with the current namespace)          |
| `e` | update form for the selected memory (re-read from disk)   |
| `s` | supersede: pick the newer memory, then confirm both sides |
| `d` | soft-delete the selected memory (y/N dialog)              |
| `r` | restore the memory selected in the deleted view (y/N)     |

### Forms (`n`, `e`, and the supersede confirmation)

| Key             | Action                                     |
| --------------- | ------------------------------------------ |
| printable chars | type into the focused field                |
| `Enter`         | next field; newline in multi-line content; submit from the last field |
| `Tab` / `Down`  | next field                                 |
| `Shift-Tab` / `Up` | previous field                          |
| `Backspace`     | delete the character before the cursor     |
| `Ctrl-S`        | submit from any field                      |
| `Ctrl-R`        | reload the target's current revision (after a conflict) |
| `Esc`           | cancel — nothing is written                |

Validation failures (blank required fields, bad revision numbers, byte
limits, tag rules) keep the form open with every typed value and show the
reason in the status line. Service-side failures — including
`REVISION_CONFLICT` when someone else wrote first — also keep the form
open; `Ctrl-R` re-reads the row and refreshes the expected revision while
keeping your edits, then `Ctrl-S` retries.

### Search (`/`)

Typing edits the query live and re-runs the same ranked search as
`mnexis search --namespace NS`. `Enter` accepts the results and returns to
normal mode; `Esc` clears the query and restores the plain list. Hits carry
rank and metadata but no content preview; press `Enter` on one to load its
detail.

### Deleted view (`x`) and restore (`r`)

`x` swaps the memory list to the soft-deleted rows of the namespace and
clears any search. In that view the detail pane reads the row through the
including-deleted read (history stays hidden), `r` opens the restore
dialog, and write flows other than restore are refused with a status
message. `x` again returns to the live list.

### Confirmation dialogs

Delete and restore ask first. The dialog renders the exact action
(`soft-delete <uuid> (revision N)? [y/N]`). Only `y` writes; `n`, `Enter`,
and `Esc` cancel without touching the database. While a dialog is open
every other key is inert.

## Scope and errors

The TUI never bypasses the profile scope: it can only list, search, and
write into the namespaces `mnexis config show` reports as readable. Rows
outside the scope simply do not appear. Every service error surfaces in
the status line with the stable error label, and a failed write never
leaves a half-applied form: either the whole write lands or nothing does.

## Exiting

`q` (normal mode) or `Ctrl-C` (any mode) quits. The terminal is restored
on the way out, including when a panic occurs inside the UI loop.
