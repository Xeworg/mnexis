# opencode-v2-session-lifecycle — the V2 session lifecycle on the seats v2.2.5 actually has

The opencode plugin's V2 entry subscribes to `session.created` and
`session.deleted` — the V1 event names, copied from `opencode.ai/docs/plugins`
(which today documents the older generation). On opencode v2.2.5 those seats
are dead: `session.deleted` is published only on an explicit session removal,
and `session.created` is not published at all in a headless run. Measured live
on 2026-11-03 in `bench-oc:verify` (opencode v2.2.5 = npm `latest`): the bus
delivered 25 event types and never one of the two the plugin waits for. So the
engine session is never closed, `ended_at` stays `null` forever, and the
workspace's "estado al cierre" record — the thing this lifecycle exists to
write — never lands.

This feature moves the V2 lifecycle onto the seats v2.2.5 really has, and
makes the test harness able to catch a dead seat in the first place.

## Specs

- S1: "en ese caso pon un agente a que revise la documentacion de opencode v2
  para encontrar si le estamos dando la funcion adecuada" — the V2 lifecycle
  must use the seats opencode v2.2.5 actually publishes, and the review that
  decides it cites the host's own contract (`@opencode/plugin@2.2.5`,
  `@opencode/client@2.2.5`, `@opencode/schema@2.2.5`, and the host repo
  `anomalyco/opencode` at tag `v2.2.5`).
- S2: "Híbrido: refrescar el estado al pausar" — what "end of session" means
  for the engine in opencode v2, where the host has no end-of-session event
  (only a per-turn idle):
  - "idle/execution.succeeded → save project_state 'estado al cierre'
    (REVISADO)"
  - "unload / sesión nueva → mnexis end (ended_at real)"
- S3: the state the next session resumes from must be honest: the record
  written while the session is still open must not claim the session ended.
- S4: the V1 entry keeps its own seats (`session.created` / `session.deleted`
  are V1 names and V1 is a different host model); this feature does not
  change V1 behavior, and records the unmeasured V1 population as a follow-up.
- S5: the suite must be able to fail on a dead seat: the fake host filters
  the event stream by the `types` the plugin asked for, exactly as the real
  host does, so a test that pushes an event the host never publishes is not
  evidence anymore.
- S6: the recorded fixtures are the payloads the host really sends (recorded
  live on v2.2.5), not the shapes the plugin wished for.

## Tasks

- T1 (S5, S6): the harness and the fixtures. The fake host's `subscribe`
  honors `types`; the V2 fixtures become the live-recorded idle-hook payload
  and the execution-event payloads; the old V2 fixtures (`session.created` /
  `session.deleted`) are kept as the V1-shaped ones they were, with the
  reason they no longer describe a V2 seat. Test-first: with the filter in
  place the current suite must go RED on the dead seat. Route: parent.
- T2 (S1, S2): the V2 entry subscribes to the seats that fire
  (`experimental.idle`, `session.execution.succeeded|failed|interrupted`) and
  the session module gains the action S2 names: refresh the closing state
  WITHOUT ending the engine session. Test-first. Route: parent.
- T3 (S2, S3): the engine session ends for real when the host goes away or
  another session starts: on the plugin's teardown, and when a new session
  starts while another one is open. The record written while open says the
  session is open. Route: parent.
- T4 (S1): the build guard passes: the reorganization's frozen baseline is
  re-frozen in the same commit, the way the guard's own rule says (a behavior
  change re-freezes; only a move never does). Route: parent.
- T5 (S1, S4): docs — RESEARCH.md's event table and the plugin README name
  the v2.2.5 seats and say what was measured; the V1 population is recorded
  as an unmeasured follow-up. Route: parent.

## Log

- L1: "en ese caso pon un agente a que revise la documentacion de opencode v2
  para encontrar si le estamos dando la funcion adecuada" (2026-11-03, the
  human, after asking whether the opencode release has the same behavior).
- L2: The review (agent `gentle-ai-explore`, task `mw0vzpvq-1-ux5d`) and the
  live measurement in `bench-oc:verify` agree, and the measurement is what
  decides:
  - The v2 taxonomy is `session.execution.{started,succeeded,failed,
    interrupted}` plus `session.step.*`, `session.text.*`, `session.inbox.*`,
    `session.usage.updated`, `session.renamed` — 25 types in one headless
    run. `session.created` and `session.deleted` were NOT among them.
  - `session.idle` exists in the union but the schema marks it `// deprecated`
    in v2.2.5, and the host did not publish it.
  - The seat that fires and means "the session finished its work and goes
    idle" is `ctx.session.hook("experimental.idle", fn)`, payload
    `{ sessionID, background }` — verified live: it fires between
    `session.execution.started` and `session.execution.succeeded`. The host's
    own built-in `goal` plugin uses it
    (`packages/core/src/plugin/goal.ts@v2.2.5`).
  - `session.deleted` is an explicit removal ("Remove a temporary session
    with `remove`"), never an end-of-run.
  - There is no process-shutdown hook in v2.2.5 (open request
    `anomalyco/opencode#10524`), and the plugin's own teardown does not run
    in a headless run (measured: 0 unload events). It does run when the
    background service stops.
  - The host's repo for v2 is `anomalyco/opencode` (it moved from
    `sst/opencode`); our RESEARCH already says so.
- L3: The engine's tolerance, measured on the binary: a prompt on an ended
  session is accepted, and registering the same id again answers the ended
  row without reopening it. So closing early does not break the capture.
- L4: The harness's defect, which is why the dead seat passed: the fake
  host's `subscribe` ignored the `types` filter, so a test could push any
  event into the queue and the plugin would consume it — evidence about a
  shape, not about the host. T1 makes the harness honest first, so the
  suite can go RED on the real defect.
- L5: The human's decision (2026-11-03, "Híbrido: refrescar el estado al
  pausar"): refresh the closing state at the pause, end the engine session
  for real only when the host goes away or another session starts.
