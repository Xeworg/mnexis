/**
 * The compaction record and a compaction that failed
 * (odd/tasks/ext-ts-rewrite.md, R4).
 *
 * A `session_compact` save used to be untitled: the engine's identity is the
 * title, so every compaction of a session wrote a new row and none of them was
 * ever revised. The record is "what this session was doing", so its identity is
 * the session: one title that names it and one topic key derived from it, which
 * makes the second compaction revise the row the first one left — and, with
 * R2's policy, makes the save retry a locked store, because it names its
 * record.
 *
 * The reverse event had no handler at all: a compaction that failed left no
 * trace. It is journaled through the existing `mnexis-compaction` entry as the
 * `failed` variant (the entry type stays four kinds wide; the `action` says
 * what happened), and an aborted compaction stays quiet, because the human who
 * cancelled it is the one who knows.
 *
 * Branch summaries stay REJECTED (the design's decision, recorded): they are
 * the branch's narrative, not this session's state, and no handler asks for
 * them.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

const NAMESPACE = "demo";
const SESSION = "01JX-COMPACT";

/**
 * A fake engine that logs every call and answers the three commands a
 * compaction save needs. `MNEXIS_TEST_SAVE_BUSY_FILE` holds how many saves
 * still answer "the store is locked"; `MNEXIS_TEST_SAVE_FAIL` makes the save
 * fail the way an engine that refuses the content does.
 */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.MNEXIS_TEST_LOG, JSON.stringify(args) + "\\n");
const send = (payload) => process.stdout.write(JSON.stringify(payload));
const key = "git:test-workspace";
if (args[0] === "project" && args[1] === "list") {
	send({ bindings: [{ key, namespace: "${NAMESPACE}" }], current_key: key });
} else if (args[0] === "config" && args[1] === "show") {
	send({ config_file_present: true, scope: { write_namespaces: ["${NAMESPACE}"] } });
} else if (args[0] === "save") {
	const busyFile = process.env.MNEXIS_TEST_SAVE_BUSY_FILE;
	if (busyFile) {
		const left = Number.parseInt(readFileSync(busyFile, "utf8"), 10) || 0;
		if (left > 0) {
			writeFileSync(busyFile, String(left - 1));
			process.stderr.write("busy: sql: database is locked");
			process.exit(10);
		}
	}
	if (process.env.MNEXIS_TEST_SAVE_FAIL) {
		process.stderr.write(process.env.MNEXIS_TEST_SAVE_FAIL);
		process.exit(2);
	}
	send({ uuid: "u-1", revision: 1 });
} else {
	send({});
}
`;

const ENV_KEYS = [
	"MNEXIS_BIN",
	"MNEXIS_TEST_LOG",
	"MNEXIS_TEST_SAVE_BUSY_FILE",
	"MNEXIS_TEST_SAVE_FAIL",
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
function scenario(t, { busySaves = 0 } = {}) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-compact-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	const busyFile = join(dir, "busy");
	writeFileSync(log, "");
	writeFileSync(busyFile, String(busySaves));
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);

	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_TEST_LOG = log;
	process.env.MNEXIS_TEST_SAVE_BUSY_FILE = busyFile;

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
function context(dir, sessionId = SESSION) {
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

/** The saves the fake engine answered, in order. */
function saves(log) {
	return calls(log).filter((args) => args[0] === "save");
}

/** One compaction event, the way pi shapes it. */
function compacted(summary) {
	return {
		compactionEntry: { summary },
		reason: "threshold",
		fromExtension: false,
		willRetry: false,
	};
}

test("two compactions of one session revise one record instead of accumulating rows", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	await emit(sc.handlers, "session_compact", compacted("el puerto del servicio es 8081"), context(sc.dir));
	await emit(
		sc.handlers,
		"session_compact",
		compacted("el puerto del servicio cambió a 8082"),
		context(sc.dir),
	);

	const saved = saves(sc.log);
	assert.equal(saved.length, 2, `one save per compaction: ${JSON.stringify(saved)}`);
	const [first, second] = saved;
	const titleOf = (args) => args[args.indexOf("--title") + 1];
	const keyOf = (args) => args[args.indexOf("--topic-key") + 1];

	assert.ok(titleOf(first), `the compaction save is titled now: ${JSON.stringify(first)}`);
	assert.match(
		titleOf(first),
		new RegExp(SESSION),
		`the title names the session it belongs to: ${titleOf(first)}`,
	);
	assert.equal(
		titleOf(second),
		titleOf(first),
		`the second compaction names the same record: ${titleOf(second)}`,
	);
	assert.equal(
		keyOf(first),
		`session.${SESSION}.compaction`,
		`the topic key is derived from the session id: ${keyOf(first)}`,
	);
	assert.equal(keyOf(second), keyOf(first), "so the same session keeps one record");
	assert.equal(first[first.indexOf("--type") + 1], "episode", "the summary is still an episode");
	assert.equal(first[first.indexOf("--tags") + 1], "compaction", "and carries the same tag");
});

test("two sessions keep two compaction records", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	await emit(sc.handlers, "session_compact", compacted("una cosa"), context(sc.dir, "01JX-ONE"));
	await emit(sc.handlers, "session_compact", compacted("otra cosa"), context(sc.dir, "01JX-TWO"));

	const saved = saves(sc.log);
	assert.equal(saved.length, 2);
	const keys = saved.map((args) => args[args.indexOf("--topic-key") + 1]);
	assert.deepEqual(
		keys,
		["session.01JX-ONE.compaction", "session.01JX-TWO.compaction"],
		`each session's record is its own: ${JSON.stringify(keys)}`,
	);
});

