/**
 * The settle nudge (odd/tasks/capture-discipline.md, S4; odd/tasks/ext-ts-rewrite.md,
 * AB5 and AB6).
 *
 * The hooks capture the mechanical floor; whether a session deserved a
 * memory is judgment. The extension asks the model once, at the final
 * actionable boundary, and only when the session's own events show unsaved
 * work: a successful write/edit happened and no memory was saved.
 * A read-only session and a session that already saved pay nothing, and the
 * question is never asked twice in one session.
 *
 * The evidence is the outcome, not the attempt (AB6): a save that failed, or
 * one the write gate denied, changed no store, so the question stays open —
 * and a denied save reaches it with the gate's own reason, which is the one
 * thing the model could not read for itself. The doctrine the question used to
 * carry (the examples and the seven types) lives in `mnexis_save`'s own
 * description now (AB5), where the model reads it at the moment of the write.
 * The gating is the one from capture-discipline S4, and the text stays inside
 * its per-fire budget (S5).
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

import mnexisMemory from "../index.ts";

/**
 * The question the model reads when the evidence says it owes one (S4).
 *
 * One judgment line, and a pointer to the tool that states what deserves to
 * survive and how to type it (AB5): the doctrine is read when the save is
 * written, not once at the end.
 */
const NUDGE = [
	"This session changed files but saved no memory. If a decision, preference, discovery or project-state change deserves to survive the session, save it now with one mnexis_save; otherwise reply that nothing deserves memory.",
	"",
	"`mnexis_save` states what deserves to survive and the seven record types, one line each, so the save arrives typed.",
].join("\n");

/** The record types the save states, in the order the save states them. */
const MEMORY_TYPES = [
	"knowledge",
	"decision",
	"preference",
	"procedure",
	"project_state",
	"episode",
	"fact",
];

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
	const messageRenderers = new Map();
	mnexisMemory({
		registerTool() {},
		registerMessageRenderer(customType, renderer) {
			messageRenderers.set(customType, renderer);
		},
		on(event, handler) {
			const list = handlers.get(event) ?? [];
			list.push(handler);
			handlers.set(event, list);
		},
		appendEntry() {},
	});
	return { handlers, messageRenderers };
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

/** A save that landed: the engine answered and nothing was refused (AB6). */
const savedMemory = {
	toolCallId: "s-1",
	toolName: "mnexis_save",
	isError: false,
	result: { content: [], details: {} },
};

/** A save the engine refused: a failed call, not a store this session updated. */
const failedSave = {
	toolCallId: "s-2",
	toolName: "mnexis_save",
	isError: true,
	result: {
		content: [{ type: "text", text: "The engine refused the save: the store is locked." }],
		details: {},
	},
};

/** The reason the write gate gave when it denied the save. */
const GATE_REFUSAL = "Authorization for namespace 'acme' was declined. No memory was written.";

/**
 * A save the write gate denied: its result carries the refusal text and the
 * `authorized: false` the gate writes into the details.
 */
const deniedSave = {
	toolCallId: "s-3",
	toolName: "mnexis_save",
	isError: true,
	result: {
		content: [{ type: "text", text: GATE_REFUSAL }],
		details: { namespace: "acme", authorized: false },
	},
};

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
						// UX4: the human can tell the question fired — it used to be
						// `display: false`, indistinguishable from agent spontaneity.
						display: true,
					},
				],
				continue: true,
			},
		],
		`the settle asks once and requests one continuation: ${JSON.stringify(results)}`,
	);
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
	await emit(handlers, "tool_execution_end", savedMemory, context("01JX"));
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

/** The text of the one question a writing session is asked. */
async function askedText(t) {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	const results = await emit(handlers, "agent_before_settle", {}, context("01JX"));
	return results[0]?.entries?.[0]?.content ?? "";
}

test("the nudge points at mnexis_save for the doctrine it no longer carries (AB5)", async (t) => {
	const text = await askedText(t);

	// S4a: the moment still names the judgment — an external change, a
	// discovery, a stated preference — because that is what makes the model
	// consider it now. The taxonomy itself moved to the tool the model writes
	// with, where it is read at the write and not once at the end.
	assert.match(text, /This session changed files but saved no memory/, `the judgment is stated: ${text}`);
	assert.match(text, /`mnexis_save` states what deserves to survive/, `the pointer names the tool: ${text}`);
	assert.doesNotMatch(
		text,
		/What deserves to survive, for example:/,
		`the examples are not carried twice: ${text}`,
	);
	const lines = text.split("\n");
	for (const type of MEMORY_TYPES) {
		assert.equal(
			lines.filter((line) => line.startsWith(`${type} — `)).length,
			0,
			`the nudge no longer defines ${type}: ${text}`,
		);
	}
});

test("the focused nudge stays inside its per-fire budget", async (t) => {
	const text = await askedText(t);
	const bytes = Buffer.byteLength(text, "utf8");

	// S5: the text is paid as one model request the moment it fires, so the
	// doctrine it points at stays in the tool description, not here.
	assert.ok(bytes <= 1200, `the nudge fits the 1200 B budget: ${bytes} B`);
});

test("a save that failed does not settle the question (AB6)", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));

	// The outcome is what counts: a refused save changed no store, so the
	// session still owes the judgment and the question is still open.
	await emit(handlers, "tool_execution_end", failedSave, context("01JX"));
	const results = await emit(handlers, "agent_before_settle", {}, context("01JX"));
	assert.equal(
		results[0]?.continue,
		true,
		`a failed save is unsaved work: ${JSON.stringify(results)}`,
	);
});

