/**
 * Parity: the session engine's state machine, asked of the real engine
 * (odd/tasks/ext-ts-rewrite.md, R3).
 *
 * The extension registers a session on every `session_start`, including the
 * ones a reload, a resume or a fork fires, and it ends one on every shutdown.
 * Whether those two calls compose — a second start of the same id, a start
 * after an end, two sessions interleaved — is a fact about the engine, and
 * the audit found the extension guessing it (the start's failure was
 * swallowed whole). So this file asks the engine itself, and then asserts the
 * extension's behaviour against the answers it gives.
 *
 * The answers recorded here (2026-11-03, `~/.local/bin/mnexis`):
 *
 *  (a) start twice with the same id → exit 0, the session it already has
 *      (same `started_at`): a no-op, never a restart and never an error.
 *  (b) start after an end → exit 0, the existing session, `ended_at` still
 *      set: the engine never reopens a session, so a resumed id stays closed
 *      and the extension must say so instead of treating it as an error.
 *  (c) two sessions interleaved (start A, start B, end A, end B) → four
 *      exit 0, two rows, each ended on its own: the rows are independent.
 *  (d) end of an id the engine never heard of → exit 3 (`NOT_FOUND`).
 *
 * It runs only when `MNEXIS_PARITY_BIN` names a real `mnexis` binary:
 *
 *   MNEXIS_PARITY_BIN=~/.local/bin/mnexis node --test \
 *     "extensions/mnexis-memory/test/parity.session-state.test.js"
 *
 * It is hermetic by construction: every run creates its own data directory and
 * its own workspace directory under the system temp directory, points
 * `MNEXIS_DATA_DIR` at the former, and asserts the engine agrees before it
 * writes anything. The user's profile is never opened.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";
import { createClient } from "../lib/cli.ts";

const NAMESPACE = "payments";
const PARITY_BINARY = process.env.MNEXIS_PARITY_BIN ?? "";
const skip =
	PARITY_BINARY === ""
		? "set MNEXIS_PARITY_BIN to a mnexis binary to run the parity checks"
		: false;

/** A real engine in a data directory nobody else can reach. */
async function realEngine(t) {
	const dataDir = mkdtempSync(join(tmpdir(), "mnexis-state-parity-data-"));
	const workDir = mkdtempSync(join(tmpdir(), "mnexis-state-parity-work-"));
	t.after(() => {
		rmSync(dataDir, { recursive: true, force: true });
		rmSync(workDir, { recursive: true, force: true });
	});
	process.env.MNEXIS_BIN = PARITY_BINARY;
	process.env.MNEXIS_DATA_DIR = dataDir;
	t.after(() => {
		delete process.env.MNEXIS_BIN;
		delete process.env.MNEXIS_DATA_DIR;
	});
	const env = { ...process.env, MNEXIS_DATA_DIR: dataDir };
	const client = createClient({ binary: PARITY_BINARY, env });

	const init = await client.init(workDir);
	assert.equal(init.code, 0, "the profile is initialised in the temp data dir");
	const config = await client.configShow(workDir);
	assert.equal(
		config.data?.data_dir,
		dataDir,
		"the parity run is isolated: every write lands in the temp data dir",
	);

	const listed = await client.projectList(workDir);
	const key = listed.data?.current_key;
	assert.ok(key, "the engine publishes the workspace key it resolves");
	const bind = await client.bind(NAMESPACE, key, workDir, workDir);
	assert.equal(bind.code, 0, `bind failed: ${bind.stderr.trim()}`);
	const allow = await client.allowNamespace(NAMESPACE, workDir);
	assert.equal(allow.code, 0, `allow-namespace failed: ${allow.stderr.trim()}`);

	const handlers = new Map();
	const entries = [];
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry: (type, data) => entries.push({ type, data }),
	});
	return { client, workDir, handlers, entries };
}

/** The pi context of a hook in this workspace, with the host's session id. */
function context(workDir, sessionId) {
	return {
		cwd: workDir,
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
		sessionManager: { getSessionId: () => sessionId },
	};
}

/** Fire one event through the handlers the extension registered for it. */
async function emit(handlers, event, payload, ctx) {
	const results = [];
	for (const handler of handlers.get(event) ?? []) {
		results.push(await handler({ type: event, ...payload }, ctx));
	}
	return results;
}

/** One session row as the engine keeps it. */
async function sessionRows(client, workDir) {
	const answer = await client.run(["sessions", "--limit", "100"], { json: true, cwd: workDir });
	assert.equal(answer.code, 0, `sessions failed: ${answer.stderr.trim()}`);
	return answer.data?.items ?? [];
}

