/**
 * `/mnexis status` — the durable card (odd/tasks/ext-ts-rewrite.md, UX3).
 *
 * A `/mnexis` that answers with `notify` is a dim line that the next message
 * overwrites, and the card holds seven fields. So the command writes a
 * `mnexis-status` entry through `pi.appendEntry` — durable, part of the
 * branch's own data, drawn by the journal's renderer and expanded to the
 * fields — and still says one summary line in the footer's own words.
 *
 * The command only reads: binding and granting are the human's decisions, and
 * UX3 deferred any grant path to its own decision.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

/** A fake `mnexis` that records its arguments and answers the read commands. */
const FAKE_BINARY = `#!/usr/bin/env node
import { appendFileSync } from "node:fs";
const args = process.argv.slice(2);
appendFileSync(process.env.MNEXIS_FAKE_LOG, JSON.stringify(args) + "\\n");
const out = (value) => process.stdout.write(JSON.stringify(value));
if (args[0] === "status") {
	out({
		workspace: {
			key: "git:github.com-xeworg-mnexis",
			namespace: "mnexis",
			candidate_namespace: null,
			candidate_refusal: null,
		},
		scope: {
			default_namespace: "default",
			read_namespaces: ["mnexis", "global"],
			write_namespaces: ["mnexis"],
			allow_global: false,
			allow_read_all: false,
		},
		memory_count: 7,
		recent: [],
	});
} else if (args[0] === "project" && args[1] === "list") {
	out({
		bindings: [{ key: "git:github.com-xeworg-mnexis", namespace: "mnexis" }],
		current_key: "git:github.com-xeworg-mnexis",
	});
} else if (args[0] === "session" || args[0] === "end") {
	out({ id: "01JX", namespace: "mnexis", started_at: "2026-11-02T00:00:00Z" });
} else if (args[0] === "prompt") {
	out({ id: args[1] ? 1 : 0 });
} else if (args[0] === "list") {
	out({ count: 1, items: [{ uuid: "m1", namespace: "mnexis", title: "t" }] });
} else if (args[0] === "config" && args[1] === "show") {
	out({ scope: { read_namespaces: ["mnexis", "global"], write_namespaces: ["mnexis"] } });
} else {
	out({});
}
`;

/** Install the fake binary and point the extension at it. */
function setup(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-status-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const log = join(dir, "calls.log");
	writeFileSync(log, "");
	const binary = join(dir, "mnexis");
	writeFileSync(binary, FAKE_BINARY);
	chmodSync(binary, 0o755);
	process.env.MNEXIS_BIN = binary;
	process.env.MNEXIS_FAKE_LOG = log;
	t.after(() => {
		delete process.env.MNEXIS_BIN;
		delete process.env.MNEXIS_FAKE_LOG;
	});
	return { dir, log };
}

/** A host that records what the extension registers and appends. */
function fakeHost() {
	const commands = new Map();
	const handlers = new Map();
	const entries = [];
	mnexisMemory({
		registerTool() {},
		registerCommand(name, options) {
			commands.set(name, options);
		},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry(customType, data) {
			entries.push({ customType, data });
		},
	});
	return { commands, handlers, entries };
}

