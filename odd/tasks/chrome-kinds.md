# The kind on the line you actually see — chrome icons without expanding

The icon investigation (2026-11-01) confirmed why the user never sees the per-kind icons
(`odd/tasks/chrome-shape.md:195`, diagnosed at `:208`): no tool declares an icon (the `🤖` is
shared by all 14), the kind icons render **only in expanded panels of read tools**, save never
draws one even expanded, and with the store at 20/20 `fact` the only renderable glyph is the
`🧠` fallback. This feature puts the kind where the user actually looks: the collapsed line.

## Specs

- **S1 (verbatim, human, 2026-11-01)** — "solo veo normalmente el de save pero los otros nunca los
  he visto" (the original complaint), plus "el tema de los iconos" named as work.
- **S2 (save's collapsed line carries the kind)** — when `mnexis_save` succeeds, the collapsed
  summary line shows the saved record's type glyph and name: `↳ ✓ saved · 🎯 decision · <title>`
  (fallback `🧠 fact` when untyped — which is also the honest signal that the save arrived
  untyped).
- **S3 (read tools' collapsed line carries the kind census)** — `mnexis_list` / `mnexis_search` /
  `mnexis_show` / `mnexis_history` / `mnexis_workspace` / `mnexis_context` collapsed summaries
  show the kinds present in the result (e.g. `↳ ✓ 3 memories · 🎯×2 📘×1`), so a result's shape
  is visible without expanding. Empty results keep their current text.
- **S4 (width math intact)** — the existing emoji-as-2-columns measurement
  (`lib/chrome.js:269-276`, pinned by `test/chrome.test.js:861-864`) must keep holding; every new
  glyph goes through the same measurement.
- **S5 (no behavior change)** — chrome only: no gate, no store, no wire change; every existing
  chrome test stays green untouched where the contract does not change.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S5 | worker: collapsed-line kinds in `lib/chrome.js`, tests in `test/chrome.test.js` | `ext/chrome-kinds` @ `tmp/wt-chrome` | `0f142f6` |
| T2 | all | parent: verify tests, commit, report | — | — |

Allowed edit surfaces: `extensions/mnexis-memory/lib/chrome.js` and
`extensions/mnexis-memory/test/chrome.test.js`.

## Log

- **L1 (2026-11-01, user)** — "si podemos hacer merge de las ramas y trabajar en worktree nuevos la
  teoria del problema del nudge y el tema de los iconos" — the go for this branch after the merges.
- **L2 (evidence)** — the icon theory T1 (gentle-ai-explore, confirmed against the live store):
  `chrome.js:74,129` (shared `🤖`), `chrome.js:63-72` (kind icons), `chrome.js:193` (collapsed has
  no emoji), `chrome.js:219-221` (save never draws one), store 20/20 `fact`.
- **L3 (2026-11-01, T1 result, parent-verified)** — Shipped in `0f142f6`: the save answer DOES
  carry `memory_type` (resolved into `details` at index.js:2079-2093 — no invented parse), so the
  collapsed save line reads `↳ ✓ saved · 🎯 decision · <title>` with the honest `🧠 fact` fallback;
  list/search/context/workspace draw the kind census, history names the kind once, show gains its
  glyph. RED observed (11 failing), GREEN 58/58, full suite re-run by the parent: 225 tests, 222
  pass, 3 parity skips, 0 fail. Differential vs the base chrome.js: 9 diffs, every one a requested
  S2/S3 addition; every error line, expanded panel and empty result byte-identical. Judgement
  calls recorded: history kind-once (not ×N), census first-seen order, untyped counts as the fact
  the expanded panel draws. Awaiting the human's merge decision.
