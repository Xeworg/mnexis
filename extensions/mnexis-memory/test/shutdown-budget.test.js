/**
 * The shutdown wall-clock budget (odd/tasks/ext-ts-rewrite.md, R1).
 *
 * pi awaits `session_shutdown` with no timeout of its own on /new, /resume,
 * /fork and quit (`agent-session-runtime.js:106-110`), and the close can spend
 * about a minute: the session end and the estado al cierre save are each given
 * thirty seconds, plus the busy retries. A human who quit waited for it.
 *
 * So every step of the close shares one wall-clock budget: whichever step does
 * not finish inside what is left of it is abandoned, journaled through the
 * existing `mnexis-session-close` entry with the reason, and the close stops
 * there — the next session may retry it, because the end is idempotent by
 * construction and the estado al cierre is one record under one title.
 *
 * Each test drives the hook pi hands the extension against a real child
 * process that hangs on the command the test names, so the assertion is the
 * hook returning: the budget is the only way out of it, and `MNEXIS_TIMEOUT_MS`
 * is left alone so the walk through the budgets is the extension's own.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

const NAMESPACE = "demo";
const CLOSE_TITLE = "estado al cierre";

/**
 * A fake engine that answers the two commands a close sends, and hangs on the
 * one `MNEXIS_TEST_HANG_ON` names — the shape of a store another process is
 * holding, where the call never comes back.
 */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.MNEXIS_TEST_LOG, JSON.stringify(args) + "\\n");
const send = (payload) => process.stdout.write(JSON.stringify(payload));
if (args[0] === (process.env.MNEXIS_TEST_HANG_ON || "")) {
	setTimeout(() => send({}), 60000);
} else if (args[0] === "end") {
	send({
		id: args[2],
		namespace: "${NAMESPACE}",
		directory: process.cwd(),
		started_at: "2026-11-03T10:00:00Z",
		ended_at: "2026-11-03T11:00:00Z",
		summary: null,
		branch: "main",
		branch_sha: "abc1234",
		branch_remote: "git:demo",
	});
} else if (args[0] === "save") {
	send({ uuid: "u-1", revision: 1 });
} else {
	send({});
}
`;

const ENV_KEYS = [
	"MNEXIS_BIN",
	"MNEXIS_TEST_LOG",
	"MNEXIS_TEST_HANG_ON",
	"MNEXIS_SHUTDOWN_BUDGET_MS",
];

/** Remember the environment so a test cannot leak into the next one. */
function snapshotEnv() {
	const saved = new Map(ENV_KEYS.map((key) => [key, process.env[key]]));
	return () => {
		for (const [key, value] of saved) {
			if (value === undefined) delete process.env[key];
			else process.env[key] = value;
		}
	};
}

/** A workspace, a fake engine, and the hooks pi would hand the extension. */
function scenario(t, { hangOn = "", budgetMs = 300 } = {}) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-shutdown-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	writeFileSync(log, "");
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);

	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_TEST_LOG = log;
	process.env.MNEXIS_TEST_HANG_ON = hangOn;
	process.env.MNEXIS_SHUTDOWN_BUDGET_MS = String(budgetMs);

	const handlers = new Map();
	const entries = [];
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry(customType, data) {
			entries.push({ customType, data });
		},
	});
	return { dir, log, handlers, entries };
}

/** The pi context a hook receives, with the session the host is running. */
function context(dir, sessionId) {
	return {
		cwd: dir,
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

/** Every argument list the fake engine received, in order. */
function calls(log) {
	return readFileSync(log, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

test("a close whose step never answers returns inside the budget and is journaled", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t, { hangOn: "end", budgetMs: 300 });

	const started = Date.now();
	const results = await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		context(sc.dir, "01JX-HANG"),
	);
	const elapsed = Date.now() - started;

	assert.deepEqual(results, [undefined], "the shutdown hook never rejects");
	assert.ok(
		elapsed < 2000,
		`the close is abandoned inside its own budget, not the engine's: ${elapsed} ms`,
	);
	assert.equal(sc.entries.length, 1, `the abandonment is journaled: ${JSON.stringify(sc.entries)}`);
	const entry = sc.entries[0];
	assert.equal(entry.customType, "mnexis-session-close");
	assert.equal(entry.data.kind, "session-close");
	assert.equal(entry.data.action, "abandoned");
	assert.equal(entry.data.step, "end");
	assert.equal(entry.data.session, "01JX-HANG");
	assert.match(
		entry.data.reason,
		/shutdown budget/,
		`the journal names the budget it ran out of: ${JSON.stringify(entry.data)}`,
	);
	assert.equal(
		calls(sc.log).filter((args) => args[0] === "save").length,
		0,
		"an abandoned end leaves no estado al cierre to write",
	);
});

test("a fast engine closes normally: the end and the estado al cierre land", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	const results = await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		context(sc.dir, "01JX-FAST"),
	);
	assert.deepEqual(results, [undefined], "the shutdown hook never rejects");
	assert.deepEqual(sc.entries, [], `a close the engine took is not journaled: ${JSON.stringify(sc.entries)}`);

	const logged = calls(sc.log);
	assert.deepEqual(logged[0], ["end", "--id", "01JX-FAST", "--json"], JSON.stringify(logged));
	const save = logged.find((args) => args[0] === "save");
	assert.ok(save, `the close still writes the state: ${JSON.stringify(logged)}`);
	assert.equal(save[save.indexOf("--title") + 1], CLOSE_TITLE);
});

test("the budget is shared: an end that lands leaves the rest for the save", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t, { hangOn: "save", budgetMs: 300 });

	const started = Date.now();
	await emit(sc.handlers, "session_shutdown", { reason: "quit" }, context(sc.dir, "01JX-HALF"));
	const elapsed = Date.now() - started;

	assert.ok(elapsed < 2000, `the close is abandoned inside its own budget: ${elapsed} ms`);
	assert.equal(
		calls(sc.log).filter((args) => args[0] === "end").length,
		1,
		"the end landed before the budget was spent",
	);
	assert.equal(sc.entries.length, 1, JSON.stringify(sc.entries));
	assert.equal(sc.entries[0].data.action, "abandoned");
	assert.equal(sc.entries[0].data.step, "save");
	assert.equal(sc.entries[0].data.namespace, NAMESPACE);
	assert.match(sc.entries[0].data.reason, /shutdown budget/);
});

test("a budget that is already spent abandons the step instead of waiting for it", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t, { hangOn: "end", budgetMs: 300 });

	// The end gets what is left of a budget the first step could not spend:
	// `budgetMs: 1` is the degenerate case a slow host reaches when the first
	// millisecond of the close is already gone.
	process.env.MNEXIS_SHUTDOWN_BUDGET_MS = "1";
	const started = Date.now();
	await emit(sc.handlers, "session_shutdown", { reason: "quit" }, context(sc.dir, "01JX-SPENT"));
	const elapsed = Date.now() - started;

	assert.ok(elapsed < 2000, `the close returns at once: ${elapsed} ms`);
	assert.equal(sc.entries.length, 1, JSON.stringify(sc.entries));
	assert.equal(sc.entries[0].data.action, "abandoned");
	assert.equal(sc.entries[0].data.step, "end");
});
