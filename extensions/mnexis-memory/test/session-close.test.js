/**
 * Session close on the pi extension (odd/tasks/capture-discipline.md, S2).
 *
 * A session that ends leaves exactly one live "estado al cierre"
 * `project_state` per workspace. The title is the engine's identity: the
 * same title always lands on the same record, so the second session updates
 * the record the first one left instead of adding another.
 *
 * The fake `mnexis` keeps a store in a JSON file and applies the engine's
 * same-title rule (a save on a title the store already holds bumps the
 * revision of that record). The tests drive the hooks pi hands the
 * extension and read the store back through the CLI surface, so they assert
 * the record a workspace ends up with, never the helpers that write it.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";
import { createClient } from "../lib/cli.ts";

const NAMESPACE = "demo";
const CLOSE_TITLE = "estado al cierre";

/** A fake `mnexis` whose store lives in a JSON file the test can read. */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";

const log = process.env.MNEXIS_FAKE_LOG;
const configPath = process.env.MNEXIS_FAKE_CONFIG;
const args = process.argv.slice(2);
appendFileSync(log, JSON.stringify(args) + "\\n");
const config = JSON.parse(readFileSync(configPath, "utf8"));
config.store = config.store ?? [];
const persist = () => writeFileSync(configPath, JSON.stringify(config));

function flag(name) {
	const index = args.indexOf(name);
	return index >= 0 ? args[index + 1] : undefined;
}

/** \`key_from_title\` (src/domain/title.rs): the same title is the same record. */
function keyFromTitle(title) {
	let out = "";
	for (const char of String(title ?? "").trim()) {
		const lowered = char.toLowerCase();
		if (/^[\\p{L}\\p{N}]+$/u.test(lowered)) {
			out += lowered;
		} else if (out !== "" && !out.endsWith("-")) {
			out += "-";
		}
	}
	return out.replace(/^-+/, "").replace(/-+$/, "");
}

function refuse(code, errorCode, message) {
	process.stderr.write(JSON.stringify({ error: { error_code: errorCode, message } }));
	persist();
	process.exit(code);
}