test("a titled compaction save retries a locked store", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t, { busySaves: 1 });

	await emit(sc.handlers, "session_compact", compacted("el puerto es 8081"), context(sc.dir));

	assert.equal(
		saves(sc.log).length,
		2,
		`a save that names its record is tried again (R2): ${JSON.stringify(calls(sc.log))}`,
	);
	assert.deepEqual(sc.entries, [], "the retry recovered the write, so there is nothing to journal");
});

test("a compaction save the engine refuses is journaled with its step", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);
	process.env.MNEXIS_TEST_SAVE_FAIL = "invalid argument: content: over the content cap";

	await emit(sc.handlers, "session_compact", compacted("el puerto es 8081"), context(sc.dir));

	assert.equal(sc.entries.length, 1, JSON.stringify(sc.entries));
	assert.equal(sc.entries[0].customType, "mnexis-compaction");
	assert.equal(sc.entries[0].data.action, "skipped");
	assert.equal(sc.entries[0].data.step, "save");
	assert.equal(sc.entries[0].data.exit_code, 2);
	assert.match(JSON.stringify(sc.entries[0].data), /over the content cap/);
});

test("a compaction that failed is journaled, and the reason the host gave is kept", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	await emit(
		sc.handlers,
		"session_compact_failed",
		{
			reason: "overflow",
			errorMessage: "the provider answered more than the context allows",
			aborted: false,
			willRetry: true,
			fromExtension: false,
		},
		context(sc.dir),
	);

	assert.equal(sc.entries.length, 1, `the failure is journaled: ${JSON.stringify(sc.entries)}`);
	const entry = sc.entries[0];
	assert.equal(entry.customType, "mnexis-compaction");
	assert.equal(entry.data.kind, "compaction-capture");
	assert.equal(entry.data.action, "failed");
	assert.equal(entry.data.step, "compact");
	assert.equal(entry.data.reason, "overflow");
	assert.equal(entry.data.will_retry, true);
	assert.match(
		JSON.stringify(entry.data),
		/the provider answered more than the context allows/,
		`the host's own message survives: ${JSON.stringify(entry.data)}`,
	);
	assert.deepEqual(calls(sc.log), [], "a failed compaction writes nothing to the store");
});

test("a compaction the human cancelled is not a failure worth a journal line", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	await emit(
		sc.handlers,
		"session_compact_failed",
		{ reason: "manual", aborted: true, willRetry: false, fromExtension: false },
		context(sc.dir),
	);

	assert.deepEqual(sc.entries, [], `an abort is the human's choice, not a lost record: ${JSON.stringify(sc.entries)}`);
});

test("a branch summary is not captured: it is the branch's narrative, not this session's", async (t) => {
	const restore = snapshotEnv();
	t.after(restore);
	const sc = scenario(t);

	const results = await emit(
		sc.handlers,
		"session_before_tree",
		{ summaryEntry: { summary: "la rama anterior hizo otra cosa" } },
		context(sc.dir),
	);

	assert.deepEqual(results, [], "the extension registers no branch-summary handler");
	assert.deepEqual(calls(sc.log), [], "and writes nothing for it");
	assert.deepEqual(sc.entries, [], "and journals nothing");
});
