/**
 * The footer adoption counters (odd/tasks/ext-ts-rewrite.md, UX2).
 *
 * Adoption has no instrument: the footer status only existed inside a tool's
 * execute, so a session that read nothing showed nothing. The counters answer
 * the product's core question — did this session use the memory engine, and
 * how — and they are set at `session_start` so the line is visible before the
 * first call, on the same footer key the tools already use.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.ts";

/** A host that records its handlers and a footer that records its writes. */
function fakeHost(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-footer-"));
	t.after(() => rmSync(dir, { recursive: true, force: true }));
	const previous = process.env.MNEXIS_BIN;
	// The session hooks this corpus does not exercise would call the engine;
	// a missing binary is the quietest way to keep the corpus hermetic.
	process.env.MNEXIS_BIN = join(dir, "missing-mnexis");
	t.after(() => {
		if (previous === undefined) {
			delete process.env.MNEXIS_BIN;
		} else {
			process.env.MNEXIS_BIN = previous;
		}
	});

	const handlers = new Map();
	const writes = [];
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry() {},
	});
	return { handlers, writes };
}

/** The context of an interactive session, whose footer records every write. */
function context(sessionId, writes) {
	return {
		cwd: process.cwd(),
		hasUI: true,
		mode: "tui",
		ui: {
			confirm: async () => false,
			setStatus(key, text) {
				writes.push({ key, text });
			},
		},
		sessionManager: { getSessionId: () => sessionId },
	};
}

/** The last line the footer was given. */
function lastText(writes) {
	const last = writes[writes.length - 1];
	return last ? last.text : "";
}

/** Fire one event through the handlers the extension registered for it. */
async function emit(handlers, event, payload, ctx) {
	const results = [];
	for (const handler of handlers.get(event) ?? []) {
		results.push(await handler({ type: event, ...payload }, ctx));
	}
	return results;
}

/** A tool call the host is about to run. */
function call(toolName, id = `c-${toolName}`) {
	return { toolCallId: id, toolName, input: {} };
}

/** A tool whose answer came back. */
function ended(toolName, isError = false, id = `e-${toolName}`) {
	return { toolCallId: id, toolName, isError, result: { content: [], details: {} } };
}

test("a session starts with the counters at zero and the line visible", async (t) => {
	const { handlers, writes } = fakeHost(t);
	const ctx = context("01JX", writes);

	await emit(handlers, "session_start", { reason: "new" }, ctx);

	assert.ok(writes.length > 0, "the footer was set at the session's start");
	assert.equal(writes[writes.length - 1].key, "mnexis", "the counters ride the tools' own key");
	assert.equal(
		lastText(writes),
		"🤖 mnexis · ctx 0 · search 0 · save 0",
		`the counters start at zero: ${lastText(writes)}`,
	);
});

test("a read counts in its own column, per call", async (t) => {
	const { handlers, writes } = fakeHost(t);
	const ctx = context("01JX", writes);
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	await emit(handlers, "tool_call", call("mnexis_context"), ctx);
	assert.match(lastText(writes), /ctx 1/, `a context call counts as ctx: ${lastText(writes)}`);

	await emit(handlers, "tool_call", call("mnexis_workspace"), ctx);
	assert.match(lastText(writes), /ctx 2/, `a workspace call counts as ctx: ${lastText(writes)}`);

	await emit(handlers, "tool_call", call("mnexis_search"), ctx);
	assert.match(lastText(writes), /search 1/, `a search counts as search: ${lastText(writes)}`);

	await emit(handlers, "tool_call", call("mnexis_show"), ctx);
	assert.match(lastText(writes), /search 2/, `a show counts as search: ${lastText(writes)}`);

	// The other tools move no counter: the line answers "how did this
	// session use the memory engine", not "how busy was the session".
	await emit(handlers, "tool_call", call("mnexis_save"), ctx);
	await emit(handlers, "tool_call", call("write"), ctx);
	assert.match(lastText(writes), /ctx 2 · search 2 · save 0/, `nothing else counted: ${lastText(writes)}`);
});

test("a save counts when it landed, and a failed save does not count", async (t) => {
	const { handlers, writes } = fakeHost(t);
	const ctx = context("01JX", writes);
	await emit(handlers, "session_start", { reason: "new" }, ctx);

	await emit(handlers, "tool_execution_end", ended("mnexis_save", true), ctx);
	assert.match(
		lastText(writes),
		/save 0/,
		`a refused save changed no store and counts nothing: ${lastText(writes)}`,
	);

	await emit(handlers, "tool_execution_end", ended("mnexis_save", false), ctx);
	assert.match(lastText(writes), /save 1/, `the save that landed is counted: ${lastText(writes)}`);

	await emit(handlers, "tool_execution_end", ended("mnexis_save", false, "e-2"), ctx);
	assert.match(lastText(writes), /save 2/, `and so is the next one: ${lastText(writes)}`);
});

test("counters carry across the session and reset with the next one", async (t) => {
	const { handlers, writes } = fakeHost(t);
	const ctx = context("01JX", writes);
	await emit(handlers, "session_start", { reason: "new" }, ctx);
	await emit(handlers, "tool_call", call("mnexis_context"), ctx);
	await emit(handlers, "tool_execution_end", ended("mnexis_save"), ctx);

	await emit(handlers, "agent_end", {}, ctx);
	assert.match(
		lastText(writes),
		/ctx 1 · search 0 · save 1/,
		`the counters stay visible after the turn: ${lastText(writes)}`,
	);

	await emit(handlers, "session_start", { reason: "reload" }, context("01JX", writes));
	assert.equal(
		lastText(writes),
		"🤖 mnexis · ctx 0 · search 0 · save 0",
		`a new session starts the counters again: ${lastText(writes)}`,
	);
});

test("a host with no footer API still runs the session", async (t) => {
	const { handlers } = fakeHost(t);
	const ctx = { ...context("01JX", []), ui: {} };
	await emit(handlers, "session_start", { reason: "new" }, ctx);
	await emit(handlers, "tool_call", call("mnexis_context"), ctx);
	assert.equal(ctx.ui.setStatus, undefined, "nothing was injected into the host's ui");
});
