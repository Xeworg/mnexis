/**
 * The capture of one message is bounded, and the turn never waits for it
 * (odd/tasks/ext-ts-rewrite.md, R7).
 *
 * The `input` hook runs on every message the human sends, and it used to await
 * a write with the thirty-second budget of a write: a store that was held by
 * another process could make the human wait half a minute before their own
 * message was sent. The capture is one indexed insert, so it gets its own
 * budget of a second and a half: on expiry the abandonment is journaled through
 * the existing `mnexis-prompt-capture` entry and the turn proceeds — never
 * later than the budget, whatever the engine is doing.
 *
 * The tests drive the hook against a real child that hangs on `prompt`, so the
 * evidence is the hook returning: the budget is the only way out of it.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

/**
 * A fake engine that logs the calls it answered and hangs on `prompt` when
 * `MNEXIS_TEST_HANG_ON` names it.
 */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.MNEXIS_TEST_LOG, JSON.stringify(args) + "\\n");
const send = (payload) => process.stdout.write(JSON.stringify(payload));
if (args[0] === (process.env.MNEXIS_TEST_HANG_ON || "")) {
	setTimeout(() => send({}), 60000);
} else if (args[0] === "prompt") {
	send({ id: 1, session_id: args[args.indexOf("--session") + 1] });
} else {
	send({});
}
`;

const ENV_KEYS = [
	"MNEXIS_BIN",
	"MNEXIS_TEST_LOG",
	"MNEXIS_TEST_HANG_ON",
	"MNEXIS_PROMPT_CAPTURE_BUDGET_MS",
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
	const dir = mkdtempSync(join(tmpdir(), "mnexis-capture-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	writeFileSync(log, "");
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);

	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_TEST_LOG = log;
	process.env.MNEXIS_TEST_HANG_ON = hangOn;
	process.env.MNEXIS_PROMPT_CAPTURE_BUDGET_MS = String(budgetMs);

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
function context(dir, sessionId = "01JX-PROMPT") {
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

test("a capture the engine holds up is abandoned inside its own budget and journaled", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t, { hangOn: "prompt", budgetMs: 300 });

	const started = Date.now();
	const results = await emit(
		sc.handlers,
		"input",
		{ text: "guardá esto", source: "user" },
		context(sc.dir),
	);
	const elapsed = Date.now() - started;

	assert.deepEqual(results, [undefined], "the turn proceeds: the hook returns nothing to change");
	assert.ok(
		elapsed < 2000,
		`the capture is abandoned inside its own budget, not the write one: ${elapsed} ms`,
	);
	assert.equal(
		calls(sc.log).filter((args) => args[0] === "prompt").length,
		1,
		"the capture was attempted once",
	);
	assert.equal(sc.entries.length, 1, `the abandonment is journaled: ${JSON.stringify(sc.entries)}`);
	const entry = sc.entries[0];
	assert.equal(entry.customType, "mnexis-prompt-capture");
	assert.equal(entry.data.kind, "prompt-capture");
	assert.equal(entry.data.action, "abandoned");
	assert.equal(entry.data.session, "01JX-PROMPT");
	assert.equal(entry.data.bytes, Buffer.byteLength("guardá esto", "utf8"));
	assert.match(
		entry.data.reason,
		/prompt capture budget/,
		`the journal names the budget it ran out of: ${JSON.stringify(entry.data)}`,
	);
	assert.doesNotMatch(
		JSON.stringify(entry.data),
		/guardá/,
		"the journal never carries the message itself",
	);
});

test("a message the engine takes is captured, and nothing is journaled", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	const results = await emit(
		sc.handlers,
		"input",
		{ text: "guardá esto", source: "user" },
		context(sc.dir),
	);

	assert.deepEqual(results, [undefined]);
	assert.deepEqual(sc.entries, [], `a capture the engine took is silent: ${JSON.stringify(sc.entries)}`);
	const captured = calls(sc.log).find((args) => args[0] === "prompt");
	assert.ok(captured, `the message reaches the store: ${JSON.stringify(calls(sc.log))}`);
	assert.equal(captured[1], "guardá esto", "the text is stored byte for byte");
	assert.equal(
		captured[captured.indexOf("--session") + 1],
		"01JX-PROMPT",
		"and joins the session it was said in",
	);
});