test("a save the gate denied keeps the question open, and the nudge carries the reason (AB6)", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	await emit(handlers, "tool_execution_end", deniedSave, context("01JX"));

	const results = await emit(handlers, "agent_before_settle", {}, context("01JX"));
	const text = results[0]?.entries?.[0]?.content ?? "";
	assert.equal(results[0]?.continue, true, `a denied save is unsaved work: ${JSON.stringify(results)}`);
	assert.ok(text.startsWith(NUDGE), `the question is still asked first: ${text}`);
	assert.ok(
		text.includes(GATE_REFUSAL),
		`the reason the save did not land reaches the model: ${text}`,
	);
	assert.match(text, /was refused/, `and the nudge says the save was refused: ${text}`);
});

test("a save that lands after a denial closes the question (AB6)", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	await emit(handlers, "tool_execution_end", deniedSave, context("01JX"));
	await emit(handlers, "tool_execution_end", savedMemory, context("01JX"));

	assert.deepEqual(
		await emit(handlers, "agent_before_settle", {}, context("01JX")),
		[undefined],
		"the session did the judgment in the end",
	);
});

test("the richer text did not change when the question fires", async (t) => {
	// S3, condition by condition. A read-only session pays nothing.
	const readOnly = fakeHost(t);
	await emit(readOnly.handlers, "tool_execution_end", wroteFile("read"), context("01JX"));
	assert.deepEqual(
		await emit(readOnly.handlers, "agent_before_settle", {}, context("01JX")),
		[undefined],
		"read-only: no question",
	);

	// A session whose save landed pays nothing.
	const saved = fakeHost(t);
	await emit(saved.handlers, "tool_execution_end", savedMemory, context("01JX"));
	await emit(saved.handlers, "tool_execution_end", wroteFile(), context("01JX"));
	assert.deepEqual(
		await emit(saved.handlers, "agent_before_settle", {}, context("01JX")),
		[undefined],
		"already saved: no question",
	);

	// Files changed and no save attempted: exactly one question, one time.
	const wrote = fakeHost(t);
	await emit(wrote.handlers, "tool_execution_end", wroteFile(), context("01JX"));
	assert.equal(
		(await emit(wrote.handlers, "agent_before_settle", {}, context("01JX")))[0]?.continue,
		true,
		"unsaved work: one question",
	);
	assert.deepEqual(
		await emit(wrote.handlers, "agent_before_settle", {}, context("01JX")),
		[undefined],
		"unsaved work: and never a second time",
	);
});

// --- R6: the evidence is the branch's, not the process's -----------------
//
// `/tree` moves the session to another leaf of the same session: the branch
// that was just left took its evidence and its spent question with it. Without
// a `session_tree` handler the latch leaked across branches — a nudge spent on
// branch A silenced branch B, and evidence from before the switch fired a
// question about work the branch does not have.

test("a branch switch resets the latch: the question spent on one branch is open on the next", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	assert.equal(
		(await emit(handlers, "agent_before_settle", {}, context("01JX")))[0]?.continue,
		true,
		"branch A is asked once",
	);

	await emit(handlers, "session_tree", { newLeafId: "leaf-b", oldLeafId: "leaf-a" }, context("01JX"));
	await emit(handlers, "tool_execution_end", wroteFile("edit"), context("01JX"));

	assert.equal(
		(await emit(handlers, "agent_before_settle", {}, context("01JX")))[0]?.continue,
		true,
		"branch B is asked on its own evidence",
	);
});

test("a branch switch clears the evidence it did not inherit", async (t) => {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));

	await emit(handlers, "session_tree", { newLeafId: "leaf-b", oldLeafId: "leaf-a" }, context("01JX"));

	assert.deepEqual(
		await emit(handlers, "agent_before_settle", {}, context("01JX")),
		[undefined],
		"the writes of the branch we left are not the work of this one",
	);
});

// --- UX4: the question is visible, and it says it was the extension ---

test("the message renderer is registered for the nudge", (t) => {
	const { messageRenderers } = fakeHost(t);
	assert.ok(
		messageRenderers.has("mnexis-settle-nudge"),
		`the nudge has its own renderer: ${JSON.stringify([...messageRenderers.keys()])}`,
	);
	assert.equal(
		typeof messageRenderers.get("mnexis-settle-nudge"),
		"function",
		"and the renderer is callable",
	);
});

test("the nudge collapses to one line and expands to its text", (t) => {
	const { messageRenderers } = fakeHost(t);
	const renderer = messageRenderers.get("mnexis-settle-nudge");
	const theme = { fg: (color, text) => `<${color}>${text}</${color}>` };
	const message = { customType: "mnexis-settle-nudge", content: NUDGE, display: true };

	const collapsed = renderer(message, { expanded: false }, theme);
	const collapsedLines = collapsed.render(80);
	assert.equal(collapsedLines.length, 1, `collapsed, the nudge is one line: ${JSON.stringify(collapsedLines)}`);
	assert.match(collapsedLines[0], /mnexis/, `it names the engine: ${collapsedLines[0]}`);
	assert.match(collapsedLines[0], /asked the agent to save/, `it says what happened: ${collapsedLines[0]}`);
	assert.match(collapsedLines[0], /once per session/, `and how often: ${collapsedLines[0]}`);

	const expanded = renderer(message, { expanded: true }, theme).render(80).join("\n");
	assert.match(expanded, /This session changed files but saved no memory/, `the question is readable: ${expanded}`);
	assert.match(expanded, /mnexis_save/, `with its pointer: ${expanded}`);
});

test("a host without message renderers still registers its tools", () => {
	// pi before the renderer API: the registration is guarded and nothing
	// throws while the extension loads.
	assert.doesNotThrow(() => {
		mnexisMemory({ registerTool() {}, on() {}, appendEntry() {} });
	});
});