if (args[0] === "project" && args[1] === "list") {
	process.stdout.write(
		JSON.stringify({
			bindings: [{ key: "test-key", namespace: config.namespace }],
			current_key: "test-key",
		}),
	);
} else if (args[0] === "config" && args[1] === "show") {
	process.stdout.write(
		JSON.stringify({
			config_file_present: true,
			scope: { write_namespaces: [config.namespace] },
		}),
	);
} else if (args[0] === "status") {
	process.stdout.write(
		JSON.stringify({
			workspace: { key: "test-key", namespace: config.namespace, candidate_namespace: null },
		}),
	);
} else if (args[0] === "end") {
	// The engine refuses an end it cannot find, and the close has to say so
	// instead of leaving the workspace without its estado al cierre.
	if (config.endFail) {
		refuse(config.endFail, "NOT_FOUND", "not found: session:" + (flag("--id") ?? ""));
	}
	const id = flag("--id") ?? "";
	const ended = {
		...config.session,
		id,
		// A session that already ended keeps its end; the fake mirrors that
		// by answering the fixed timestamp the scenario configured.
		ended_at: (config.ends ?? {})[id] ?? config.session.ended_at,
	};
	process.stdout.write(JSON.stringify(ended));
} else if (args[0] === "save") {
	if (config.saveFail) {
		refuse(config.saveFail, "INVALID_ARGUMENT", "content: over the content cap");
	}
	const namespace = flag("--namespace") ?? config.namespace;
	const title = flag("--title") ?? "";
	const type = flag("--type") ?? "fact";
	const end = args.indexOf("--json");
	const content = args[end - 1] ?? "";
	const topicKey = keyFromTitle(title);
	const existing = config.store.find(
		(item) => item.namespace === namespace && item.topic_key === topicKey,
	);
	if (existing) {
		// One fact, one record: the same title updates the record the
		// previous session left, and the revision says so.
		existing.revision += 1;
		existing.title = title;
		existing.memory_type = type;
		existing.content = content;
		existing.updated_at = "2026-10-31T12:00:00Z";
		persist();
		process.stdout.write(
			JSON.stringify({
				uuid: existing.uuid,
				revision: existing.revision,
				title,
				topic_key: topicKey,
				memory_type: type,
			}),
		);
	} else {
		const id = config.store.length + 1;
		const item = {
			id,
			uuid: "u-" + id,
			namespace,
			title,
			topic_key: topicKey,
			content,
			memory_type: type,
			state: "active",
			revision: 1,
			created_at: "2026-10-31T12:00:00Z",
			updated_at: "2026-10-31T12:00:00Z",
		};
		config.store.push(item);
		persist();
		process.stdout.write(
			JSON.stringify({
				uuid: item.uuid,
				revision: item.revision,
				title,
				topic_key: topicKey,
				memory_type: type,
			}),
		);
	}
} else if (args[0] === "list") {
	const namespace = flag("--namespace");
	const type = flag("--type");
	let items = config.store.filter((item) => item.state === "active");
	if (namespace) {
		items = items.filter((item) => item.namespace === namespace);
	}
	if (type) {
		items = items.filter((item) => item.memory_type === type);
	}
	process.stdout.write(
		JSON.stringify({ count: items.length, items, read_scope: "restricted" }),
	);
} else {
	process.stdout.write("{}");
}
`;

/** A workspace, a store, and the hooks pi would hand the extension. */
function scenario(
	t,
	{ session = {}, ends = {}, endFail = 0, saveFail = 0 } = {},
) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-close-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	const configPath = join(dir, "engine.json");
	writeFileSync(log, "");
	writeFileSync(
		configPath,
		JSON.stringify({
			namespace: NAMESPACE,
			store: [],
			session: {
				namespace: NAMESPACE,
				directory: "/work/project",
				started_at: "2026-10-31T10:00:00Z",
				ended_at: "2026-10-31T11:00:00Z",
				summary: null,
				branch: "main",
				branch_sha: "abc1234",
				branch_remote: "git:github.com-xeworg-mnexis",
				...session,
			},
			ends,
			endFail,
			saveFail,
		}),
	);
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_FAKE_LOG = log;
	process.env.MNEXIS_FAKE_CONFIG = configPath;
	process.env.MNEXIS_BIN = binary;
	t.after(() => {
		delete process.env.MNEXIS_FAKE_LOG;
		delete process.env.MNEXIS_FAKE_CONFIG;
		delete process.env.MNEXIS_BIN;
	});

	const handlers = new Map();
	const entries = [];
	const pi = {
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry(customType, data) {
			entries.push({ customType, data });
		},
	};
	mnexisMemory(pi);

	const env = {
		...process.env,
		MNEXIS_FAKE_LOG: log,
		MNEXIS_FAKE_CONFIG: configPath,
	};
	const client = createClient({ binary, env });
	return { dir, log, configPath, binary, handlers, entries, client };
}

/** The pi context a hook receives, with the session the host is running. */
function sessionContext(sessionId, cwd) {
	return {
		cwd,
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

/** The live "estado al cierre" records of the workspace, read through the CLI. */
async function closeRecords(client, cwd) {
	const answer = await client.list(
		{ namespace: NAMESPACE, type: "project_state", full: true },
		cwd,
	);
	assert.equal(answer.code, 0, `list failed: ${answer.stderr.trim()}`);
	return (answer.data?.items ?? []).filter(
		(item) => item.state === "active" && item.title === CLOSE_TITLE,
	);
}

const SESSION_A_CONTENT =
	"Session 01JX-A ended at 2026-10-31T11:00:00Z.\n" +
	"It started at 2026-10-31T10:00:00Z.\n" +
	"Directory: /work/project.\n" +
	"Branch: main at abc1234 (git:github.com-xeworg-mnexis).";

test("a closing session ends the engine session and leaves the estado al cierre record", async (t) => {
	const sc = scenario(t, { ends: { "01JX-A": "2026-10-31T11:00:00Z" } });
	await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		sessionContext("01JX-A", sc.dir),
	);

	const logged = calls(sc.log);
	assert.deepEqual(
		logged[0],
		["end", "--id", "01JX-A", "--json"],
		`the engine session ends first: ${JSON.stringify(logged)}`,
	);
	assert.deepEqual(
		logged[1],
		[
			"save",
			"--namespace",
			NAMESPACE,
			"--type",
			"project_state",
			"--title",
			CLOSE_TITLE,
			SESSION_A_CONTENT,
			"--json",
		],
		`the mechanical state is one project_state under the canonical title: ${JSON.stringify(logged)}`,
	);

	const records = await closeRecords(sc.client, sc.dir);
	assert.equal(records.length, 1, `exactly one live record: ${JSON.stringify(records)}`);
	assert.equal(records[0].revision, 1);
	assert.equal(records[0].memory_type, "project_state");
	assert.equal(records[0].namespace, NAMESPACE);
	assert.equal(
		records[0].content,
		SESSION_A_CONTENT,
		`the record says the session's mechanical facts: ${records[0].content}`,
	);
});