/** The pi context of an interactive session. */
function context(sessionId) {
	const notices = [];
	return {
		notices,
		cwd: process.cwd(),
		hasUI: true,
		mode: "tui",
		ui: {
			confirm: async () => false,
			notify(message, level) {
				notices.push({ message, level });
			},
			setStatus() {},
			select: async () => undefined,
		},
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

/** Every argument list the fake binary received. */
function calls(log) {
	return readFileSync(log, "utf8")
		.split("\n")
		.filter(Boolean)
		.map((line) => JSON.parse(line));
}

test("the command registers with a description, its completions and its handler", () => {
	const { commands } = fakeHost();
	const command = commands.get("mnexis");

	assert.ok(command, "/mnexis is registered");
	assert.equal(typeof command.description, "string", "the command says what it does");
	assert.match(command.description, /status/i, `the description names what it shows: ${command.description}`);
	assert.equal(typeof command.handler, "function", "the command has a handler");
	assert.equal(typeof command.getArgumentCompletions, "function", "the command offers completions");
});

test("the completions offer the status subcommand", () => {
	const { commands } = fakeHost();
	const complete = commands.get("mnexis").getArgumentCompletions;

	const all = complete("");
	assert.ok(Array.isArray(all), `an empty prefix lists the subcommands: ${JSON.stringify(all)}`);
	assert.ok(
		all.some((option) => option.value === "status"),
		`status is offered: ${JSON.stringify(all)}`,
	);

	const typed = complete("sta");
	assert.deepEqual(
		typed.map((option) => option.value),
		["status"],
		`a prefix narrows to status: ${JSON.stringify(typed)}`,
	);

	const nothing = complete("grant");
	assert.ok(
		nothing === null || nothing.length === 0,
		`no grant path is offered: ${JSON.stringify(nothing)}`,
	);
});

test("the handler writes the card with every field it gathered", async (t) => {
	const { log } = setup(t);
	const { commands, handlers, entries } = fakeHost();
	const ctx = context("01JX");

	// The session the engine registered, two captured messages, and the
	// session's own use of the engine: the card reads all of it.
	await emit(handlers, "session_start", { reason: "new" }, ctx);
	await emit(handlers, "input", { text: "el puerto es 8081", source: "user" }, ctx);
	await emit(handlers, "input", { text: "y el panel", source: "user" }, ctx);
	await emit(handlers, "tool_call", { toolCallId: "c1", toolName: "mnexis_context", input: {} }, ctx);
	await emit(handlers, "tool_call", { toolCallId: "c2", toolName: "mnexis_search", input: {} }, ctx);
	await emit(
		handlers,
		"tool_execution_end",
		{ toolCallId: "e1", toolName: "mnexis_save", isError: false, result: {} },
		ctx,
	);

	await commands.get("mnexis").handler("status", ctx);

	assert.equal(entries.length, 1, `one card was written: ${JSON.stringify(entries)}`);
	const [entry] = entries;
	assert.equal(entry.customType, "mnexis-status", "the card is the journal kind the renderer draws");

	const card = entry.data;
	assert.equal(card.workspace, "git:github.com-xeworg-mnexis", "the workspace key");
	assert.equal(card.namespace, "mnexis", "the bound namespace");
	assert.equal(card.bound, true, "the workspace is bound");
	assert.deepEqual(card.read_namespaces, ["mnexis", "global"], "the read allowlist");
	assert.deepEqual(card.write_namespaces, ["mnexis"], "the write allowlist");
	assert.equal(card.session, "01JX", "the session id");
	assert.equal(card.session_state, "started", "the engine registered the session");
	assert.equal(card.prompts_captured, 2, "the messages this session captured");
	assert.equal(card.memories, 7, "the memories the engine counts");
	assert.equal(card.memories_read_all, false, "the count is read in the workspace's own scope");
	assert.deepEqual(card.counters, { ctx: 1, search: 1, save: 1 }, "the footer counters");
	assert.equal(card.nudge, "open", "the settle question has not been asked");
	assert.match(card.footer, /ctx 1/, `the footer line is carried as it reads: ${card.footer}`);
	assert.equal(typeof card.ts, "string", "the entry carries when it was written");

	// One bounded engine call, short-budgeted like the prompt capture: the
	// status the card is made of.
	const statusCalls = calls(log).filter((args) => args[0] === "status");
	assert.equal(statusCalls.length, 1, `one status call made the card: ${JSON.stringify(statusCalls)}`);

	// And one summary line for the human, in the footer's own words.
	assert.equal(ctx.notices.length, 1, `one notification: ${JSON.stringify(ctx.notices)}`);
	assert.match(ctx.notices[0].message, /mnexis/, `the line names the namespace: ${ctx.notices[0].message}`);
	assert.match(ctx.notices[0].message, /7 memories/, `and the count: ${ctx.notices[0].message}`);
});

test("the nudge state says sent once the question was asked", async (t) => {
	setup(t);
	const { commands, handlers, entries } = fakeHost();
	const ctx = context("01JX");
	await emit(handlers, "session_start", { reason: "new" }, ctx);
	await emit(
		handlers,
		"tool_execution_end",
		{ toolCallId: "w1", toolName: "write", isError: false, result: {} },
		ctx,
	);
	const asked = await emit(handlers, "agent_before_settle", {}, ctx);
	assert.equal(asked[0]?.continue, true, "the question was asked");

	await commands.get("mnexis").handler("status", ctx);

	assert.equal(entries[0].data.nudge, "sent", "the card says the question was spent");
});

test("the command grants nothing and binds nothing", async (t) => {
	const { log } = setup(t);
	const { commands } = fakeHost();
	const ctx = context("01JX");

	await commands.get("mnexis").handler("status", ctx);
	await commands.get("mnexis").handler("grant mnexis", ctx);

	const logged = calls(log);
	const write = logged.filter(
		(args) =>
			(args[0] === "project" && args[1] === "bind") ||
			(args[0] === "config" && args[1] === "allow-namespace"),
	);
	assert.deepEqual(write, [], `the card never grants or binds: ${JSON.stringify(write)}`);

	// The unknown subcommand says what the command does instead of acting.
	assert.equal(ctx.notices.length, 2, `both runs said one line: ${JSON.stringify(ctx.notices)}`);
	assert.match(
		ctx.notices[1].message,
		/not a subcommand|only .*status/i,
		`the refusal names the one subcommand: ${ctx.notices[1].message}`,
	);
});

test("an engine that cannot answer still yields a card, and says why", (t) => {
	const { dir } = setup(t);
	// A store the engine cannot read is not a session with no memories.
	process.env.MNEXIS_BIN = join(dir, "missing-mnexis");
	const { commands, entries } = fakeHost();
	// The extension builds its client when the factory runs, so the missing
	// binary is the one every call of this session gets.
	return (async () => {
		await commands.get("mnexis").handler("status", context("01JX"));

		assert.equal(entries.length, 1, "the card is still written");
		const card = entries[0].data;
		assert.equal(card.namespace, null, "nothing is invented about the binding");
		assert.ok(
			card.engine && typeof card.engine.error === "string",
			`the failure is carried: ${JSON.stringify(card.engine)}`,
		);
	})();
});
