# The enriched nudge — examples and the type vocabulary, priced per need

The trim-nudge bench measured both sides of this: prose invites saving (the fat block pulled
24 save attempts vs the trim's 11, and the extras were the marginal `knowledge`/`procedure`
facts of H1-G3), but standing prose is priced **per call** (+11.5 % billed tokens). The nudge
is priced **per need** — it fired 0/10 in the bench and cost zero. This feature puts the
doctrine where the moment of judgment is, not where every call pays for it.

## Specs

- **S1 (verbatim, human steering, 2026-11-01)** — "talvez el agente necite ejemplos de que es lo
  que deberia guardar o una explicacion mas amplia de los tipos de memoria igual es una teoria que
  podriamos probar luego".
- **S2 (board constraint)** — the enrichment lives in the **nudge text only**. The tools block is
  untouched: the ≤ 5 500 B pin stands (`odd/next.md`, the 2026-10-28 decision "extension before
  more skill text" and the trim's measured price).
- **S3 (gating unchanged)** — the nudge still asks **once**, only when files changed AND no
  `mnexis_save` was attempted; read-only sessions and sessions that already saved pay nothing.
  Enriching the text must not change when it fires or how many times.
- **S4 (content)** — `SETTLE_NUDGE` grows: (a) two or three concrete examples of what deserves to
  survive this session (a decision forced by an external change, a discovery about how the repo
  works, a stated preference), (b) the seven-type vocabulary in one line each (`knowledge`,
  `decision`, `preference`, `procedure`, `project_state`, `episode`, `fact`) so the save arrives
  typed — typed marginal saves are what lights the per-kind chrome icons the user never sees,
  and the B1 axis measured the model writes the type when the moment names it.
- **S5 (budget)** — the enriched nudge stays under ~1 200 B: it is paid as one model request the
  moment it fires; doctrine beyond that belongs to the skill, not the nudge.

## Tasks

| ID | Specs | Route | Branch / worktree | Commit |
| --- | --- | --- | --- | --- |
| T1 | S1–S5 | worker: enrich `SETTLE_NUDGE`, extend `settle-nudge.test.js` | `ext/nudge-enriched` @ `tmp/wt-nudge` | `3d18acc` |
| T2 | all | parent: verify tests, commit, report; the bench (de-saturated fixture) is a separate follow-up that gates on this landing | — | — |

Allowed edit surfaces: `extensions/mnexis-memory/index.js` (the `SETTLE_NUDGE` constant and its
immediate comment only) and `extensions/mnexis-memory/test/settle-nudge.test.js`.

## Log

- **L1 (2026-11-01, user)** — "si podemos hacer merge de las ramas y trabajar en worktree nuevos la
  teoria del problema del nudge y el tema de los iconos" — the go for this branch after the merges.
- **L2 (evidence)** — `tmp/bench/trim-nudge-report.md`: H1 fails on G3 (the marginal fact A saves
  5/10, B 1/10), H2 at its declared ceiling (baseline saves 9/10; the nudge never fired). The
  bench that PROVES the enriched nudge needs a de-saturated fixture and rides its own branch.
- **L3 (2026-11-01, T1 result, parent-verified)** — Shipped in `3d18acc`: the judgment sentence
  preserved verbatim, three survival examples, the seven types one gloss line each; 866 B (28 %
  headroom under the 1 200 B budget); gating byte-identical and pinned by a new firing-matrix
  test; tools block re-measured 5 490 B (the pin). Suite re-run by the parent: 220 tests, 217
  pass, 3 parity skips, 0 fail. The text is now 12 lines as one `custom_message` — repo-wide grep
  found no other consumer. RED was observed before the edit (3 failing assertions). Awaiting the
  human's merge decision; the proving bench (de-saturated fixture) is the follow-up.
