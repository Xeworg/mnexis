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
 * The question carries the doctrine, because the question is priced per fire
 * and not per call (odd/tasks/nudge-enriched.md, S4): the examples of what
 * deserves to survive, then the seven record types one line each so the save
 * arrives typed. The gating is the one from capture-discipline S4 and does not
 * move with the text (S3), and the text stays inside its per-fire budget (S5).
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

/** The question the model reads when the evidence says it owes one (S4). */
const NUDGE = [
	"This session changed files but saved no memory. If a decision, preference, discovery or project-state change deserves to survive the session, save it now with one mnexis_save; otherwise reply that nothing deserves memory.",
	"",
	"What deserves to survive, for example: a decision an external change forced (a version pin, a constraint that moved); a discovery about how this repo actually works; a preference the user stated about how they want things done.",
	"",
	"Save it typed — pass the one that fits:",
	"knowledge — a durable fact about the domain, the tools or the data.",
	"decision — a choice made, and what it beat.",
	"preference — how the user wants things done.",
	"procedure — the steps that reliably achieve something.",
	"project_state — where the work stands now.",
	"episode — something that happened in a session.",
	"fact — the neutral default when none of the others fit.",
].join("\n");

/** The record types the save must name, in the order the nudge lists them. */
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

/** The text of the one question a writing session is asked. */
async function askedText(t) {
	const { handlers } = fakeHost(t);
	await emit(handlers, "tool_execution_end", wroteFile(), context("01JX"));
	const results = await emit(handlers, "agent_before_settle", {}, context("01JX"));
	return results[0]?.entries?.[0]?.content ?? "";
}

test("the nudge names what deserves to survive, with three concrete examples", async (t) => {
	const text = await askedText(t);

	// S4a: the moment names the judgment instead of asking for it in the
	// abstract — an external change, a discovery, a stated preference.
	assert.match(text, /What deserves to survive, for example:/, `the examples are labelled: ${text}`);
	for (const example of [
		"a decision an external change forced",
		"a discovery about how this repo actually works",
		"a preference the user stated",
	]) {
		assert.ok(text.includes(example), `the nudge names the example "${example}": ${text}`);
	}
});

test("the nudge names all seven record types, one line each, so the save arrives typed", async (t) => {
	const text = await askedText(t);
	const lines = text.split("\n");

	// S4b: the type is a parameter of the save, and the model writes it only
	// when the moment names it.
	for (const type of MEMORY_TYPES) {
		assert.equal(
			lines.filter((line) => line.startsWith(`${type} — `)).length,
			1,
			`exactly one line defines ${type}: ${text}`,
		);
	}
});

test("the enriched nudge stays inside its per-fire budget", async (t) => {
	const text = await askedText(t);
	const bytes = Buffer.byteLength(text, "utf8");

	// S5: the text is paid as one model request the moment it fires, so
	// doctrine past this belongs to the skill, not to the nudge.
	assert.ok(bytes <= 1200, `the nudge fits the 1200 B budget: ${bytes} B`);
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

	// A session that already attempted a save pays nothing.
	const saved = fakeHost(t);
	await emit(saved.handlers, "tool_call", savedMemory, context("01JX"));
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