test("a second session updates the one record the first one left", async (t) => {
	const sc = scenario(t, {
		ends: {
			"01JX-A": "2026-10-31T11:00:00Z",
			"01JX-B": "2026-10-31T13:00:00Z",
		},
	});
	await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		sessionContext("01JX-A", sc.dir),
	);
	const first = await closeRecords(sc.client, sc.dir);
	assert.equal(first.length, 1);

	await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		sessionContext("01JX-B", sc.dir),
	);
	const second = await closeRecords(sc.client, sc.dir);
	assert.equal(
		second.length,
		1,
		`the workspace keeps exactly one live estado al cierre: ${JSON.stringify(second)}`,
	);
	assert.equal(second[0].uuid, first[0].uuid, "the same record, not a second one");
	assert.equal(second[0].revision, 2, "the same-title rule revised it");
	assert.match(second[0].content, /01JX-B/, "the record says which session closed last");
	assert.doesNotMatch(second[0].content, /01JX-A/, "the previous close was replaced, not piled on");

	const saves = calls(sc.log).filter((args) => args[0] === "save");
	assert.equal(saves.length, 2, `one save per close: ${JSON.stringify(saves)}`);
	for (const save of saves) {
		assert.equal(save[save.indexOf("--title") + 1], CLOSE_TITLE, "the title never changes");
		assert.equal(save[save.indexOf("--type") + 1], "project_state", "the kind never changes");
	}
});

test("a reload tears the runtime down, not the session, so it does not close it", async (t) => {
	const sc = scenario(t, { ends: { "01JX-A": "2026-10-31T11:00:00Z" } });
	await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "reload" },
		sessionContext("01JX-A", sc.dir),
	);
	assert.deepEqual(calls(sc.log), [], "a reload neither ends the session nor writes its state");
	assert.equal((await closeRecords(sc.client, sc.dir)).length, 0);

	await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		sessionContext("01JX-A", sc.dir),
	);
	assert.equal(calls(sc.log).filter((args) => args[0] === "end").length, 1);
	assert.equal((await closeRecords(sc.client, sc.dir)).length, 1);
});

test("a save the engine refuses does not fail the close, and the journal says why", async (t) => {
	const sc = scenario(t, { saveFail: 2 });
	const results = await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		sessionContext("01JX-A", sc.dir),
	);
	assert.deepEqual(results, [undefined], "the close never rejects");
	assert.equal(
		calls(sc.log).filter((args) => args[0] === "end").length,
		1,
		"the session still ended",
	);
	assert.equal((await closeRecords(sc.client, sc.dir)).length, 0);

	assert.equal(sc.entries.length, 1, JSON.stringify(sc.entries));
	const trace = sc.entries[0];
	assert.equal(trace.customType, "mnexis-session-close");
	assert.equal(trace.data.kind, "session-close");
	assert.equal(trace.data.action, "skipped");
	assert.equal(trace.data.step, "save");
	assert.equal(trace.data.namespace, NAMESPACE);
	assert.equal(trace.data.exit_code, 2);
	assert.equal(trace.data.session, "01JX-A");
	assert.match(
		JSON.stringify(trace.data),
		/over the content cap/,
		`the journal keeps the engine's own message: ${JSON.stringify(trace)}`,
	);
});

test("a session the engine cannot end leaves no state and a journal line", async (t) => {
	const sc = scenario(t, { endFail: 2 });
	const results = await emit(
		sc.handlers,
		"session_shutdown",
		{ reason: "quit" },
		sessionContext("01JX-A", sc.dir),
	);
	assert.deepEqual(results, [undefined], "the close never rejects");
	assert.deepEqual(
		calls(sc.log).filter((args) => args[0] === "save"),
		[],
		"a session the engine never registered has no namespace to write into",
	);
	assert.equal((await closeRecords(sc.client, sc.dir)).length, 0);
	assert.equal(sc.entries.length, 1, JSON.stringify(sc.entries));
	assert.equal(sc.entries[0].customType, "mnexis-session-close");
	assert.equal(sc.entries[0].data.step, "end");
	assert.equal(sc.entries[0].data.exit_code, 2);
	assert.match(JSON.stringify(sc.entries[0].data), /not found: session:01JX-A/);
});

test("a session with no id leaves no record and never fails", async (t) => {
	const sc = scenario(t);
	const results = await emit(sc.handlers, "session_shutdown", { reason: "quit" }, {
		cwd: sc.dir,
		hasUI: false,
		mode: "print",
		ui: { confirm: async () => false },
	});
	assert.deepEqual(results, [undefined]);
	assert.deepEqual(calls(sc.log), [], "nothing to end and nothing to name it with");
	assert.equal(sc.entries.length, 0);
});

test("every shutdown that ends a session still ends it in the engine", async (t) => {
	// The hook already ended the engine session before this feature; the
	// close adds the state and must not take the end with it. A quit is a
	// process exit, a new/resume/fork is a session replacement — all four
	// are ends. Only a reload is not.
	for (const reason of ["quit", "new", "resume", "fork"]) {
		const sc = scenario(t, {
			ends: { [`01JX-${reason}`]: "2026-10-31T11:00:00Z" },
		});
		await emit(
			sc.handlers,
			"session_shutdown",
			{ reason },
			sessionContext(`01JX-${reason}`, sc.dir),
		);
		assert.equal(
			calls(sc.log).filter((args) => args[0] === "end").length,
			1,
			`${reason}: the engine session ends`,
		);
		assert.equal(
			(await closeRecords(sc.client, sc.dir)).length,
			1,
			`${reason}: the workspace keeps its estado al cierre`,
		);
	}
});