test("parity (a): a second start of the same id is a no-op, never a restart", { skip }, async (t) => {
	const { client, workDir, handlers, entries } = await realEngine(t);

	const first = await client.sessionStart(
		{ sessionId: "01JX-STATE-A", directory: workDir },
		workDir,
	);
	assert.equal(first.code, 0, `the first start lands: ${first.stderr.trim()}`);
	const again = await client.sessionStart(
		{ sessionId: "01JX-STATE-A", directory: workDir },
		workDir,
	);
	assert.equal(again.code, 0, `the engine answers a second start without an error: ${again.stderr.trim()}`);
	assert.equal(
		again.data?.started_at,
		first.data?.started_at,
		`the engine never restarts a session it has: ${JSON.stringify(again.data)}`,
	);
	assert.equal(again.data?.ended_at, null, "and it stays open");

	// The extension fires the start on every reason a session begins, reload
	// included: a no-op must leave no journal line behind.
	await emit(handlers, "session_start", { reason: "startup" }, context(workDir, "01JX-STATE-A"));
	await emit(handlers, "session_start", { reason: "reload" }, context(workDir, "01JX-STATE-A"));
	assert.deepEqual(entries, [], `a start the engine took is not journaled: ${JSON.stringify(entries)}`);

	const rows = (await sessionRows(client, workDir)).filter((row) => row.id === "01JX-STATE-A");
	assert.equal(rows.length, 1, `one row, however many starts: ${JSON.stringify(rows)}`);
});

test("parity (b): a start after an end keeps the session ended, and the extension says so", { skip }, async (t) => {
	const { client, workDir, handlers, entries } = await realEngine(t);

	const started = await client.sessionStart(
		{ sessionId: "01JX-STATE-B", directory: workDir },
		workDir,
	);
	assert.equal(started.code, 0, `start failed: ${started.stderr.trim()}`);
	const ended = await client.sessionEnd({ sessionId: "01JX-STATE-B" }, workDir);
	assert.equal(ended.code, 0, `end failed: ${ended.stderr.trim()}`);
	assert.ok(ended.data?.ended_at, "the close stamps the end");

	const resumed = await client.sessionStart(
		{ sessionId: "01JX-STATE-B", directory: workDir },
		workDir,
	);
	assert.equal(resumed.code, 0, `the engine takes the start: ${resumed.stderr.trim()}`);
	assert.equal(
		resumed.data?.ended_at,
		ended.data?.ended_at,
		`the engine never reopens a closed session: ${JSON.stringify(resumed.data)}`,
	);

	// A resumed id is exactly this case: the extension must not treat the
	// engine's answer as a failure, and must not swallow it either — the
	// session the human is in is closed in the engine, and that is a fact.
	const results = await emit(
		handlers,
		"session_start",
		{ reason: "resume" },
		context(workDir, "01JX-STATE-B"),
	);
	assert.deepEqual(results, [undefined], "the start hook never rejects on an ended id");
	assert.equal(entries.length, 1, `the stale close is journaled: ${JSON.stringify(entries)}`);
	assert.equal(entries[0].type, "mnexis-session-close");
	assert.equal(entries[0].data.step, "start");
	assert.equal(entries[0].data.already_ended, true);
	assert.equal(entries[0].data.ended_at, ended.data.ended_at);

	const rows = (await sessionRows(client, workDir)).filter((row) => row.id === "01JX-STATE-B");
	assert.equal(rows.length, 1, `the resumed id is still one row: ${JSON.stringify(rows)}`);
});

test("parity (c): two interleaved sessions end independently", { skip }, async (t) => {
	const { client, workDir, handlers, entries } = await realEngine(t);

	for (const id of ["01JX-STATE-C1", "01JX-STATE-C2"]) {
		await emit(handlers, "session_start", { reason: "startup" }, context(workDir, id));
	}
	const firstEnd = await client.sessionEnd({ sessionId: "01JX-STATE-C1" }, workDir);
	assert.equal(firstEnd.code, 0, `ending the first session failed: ${firstEnd.stderr.trim()}`);
	const secondEnd = await client.sessionEnd({ sessionId: "01JX-STATE-C2" }, workDir);
	assert.equal(secondEnd.code, 0, `ending the second session failed: ${secondEnd.stderr.trim()}`);
	assert.notEqual(
		firstEnd.data?.ended_at,
		undefined,
		`the first end is stamped: ${JSON.stringify(firstEnd.data)}`,
	);

	const rows = (await sessionRows(client, workDir)).filter((row) =>
		["01JX-STATE-C1", "01JX-STATE-C2"].includes(row.id),
	);
	assert.equal(rows.length, 2, `two rows, one per session: ${JSON.stringify(rows)}`);
	for (const row of rows) {
		assert.ok(row.ended_at, `session ${row.id} ended: ${JSON.stringify(row)}`);
	}
	assert.deepEqual(entries, [], `interleaving is not an event worth journaling: ${JSON.stringify(entries)}`);
});

test("parity (d): an end of a session the engine never heard of is journaled, not swallowed", { skip }, async (t) => {
	const { client, workDir, handlers, entries } = await realEngine(t);

	const unknown = await client.sessionEnd({ sessionId: "01JX-STATE-NEVER" }, workDir);
	assert.equal(unknown.code, 3, `the engine refuses an unknown session with NOT_FOUND: ${unknown.stderr.trim()}`);

	await emit(
		handlers,
		"session_shutdown",
		{ reason: "quit" },
		context(workDir, "01JX-STATE-NEVER"),
	);
	assert.equal(entries.length, 1, `the refusal reaches the journal: ${JSON.stringify(entries)}`);
	assert.equal(entries[0].type, "mnexis-session-close");
	assert.equal(entries[0].data.step, "end");
	assert.equal(entries[0].data.exit_code, 3);
	assert.equal(entries[0].data.not_registered, true);
	assert.match(
		JSON.stringify(entries[0].data),
		/not found: session:01JX-STATE-NEVER/,
		`the engine's own message is kept: ${JSON.stringify(entries[0].data)}`,
	);
});
