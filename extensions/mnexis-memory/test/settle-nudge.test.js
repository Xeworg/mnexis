/**
 * The settle nudge (odd/tasks/capture-discipline.md, S4).
 *
 * The hooks capture the mechanical floor; whether a session deserved a
 * memory is judgment. The extension asks the model once, at the final
 * actionable boundary, and only when the session's own events show unsaved
 * work: a successful write/edit happened and no `mnexis_save` was called.
 * A read-only session and a session that already saved pay nothing, and the
 * question is never asked twice in one session.
 *
 * Each test drives the hooks pi hands the extension and asserts the public
 * boundary result (the entry it would append and whether it requests a
 * continuation), never the counters behind it.
 */

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import mnexisMemory from "../index.js";

/** The one-line question the model reads when the evidence says it owes one. */
const NUDGE =
	"This session changed files but saved no memory. If a decision, preference, discovery or project-state change deserves to survive the session, save it now with one mnexis_save; otherwise reply that nothing deserves memory.";

/** A host that records the handlers, with no engine under the hooks. */
function fakeHost(t) {
	const dir = mkdtempSync(join(tmpdir(), "mnexis-nudge-"));
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
	mnexisMemory({
		registerTool() {},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry() {},
	});
	return { handlers };
}

/** The pi context of a session. */
function context(sessionId) {
	return {
		cwd: process.cwd(),
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

/** A tool that really wrote a file. */
function wroteFile(toolName = "write") {
	return { toolCallId: "w-1", toolName, isError: false, result: {} };
}

/** One model-issued `mnexis_save` call. */
const savedMemory = { toolCallId: "s-1", toolName: "mnexis_save", input: {} };

test("a session that changed files and saved nothing is asked once", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX"));
	assert.deepEqual(
		results,
		[
			{
				entries: [
					{
						type: "custom_message",
						customType: "mnexis-settle-nudge",
						content: NUDGE,
						display: false,
					},
				],
				continue: true,
			},
		],
		`the settle asks once and requests one continuation: ${JSON.stringify(results)}`,
	);
	assert.equal(NUDGE.includes("\n"), false, "the nudge is one line");
});

test("the settle never asks twice in one session, even after another write", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	const first = await emit(handlers, "agent_before_settle", {}, context("01JX"));
	assert.equal(first.length, 1);
	assert.equal(first[0]?.continue, true);

	// A second write after the question does not reopen it: the model was
	// already asked, and a loop is how a nudge becomes a bill.
	await emit(handlers, "tool_execution_end", wroteFile("edit"), context("01JX"));
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);
});

test("a read-only session pays nothing, and a failed write is not evidence", async (t) => {
	const { handlers } = fakeHost(t);
	for (const toolName of ["read", "grep", "find", "ls"]) {
		await emit(
			handlers,
			"tool_execution_end",
			{ toolCallId: `r-${toolName}`, toolName, isError: false, result: {} },
			context("01JX"),
		);
	}
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);

	// A write the engine refused did not change a file, so there is no
	// unsaved work to ask about.
	await emit(
		handlers,
		"tool_execution_end",
		{ toolCallId: "w-failed", toolName: "write", isError: true, result: {} },
		context("01JX"),
	);
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);
});

test("a session that already saved a memory is not asked", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_call", savedMemory, context("01JX"));
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);

	// And a save after the writes still settles the question: the session
	// did the judgment, whatever the order.
	await emit(handlers, "tool_execution_end", wroteFile("edit"), context("01JX"));
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);
});

test("the evidence and the once-per-session question reset with the next session", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	assert.equal((await emit(handlers, "agent_before_settle", {}, context("01JX")))[0]?.continue, true);
	assert.deepEqual(await emit(handlers, "agent_before_settle", {}, context("01JX")), [undefined]);

	// A session that starts fresh carries neither the previous session's
	// evidence nor its spent question — for every reason a session starts.
	for (const reason of ["reload", "new", "resume", "fork"]) {
		await emit(handlers, "session_start", { reason }, context(`01JX-${reason}`));
		assert.deepEqual(
			await emit(handlers, "agent_before_settle", {}, context(`01JX-${reason}`)),
			[undefined],
			`${reason}: the previous session's evidence is gone`,
		);
		await emit(handlers, "tool_execution_end", wroteFile(), context(`01JX-${reason}`));
		assert.equal(
			(await emit(handlers, "agent_before_settle", {}, context(`01JX-${reason}`)))[0]?.continue,
			true,
			`${reason}: the new session can be asked once`,
		);
		assert.deepEqual(
			await emit(handlers, "agent_before_settle", {}, context(`01JX-${reason}`)),
			[undefined],
			`${reason}: and only once`,
		);
	}
});
