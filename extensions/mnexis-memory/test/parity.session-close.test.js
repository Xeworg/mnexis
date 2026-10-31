/**
 * Parity: the session close against the real engine
 * (odd/tasks/capture-discipline.md, S2).
 *
 * The unit corpus in `session-close.test.js` runs against a fake that applies
 * the engine's same-title rule. That rule is the whole property — one live
 * "estado al cierre" `project_state` per workspace, however many sessions
 * close — so this file asks the engine itself.
 *
 * It runs only when `MNEXIS_PARITY_BIN` names a real `mnexis` binary:
 *
 *   MNEXIS_PARITY_BIN=~/.local/bin/mnexis node --test \
 *     "extensions/mnexis-memory/test/parity.session-close.test.js"
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

import mnexisMemory from "../index.js";
import { createClient } from "../lib/cli.js";

const NAMESPACE = "payments";
const CLOSE_TITLE = "estado al cierre";
const PARITY_BINARY = process.env.MNEXIS_PARITY_BIN ?? "";
const skip = PARITY_BINARY === "" ? "set MNEXIS_PARITY_BIN to a mnexis binary to run the parity checks" : false;

/** A real engine in a data directory nobody else can reach. */
async function realEngine(t) {
	const dataDir = mkdtempSync(join(tmpdir(), "mnexis-close-parity-data-"));
	const workDir = mkdtempSync(join(tmpdir(), "mnexis-close-parity-work-"));
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

	// Bind this temp workspace to the namespace, the way a human would. Both
	// permissions go to the temp profile, never to the user's.
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

/** The live "estado al cierre" records, read the way a caller reads them. */
async function closeRecords(client, workDir) {
	const answer = await client.list(
		{ namespace: NAMESPACE, type: "project_state", full: true },
		workDir,
	);
	assert.equal(answer.code, 0, `list failed: ${answer.stderr.trim()}`);
	return (answer.data?.items ?? []).filter(
		(item) => item.state === "active" && item.title === CLOSE_TITLE,
	);
}

test("parity: two closed sessions leave exactly one live estado al cierre", { skip }, async (t) => {
	const { client, workDir, handlers, entries } = await realEngine(t);

	for (const id of ["01JX-CLOSE-A", "01JX-CLOSE-B"]) {
		await emit(handlers, "session_start", { reason: "startup" }, context(workDir, id));
		await emit(handlers, "session_shutdown", { reason: "quit" }, context(workDir, id));
	}
	assert.deepEqual(entries, [], `the engine took both closes: ${JSON.stringify(entries)}`);

	const records = await closeRecords(client, workDir);
	assert.equal(
		records.length,
		1,
		`exactly one live estado al cierre per workspace: ${JSON.stringify(records)}`,
	);
	assert.equal(records[0].revision, 2, "the second close revised the record the first left");
	assert.match(records[0].content, /01JX-CLOSE-B/, "the record says which session closed last");
	assert.doesNotMatch(records[0].content, /01JX-CLOSE-A/, "the previous close was replaced");
	assert.equal(records[0].memory_type, "project_state");
	assert.equal(records[0].namespace, NAMESPACE);

	// The session half of the same close: both engine sessions ended.
	const sessions = await client.run(["sessions"], { json: true, cwd: workDir });
	assert.equal(sessions.code, 0, `sessions failed: ${sessions.stderr.trim()}`);
	const closed = (sessions.data?.items ?? []).filter((item) =>
		["01JX-CLOSE-A", "01JX-CLOSE-B"].includes(item.id),
	);
	assert.equal(closed.length, 2, `both sessions are known: ${JSON.stringify(sessions.data)}`);
	for (const session of closed) {
		assert.ok(session.ended_at, `session ${session.id} ended: ${JSON.stringify(session)}`);
	}
});
